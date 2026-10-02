#!/usr/bin/env bash
set -euo pipefail

DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

# Ensure macos SDK tbd stubs are available
if [ ! -f /tmp/macos_usr_lib_tbd.tar.gz ]; then
    if [ -d /Library/Developer/CommandLineTools/SDKs/MacOSX.sdk/usr/lib ]; then
        tar -czf /tmp/macos_usr_lib_tbd.tar.gz -C /Library/Developer/CommandLineTools/SDKs/MacOSX.sdk usr/lib
    elif [ -d /Applications/Xcode.app/Contents/Developer/Platforms/MacOSX.platform/Developer/SDKs/MacOSX.sdk/usr/lib ]; then
        tar -czf /tmp/macos_usr_lib_tbd.tar.gz -C /Applications/Xcode.app/Contents/Developer/Platforms/MacOSX.platform/Developer/SDKs/MacOSX.sdk usr/lib
    else
        echo "Error: macOS SDK usr/lib not found to generate tbd bundle" >&2
        exit 1
    fi
fi

CONTAINER_ID="$(docker create -v unarc-cargo-registry:/usr/local/cargo/registry -v unarc-cargo-git:/usr/local/cargo/git -w /workspace unarc-dev bash -c '
set -e
if [ ! -f /opt/zig/zig ]; then
    mkdir -p /opt/zig
    curl -sSL https://ziglang.org/download/0.13.0/zig-linux-aarch64-0.13.0.tar.xz | tar -xJ --strip-components=1 -C /opt/zig
fi
mkdir -p /opt/macos-sdk
tar -xzf /tmp/macos_usr_lib_tbd.tar.gz -C /opt/macos-sdk
rustup target add aarch64-apple-darwin
cat << "EOF" > /usr/local/bin/zig-darwin-linker
#!/bin/sh
exec /opt/zig/zig cc -target aarch64-macos -L /opt/macos-sdk/usr/lib "$@"
EOF
chmod +x /usr/local/bin/zig-darwin-linker
RUSTFLAGS="-C linker=zig-darwin-linker" cargo build --target aarch64-apple-darwin --release
')"

docker cp /tmp/macos_usr_lib_tbd.tar.gz "${CONTAINER_ID}:/tmp/"
docker cp "${DIR}/Cargo.toml" "${CONTAINER_ID}:/workspace/"
docker cp "${DIR}/Cargo.lock" "${CONTAINER_ID}:/workspace/"
docker cp "${DIR}/src" "${CONTAINER_ID}:/workspace/"
docker cp "${DIR}/tests" "${CONTAINER_ID}:/workspace/"

docker start -a "${CONTAINER_ID}"
mkdir -p "${DIR}/target/aarch64-apple-darwin/release"
docker cp "${CONTAINER_ID}:/workspace/target/aarch64-apple-darwin/release/unarc" "${DIR}/target/aarch64-apple-darwin/release/"
docker rm -f "${CONTAINER_ID}" >/dev/null 2>&1 || true
chmod +x "${DIR}/target/aarch64-apple-darwin/release/unarc"
file "${DIR}/target/aarch64-apple-darwin/release/unarc"
