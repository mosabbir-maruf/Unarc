# Pinned Rust toolchain for Unarc development and CI
FROM rust:1.85.0-slim

# Install system dependencies needed for engine bundling
RUN apt-get update && apt-get install -y --no-install-recommends \
        curl \
        ca-certificates \
        xz-utils \
    && rm -rf /var/lib/apt/lists/*

# Pinned 7-Zip version: 26.03
# Target architecture: aarch64 (linux-arm64) or x86_64 (linux-x64)
RUN ARCH=$(uname -m) && \
    mkdir -p /opt/unarc/bin && \
    if [ "$ARCH" = "aarch64" ] || [ "$ARCH" = "arm64" ]; then \
        EXPECTED_SHA="2389ba20e4d8295e8709c20b6263b69bd1ec4972fe38a04ad7a1badbf595b996"; \
        TAR_FILE="7z2603-linux-arm64.tar.xz"; \
    elif [ "$ARCH" = "x86_64" ]; then \
        EXPECTED_SHA="dc99eff5008f1ab79bd7084c68513701547a808a89502bf4133683535ab3c695"; \
        TAR_FILE="7z2603-linux-x64.tar.xz"; \
    else \
        echo "Unsupported architecture: $ARCH" && exit 1; \
    fi && \
    curl -sSL "https://github.com/ip7z/7zip/releases/download/26.03/${TAR_FILE}" -o "/tmp/${TAR_FILE}" && \
    echo "${EXPECTED_SHA}  /tmp/${TAR_FILE}" | sha256sum -c - && \
    tar -xJf "/tmp/${TAR_FILE}" -C /opt/unarc/bin 7zz && \
    chmod 0755 /opt/unarc/bin/7zz && \
    rm -f "/tmp/${TAR_FILE}"

# Set environment variable pointing to the bundled engine
ENV UNARC_BUNDLED_7ZZ=/opt/unarc/bin/7zz

# Install required toolchain components
RUN rustup component add rustfmt clippy

# Create workspace directory
WORKDIR /workspace

# Default command
CMD ["cargo", "test"]
