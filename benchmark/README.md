# Benchmarking Cairo Native

This crate supports benchmarking both execution and compilation of Starknet contract classes.

## Commands

Here we only provide an overview of the existing commands. For more information refer to the `--help` option.

### Benchmarking a Block Range

For benchmarking a full block range, you can use the `bench-block-range` command:

```bash
replay bench-block-range 90000 90009 mainnet 10 \
  --tx-data tx_data.csv --call-data call_data.csv
```

### Benchmarking a Single Transaction

For benchmarking a single transaction, you can use the `bench-tx` command:

```bash
replay bench-tx 0x04ba569a40a866fd1cbb2f3d3ba37ef68fb91267a4931a377d6acc6e5a854f9a \
  mainnet 648461 10 \
  --tx-data tx_data.csv --call-data call_data.csv
```

### Benchmarking Compilation

For benchmarking contract class compilation, you can use the `bench-classes` command. First create a file with the following content:

```
mainnet 0x00009e6d3abd4b649e6de59bf412ab99bc9609414bbe7ba86b83e09e96dcb120
mainnet 0x0002a2838ed37071ced0a289a9bf87926c76b9da1973b5a2ecb5e487bef48b2b
mainnet 0x0005fd5fddec073a363f000b46a4639c1f8df79416778d315462d84072567461
...
```

Then, you can benchmark the compilation of those classes by running:

```bash
replay bench-classes classes.txt --output compilation.csv --runs 10
```

## Scripts

There are also multiple scripts for automating the benchmarking process, processing the raw data.

## Benchmarking Compilation

Create a file with the following content:

```
mainnet 0x00009e6d3abd4b649e6de59bf412ab99bc9609414bbe7ba86b83e09e96dcb120
mainnet 0x0002a2838ed37071ced0a289a9bf87926c76b9da1973b5a2ecb5e487bef48b2b
mainnet 0x0005fd5fddec073a363f000b46a4639c1f8df79416778d315462d84072567461
...
```

Then run:

```bash
bash benchmark/benchmark_classes.sh classes.txt
```

The finished benchmark looks like this:

```
compilation-2025-09-30T18:14:49Z/
├── artifacts                    # Contains benchmark artifacts
│   └── ...
├── classes.txt                  # Contains the classes that were benchmarked
├── data.csv                     # Contains the raw benchmark data
├── info.json                    # Contains the benchmark information
└── report.html                  # Contains the benchmark report
```

The directory is self-contained and can be zipped and sent to someone else. You can open the `report.html` with any browser.

## Benchmarking a Block Range

The following command will benchmark the mainnet block range 1000000-1000010, 10 times.

```bash
bash benchmark/benchmark_block_range.sh -n 10 mainnet 1000000 10
```

The finished benchmark looks like this:

```
execution-2025-10-23T14:33:04Z/
├── artifacts.                       # Contains benchmark artifacts
│   └── ...
├── native-call-data.csv             # Raw Native call bench data
├── native-tx-data.csv               # Raw Native tx bench data
├── vm-call-data.csv                 # Raw VM call bench data
├── vm-tx-data.csv                   # Raw VM tx bench data
├── info.json                        # Contains the benchmark information
└── report.html                      # Contains the benchmark report 
```

The directory is self-contained and can be zipped and sent to someone else. You can open the `report.html` with any browser.

## Benchmarking Standalone Transactions

Create a file with the following content:

```
testnet 291652 0x01e06dfbd41e559ee5edd313ab95605331873a5aed09bf1c7312456b7aa2a1c7
testnet 291712 0x043f7fc80de5e17f599d3d4de951778828adedc83a59c27c0bc379b2aed60b08
mainnet 874004 0x02ea16cfbfe93de3b0114a8a04b3cf79ed431a41be29aa16024582de6017f1dd
```

Then run:

```bash
bash benchmark/benchmark_txs.sh -n 100 txs.txt
```

All the benchmark data will be saved to a self-contained directory and can be zipped and sent to someone else. At the end, it will also output a benchmark summary:

```
tx_hash                                                            speedup
0x1e06dfbd41e559ee5edd313ab95605331873a5aed09bf1c7312456b7aa2a1c7  5.515991020776905
0x43f7fc80de5e17f599d3d4de951778828adedc83a59c27c0bc379b2aed60b08  9.313852257007627
0x2ea16cfbfe93de3b0114a8a04b3cf79ed431a41be29aa16024582de6017f1dd  14.088840391723119
````
