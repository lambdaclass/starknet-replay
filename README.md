# starknet-replay
Provides a way of reading a real Starknet State, so you can re-execute an existing transaction in any of the Starknet networks in an easy way

## Getting Started

### Prerequisites

- Linux or macOS (aarch64 included) only for now
- LLVM 19 with MLIR
- Rust 1.78.0 or later, since cairo-native makes use of the u128 abi change.
- Git

### Setup

Run the following make target to install dependencies:
```bash
make deps
```
It will automatically install LLVM 19 with MLIR on macos, if you are using linux you must do it manually. On debian, you can use `apt.llvm.org`, or build it from source.

This project is integrated with Cairo Native, see [Cairo Native Setup](#cairo-native-setup) to set it up correctly

Some environment variable are needed, you can automatically set them by sourcing `env.sh`. If the script doesn't adjust to your specific environment you can `cp` it into `.env` or `.envrc` and modify it.
```bash
# Cairo Native
export LLVM_SYS_191_PREFIX=/path/to/llvm-19
export MLIR_SYS_190_PREFIX=/path/to/llvm-19
export TABLEGEN_190_PREFIX=/path/to/llvm-19
export CAIRO_NATIVE_RUNTIME_LIBRARY=/path/to/cairo_native/target/release/libcairo_native_runtime.a
# RPC
export RPC_ENDPOINT_MAINNET=rpc.endpoint.mainnet.com
export RPC_ENDPOINT_TESTNET=rpc.endpoint.testnet.com
```

Once you have installed dependencies and set the needed environment variables, you can build the project and run the tests:
```bash
make build
make test
```

### Cairo Native Setup

Starknet Replay is currenlty integrated with [Cairo Native](https://github.com/lambdaclass/cairo_native), which makes the execution of sierra programs possible through native machine code. To use it, the following needs to be setup:

- On mac with brew, running `make deps` should have installed LLVM 19 with MLIR, otherwise, you must install it manually. On Debian, you can use `apt.llvm.org`, or build it from source.

- The `LLVM_SYS_191_PREFIX`, `MLIR_SYS_190_PREFIX` and `TABLEGEN_190_PREFIX` environment variable needs to point to said installation. In macOS, run:
  ```
  export LLVM_SYS_190_PREFIX=/opt/homebrew/opt/llvm@19
  export MLIR_SYS_191_PREFIX=/opt/homebrew/opt/llvm@19
  export TABLEGEN_190_PREFIX=/opt/homebrew/opt/llvm@19
  ```
  and you're set.

Afterwards, compiling with the feature flag `cairo-native` will enable native execution. You can check out some example test code that uses it under `tests/cairo_native.rs`.

#### Using ahead of time compilation with Native.

Currently cairo-native with AOT needs a runtime library in a known place. For this you need to compile the [cairo-native-runtime](https://github.com/lambdaclass/cairo_native/tree/main/runtime) crate and point the following environment variable to a folder containing the dynamic library. The path **must** be an absolute path.

```bash
CAIRO_NATIVE_RUNTIME_LIBRARY=/absolute/path/to/cairo-native/target/release/libcairo_native_runtime.a
```

If you don't do this you will get a linker error when using AOT.

## replay
You can use the replay crate to execute transactions or blocks via the CLI. For example:

```bash
* cargo run tx 0x04ba569a40a866fd1cbb2f3d3ba37ef68fb91267a4931a377d6acc6e5a854f9a mainnet 648461
* cargo run block mainnet 648655
* cargo run block-range 90000 90002 mainnet
```

### Benchmarks

To run benchmarks with the replay crate, you can use either `bench-block-range` or `bench-tx` commands. These make sure to cache all needed information (including cairo native compilation) before the actual execution. To use it you must compile the binary under the benchmark flag.

```bash
* cargo run --features benchmark bench-tx 0x04ba569a40a866fd1cbb2f3d3ba37ef68fb91267a4931a377d6acc6e5a854f9a mainnet 648461 1
* cargo run --features benchmark bench-block-range 90000 90002 mainnet 1
```

These commands are like `tx` and `block-range` commands, but with the number of runs to execute as their last argument.

### Logging

This projects uses tracing with env-filter, so logging can be modified by the RUST_LOG environment variable. By default, only info events from the replay crate are shown.

As an example, to show only error messages from the replay crate, run:
```bash
RUST_LOG=replay=error cargo run block mainnet 648461
```

### Plotting

In the `plotting` directory, you can find python scripts to plot relevant information. Before using them, you must first execute the replay with the `structured_logging` feature, and redirect the output to a file. You should do it with both Native execution and VM execution.

Make sure to erase the `compiled_programs` directory, then run:

```bash
cargo run --features structured_logging block mainnet 724000 | tee native-logs
cargo run --features structured_logging,only_cairo_vm block mainnet 724000 | tee vm-logs
```

Once you have done this, you can use the plotting scripts:

- `python ./plotting/plot_compilation_memory.py native-logs`: Plots the size of the compiled native libraries, by contract class.
- `python ./plotting/plot_compilation_memory_corr.py native-logs vm-logs`: Plots the relation between the compiled native libraries and casm size.
- `python ./plotting/plot_compilation_memory_trend.py native-logs vm-logs`: Plots the relation between the compiled native libraries and casm size, with the original sierra size.
- `python ./plotting/plot_compilation_time.py native-logs`: Plots the time it takes to compile each contract class.
- `python ./plotting/plot_compilation_time_trend.py native-logs vm-logs`: Plots the relation between the time takes to compile native libraries and casm contracts, with the original sierra size.
- `python ./plotting/plot_execution_time.py native-logs vm-logs`: Plots the execution time of each contract class, and compares it with the VM. This is best used with the benchmark feature, as it ignores compilation and RPC calls.
- `python ./plotting/plot_compilation_time_finer.py native-logs`: Plots the time it takes to compile each contract class, at each step. It requires a specific [Cairo Native branch](https://github.com/lambdaclass/cairo_native/tree/time-compilation) (as it need finer logging)

