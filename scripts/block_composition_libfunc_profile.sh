#!/usr/bin/env bash

set -e

usage() {
cat <<EOF
Usage: $0 <start> <end> <net>

Executes a Range of Blocks and generates Block Composition and Libfunc Profiling data
EOF
}

DIR=$(dirname "$0")
PLOTTING_SCRIPT=$DIR/../plotting/block_composition_plots/plot_syscall_heavy_composition.py
BLOCKS_DIR=$DIR/../block_composition
LIBFUNC_PROFILES_DIR=$DIR/../libfunc_profiles

START=$1
END=$2
NET=$3

if ! [ "$#" -ge "3" ]; then
    usage
    exit 1
fi

# Build replay with block composition
cargo build --release --features block-composition
cp target/release/replay target/release/replay-block-composition

# Build replay with libfunc profiling
cargo build --release --features with-libfunc-profiling
cp target/release/replay target/release/replay-libfunc-profiling

# Create dirs to isolate the executions 
# This is to prevent compiled_programs/ folder to be overwriten 
if [[ -f "run-with-profiling"]]; then
    mkdir run-with-profiling
fi
if [[ -f "run-with-block-composition"]]; then
    mkdir run-with-block-composition
fi

echo "Executing Block composition"

cd run-with-block-composition

../target/release/replay-block-composition -- $START $END $NET

mv block-composition ..

rm -rf run-with-block-composition

echo "Executing Libfunc Profiling"

cd ../run-with-libfunc-profiling

../target/release/replay-libfunc-profiling -- $START $END $NET

mv libfunc-profiles ..

rm -rf run-with-block-composition

cd ..

echo "Generating Plot"

python $PLOTTING_SCRIPT $BLOCKS_DIR $LIBFUNC_PROFILES_DIR
