#!/usr/bin/env bash

set -euo pipefail

yell() { echo "$0:" "$@" >&2; }

usage() {
cat >&2 <<EOF
Usage: $0 [OPTIONS] <TXS>

A helper script for benchmarking the execution of multiple standalone
transactions. It also processes the benchmark data and generates a report. The
full benchmark result is saved to a self-contained directory.

Each line from the input line should contain three whitespace separated values:
- Network, either mainnet or testnet.
- Transaction Hash, in hexadecimal form.
- Block number.

Argument:
  <TXS>  Path to read input transactions from

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
if [[ $# -lt 1 ]]; then
	yell "expected 1 positional argument\n"
	usage
fi
TXS="$1"

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
BENCHMARK_NAME="execution-tx-$DATE"
BENCHMARK_DIR="$BENCHMARK_ROOT/$BENCHMARK_NAME"

BENCHMARK_NATIVE_TX_DATA_PATH="$BENCHMARK_DIR/native-tx-data.csv"
BENCHMARK_VM_TX_DATA_PATH="$BENCHMARK_DIR/vm-tx-data.csv"

BENCHMARK_INFO_PATH="$BENCHMARK_DIR/info.json"
BENCHMARK_ARTIFACTS_PATH="$BENCHMARK_DIR/artifacts"

mkdir -p "$BENCHMARK_DIR"

tmp_tx_bench_data=$(mktemp)

echo "Benchmarking with Cairo Native"
header=true
while read -r network block tx; do
	echo "- Transaction $tx"
	RUST_LOG="" cargo run --quiet --release --bin replay --features benchmark -- bench-tx \
		"$tx" "$network" "$block" "$RUNS" \
		--tx-data "$tmp_tx_bench_data"

		if [ $header == true ]; then
			cp "$tmp_tx_bench_data" "$BENCHMARK_NATIVE_TX_DATA_PATH"
		else
			tail -n +2 "$tmp_tx_bench_data" >> "$BENCHMARK_NATIVE_TX_DATA_PATH"
		fi

		header=false
done < "$TXS"

echo "Saved tx benchmark data to $BENCHMARK_NATIVE_TX_DATA_PATH" 
echo

echo "Benchmarking with Cairo VM"
header=true
while read -r network block tx; do
	echo "- Transaction $tx"
	RUST_LOG="" cargo run --quiet --release --bin replay --features benchmark,only_cairo_vm -- bench-tx \
		"$tx" "$network" "$block" "$RUNS" \
		--tx-data "$tmp_tx_bench_data"

		if [ $header == true ]; then
			cp "$tmp_tx_bench_data" "$BENCHMARK_VM_TX_DATA_PATH"
		else
			tail -n +2 "$tmp_tx_bench_data" >> "$BENCHMARK_VM_TX_DATA_PATH"
		fi

		header=false
done < "$TXS"

echo "Saved tx benchmark data to $BENCHMARK_VM_TX_DATA_PATH" 
echo

echo "Processing tx benchmark data"
python benchmark/plot_tx_execution.py "$BENCHMARK_NATIVE_TX_DATA_PATH" "$BENCHMARK_VM_TX_DATA_PATH" "$BENCHMARK_ARTIFACTS_PATH"
echo "Saved tx benchmark artifacts to $BENCHMARK_ARTIFACTS_PATH"

echo "Saving benchmark info to $BENCHMARK_INFO_PATH"
python benchmark/gather_info.py | jq \
	'{
		"Title": "Tx Execution Benchmark",
		"Native profile": "default"
	} + .' > "$BENCHMARK_INFO_PATH"

echo "Compressing benchmark to" "$BENCHMARK_ROOT/$BENCHMARK_NAME.zip"
(cd "$BENCHMARK_ROOT" && zip -qr "$BENCHMARK_NAME.zip" "$BENCHMARK_NAME")

echo "Saved full benchmark to" "$BENCHMARK_DIR"

echo
cut -d, -f 1,8 "$BENCHMARK_ARTIFACTS_PATH/cairo-vm-vs-cairo-native.csv" | column -ts,
