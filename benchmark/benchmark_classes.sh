#!/usr/bin/env bash

set -euo pipefail

errcho() { echo "$@" >&2; }
yell() { errcho "$0:" "$@"; }

usage() {
cat >&2 <<EOF
Usage: $0 [OPTIONS] <CLASSES>

Options:
  -h  Print help
  -n  Number of times to compile each class
EOF
exit 1
}

RUNS=1
# Parse optional flags.
while getopts "hn:" opt; do
	case $opt in
		h) usage ;;
		n) RUNS=$OPTARG;;
		*) errcho; usage ;;
	esac
done
# Skip optional flags from ARGS.
shift $((OPTIND - 1))
# Expect 1 positional argument.
if [[ $# -lt 1 ]]; then
	yell "expected 1 positional argument\n"
	usage
fi
CLASSES="$1"

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
BENCHMARK_NAME="compilation-$DATE"
BENCHMARK_DIR="$BENCHMARK_ROOT/$BENCHMARK_NAME"

BENCHMARK_DATA_PATH="$BENCHMARK_DIR/data.csv"
BENCHMARK_INFO_PATH="$BENCHMARK_DIR/info.json"
BENCHMARK_ARTIFACTS_PATH="$BENCHMARK_DIR/artifacts"
BENCHMARK_REPORT_PATH="$BENCHMARK_DIR/report.html"

mkdir -p "$BENCHMARK_DIR"
cp "$CLASSES" "$BENCHMARK_DIR/classes.txt"

cargo run --quiet --release --bin replay --features benchmark -- bench-compilation \
	--runs "$RUNS" --output "$BENCHMARK_DATA_PATH" "$CLASSES"

python benchmark/plot_compilation.py "$BENCHMARK_DATA_PATH" "$BENCHMARK_ARTIFACTS_PATH"

python benchmark/gather_info.py | jq '{
		"Title": "Compilation Benchmark",
		"Native profile": "default"
	} + .' > "$BENCHMARK_INFO_PATH"

python benchmark/generate_report.py "$BENCHMARK_INFO_PATH" \
	"$BENCHMARK_ARTIFACTS_PATH/compilation-time-distribution.svg" \
	"$BENCHMARK_ARTIFACTS_PATH/sierra-size-vs-compilation-time.svg" \
	"$BENCHMARK_ARTIFACTS_PATH/compiled-contract-size-distribution.svg" \
	"$BENCHMARK_ARTIFACTS_PATH/sierra-size-vs-compiled-contract-size.svg" \
	"$BENCHMARK_ARTIFACTS_PATH/casm-compilation-time-vs-native-compilation-time.svg" \
	"$BENCHMARK_REPORT_PATH" --self-contained

open "$BENCHMARK_REPORT_PATH"

(cd "$BENCHMARK_ROOT" && zip -r "$BENCHMARK_NAME.zip" "$BENCHMARK_NAME")

