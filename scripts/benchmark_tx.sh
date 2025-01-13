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

log_output="logs-$TX-$NET.jsonl"
native_log_output="native-$log_output"
vm_log_output="vm-$log_output"

data_output="data-$TX-$NET.json"
native_data_output="native-$data_output"
vm_data_output="vm-$data_output"

echo "Executing with Native"
cargo run --release --features benchmark,structured_logging bench-tx "$TX" "$NET" "$BLOCK" "$LAPS" -o "$native_data_output" > "$native_log_output"

native_time=$(tail -n1 "$native_log_output" | jq .fields.average_run_time)
echo "Average Native time: $native_time"

echo "Executing with VM"
cargo run --release --features benchmark,structured_logging,only_cairo_vm bench-tx "$TX" "$NET" "$BLOCK" "$LAPS" -o "$vm_data_output" > "$vm_log_output"

vm_time=$(tail -n1 "$vm_log_output" | jq .fields.average_run_time)
echo "Average VM time: $vm_time"

speedup=$(bc -l <<< "$vm_time/$native_time")
echo "Native Speedup: $speedup"
