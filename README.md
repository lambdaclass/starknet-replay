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
> Compiled contracts are cached to disk at `compiled_programs` directory. This saves time when reexecuting transactions, but can also cause errors if you try to run a contract that was compiled with a different Cairo Native version.
>
> Make sure to remove the directory every time you update the Cairo Native version. Running `make clean` will automatically remove it.

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
- If executing Native, the dumps will be saved at: `state_dumps/native/block{block_number}/{tx_hash}.json`
- If paired with `only_cairo_vm` feature, the dumps will be saved at: `state_dumps/vm/block{block_number}/{tx_hash}.json`

To compare the outputs, you can use the following scripts. Some of them required `delta` (modern diff).
- `cmp_state_dumps.sh`. Prints which transactions match with the VM and which differ.
   ```bash
   > ./scripts/cmp_state_dumps.sh
   diff:  0x636326f93a16be14b36b7e62c546370d81d285d1f5398e13d5348fa03a00d05.json
   match: 0x6902da2a7ef7f7ab2e984c0cdfa94c535dedd7cc081c91f04b9f87a9805411b.json
   diff:  0x75ae71b0aaba9454965d2077d53f056ffd426481bad709831e8d76d50f32dbe.json
   match: 0x7895207d7d46df77f5b0de6b647cd393b9fc7bb18c52b6333c6ea852cf767e.json
   match: 0x2335142d7b7938eeb4512fbf59be7ec2f2284e6533c14baf51460c8de427dc7.json
   match: 0x26f6d10918250f16cddaebb8b69c5cececf9387d4a152f4d9197e1c03c40626.json

   Finished comparison
   - Matching: 4
   - Diffing:  16
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

### Benchmarking

To run benchmarks manually, you must compile with release and the benchmark feature:

```bash
cargo run --release --features benchmark bench-tx 0x04ba569a40a866fd1cbb2f3d3ba37ef68fb91267a4931a377d6acc6e5a854f9a mainnet 648461 1000
cargo run --release --features benchmark bench-block-range 90000 90002 mainnet 1000
```

However, we recommend using the scripts defined `scripts/benchmark_*`, as they are easier to use.

First, make sure to remove the `compiled_programs` directory and build the benchmarking binaries.
```bash
rm -rf compiled_programs
make deps-bench
```

Then, you can benchmark a single transaction by running:
```bash
./scripts/benchmark_tx.sh <tx> <net> <block> <laps>
```

If you want to benchmark a full block, you could run:
```bash
./scripts/benchmark_block.sh <block-start> <block-end> <net> <laps>
```

If you just want to benchmarks a few different sample transactions, run:
```bash
./scripts/benchmark_txs.sh
```

This generates the following files in the `bench_data` directory:
- `{native,vm}-data-*.json` - execution time of each contract call.
- `{native,vm}-data-*.json` - stdout from running the benchmark.

Additionally, the benchmarking scripts also run `plot_execution_time.py`, generating execution plots in the `bench_data` directory:
- `plot-*.svg` - bar plot for the execution time by contract class
- `plot-*-speedup.svg` - violin plot for the speedup by contract class
- `plot-*.csv` - raw csv preprocessed data

## Block Composition
You can check the average of txs, swaps, transfers (the last two in %) inside an average block, separeted by the day of execution. The results
will be saved in a json file inside the floder `block_composition` as a vector of block execution where each of the is entrypoint call tree.

To generate the need information run this command:
`cargo run --release -F block-composition block-compose <block_start> <block_end> <chain>`

## Libfunc Profiling
You can gather information about each libfunc execution in a transaction. To do so, run this command:
`cargo run --release -F with-libfunc-profiling block-range <block_start> <block_end> <chain>`

This will create a `block<number>/<tx_hash>.json` for every transaction executed, containing a list of libfunc profiles for every entrypoint executed. An example of a profile would be:

```json
[
   {
      "block_number": 1126248,
      "tx": "0x26e727eac8352c7ded024010df0db274ae59737a9b5b0f7196ace1e7f1aa848",
      "selector": "0xc844fd57777b0cd7e75c8ea68deec0adf964a6308da7a58de32364b7131cc8",
      "data": [
         {
            "libfunc_idx": 0,
            "samples": 1,
            "total_time": 0,
            "average_time": 0.0,
            "std_deviation": 0.0,
            "quartiles": [
               0,
               0,
               0,
               0,
               0
            ]
         },
      ...
      ],
   }
   ...
]
```

## Plotting

In the `plotting` directory, you can find python scripts to plot relevant information.

To run them, you must first execute the benchmarks to obtain both the execution data and the execution logs.

- `python ./plotting/plot_execution_time.py native-data vm-data`: Plots the execution time of Native vs VM, by contract class.
- `python ./plotting/plot_compilation.py native-logs`: Plots the compilation statistics of Native.
- `python ./plotting/plot_compilation_time_finer.py native-logs`: Native compilation time, with fine-grained stage separation, by contract class.
- `python ./plotting/plot_block_composition.py native-logs`: Average of txs, swaps, transfers inside an average block, separeted by the day of execution.
