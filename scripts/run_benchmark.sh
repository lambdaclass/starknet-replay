#!/usr/bin/env bash

START=874000
END=874009
LAPS=100

echo "Running Native benchmark, to 'native-logs'"
cargo run --release --features benchmark,structured_logging bench-block-range $START $END mainnet $LAPS > native-logs

echo "Running VM benchmark, to 'vm-logs'"
cargo run --release --features benchmark,structured_logging,only_cairo_vm bench-block-range $START $END mainnet $LAPS > vm-logs
