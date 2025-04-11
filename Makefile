.PHONY: usage deps build check test clean

UNAME := $(shell uname)

usage:
	@echo "Usage:"
	@echo "    deps:       Installs the necesarry dependencies."
	@echo "    build:      Builds the crate."
	@echo "    check:      Checks format and lints."
	@echo "    test:       Runs all tests."
	@echo "    clean:      Cleans the built artifacts."
	@echo "    corelib:    Downloads development corelib."

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

deps-bench:
	cargo build --release --features benchmark,profiling,structured_logging
	cp target/release/replay target/release/replay-bench-native
	cargo build --release --features benchmark,profiling,structured_logging,only_cairo_vm
	cp target/release/replay target/release/replay-bench-vm

CAIRO_2_VERSION := v2.12.0-dev.0
CAIRO_2_TAR := cairo-${CAIRO_2_VERSION}-macos.tar

# ej: make cairo-v2.0.0-macos.tar
cairo-%-macos.tar:
	curl -L -o "$@" "https://github.com/starkware-libs/cairo/releases/download/$*/release-aarch64-apple-darwin.tar"


cairo2: ${CAIRO_2_TAR}
	rm -rf cairo2 \
	&& tar -mxzvf $< \
	&& mv cairo/ $@
.PHONY: cairo2

corelib: cairo2
	ln -s cairo2/corelib corelib
