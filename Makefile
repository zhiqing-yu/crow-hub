# Crow Hub Makefile
# Provides convenient commands for development

.PHONY: help build test clean install dev fmt lint doc docker

# Default target
help:
	@echo "Crow Hub - Available Commands:"
	@echo ""
	@echo "  make build       - Build all crates"
	@echo "  make build-release - Build release binaries"
	@echo "  make test        - Run all tests"
	@echo "  make dev         - Run in development mode with hot reload"
	@echo "  make run         - Run the TUI"
	@echo "  make fmt         - Format code"
	@echo "  make lint        - Run clippy lints"
	@echo "  make clean       - Clean build artifacts"
	@echo "  make install     - Install locally"
	@echo "  make doc         - Generate documentation"
	@echo "  make docker      - Build Docker image"
	@echo ""
	@echo "Cross-compilation:"
	@echo "  make build-linux-x64"
	@echo "  make build-linux-arm64"
	@echo "  make build-macos-x64"
	@echo "  make build-macos-arm64"
	@echo "  make build-windows-x64"

# Build commands
build:
	cargo build --all

build-release:
	cargo build --release --all

# Test commands
test:
	cargo test --all --verbose
	test-coverage:
	cargo tarpaulin --all --out Html

# Development commands
dev:
	cargo watch -x 'run --bin crow'

run:
	cargo run --bin crow

run-server:
	cargo run --bin crow -- server

# Code quality
fmt:
	cargo fmt --all

lint:
	cargo clippy --all-targets --all-features -- -D warnings

check:
	cargo check --all

# Cleaning
clean:
	cargo clean
	rm -rf target/
	rm -rf dist/

# Installation
install:
	cargo install --path crates/ch-tui

install-local: build-release
	mkdir -p ~/.local/bin
	cp target/release/crow ~/.local/bin/
	@echo "Installed to ~/.local/bin/crow"

# Documentation
doc:
	cargo doc --all --no-deps --open

# Docker
docker:
	docker build -t crow-hub:latest .

docker-run:
	docker run -p 8080:8080 -v $(PWD)/data:/data crow-hub:latest

# Cross-compilation
build-linux-x64:
	cargo build --release --target x86_64-unknown-linux-gnu

build-linux-arm64:
	cargo build --release --target aarch64-unknown-linux-gnu

build-macos-x64:
	cargo build --release --target x86_64-apple-darwin

build-macos-arm64:
	cargo build --release --target aarch64-apple-darwin

build-windows-x64:
	cargo build --release --target x86_64-pc-windows-msvc

# Release packaging
package: build-release
	mkdir -p dist
	cp target/release/crow dist/
	cp README.md LICENSE dist/
	cp -r examples dist/
	tar czf crow-hub-$(shell uname -s)-$(shell uname -m).tar.gz -C dist .

# Setup development environment
setup:
	rustup target add x86_64-unknown-linux-gnu aarch64-unknown-linux-gnu
	rustup target add x86_64-apple-darwin aarch64-apple-darwin
	rustup target add x86_64-pc-windows-msvc
	cargo install cargo-watch cargo-tarpaulin cargo-edit

# Database operations
db-init:
	mkdir -p data
	sqlite3 data/memory.db "VACUUM;"

db-reset:
	rm -f data/memory.db
	make db-init

# Benchmarks
bench:
	cargo bench

# Security audit
audit:
	cargo audit

# Update dependencies
update:
	cargo update

# Full CI pipeline
ci: fmt lint test build
	@echo "✓ All CI checks passed"
