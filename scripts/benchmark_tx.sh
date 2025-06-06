#!/usr/bin/env bash

set -e

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

DIR=$(dirname "$0")
NATIVE_TARGET=$DIR/../target/release/replay-bench-native
VM_TARGET=$DIR/../target/release/replay-bench-vm

if [ ! -x "$NATIVE_TARGET" ] || [ ! -x "$VM_TARGET" ]; then
    echo "benchmark target is missing, please run: make deps-bench"
    exit 1
fi

TX=$1
NET=$2
BLOCK=$3
LAPS=$4

DATA_DIR="bench_data"
mkdir -p $DATA_DIR

log_output="logs-$TX-$NET.jsonl"
native_log_output="$DATA_DIR/native-$log_output"
vm_log_output="$DATA_DIR/vm-$log_output"

data_output="data-$TX-$NET.json"
native_data_output="$DATA_DIR/native-$data_output"
vm_data_output="$DATA_DIR/vm-$data_output"

echo "Executing with Native"
$NATIVE_TARGET bench-tx "$TX" "$NET" "$BLOCK" "$LAPS" -o "$native_data_output" > "$native_log_output"

native_time=$(jq '.transactions | map(.time_ns) | add' "$native_data_output")
echo "Average Native time: $native_time ns"

echo "Executing with VM"
$VM_TARGET bench-tx "$TX" "$NET" "$BLOCK" "$LAPS" -o "$vm_data_output" > "$vm_log_output"

vm_time=$(jq '.transactions | map(.time_ns) | add' "$vm_data_output")
echo "Average VM time: $vm_time ns"

speedup=$(bc -l <<< "$vm_time/$native_time")
echo "Native Speedup: $speedup"
