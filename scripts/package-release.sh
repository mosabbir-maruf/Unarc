#!/usr/bin/env bash
set -euo pipefail

DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
TARGET_OS="${1:-}"
TARGET_ARCH="${2:-}"
BIN_DIR="${3:-}"

if [ -z "${TARGET_OS}" ] || [ -z "${TARGET_ARCH}" ] || [ -z "${BIN_DIR}" ]; then
    echo "Usage: $0 <macos|linux> <arm64|aarch64|x86_64> <binary-dir>" >&2
    echo "Example: $0 macos arm64 target/aarch64-apple-darwin/release" >&2
    exit 1
fi

VERSION=$(grep '^version =' "${DIR}/Cargo.toml" | head -1 | cut -d '"' -f 2)
DIST_DIR="${DIR}/dist"
mkdir -p "${DIST_DIR}"

UNARC_BIN="${BIN_DIR}/unarc"
SEVENZZ_BIN="${BIN_DIR}/7zz"

if [ ! -f "${UNARC_BIN}" ]; then
    echo "Error: unarc binary not found at '${UNARC_BIN}'" >&2
    exit 1
fi
if [ ! -f "${SEVENZZ_BIN}" ]; then
    echo "Notice: Separate 7zz binary not found at '${SEVENZZ_BIN}'. Standalone executable contains embedded 7zz."
fi

echo "=========================================================="
echo "Packaging Unarc Standalone Release: v${VERSION} (${TARGET_OS}-${TARGET_ARCH})"
echo "=========================================================="

PACKAGE_NAME="unarc-${VERSION}-${TARGET_OS}-${TARGET_ARCH}"

# 1. Primary standalone executable
STANDALONE_BIN="${DIST_DIR}/${PACKAGE_NAME}"
cp "${UNARC_BIN}" "${STANDALONE_BIN}"
chmod 0755 "${STANDALONE_BIN}"
STANDALONE_SHA=$(shasum -a 256 "${STANDALONE_BIN}" | awk '{print $1}')
echo "${STANDALONE_SHA}  ${PACKAGE_NAME}" > "${DIST_DIR}/${PACKAGE_NAME}.sha256"
echo "[1/4] Created standalone executable: ${STANDALONE_BIN} (${STANDALONE_SHA})"

# 2. Release tarball bundle (for documentation / package managers)
STAGE_DIR="$(mktemp -d /tmp/unarc_package_XXXXXX)"
trap 'rm -rf "${STAGE_DIR}"' EXIT

mkdir -p "${STAGE_DIR}/${PACKAGE_NAME}"
cp "${UNARC_BIN}" "${STAGE_DIR}/${PACKAGE_NAME}/unarc"
if [ -f "${SEVENZZ_BIN}" ]; then
    cp "${SEVENZZ_BIN}" "${STAGE_DIR}/${PACKAGE_NAME}/7zz"
    chmod 0755 "${STAGE_DIR}/${PACKAGE_NAME}/7zz"
fi
chmod 0755 "${STAGE_DIR}/${PACKAGE_NAME}/unarc"

if [ -f "${DIR}/README.md" ]; then
    cp "${DIR}/README.md" "${STAGE_DIR}/${PACKAGE_NAME}/"
fi
if [ -f "${DIR}/LICENSE" ]; then
    cp "${DIR}/LICENSE" "${STAGE_DIR}/${PACKAGE_NAME}/"
fi
if [ -f "${DIR}/THIRD-PARTY-NOTICES.md" ]; then
    cp "${DIR}/THIRD-PARTY-NOTICES.md" "${STAGE_DIR}/${PACKAGE_NAME}/"
fi

ARCHIVE_PATH="${DIST_DIR}/${PACKAGE_NAME}.tar.gz"
tar -czf "${ARCHIVE_PATH}" -C "${STAGE_DIR}" "${PACKAGE_NAME}"
ARCHIVE_SHA=$(shasum -a 256 "${ARCHIVE_PATH}" | awk '{print $1}')
SHA_FILE="${DIST_DIR}/${PACKAGE_NAME}.tar.gz.sha256"
echo "${ARCHIVE_SHA}  ${PACKAGE_NAME}.tar.gz" > "${SHA_FILE}"
echo "[2/4] Created release tarball: ${ARCHIVE_PATH} (${ARCHIVE_SHA})"

# 3. Sign manifest using unarc-sign for standalone executable
MANIFEST_FILE="${DIST_DIR}/${PACKAGE_NAME}.manifest.json"
echo "[3/4] Generating and signing release manifest for standalone executable..."

# Normalise OS and ARCH names for manifest
MANIFEST_OS="${TARGET_OS}"
MANIFEST_ARCH="${TARGET_ARCH}"
if [ "${MANIFEST_ARCH}" = "arm64" ]; then
    MANIFEST_ARCH="aarch64"
fi

STRICT_ARGS=()
if [ "${STRICT_SIGNING:-false}" = "true" ] || [ "${4:-}" = "--strict" ]; then
    STRICT_ARGS=(--strict)
    if [ -z "${RELEASE_SIGNING_KEY:-}" ]; then
        echo "Error: RELEASE_SIGNING_KEY secret is required in strict release mode" >&2
        exit 1
    fi
fi

if [ -n "${RELEASE_SIGNING_KEY:-}" ]; then
    # Run unarc-sign (either locally if available or through Docker dev container)
    if [ -f "${DIR}/target/release/unarc-sign" ]; then
        "${DIR}/target/release/unarc-sign" \
            --artifact "${STANDALONE_BIN}" \
            --os "${MANIFEST_OS}" \
            --arch "${MANIFEST_ARCH}" \
            --version "${VERSION}" \
            --out-manifest "${MANIFEST_FILE}" \
            --artifact-url "${PACKAGE_NAME}" \
            ${STRICT_ARGS[@]+"${STRICT_ARGS[@]}"}
    elif [ -f "${DIR}/target/debug/unarc-sign" ]; then
        "${DIR}/target/debug/unarc-sign" \
            --artifact "${STANDALONE_BIN}" \
            --os "${MANIFEST_OS}" \
            --arch "${MANIFEST_ARCH}" \
            --version "${VERSION}" \
            --out-manifest "${MANIFEST_FILE}" \
            --artifact-url "${PACKAGE_NAME}" \
            ${STRICT_ARGS[@]+"${STRICT_ARGS[@]}"}
    elif [ -f "${DIR}/target/aarch64-apple-darwin/release/unarc-sign" ]; then
        "${DIR}/target/aarch64-apple-darwin/release/unarc-sign" \
            --artifact "${STANDALONE_BIN}" \
            --os "${MANIFEST_OS}" \
            --arch "${MANIFEST_ARCH}" \
            --version "${VERSION}" \
            --out-manifest "${MANIFEST_FILE}" \
            --artifact-url "${PACKAGE_NAME}" \
            ${STRICT_ARGS[@]+"${STRICT_ARGS[@]}"}
    else
        # Run inside Docker via dev.sh (handles macOS /Volumes bind-mount fallback)
        "${DIR}/scripts/dev.sh" cargo run --bin unarc-sign -- \
            --artifact "/workspace/dist/${PACKAGE_NAME}" \
            --os "${MANIFEST_OS}" \
            --arch "${MANIFEST_ARCH}" \
            --version "${VERSION}" \
            --out-manifest "/workspace/dist/${PACKAGE_NAME}.manifest.json" \
            --artifact-url "${PACKAGE_NAME}" \
            ${STRICT_ARGS[@]+"${STRICT_ARGS[@]}"}
    fi

    # 4. Verify release manifest signature and artifact hash
    echo "[4/4] Verifying release manifest signature and artifact hash..."
    if [ -f "${DIR}/target/release/unarc-sign" ]; then
        "${DIR}/target/release/unarc-sign" --verify-manifest "${MANIFEST_FILE}"
    elif [ -f "${DIR}/target/debug/unarc-sign" ]; then
        "${DIR}/target/debug/unarc-sign" --verify-manifest "${MANIFEST_FILE}"
    elif [ -f "${DIR}/target/aarch64-apple-darwin/release/unarc-sign" ]; then
        "${DIR}/target/aarch64-apple-darwin/release/unarc-sign" --verify-manifest "${MANIFEST_FILE}"
    else
        "${DIR}/scripts/dev.sh" cargo run --bin unarc-sign -- --verify-manifest "/workspace/dist/${PACKAGE_NAME}.manifest.json"
    fi

    # Create canonical unversioned manifest aliases for direct latest release downloads
    cp "${MANIFEST_FILE}" "${DIST_DIR}/unarc-${TARGET_OS}-${TARGET_ARCH}.manifest.json"
    cp "${MANIFEST_FILE}" "${DIST_DIR}/manifest-${TARGET_OS}-${TARGET_ARCH}.json"
else
    echo "[3/4] Notice: RELEASE_SIGNING_KEY not set; skipping release manifest signing."
    echo "[4/4] Release packaging complete (unsigned development artifact)."
fi

echo "=========================================================="
echo "Artifacts successfully created in ${DIST_DIR}:"
ls -la "${DIST_DIR}/${PACKAGE_NAME}"*
echo "=========================================================="
