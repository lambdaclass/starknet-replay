#!/usr/bin/env bash
#
# Calls `benchmark_compilation_tx` with a predefined list of transactions

dir=$(dirname "$0")
benchmark_tx_script="$dir/benchmark_compilation_tx.sh"

cases=(
  "0x011e8fa404e60e6d751e25911d8d8270878a6cbc7d62f644e5f0cbd0797e7682 mainnet 1859779"
  "0x076a5a71b10b8f18e7495f6b3d324771a738a3971ce9e95f263c0cc649be5cbe mainnet 1671732"
  "0x04aa88cc2ed4fc470d5fb07179ed27e29520ba9b601801d1edba88c042c7579f mainnet 1383083"
)

for case in "${cases[@]}"; do
  read -r tx net block <<< "$case"

  echo "Benchmarking tx compilation $tx from $net"
  echo
  $benchmark_tx_script "$tx" "$net" "$block"
  echo
done
