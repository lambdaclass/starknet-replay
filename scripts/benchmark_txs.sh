#!/usr/bin/env bash
#
# Calls `benchmark_tx` with a predefined list of transactions

dir=$(dirname "$0")
benchmark_tx_script="$dir/benchmark_tx.sh"

cat <<EOF | xargs -n 4 "$benchmark_tx_script"
0x01e06dfbd41e559ee5edd313ab95605331873a5aed09bf1c7312456b7aa2a1c7 testnet 291652 10000
0x043f7fc80de5e17f599d3d4de951778828adedc83a59c27c0bc379b2aed60b08 testnet 291712 10000
0x02ea16cfbfe93de3b0114a8a04b3cf79ed431a41be29aa16024582de6017f1dd mainnet 874004 10000
EOF
