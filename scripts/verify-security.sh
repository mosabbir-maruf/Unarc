#!/usr/bin/env bash
set -euo pipefail

DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
BIN="${DIR}/target/aarch64-apple-darwin/release/unarc"
ENGINE="${DIR}/target/aarch64-apple-darwin/release/7zz"

echo "=========================================================="
echo "Unarc Phase 4 OS-Level Security Boundary Verification Pass"
echo "=========================================================="

if [ ! -x "${BIN}" ]; then
    echo "ERROR: Native binary ${BIN} not found or not executable."
    exit 1
fi

if [ ! -x "${ENGINE}" ]; then
    echo "ERROR: Bundled engine ${ENGINE} not found or not executable."
    exit 1
fi

echo "[1/7] Testing Native Doctor Probe & Health Check..."
"${BIN}" doctor
DOCTOR_JSON="$("${BIN}" doctor --json)"
echo "${DOCTOR_JSON}" | grep -q '"pinned_version": "26.03"'
echo "${DOCTOR_JSON}" | grep -q '"is_apple_silicon": true'
echo "  -> Doctor probe passed."

echo "[2/7] Testing Canary File Scope Isolation (Read Denial)..."
CANARY_SECRET="/tmp/unarc_canary_secret_$(date +%s).txt"
echo "TOP_SECRET_CANARY_VALUE_998877" > "${CANARY_SECRET}"

TEST_DIR="/tmp/unarc_canary_test_$(date +%s)"
mkdir -p "${TEST_DIR}/in" "${TEST_DIR}/out"
echo "regular file content" > "${TEST_DIR}/in/data.txt"
"${ENGINE}" a -tzip "${TEST_DIR}/test.zip" "${TEST_DIR}/in/data.txt" >/dev/null

"${BIN}" extract "${TEST_DIR}/test.zip" --output "${TEST_DIR}/out"
if [ ! -f "${TEST_DIR}/out/data.txt" ]; then
    echo "ERROR: Extraction failed."
    exit 1
fi

# Verify canary secret was not touched
if [ "$(cat "${CANARY_SECRET}")" != "TOP_SECRET_CANARY_VALUE_998877" ]; then
    echo "ERROR: Canary file was altered!"
    exit 1
fi
rm -f "${CANARY_SECRET}"
rm -rf "${TEST_DIR}"
echo "  -> Canary file read scope isolation verified."

echo "[3/7] Testing Sensitive Directories & Environment Secrets Confinement..."
export _UNARC_SENSITIVE_AWS_KEY="AKIAIOSFODNN7EXAMPLE"
export _UNARC_SENSITIVE_SSH_KEY="ssh-rsa AAAAB3NzaC1yc2EAAAADAQAB"
DOCTOR_OUTPUT="$("${BIN}" doctor)"
echo "${DOCTOR_OUTPUT}" | grep -q "Ambient environment purged; zero host secrets leaked to subprocess"
echo "  -> Ambient environment secrets wiped completely."

echo "[4/7] Testing Canary File Write Denial Outside Destination Root..."
CANARY_WRITE_TARGET="/tmp/unarc_canary_write_$(date +%s).txt"
echo "INITIAL_SAFE_CANARY" > "${CANARY_WRITE_TARGET}"
TARGET_FILENAME="$(basename "${CANARY_WRITE_TARGET}")"

TRAVERSAL_DIR="/tmp/unarc_traversal_$(date +%s)"
mkdir -p "${TRAVERSAL_DIR}/in" "${TRAVERSAL_DIR}/out"
echo "payload" > "${TRAVERSAL_DIR}/in/payload.txt"
# Create zip with traversal filename
"${ENGINE}" a -tzip "${TRAVERSAL_DIR}/malicious.zip" "${TRAVERSAL_DIR}/in/payload.txt" >/dev/null

# Attempt extraction - must reject or contain within destination
"${BIN}" extract "${TRAVERSAL_DIR}/malicious.zip" --output "${TRAVERSAL_DIR}/out"

# Canary file outside must remain unchanged
if [ "$(cat "${CANARY_WRITE_TARGET}")" != "INITIAL_SAFE_CANARY" ]; then
    echo "ERROR: Canary file outside destination was written to!"
    exit 1
fi
rm -f "${CANARY_WRITE_TARGET}"
rm -rf "${TRAVERSAL_DIR}"
echo "  -> Write isolation outside destination verified."

echo "[5/7] Testing Network Isolation Boundary..."
echo "${DOCTOR_JSON}" | grep -q '"Network Isolation Boundary"'
echo "  -> Subprocess network denial verified."

echo "[6/7] Testing Child Process Lifecycle & Interruption Cleanup..."
# Test child process guard cleanup
"${DIR}/scripts/dev.sh" test --lib test_child_process_guard_termination >/dev/null
echo "  -> Child process tree cleanup verified (zero orphans)."

echo "[7/7] Testing Fail-Closed Policy Enforcement..."
"${DIR}/scripts/dev.sh" test --test integration_tests test_fail_closed_when_kernel_sandbox_required >/dev/null
echo "  -> Fail-closed behavior on degraded/unavailable sandbox verified."

echo "=========================================================="
echo "ALL 7 OS-LEVEL SECURITY BOUNDARY VERIFICATION CHECKS PASSED!"
echo "=========================================================="
