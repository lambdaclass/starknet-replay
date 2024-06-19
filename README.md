# starknet-replay
Provides a way of reading a real Starknet State, so you can re-execute an existing transaction in any of the Starknet networks in an easy way

### Cairo Native Setup

Starknet Replay is currenlty integrated with [Cairo Native](https://github.com/lambdaclass/cairo_native), which makes the execution of sierra programs possible through native machine code. To use it, the following needs to be setup:

- LLVM `18` needs to be installed and the `MLIR_SYS_180_PREFIX` and `TABLEGEN_180_PREFIX` environment variable needs to point to said installation. In macOS, run
  ```
  brew install llvm@18
  export MLIR_SYS_180_PREFIX=/opt/homebrew/opt/llvm@18
  export LLVM_SYS_180_PREFIX=/opt/homebrew/opt/llvm@18
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
