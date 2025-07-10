#!/usr/bin/env bash

set -e

usage() {
cat <<EOF
Usage: $0 <start> <end> <net>
EOF
}


if ! [ "$#" -ge "3" ]; then
    usage
    exit 1
fi


START=$1
END=$2
NET=$3

DATA_DIR="bench_data"
OUTPUT_PATH="$DATA_DIR/compilation-$START-$END-$NET.json"
mkdir -p $DATA_DIR

rm -rf ./compiled_programs/
cargo run --release --bin replay --features with-comp-stats block-range "$START" "$END" "$NET"

find_version() {
    cargo metadata --no-deps --format-version 1 \
    | jq '
        .packages | map(select(.name=="rpc-state-reader"))[0] |
        .dependencies | map(select(.name==$crate))[0] |
        .source
    ' --raw-output --arg crate "$1"
}

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

cairo_native_version=$(find_version "cairo-native")
os=$(uname -o)
arch=$(uname -m)

shift
jq \
	--arg date "$(date)" \
	--arg block_start "$START" \
	--arg block_end "$END" \
	--arg net "$NET" \
	--arg native_profile "aggressive" \
	--arg rust_profile "release" \
	--arg cairo_native_version "$cairo_native_version" \
	--arg os "$os" \
	--arg arch "$arch" \
	--arg memory "$memory" \
	--arg cpu "$cpu" \
	'{
		"info": {
	    "date": $date,
	    "block_start": $block_start,
	    "block_end": $block_end,
	    "net": $net,
	    "native_profile": $native_profile,
	    "rust_profile": $rust_profile,
	    "cairo_native_version": $cairo_native_version,
	    "os": $os,
	    "arch": $arch,
	    "memory": $memory,
	    "cpu": $cpu,
		},
		"classes": .
	}' --slurp ./compiled_programs/*.stats.json > "$OUTPUT_PATH" 
