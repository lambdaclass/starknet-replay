#!/usr/bin/env bash

usage() {
cat <<EOF
Usage: $0 <start> <end> <net> <laps>

Benches a block range
- Saves output to native-<start>-<end>-<net> and vm-<start>-<end>-<net>
- Prints speedup
EOF
}

if ! [ "$#" -ge "4" ]; then
    usage
    exit 1
fi


START=$1
END=$2
NET=$3
LAPS=$4

log_output="logs-$START-$END-$NET.jsonl"
native_log_output="native-$log_output"
vm_log_output="vm-$log_output"

data_output="data-$START-$END-$NET.jsonl"
native_data_output="native-$data_output"
vm_data_output="vm-$data_output"

echo "Executing with Native"
cargo run --release --features benchmark,structured_logging bench-block-range "$START" "$END" "$NET" "$LAPS" -o "$native_data_output" > "$native_log_output"

native_time=$(tail -n1 "$native_log_output" | jq .fields.average_run_time)
echo "Average Native time: $native_time"

echo "Executing with VM"
cargo run --release --features benchmark,structured_logging,only_cairo_vm bench-block-range "$START" "$END" "$NET" "$LAPS" -o "$vm_data_output" > "$vm_log_output"

vm_time=$(tail -n1 "$vm_log_output" | jq .fields.average_run_time)
echo "Average VM time: $vm_time"

speedup=$(bc -l <<< "$vm_time/$native_time")
echo "Native Speedup: $speedup"
