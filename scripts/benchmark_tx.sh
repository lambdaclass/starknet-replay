#!/usr/bin/env bash

usage() {
cat <<EOF
Usage: $0 <tx> <net> <block> <laps>

Benches a single transaction:
- Saves output to native-<tx>-<net> and vm-<tx>-<net>
- Prints speedup
EOF
}

if ! [ "$#" -ge "4" ]; then
    usage
    exit 1
fi

TX=$1
NET=$2
BLOCK=$3
LAPS=$4

echo "Benchmarking $NET $TX"

output="$TX-$NET.jsonl"
native_output="native-$output"
vm_output="vm-$output"

echo "Executing with Native"
cargo run --release --features benchmark,structured_logging bench-tx "$TX" "$NET" "$BLOCK" "$LAPS" > "$native_output" 2>/dev/null

native_time=$(tail -n1 "$native_output" | jq .fields.average_run_time)
echo "Average Native time: $native_time"

echo "Executing with VM"
cargo run --release --features benchmark,structured_logging,only_cairo_vm bench-tx "$TX" "$NET" "$BLOCK" "$LAPS" > "$vm_output" 2>/dev/null

vm_time=$(tail -n1 "$vm_output" | jq .fields.average_run_time)
echo "Average VM time: $vm_time"

speedup=$(bc -le "$vm_time/$native_time")
echo "Native Speedup: $speedup"
