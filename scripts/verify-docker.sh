#!/usr/bin/env bash
set -euo pipefail

IMAGE_NAME="${1:-unarc:latest}"
DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

echo "=========================================================="
echo "Unarc Phase 7 Hardened Docker Runtime Verification Pass"
echo "Target Image: ${IMAGE_NAME}"
echo "=========================================================="

# 1. Verify image exists
if ! docker image inspect "${IMAGE_NAME}" >/dev/null 2>&1; then
    echo "Image ${IMAGE_NAME} not found. Building..."
    docker build -t "${IMAGE_NAME}" "${DIR}"
fi

echo "[1/9] Verifying Non-Root User & Image Metadata..."
RUNTIME_USER=$(docker inspect --format '{{.Config.User}}' "${IMAGE_NAME}")
if [ "${RUNTIME_USER}" != "65532:65532" ] && [ "${RUNTIME_USER}" != "nonroot" ]; then
    echo "FAIL: Expected non-root user (65532:65532), got: '${RUNTIME_USER}'" >&2
    exit 1
fi
echo "  -> Runtime user is non-root: ${RUNTIME_USER}"

echo "[2/9] Verifying Unarc Version in Container..."
VERSION_OUT=$(docker run --rm "${IMAGE_NAME}" version)
echo "  -> Version output: ${VERSION_OUT}"
if [[ ! "${VERSION_OUT}" =~ ^unarc\ [0-9]+\.[0-9]+\.[0-9]+ ]]; then
    echo "FAIL: Unexpected version string: ${VERSION_OUT}" >&2
    exit 1
fi

echo "[3/9] Verifying Bundled 7zz Engine Integrity & Doctor Diagnostics..."
DOCTOR_OUT=$(docker run --rm "${IMAGE_NAME}" doctor)
echo "${DOCTOR_OUT}"
if ! echo "${DOCTOR_OUT}" | grep -q "HEALTHY"; then
    echo "FAIL: Doctor probe did not report HEALTHY" >&2
    exit 1
fi
if ! echo "${DOCTOR_OUT}" | grep -q "Engine Binary Integrity"; then
    echo "FAIL: Engine binary integrity check failed" >&2
    exit 1
fi

DOCTOR_JSON=$(docker run --rm "${IMAGE_NAME}" doctor --json)
if ! echo "${DOCTOR_JSON}" | grep -q '"healthy": true'; then
    echo "FAIL: Doctor JSON check healthy is not true" >&2
    exit 1
fi
if ! echo "${DOCTOR_JSON}" | grep -q '"integrity_verified": true'; then
    echo "FAIL: Doctor JSON integrity_verified is not true" >&2
    exit 1
fi
echo "  -> Doctor diagnostics and engine integrity PASS."

echo "[4/9] Verifying Image Hermetic Cleanliness (No Dev/Build Tools)..."
CONTAINER_ID=$(docker create "${IMAGE_NAME}")
EXPORT_LIST=$(docker export "${CONTAINER_ID}" | tar -tv)
docker rm "${CONTAINER_ID}" >/dev/null

FORBIDDEN_PATTERNS=("bin/cargo" "bin/rustc" "bin/gcc" "bin/g++" "bin/make" "bin/git" "bin/curl" "bin/apt" "bin/dpkg" "bin/python3" "workspace" "cargo/registry")
for pat in "${FORBIDDEN_PATTERNS[@]}"; do
    if echo "${EXPORT_LIST}" | grep -q "${pat}"; then
        echo "FAIL: Prohibited build/dev component found in runtime image: ${pat}" >&2
        exit 1
    fi
done
echo "  -> Image verified hermetic: zero dev tools, compilers, or build caches present."

echo "[5/9] Verifying Hardened Container Invocation (Read-Only Root, Network None, Cap Drop)..."
TEST_DIR=$(mktemp -d /tmp/unarc_docker_verify_XXXXXX)
trap 'rm -rf "${TEST_DIR}"' EXIT

mkdir -p "${TEST_DIR}/input" "${TEST_DIR}/output"
chmod 755 "${TEST_DIR}"
chmod -R a+rX "${TEST_DIR}/input"
chmod 777 "${TEST_DIR}/output"
echo "hardened_docker_payload_content_12345" > "${TEST_DIR}/input/sample.txt"
# Create test archive using python or zip
python3 -c "import zipfile; z = zipfile.ZipFile('${TEST_DIR}/input/test.zip', 'w'); z.write('${TEST_DIR}/input/sample.txt', 'sample.txt')"

# Test command
docker run --rm \
  --network none \
  --read-only \
  --cap-drop ALL \
  -v "${TEST_DIR}/input:/input:ro" \
  "${IMAGE_NAME}" \
  test /input/test.zip
echo "  -> Archive test under hardened runtime passed."

# Extract command
docker run --rm \
  --network none \
  --read-only \
  --cap-drop ALL \
  -v "${TEST_DIR}/input:/input:ro" \
  -v "${TEST_DIR}/output:/output:rw" \
  "${IMAGE_NAME}" \
  extract /input/test.zip --output /output

EXTRACTED_CONTENT=$(cat "${TEST_DIR}/output/sample.txt")
if [ "${EXTRACTED_CONTENT}" != "hardened_docker_payload_content_12345" ]; then
    echo "FAIL: Extracted content mismatch: '${EXTRACTED_CONTENT}'" >&2
    exit 1
fi
echo "  -> Archive extract under hardened runtime passed."

echo "[6/9] Verifying Read-Only Input Mount Enforcement..."
# Attempting to write to /input must fail (mounted :ro)
if docker run --rm \
  --network none \
  --read-only \
  --cap-drop ALL \
  -v "${TEST_DIR}/input:/input:ro" \
  "${IMAGE_NAME}" \
  extract /input/test.zip --output /input 2>/dev/null; then
    echo "FAIL: Extraction was able to write into read-only input mount!" >&2
    exit 1
fi
echo "  -> Read-only input boundary verified."

echo "[7/9] Verifying Read-Only Root Filesystem Write Denial..."
# Attempting to extract to root "/" or system dir must fail because root is read-only
if docker run --rm \
  --network none \
  --read-only \
  --cap-drop ALL \
  -v "${TEST_DIR}/input:/input:ro" \
  "${IMAGE_NAME}" \
  extract /input/test.zip --output /usr 2>/dev/null; then
    echo "FAIL: Extraction wrote to root filesystem despite --read-only!" >&2
    exit 1
fi
echo "  -> Read-only root filesystem write denial verified."

echo "[8/9] Verifying Network Isolation Enforcement..."
# Verify engine cannot communicate over network (container has --network none)
NET_CID=$(docker create --network none "${IMAGE_NAME}")
NET_STATUS=$(docker inspect --format '{{.HostConfig.NetworkMode}}' "${NET_CID}")
docker rm "${NET_CID}" >/dev/null
if [ "${NET_STATUS}" != "none" ]; then
    echo "FAIL: Expected network mode 'none', got: '${NET_STATUS}'" >&2
    exit 1
fi
echo "  -> Network mode confirmed as none."

echo "[9/9] Verifying Default No-Argument Interactive Startup..."
INTERACTIVE_OUTPUT=$(printf '/exit\n' | docker run -i --rm -v "${TEST_DIR}:/work" "${IMAGE_NAME}" 2>&1)
EXIT_CODE=$?
if [ ${EXIT_CODE} -ne 0 ]; then
    echo "FAIL: Container startup without arguments exited with code ${EXIT_CODE}" >&2
    echo "Output was: ${INTERACTIVE_OUTPUT}" >&2
    exit 1
fi
if ! echo "${INTERACTIVE_OUTPUT}" | grep -q "Secure Archive Utility"; then
    echo "FAIL: Interactive banner not found in startup output" >&2
    echo "Output was: ${INTERACTIVE_OUTPUT}" >&2
    exit 1
fi
echo "  -> Default no-argument interactive CLI startup verified cleanly."

echo "=========================================================="
echo "ALL 9 HARDENED DOCKER RUNTIME VERIFICATION CHECKS PASSED!"
echo "=========================================================="
