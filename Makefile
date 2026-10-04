.PHONY: all fmt fmt-check clippy test build release run check docker-build clean help

SHELL := /usr/bin/env bash

help:
	@echo "Unarc Development Automation Commands:"
	@echo "  make check        - Run fmt-check, clippy, tests, and release build check"
	@echo "  make run          - Run Unarc interactive TUI or binary directly"
	@echo "  make fmt          - Automatically format code inside Docker"
	@echo "  make fmt-check    - Check formatting without modifying files"
	@echo "  make clippy       - Run Clippy lints inside Docker with warnings denied"
	@echo "  make test         - Run all unit and integration tests inside Docker"
	@echo "  make build        - Compile debug binary inside Docker"
	@echo "  make release      - Compile optimized release binary inside Docker"
	@echo "  make docker-build - Rebuild development Docker image"
	@echo "  make docker-prod  - Build hardened distroless production runtime Docker image"
	@echo "  make clean        - Remove local build artifacts"

all: check

docker-build:
	docker build --target dev -t unarc-dev .

docker-prod:
	docker build -t unarc:latest .

fmt:
	./scripts/dev.sh fmt

fmt-check:
	./scripts/dev.sh fmt-check

clippy:
	./scripts/dev.sh clippy

test:
	./scripts/dev.sh test

build:
	./scripts/dev.sh build

release:
	./scripts/dev.sh release

run:
	@if [ -x "./target/aarch64-apple-darwin/release/unarc" ]; then \
		./target/aarch64-apple-darwin/release/unarc $(ARGS); \
	elif [ -x "./target/release/unarc" ]; then \
		./target/release/unarc $(ARGS); \
	elif command -v cargo >/dev/null 2>&1; then \
		cargo run --release -- $(ARGS); \
	else \
		./scripts/dev.sh cargo run --release -- $(ARGS); \
	fi

bench:
	./scripts/benchmark.sh

check:
	./scripts/dev.sh check

clean:
	rm -rf target
