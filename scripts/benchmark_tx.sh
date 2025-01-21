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
PLOTTING_SCRIPT=$DIR/../plotting/plot_execution_time.py

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

plotting_output="$DATA_DIR/plot-$TX-$NET"

echo "Benchmarking $LAPS times $NET transaction $TX"
echo

echo "Executing with Native"
$NATIVE_TARGET bench-tx "$TX" "$NET" "$BLOCK" "$LAPS" -o "$native_data_output" > "$native_log_output"

native_time_secs=$(jq .average_time.secs "$native_data_output")
native_time_nanos=$(jq .average_time.nanos "$native_data_output")
native_time=$(bc -l <<< "$native_time_secs * 1000000000 + $native_time_nanos")
echo "Average Native time: $native_time ns"
echo

echo "Executing with VM"
$VM_TARGET bench-tx "$TX" "$NET" "$BLOCK" "$LAPS" -o "$vm_data_output" > "$vm_log_output"

vm_time_secs=$(jq .average_time.secs "$vm_data_output")
vm_time_nanos=$(jq .average_time.nanos "$vm_data_output")
vm_time=$(bc -l <<< "$vm_time_secs * 1000000000 + $vm_time_nanos")
echo "Average VM time: $vm_time ns"
echo

speedup=$(bc -l <<< "$vm_time/$native_time")
echo "Native Speedup: $speedup"

python "$PLOTTING_SCRIPT" "$native_data_output" "$vm_data_output" --speedup --output "$plotting_output"
