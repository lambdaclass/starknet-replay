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

rm -rf ./cache/native
cargo run --release --bin replay --features with-comp-stats block-range "$START" "$END" "$NET"

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
	}' --slurp ./cache/native/*.stats.json > "$OUTPUT_PATH"
