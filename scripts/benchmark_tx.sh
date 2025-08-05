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

log_output="logs-$TX-$NET-$LAPS.jsonl"
native_log_output="$DATA_DIR/native-$log_output"
vm_log_output="$DATA_DIR/vm-$log_output"

data_output="data-$TX-$NET-$LAPS.json"
native_data_output="$DATA_DIR/native-$data_output"
vm_data_output="$DATA_DIR/vm-$data_output"

find_version() {
    dependency=$(
        cargo metadata --no-deps --format-version 1 |
        jq '
            .packages | map(select(.name=="state-reader"))[0] |
            .dependencies | map(select(.name==$crate))[0]
        ' --arg crate "$1"
    )

    path=$(
        echo "$dependency" |
        jq '.path' --raw-output
    )
    source=$(
        echo "$dependency" |
        jq '.source' --raw-output
    )
    req=$(
        echo "$dependency" |
        jq '.req' --raw-output
    )

    if [ "$path" != "null" ]; then
        # If path is not null, it is a path dependency
        # and we save the version by taking the current git revision
        echo "$(cd "$path" ; git rev-parse HEAD)"
    elif [ "$req" != "*" ]; then
        # If req is not *, it is crate.io dependency and we just return the used version.
        echo "$req"
    elif [[ $source =~ rev=([a-z1-9]+) ]]; then
        # If the source is a git URL, we find the pinned `rev` and return it.
        echo "${BASH_REMATCH[1]}"
    else
        # In the worst case, we just return the entire source.
        echo "$source"
    fi
}

inject_info() {
    input=$1
    mode=$2

    cairo_native_version=$(find_version "cairo-native")
    sequencer_version=$(find_version "blockifier")
    os=$(uname -o)
    arch=$(uname -m)

    case $(uname) in
      Darwin)
          memory=$(sysctl -n hw.memsize)
          cpu=$(sysctl -n machdep.cpu.brand_string)
      ;;
      Linux)
          memory=$(free -b | awk '/Mem:/ {print $2}')
          cpu=$(lscpu | grep "Model name" | sed 's/Model name:\s*//')
      ;;
    esac


    tmp=$(mktemp)
    jq \
        --arg date "$(date)" \
        --arg block_start "$START" \
        --arg block_end "$END" \
        --arg net "$NET" \
        --arg laps "$LAPS" \
        --arg mode "$mode" \
        --arg native_profile "aggressive" \
        --arg rust_profile "release" \
        --arg cairo_native_version "$cairo_native_version" \
        --arg sequencer_version "$sequencer_version" \
        --arg os "$os" \
        --arg arch "$arch" \
        --arg memory "$memory" \
        --arg cpu "$cpu" \
        ' {
            "info": {
                "date": $date,
                "block_start": $block_start,
                "block_end": $block_end,
                "net": $net,
                "laps": $laps,
                "mode": $mode,
                "native_profile": $native_profile,
                "rust_profile": $rust_profile,
                "cairo_native_version": $cairo_native_version,
                "sequencer_version": $sequencer_version,
                "os": $os,
                "arch": $arch,
                "memory": $memory,
                "cpu": $cpu,
            }
        } + .' "$input" > "$tmp"
    mv "$tmp" "$input"
}

echo "Executing with Native"
$NATIVE_TARGET bench-tx "$TX" "$NET" "$BLOCK" "$LAPS" -o "$native_data_output" > "$native_log_output"

inject_info "$native_data_output" "native"

native_time=$(jq '.transactions | map(.time_ns) | add' "$native_data_output")
echo "Average Native time: $native_time ns"

echo "Executing with VM"
$VM_TARGET bench-tx "$TX" "$NET" "$BLOCK" "$LAPS" -o "$vm_data_output" > "$vm_log_output"

inject_info "$vm_data_output" "vm"

vm_time=$(jq '.transactions | map(.time_ns) | add' "$vm_data_output")
echo "Average VM time: $vm_time ns"

speedup=$(bc -l <<< "$vm_time/$native_time")
echo "Native Speedup: $speedup"
