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
# RPC
export RPC_ENDPOINT_MAINNET=rpc.endpoint.mainnet.com
export RPC_ENDPOINT_TESTNET=rpc.endpoint.testnet.com
```

On macos, you may also need to set the following to avoid linking errors:

```bash
export LIBRARY_PATH=/opt/homebrew/lib
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

## replay
You can use the replay crate to execute transactions or blocks via the CLI. For example:

```bash
* cargo run tx 0x04ba569a40a866fd1cbb2f3d3ba37ef68fb91267a4931a377d6acc6e5a854f9a mainnet 648461
* cargo run block mainnet 648655
* cargo run block-range 90000 90002 mainnet
* cargo run block-txs mainnet 633538 0x021c594980fc2503b2e62a1bb9ce811e7ae22c1478fb0602146745edc9d03bb6 0x13e148692edfbbb4de5d983c6875780e2397e34a432a7355bf2172435ecec0e
```

> [!IMPORTANT]
> Compiled contracts are cached to disk at `./cache/native/` directory. This saves time when reexecuting transactions, but can also cause errors if you try to run a contract that was compiled with a different Cairo Native version.
>
> Make sure to remove the directory every time you update the Cairo Native version. Running `make clean` will automatically remove it.

### RPC Timeout
The RPC time out is handled in two different ways:

- RPC request timeout (in seconds): How many seconds to wait before generating a timeout. By default, the RPC timeout is set to 90 seconds. However, using the env var `RPC_TIMEOUT` this value can be customized.
- RPC request retry: How many times the request is re-sent before failing due to timeout. By default, every RPC request is retried 10 times. However, this limit can be customized by setting the `RPC_RETRY_LIMIT` env var.

By setting both env vars, if any RPC request fails with a timeout, starknet-replay will retry sending the request with a limit of `RPC_RETRY_LIMIT` times awaiting `RPC_TIMEOUT` seconds for the response before generating another timeout.

An exponential-backoff algorithm distributes the retries in time, reducing the amount of simultaneous RPC requests to avoid new timeouts. If the limit of retries is reached, a new request timeout will cease the retrail process and return an timeout error.

### Logging

This projects uses tracing with env-filter, so logging can be modified by the RUST_LOG environment variable. By default, only info events from the replay crate are shown.

As an example, to show only error messages from the replay crate, run:
```bash
RUST_LOG=replay=error cargo run block mainnet 648461
```

### Comparing with VM

To compare Native execution with the VM, you can use the `state_dump` feature. It will save to disk the execution info and state diff of every contract executed.
- If executing Native, the dumps will be saved at: `state_dumps/native/block{block_number}/{tx_hash}.json`
- If paired with `only_cairo_vm` feature, the dumps will be saved at: `state_dumps/vm/block{block_number}/{tx_hash}.json`

To compare the outputs, you can use the following scripts. Some of them required `delta` (modern diff).
- `cmp_state_dumps.py`. Prints which transactions match with the VM and which differ.
   ```bash
   > python3 ./scripts/cmp_state_dumps.py
   Starting comparison with 16 workers
   DIFF 1478358 0xde8db1dc28c7ab48192d9aad1d5c8b08e732738f12b9945f591caa48e4dfa0
   Finished comparison

   MATCH 9
   DIFF 1
   ```
- `delta_state_dumps.sh`. It opens delta to review the differences between VM and Native with each transaction.
   ```bash
   > ./scripts/delta_state_dumps.sh
   ```

### Replaying isolated calls

The replay crate supports executing isolated calls inside of a transaction, although it probably won't work in every scenario.

First, obtain the full state dump of a transaction:

```bash
cargo run --features state_dump -- tx \
   0x01368e23fc6ba5eaf064b9e64f5cddda0c6d565b6f64cb8f036e0d1928a99c79 mainnet 1000000
```

Then, extract the desired call (by its call index). In this case, I will try to re-execute starting from the third call (that is, with index 2).

```bash
./scripts/extract_call.py \
   state_dumps/native/block1000000/0x01368e23fc6ba5eaf064b9e64f5cddda0c6d565b6f64cb8f036e0d1928a99c79.json \
   2 > call.json 
```

Finally, re-execute it with the `call` command.

```bash
cargo run -- call call.json \
   0x01368e23fc6ba5eaf064b9e64f5cddda0c6d565b6f64cb8f036e0d1928a99c79 1000000 mainnet
```

The `state_dump` feature can be used to save the execution result to either
- `call_state_dumps/native/{tx_hash}.json`
- `call_state_dumps/vm/{tx_hash}.json`

## Block Composition
You can check the average of txs, swaps, transfers (the last two in %) inside an average block, separeted by the day of execution. The results
will be saved in a json file inside the floder `block_composition` as a vector of block execution where each of the is entrypoint call tree.

To generate the need information run this command:
`cargo run --release -F block-composition block-compose <block_start> <block_end> <chain>`

## Libfunc Profiling
You can gather information about each libfunc execution in a transaction. To do so, run this command:
`cargo run --release -F with-libfunc-profiling block-range <block_start> <block_end> <chain>`

This will create a `libfunc_profiles/block<number>/<tx_hash>.json` for every transaction executed, containing a list of entrypoints executed. Every entrypoint of that list contains a `profile_summary`, which contains information about the execution of every libfunc. An example of a profile would be:

```json
{
   "block_number": 641561,
   "tx": "0x2e0abd9a260095622f71ff8869aaee0267af1199be78ad5ad91a3c83df0ad08",
   "entrypoints": [
      {
         "class_hash": "0x36078334509b514626504edc9fb252328d1a240e4e948bef8d0c08dff45927f",
         "selector": "0x162da33a4585851fe8d3af3c2a9c60b557814e221e0d4f30ff0b2189d9c7775",
         "profile_summary": [
            {
               "libfunc_name": "struct_construct",
               "samples": 1,
               "total_time": 1,
               "average_time": 1.0,
               "std_deviation": 0.0,
               "quartiles": [
                  1,
                  1,
                  1,
                  1,
                  1
               ]
            },
            ...
         ]
      },
      ...
   ]
}
```

## Plotting

In the `plotting` directory, you can find python scripts to plot relevant information.

- `python ./plotting/plot_block_composition.py native-logs`: Average of txs, swaps, transfers inside an average block, separeted by the day of execution.
