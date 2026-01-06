#!/usr/bin/env bash

set -euo pipefail

yell() { echo "$0:" "$@" >&2; }

usage() {
cat >&2 <<EOF
Usage: $0 [OPTIONS] <NETWORK> <BLOCK> <N_BLOCKS>

A helper script for benchmarking the execution of a block range. It also
processes the benchmark data and generates a report. The full benchmark result
is saved to a self-contained directory.

Arguments:
  <NETWORK>   Either mainnet or testnet
  <BLOCK>     Starting block
  <N_BLOCKS>  Size of block range

Options:
  -h  Print help
  -n  Number of times to execute the block range
EOF
exit 1
}

RUNS=1
# Parse optional flags.
while getopts "hn:" opt; do
	case $opt in
		h) usage ;;
		n) RUNS=$OPTARG;;
		*) echo >&2; usage ;;
	esac
done
# Skip optional flags from ARGS.
shift $((OPTIND - 1))
# Parse positional arguments.
if [[ $# -lt 3 ]]; then
	yell "expected 3 positional argument\n"
	usage
fi
NETWORK="$1"
START_BLOCK="$2"
RANGE_SIZE="$3"

end_block=$((START_BLOCK + RANGE_SIZE - 1))

case $(uname) in
  Darwin)
		DATE=$(date -zutc +"%FT%TZ")
  ;;
  Linux)
		DATE=$(date --utc +"%FT%TZ")
  ;;
  *) yell "unsupported platform" ;;
esac

BENCHMARK_ROOT="benchmark_data"
BENCHMARK_NAME="execution-$DATE"
BENCHMARK_DIR="$BENCHMARK_ROOT/$BENCHMARK_NAME"

BENCHMARK_NATIVE_TX_DATA_PATH="$BENCHMARK_DIR/native-tx-data.csv"
BENCHMARK_NATIVE_CALL_DATA_PATH="$BENCHMARK_DIR/native-call-data.csv"
BENCHMARK_VM_TX_DATA_PATH="$BENCHMARK_DIR/vm-tx-data.csv"
BENCHMARK_VM_CALL_DATA_PATH="$BENCHMARK_DIR/vm-call-data.csv"

BENCHMARK_INFO_PATH="$BENCHMARK_DIR/info.json"
BENCHMARK_ARTIFACTS_PATH="$BENCHMARK_DIR/artifacts"
BENCHMARK_REPORT_PATH="$BENCHMARK_DIR/report.html"

mkdir -p "$BENCHMARK_DIR"

echo "Benchmarking $NETWORK block range $START_BLOCK-$end_block with Cairo Native"
RUST_LOG="" cargo run --quiet --release --bin replay --features benchmark -- bench-block-range \
	"$START_BLOCK" "$end_block" "$NETWORK" "$RUNS" \
	--tx-data "$BENCHMARK_NATIVE_TX_DATA_PATH" \
	--call-data "$BENCHMARK_NATIVE_CALL_DATA_PATH"
echo "Saved tx benchmark data to $BENCHMARK_NATIVE_TX_DATA_PATH" 
echo "Saved call benchmark data to $BENCHMARK_NATIVE_CALL_DATA_PATH" 

echo "Benchmarking $NETWORK block range $START_BLOCK-$end_block with Cairo VM"
RUST_LOG="" cargo run --quiet --release --bin replay --features benchmark,only_cairo_vm -- bench-block-range \
	"$START_BLOCK" "$end_block" "$NETWORK" "$RUNS" \
	--tx-data "$BENCHMARK_VM_TX_DATA_PATH" \
	--call-data "$BENCHMARK_VM_CALL_DATA_PATH"
echo "Saved tx benchmark data to $BENCHMARK_VM_TX_DATA_PATH" 
echo "Saved call benchmark data to $BENCHMARK_VM_CALL_DATA_PATH" 

echo "Processing tx benchmark data"
python benchmark/plot_tx_execution.py "$BENCHMARK_NATIVE_TX_DATA_PATH" "$BENCHMARK_VM_TX_DATA_PATH" "$BENCHMARK_ARTIFACTS_PATH"
echo "Saved tx benchmark artifacts to $BENCHMARK_ARTIFACTS_PATH"

echo "Processing call benchmark data"
python benchmark/plot_call_execution.py "$BENCHMARK_NATIVE_CALL_DATA_PATH" "$BENCHMARK_VM_CALL_DATA_PATH" "$BENCHMARK_ARTIFACTS_PATH"
echo "Saved call benchmark artifacts to $BENCHMARK_ARTIFACTS_PATH"

echo "Saving benchmark info to $BENCHMARK_INFO_PATH"
python benchmark/gather_info.py | jq \
	--arg block_start "$START_BLOCK" \
	--arg block_end "$end_block" \
	'{
		"Title": "Execution Benchmark",
		"Start Block": $block_start,
		"End Block": $block_end,
		"Native profile": "default"
	} + .' > "$BENCHMARK_INFO_PATH"

echo "Generating report to $BENCHMARK_REPORT_PATH"
python benchmark/generate_report.py "$BENCHMARK_INFO_PATH" \
	"$BENCHMARK_ARTIFACTS_PATH/tx-speedup-distribution.svg" \
	"$BENCHMARK_ARTIFACTS_PATH/contract-class-speedup-distribution.svg" \
	"$BENCHMARK_ARTIFACTS_PATH/edge-contract-classes.csv" \
	"$BENCHMARK_ARTIFACTS_PATH/native-throughput-distribution.svg" \
	"$BENCHMARK_ARTIFACTS_PATH/vm-throughput-distribution.svg" \
	"$BENCHMARK_REPORT_PATH"

echo "Compressing benchmark to" "$BENCHMARK_ROOT/$BENCHMARK_NAME.zip"
(cd "$BENCHMARK_ROOT" && zip -qr "$BENCHMARK_NAME.zip" "$BENCHMARK_NAME")

echo "Saved full benchmark to" "$BENCHMARK_DIR"
