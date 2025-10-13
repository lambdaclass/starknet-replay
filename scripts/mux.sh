#!/usr/bin/env bash

set -euo pipefail

ENVRC=.envrc

yell() { echo -e "$0:" "$@" >&2; }

usage() {
cat >&2 <<EOF
Usage: $0
EOF
exit 1
}

spawn() {
	name="$1"
	command="$2"

	if tmux has-session -t "$name" 2>/dev/null; then
		echo "Session $name already exists"
		return 1
	else
		tmux new-session -d -s "$name" "/bin/bash"
		tmux send-keys -t "$name" "$command" C-m
		return 0
	fi
}

range() {
	if [[ $# -lt 3 ]]; then
		yell "range expects 3 positional argument\n"
		usage
	fi
	START_BLOCK="$1"
	RANGE_SIZE="$2"
	N_WORKERS="$3"

	step_size=$(((RANGE_SIZE + N_WORKERS - 1) / N_WORKERS))
	end_block=$((START_BLOCK + RANGE_SIZE - 1))

	echo "Building replay for Cairo Native"
	cargo build -q --release --features state_dump 2>/dev/null
	cp ./target/release/replay ./target/release/replay-native
	echo "Building replay for Cairo VM"
	cargo build -q --release --features state_dump,only_cairo_vm 2>/dev/null
	cp ./target/release/replay ./target/release/replay-vm

	for ((i = START_BLOCK ; i <= end_block ; i += step_size )); do
		current_start_block="$i"
		current_end_block=$((i + step_size - 1))
		current_end_block=$((current_end_block > end_block ? end_block : current_end_block))

		name="${NAME}-vm-${current_start_block}-${current_end_block}"
		command=$(
			cat <<- END
				bash
				source $ENVRC
				time ./target/release/replay-vm \\
					block-range $current_start_block $current_end_block mainnet
			END
		)
		spawn "$name" "$command" && {
			echo "Replaying block range $current_start_block-$current_end_block in session $name"
	  }

		name="${NAME}-native-${current_start_block}-${current_end_block}"
		command=$(
			cat <<- END
				bash
				source $ENVRC
				time ./target/release/replay-native \\
					block-range $current_start_block $current_end_block mainnet
			END
		)
		spawn "$name" "$command" && {
			echo "Replaying block range $current_start_block-$current_end_block in session $name"
	  }
	done
}

# Parse optional flags.
NAME="replay"
while getopts "hn:" opt; do
	case $opt in
		h) usage ;;
		n) NAME=$OPTARG ;;
		*) echo >&2; usage ;;
	esac
done
shift $((OPTIND - 1))

# Read subcommand
if [[ $# -lt 1 ]]; then
	yell "expected subcommand\n"
	usage
fi
case "$1" in
	range) ;;
	*) yell "unknown subcommand" ;;
esac

# Call subcommand
"$@"
