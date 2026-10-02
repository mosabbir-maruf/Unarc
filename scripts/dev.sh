#!/usr/bin/env bash
set -euo pipefail

IMAGE_NAME="unarc-dev"
DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

# Ensure Docker image is built
if ! docker image inspect "${IMAGE_NAME}" >/dev/null 2>&1; then
    echo "Building ${IMAGE_NAME} Docker image..."
    docker build -t "${IMAGE_NAME}" "${DIR}"
fi

# Function to run cargo command in Docker
run_cargo() {
    # Check if direct bind-mount is permitted by Docker daemon
    if docker run --rm -v "${DIR}:/workspace" -w /workspace "${IMAGE_NAME}" true >/dev/null 2>&1; then
        docker run --rm \
            -v "${DIR}:/workspace" \
            -w /workspace \
            "${IMAGE_NAME}" \
            cargo "$@"
    else
        # Fallback for environments where host path cannot be bind-mounted (e.g. macOS /Volumes in Docker Desktop)
        CONTAINER_ID="$(docker create -v unarc-cargo-registry:/usr/local/cargo/registry -v unarc-cargo-git:/usr/local/cargo/git -w /workspace "${IMAGE_NAME}" cargo "$@")"
        cleanup() {
            docker rm -f "${CONTAINER_ID}" >/dev/null 2>&1 || true
        }
        trap cleanup EXIT

        # Copy workspace files to container
        docker cp "${DIR}/Cargo.toml" "${CONTAINER_ID}:/workspace/"
        if [ -f "${DIR}/Cargo.lock" ]; then
            docker cp "${DIR}/Cargo.lock" "${CONTAINER_ID}:/workspace/"
        fi
        docker cp "${DIR}/src" "${CONTAINER_ID}:/workspace/"
        if [ -d "${DIR}/tests" ]; then
            docker cp "${DIR}/tests" "${CONTAINER_ID}:/workspace/"
        fi

        # Start container and stream output
        docker start -a "${CONTAINER_ID}"
        EXIT_CODE=$?

        # Copy back generated/updated files (Cargo.lock, formatted src)
        docker cp "${CONTAINER_ID}:/workspace/Cargo.lock" "${DIR}/" >/dev/null 2>&1 || true
        if [ "${1:-}" = "fmt" ]; then
            docker cp "${CONTAINER_ID}:/workspace/src/." "${DIR}/src/" >/dev/null 2>&1 || true
            if [ -d "${DIR}/tests" ]; then
                docker cp "${CONTAINER_ID}:/workspace/tests/." "${DIR}/tests/" >/dev/null 2>&1 || true
            fi
        fi

        trap - EXIT
        cleanup
        return ${EXIT_CODE}
    fi
}

COMMAND="${1:-test}"
shift || true

case "${COMMAND}" in
    fmt)
        run_cargo fmt --all "$@"
        ;;
    fmt-check)
        run_cargo fmt --all -- --check "$@"
        ;;
    clippy)
        run_cargo clippy --all-targets --all-features -- -D warnings "$@"
        ;;
    test)
        run_cargo test --all-targets "$@"
        ;;
    build)
        run_cargo build "$@"
        ;;
    release)
        run_cargo build --release "$@"
        ;;
    check)
        echo "=== [1/4] Checking Formatting ==="
        run_cargo fmt --all -- --check
        echo "=== [2/4] Running Clippy ==="
        run_cargo clippy --all-targets --all-features -- -D warnings
        echo "=== [3/4] Running Tests ==="
        run_cargo test --all-targets
        echo "=== [4/4] Building Release Binary ==="
        run_cargo build --release
        echo "=== All Checks Passed! ==="
        ;;
    cargo)
        run_cargo "$@"
        ;;
    *)
        run_cargo "${COMMAND}" "$@"
        ;;
esac
