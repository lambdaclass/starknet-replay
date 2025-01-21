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

NATIVE_TARGET=target/release/replay-bench-native
VM_TARGET=target/release/replay-bench-vm

if [ ! -x $NATIVE_TARGET ] || [ ! -x $VM_TARGET ]; then
    echo "benchmark target is missing, please run: make deps-bench"
    exit 1
fi

START=$1
END=$2
NET=$3
LAPS=$4

DATA_DIR="bench_data"
mkdir -p $DATA_DIR

log_output="logs-$START-$END-$NET.jsonl"
native_log_output="$DATA_DIR/native-$log_output"
vm_log_output="$DATA_DIR/vm-$log_output"

data_output="data-$START-$END-$NET.json"
native_data_output="$DATA_DIR/native-$data_output"
vm_data_output="$DATA_DIR/vm-$data_output"


echo "Benchmarking $LAPS times $NET blocks from $START to $END"
echo

echo "Executing with Native"
$NATIVE_TARGET bench-block-range "$START" "$END" "$NET" "$LAPS" -o "$native_data_output" > "$native_log_output"

native_time_secs=$(jq .average_time.secs "$native_data_output")
native_time_nanos=$(jq .average_time.nanos "$native_data_output")
native_time=$(bc -l <<< "$native_time_secs * 1e9 + $native_time_nanos")
echo "Average Native time: $native_time ns"
echo

echo "Executing with VM"
$VM_TARGET bench-block-range "$START" "$END" "$NET" "$LAPS" -o "$vm_data_output" > "$vm_log_output"

vm_time_secs=$(jq .average_time.secs "$vm_data_output")
vm_time_nanos=$(jq .average_time.nanos "$vm_data_output")
vm_time=$(bc -l <<< "$vm_time_secs * 1e9 + $vm_time_nanos")
echo "Average VM time: $vm_time ns"
echo

speedup=$(bc -l <<< "$vm_time/$native_time")
echo "Native Speedup: $speedup"
