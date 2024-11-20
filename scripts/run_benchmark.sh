#!/usr/bin/env bash

START=874000
END=874009
NET=mainnet
LAPS=100

output="block-$START-$END-$NET.jsonl"

echo "Running Native benchmark"
cargo run --release --features benchmark,structured_logging bench-block-range $START $END $NET $LAPS | tee "native-$output"

echo "Running VM benchmark"
cargo run --release --features benchmark,structured_logging,only_cairo_vm bench-block-range $START $END $NET $LAPS | tee "vm-$output"
