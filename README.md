# starknet-replay
Provides a way of reading a real Starknet State, so you can re-execute an existing transaction in any of the Starknet networks in an easy way

## Getting Started

### Prerequisites

- Linux or macOS (aarch64 included) only for now
- LLVM 18 with MLIR
- Rust 1.78.0 or later, since cairo-native makes use of the u128 abi change.
- Git

### Setup

Run the following make target to install dependencies:
```bash
make deps
```
It will automatically install LLVM 18 with MLIR on macos, if you are using linux you must do it manually. On debian, you can use `apt.llvm.org`, or build it from source.

This project is integrated with Cairo Native, see [Cairo Native Setup](#cairo-native-setup) to set it up correctly

Some environment variable are needed, you can automatically set them by sourcing `env.sh`. If the script doesn't adjust to your specific environment you can `cp` it into `.env` or `.envrc` and modify it.
```bash
# Cairo Native
export LLVM_SYS_181_PREFIX=/path/to/llvm-18
export MLIR_SYS_180_PREFIX=/path/to/llvm-18
export TABLEGEN_180_PREFIX=/path/to/llvm-18
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

- On mac with brew, running `make deps` should have installed LLVM 18 with MLIR, otherwise, you must install it manually. On Debian, you can use `apt.llvm.org`, or build it from source.

- The `LLVM_SYS_181_PREFIX`, `MLIR_SYS_180_PREFIX` and `TABLEGEN_180_PREFIX` environment variable needs to point to said installation. In macOS, run:
  ```
  export LLVM_SYS_180_PREFIX=/opt/homebrew/opt/llvm@18
  export MLIR_SYS_181_PREFIX=/opt/homebrew/opt/llvm@18
  export TABLEGEN_180_PREFIX=/opt/homebrew/opt/llvm@18
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

### Comparing with VM

To compare Native execution with the VM, you can use the `state_dump` feature. It will save to disk the execution info and state diff of every contract executed.
- If executing Native, the dumps will be saved at: `state_dump/native/{tx_hash}.json`
- If paired with `only_cairo_vm` feature, the dumps will be saved at: `state_dump/vm/{tx_hash}.json`

To compare the outputs, you can use the following scripts. Some of them required `delta` (modern diff).
- `cmp_state_dump.sh`. Prints which transactions match with the VM and which differ. It moves matching transaction to another directory `good_state_dump`.
   ```bash
   > ./scripts/cmp_state_dumps.sh
   match at 0x2216397e9d6e435b8e6e42550fc1d03906fa54efaf0fec5433df820c0abf03e.json
   diff at 0x227cfe3edc87827dfa799ab11ac405629579df05ff53ccf16559e53abbf929.json
   match at 0x235a801cdc27ed6c8831d3dfd858a3639a11c3aacba8a7c6a0e624bdbdd90c8.json
   diff at 0x2396235a8e8d31d2ec5f2d156557cea89e5c338733ba2a479f81df4d68c5e14.json
   diff at 0x26564061cbc9ba9bb7bed562f5018dd6554fbf6434d8f2a41127a472c1b4553.json
   match at 0x26f6d10918250f16cddaebb8b69c5cececf9387d4a152f4d9197e1c03c40626.json
   ```
- `delta_state_dump.sh`. It opens delta to review the differences between VM and Native with each transaction.
   ```bash
   > ./scripts/delta_state_dumps.sh
   ```
- `delta_state_dump.sh`. It opens delta to review the differences between VM and Native with a single transaction.
   ```bash
   > ./scripts/delta_state_dump.sh 0x227cfe3edc87827dfa799ab11ac405629579df05ff53ccf16559e53abbf929.json
   ```
