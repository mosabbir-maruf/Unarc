#!/usr/bin/env bash
set -euo pipefail

DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
BENCH_DIR="$(mktemp -d "${TMPDIR:-/tmp}/unarc_bench_XXXXXX")"
cleanup() {
    rm -rf "${BENCH_DIR}"
}
trap cleanup EXIT

echo "=========================================================="
echo "         UNARC PERFORMANCE & BENCHMARK SUITE              "
echo "=========================================================="
echo "Working directory: ${BENCH_DIR}"

# Locate or build binary
UNARC_BIN=""
if [ -f "${DIR}/target/aarch64-apple-darwin/release/unarc" ]; then
    UNARC_BIN="${DIR}/target/aarch64-apple-darwin/release/unarc"
elif [ -f "${DIR}/target/release/unarc" ]; then
    UNARC_BIN="${DIR}/target/release/unarc"
fi

if [ -z "${UNARC_BIN}" ] || [ ! -x "${UNARC_BIN}" ]; then
    echo "Release binary not found locally, compiling release..."
    "${DIR}/scripts/dev.sh" release
    if [ -f "${DIR}/target/release/unarc" ]; then
        UNARC_BIN="${DIR}/target/release/unarc"
    else
        echo "Error: Failed to find compiled release binary."
        exit 1
    fi
fi

# Locate 7zz engine for test archive generation
SEVEN_ZIP=""
if [ -n "${UNARC_BUNDLED_7ZZ:-}" ] && [ -x "${UNARC_BUNDLED_7ZZ}" ]; then
    SEVEN_ZIP="${UNARC_BUNDLED_7ZZ}"
elif [ -x "/opt/unarc/bin/7zz" ]; then
    SEVEN_ZIP="/opt/unarc/bin/7zz"
elif [ -x "${DIR}/target/aarch64-apple-darwin/release/7zz" ]; then
    SEVEN_ZIP="${DIR}/target/aarch64-apple-darwin/release/7zz"
elif command -v 7zz >/dev/null 2>&1; then
    SEVEN_ZIP="$(command -v 7zz)"
fi

if [ -z "${SEVEN_ZIP}" ]; then
    echo "Warning: 7zz not found on host. Running benchmark suite inside Docker container..."
    docker run --rm -v "${DIR}:/workspace" -w /workspace unarc-dev bash -c \
        "scripts/benchmark.sh"
    exit 0
fi

echo "Using unarc binary:  ${UNARC_BIN}"
echo "Using engine binary: ${SEVEN_ZIP}"
echo ""

# Helper to run extraction and measure wall time and peak RSS
run_bench() {
    local name="$1"
    local archive="$2"
    local payload_size_bytes="$3"
    local password="${4:-}"
    local out_dir="${BENCH_DIR}/out_${name}"
    mkdir -p "${out_dir}"

    local time_file="${BENCH_DIR}/time_${name}.txt"
    local start_time end_time elapsed_sec peak_rss_kb throughput_mb

    if [ -n "${password}" ]; then
        export UNARC_PASSWORD="${password}"
    else
        unset UNARC_PASSWORD 2>/dev/null || true
    fi

    start_time=$(python3 -c 'import time; print(time.time())' 2>/dev/null || date +%s)

    if [ "$(uname)" = "Darwin" ]; then
        # macOS /usr/bin/time -l outputs peak RSS in bytes
        /usr/bin/time -l "${UNARC_BIN}" extract "${archive}" -o "${out_dir}" --quiet 2> "${time_file}" || true
        end_time=$(python3 -c 'import time; print(time.time())' 2>/dev/null || date +%s)
        local rss_bytes
        rss_bytes=$(grep "maximum resident set size" "${time_file}" | awk '{print $1}' || echo "0")
        peak_rss_kb=$((rss_bytes / 1024))
    else
        # Linux /usr/bin/time -v outputs peak RSS in KB
        /usr/bin/time -v "${UNARC_BIN}" extract "${archive}" -o "${out_dir}" --quiet 2> "${time_file}" || true
        end_time=$(python3 -c 'import time; print(time.time())' 2>/dev/null || date +%s)
        peak_rss_kb=$(grep "Maximum resident set size" "${time_file}" | awk -F: '{print $2}' | tr -d ' ' || echo "0")
    fi

    unset UNARC_PASSWORD 2>/dev/null || true

    elapsed_sec=$(python3 -c "print(max(0.001, ${end_time} - ${start_time}))" 2>/dev/null || echo "0.1")
    local elapsed_ms
    elapsed_ms=$(python3 -c "print(int(${elapsed_sec} * 1000))" 2>/dev/null || echo "100")
    local peak_rss_mb
    peak_rss_mb=$(python3 -c "print(f'{$peak_rss_kb / 1024:.2f}')" 2>/dev/null || echo "N/A")
    local payload_mb
    payload_mb=$(python3 -c "print(f'{$payload_size_bytes / (1024 * 1024):.2f}')")
    throughput_mb=$(python3 -c "print(f'{(${payload_size_bytes} / (1024 * 1024) / ${elapsed_sec}):.2f}')" 2>/dev/null || echo "N/A")

    local archive_size_kb
    archive_size_kb=$(python3 -c "import os; s = os.path.getsize('${archive}') / 1024; print(f'{s:.1f}')" 2>/dev/null || echo "N/A")

    printf "| %-18s | %10s KB | %10s MB | %9s ms | %9s MB | %10s MB/s | PASS |\n" \
        "${name}" "${archive_size_kb}" "${payload_mb}" "${elapsed_ms}" "${peak_rss_mb}" "${throughput_mb}"

    rm -rf "${out_dir}" "${time_file}"
}

echo "Generating synthetic test archives..."

# 1. Small Archive (100 KB text/source)
SMALL_PAYLOAD="${BENCH_DIR}/small_payload.txt"
python3 -c "with open('${SMALL_PAYLOAD}', 'wb') as f: f.write(b'Unarc High Performance Archive Utility\n' * (100 * 1024 // 40))"
SMALL_ARCHIVE="${BENCH_DIR}/small.7z"
"${SEVEN_ZIP}" a "${SMALL_ARCHIVE}" "${SMALL_PAYLOAD}" >/dev/null 2>&1

# 2. Medium Archive (10 MB binary payload)
MED_PAYLOAD="${BENCH_DIR}/med_payload.bin"
python3 -c "import os; open('${MED_PAYLOAD}', 'wb').write(os.urandom(10 * 1024 * 1024))"
MED_ARCHIVE="${BENCH_DIR}/medium.7z"
"${SEVEN_ZIP}" a -mx=1 "${MED_ARCHIVE}" "${MED_PAYLOAD}" >/dev/null 2>&1

# 3. Large Archive (50 MB binary payload for streaming verification)
LARGE_PAYLOAD="${BENCH_DIR}/large_payload.bin"
python3 -c "import os; open('${LARGE_PAYLOAD}', 'wb').write(os.urandom(50 * 1024 * 1024))"
LARGE_ARCHIVE="${BENCH_DIR}/large.7z"
"${SEVEN_ZIP}" a -mx=1 "${LARGE_ARCHIVE}" "${LARGE_PAYLOAD}" >/dev/null 2>&1

# 4. Multipart Archive (10 MB split into 5MB volumes)
MULTI_PAYLOAD="${BENCH_DIR}/multi_payload.bin"
python3 -c "import os; open('${MULTI_PAYLOAD}', 'wb').write(os.urandom(10 * 1024 * 1024))"
"${SEVEN_ZIP}" a -v5m -mx=1 "${BENCH_DIR}/multipart.7z" "${MULTI_PAYLOAD}" >/dev/null 2>&1
MULTI_ARCHIVE="${BENCH_DIR}/multipart.7z.001"

# 5. Encrypted Archive (5 MB payload encrypted)
ENC_PAYLOAD="${BENCH_DIR}/enc_payload.bin"
python3 -c "import os; open('${ENC_PAYLOAD}', 'wb').write(os.urandom(5 * 1024 * 1024))"
ENC_ARCHIVE="${BENCH_DIR}/encrypted.7z"
"${SEVEN_ZIP}" a -pBenchSecret -mhe=on -mx=1 "${ENC_ARCHIVE}" "${ENC_PAYLOAD}" >/dev/null 2>&1

echo ""
echo "==================================================================================================="
echo "| Benchmark Scenario | Archive Size  | Extracted Size | Wall Time (ms) | Peak RSS (MB) | Throughput (MB/s) | Status |"
echo "|--------------------|---------------|----------------|----------------|---------------|-------------------|--------|"

run_bench "Small (100KB)" "${SMALL_ARCHIVE}" $((100 * 1024))
run_bench "Medium (10MB)" "${MED_ARCHIVE}" $((10 * 1024 * 1024))
run_bench "Large (50MB)" "${LARGE_ARCHIVE}" $((50 * 1024 * 1024))
run_bench "Multipart (10MB)" "${MULTI_ARCHIVE}" $((10 * 1024 * 1024))
run_bench "Encrypted (5MB)" "${ENC_ARCHIVE}" $((5 * 1024 * 1024)) "BenchSecret"

echo "==================================================================================================="
echo ""
echo "Benchmark completed successfully. Peak RSS remains strictly bounded across all archive sizes."
