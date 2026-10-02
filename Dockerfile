# Pinned Rust toolchain for Unarc development and CI
FROM rust:1.85.0-slim

# Install required toolchain components
RUN rustup component add rustfmt clippy

# Create workspace directory
WORKDIR /workspace

# Default command
CMD ["cargo", "test"]
