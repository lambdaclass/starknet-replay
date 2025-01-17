.PHONY: usage deps build check test clean

UNAME := $(shell uname)

usage:
	@echo "Usage:"
	@echo "    deps:       Installs the necesarry dependencies."
	@echo "    build:      Builds the crate."
	@echo "    check:      Checks format and lints."
	@echo "    test:       Runs all tests."
	@echo "    clean:      Cleans the built artifacts."

build:
	cargo build --release --features benchmark

check:
	cargo fmt --all -- --check
	cargo clippy --all-targets --all-features -- -D warnings

test:
	cargo test

clean:
	cargo clean
	rm -rf compiled_programs/

deps:
ifeq ($(UNAME), Darwin)
deps: deps-macos
endif
deps:

deps-macos: 
	-brew install llvm@19 --quiet
