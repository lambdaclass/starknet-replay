#!/usr/bin/env bash

TX=0x01e06dfbd41e559ee5edd313ab95605331873a5aed09bf1c7312456b7aa2a1c7
BLOCK=291652
LAPS=100
NET=testnet

output="tx-$TX.jsonl"

echo "Running Native benchmark, to 'native-logs'"
cargo run --release --features benchmark,structured_logging bench-tx $TX $NET $BLOCK $LAPS | tee "native-$output"

echo "Running VM benchmark, to 'vm-logs'"
cargo run --release --features benchmark,structured_logging,only_cairo_vm bench-tx $TX $NET $BLOCK $LAPS | tee "vm-$output"
