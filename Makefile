# Makefile for stickmap — cross-build to Windows x86_64 from WSL.

TARGET   = x86_64-pc-windows-gnu
BIN      = target/$(TARGET)/release/stickmap.exe

.PHONY: build-windows selftest-windows test clippy fmt

build-windows:
	cargo build --release --target $(TARGET)

selftest-windows: build-windows
	./$(BIN) --selftest

test:
	cargo test

clippy:
	cargo clippy --target $(TARGET)

fmt:
	cargo fmt