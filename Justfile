# Project Picker - Justfile

# Build release binary and install to cargo bin
install:
    cargo install --path .

# Build debug binary
build:
    cargo build

# Build release binary
build-release:
    cargo build --release

# Run clippy lints
lint:
    cargo clippy -- -D warnings

# Format code
fmt:
    cargo fmt

# Check formatting without modifying
fmt-check:
    cargo fmt -- --check

# Run all checks (lint + format check + build)
check: lint fmt-check build

# Build release and install (use after every code change)
reinstall: lint fmt-check install
