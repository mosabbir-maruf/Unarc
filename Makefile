.PHONY: all fmt fmt-check clippy test build release check docker-build clean help

SHELL := /usr/bin/env bash

help:
	@echo "Unarc Development Automation Commands:"
	@echo "  make check        - Run fmt-check, clippy, tests, and release build check"
	@echo "  make fmt          - Automatically format code inside Docker"
	@echo "  make fmt-check    - Check formatting without modifying files"
	@echo "  make clippy       - Run Clippy lints inside Docker with warnings denied"
	@echo "  make test         - Run all unit and integration tests inside Docker"
	@echo "  make build        - Compile debug binary inside Docker"
	@echo "  make release      - Compile optimized release binary inside Docker"
	@echo "  make docker-build - Rebuild development Docker image"
	@echo "  make clean        - Remove local build artifacts"

all: check

docker-build:
	docker build -t unarc-dev .

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

check:
	./scripts/dev.sh check

clean:
	rm -rf target
