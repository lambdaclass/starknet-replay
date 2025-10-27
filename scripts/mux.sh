#!/usr/bin/env bash
#
# Dependencies:
#   tmux
#   coreutils
#   jq

set -euo pipefail

ENVRC=.envrc

usage() {
cat <<EOF
Usage: $0 [OPTIONS] <COMMAND>

A helper script for replaying blocks with both Cairo Native, and Cairo VM,
inside of persistent tmux sessions.

Note that optional flags always go before positional arguments.

Options:
  -n NAME  Prefix for the created/retrieved TMUX sessions. Default: "replay".

Commands:
  range [OPTIONS] <NETWORK> <BLOCK> <N_BLOCKS> <N_WORKERS>

    Replays N_BLOCKS contiguous blocks, starting at BLOCK from NETWORK, in
    N_WORKERS TMUX sessions for each executor.

    For each session, an $ENVRC file in the current directory is sourced, which
    should contain environment variables required for the execution.

    Options:
      -s <EXECUTOR>  Skips the given executor (either native, or vm)

  block [OPTIONS] <NETWORK> <BLOCK ...>

    Replays all BLOCKS, from NETWORK, each one in a different TMUX session, for
    each executor.

    For each session, an .envrc file in the current directory is sourced, which
    should contain environment variables required for the execution.

    Options:
      -s <EXECUTOR>  Skips the given executor (either native, or vm)

  status

    Shows the status of each TMUX session with the given prefix, as a table.

  stop [OPTIONS]

    Kills all stopped TMUX sessions with the given prefix.

    Options:
      -a  Kills all TMUX sessions, even if running.
EOF
exit 1
}

###########################################################
#                        UTILITIES                        #
###########################################################

yell() { echo -e "$0:" "$@"; }

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

build_native() {
	echo "Building replay for Cairo Native"
	cargo build --quiet --release --features structured_logging,state_dump
	rm -f ./target/release/replay-native
	mv ./target/release/replay ./target/release/replay-native
}

build_vm() {
	echo "Building replay for Cairo VM"
	cargo build --quiet --release --features structured_logging,state_dump,only_cairo_vm
	rm -f ./target/release/replay-vm
	mv ./target/release/replay ./target/release/replay-vm
}

replay_range() {
	EXE="$1"
	NAME="$2"
	NETWORK="$3"
	START="$4"
	END="$5"

	name="${NAME}-$EXE-${NETWORK}-${START}-${END}"
	command=$(
		cat <<- END
			bash
			source $ENVRC
			time ./target/release/replay-$EXE \\
				block-range $START $END $NETWORK
		END
	)
	spawn "$name" "$command" && {
		echo "Replaying $NETWORK block range $START-$END in session $name"
  }
  return 0
}

replay_block() {
	EXE="$1"
	NAME="$2"
	NETWORK="$3"
	BLOCK="$4"

	name="${NAME}-$EXE-${NETWORK}-${BLOCK}"
	command=$(
		cat <<- END
			bash
			source $ENVRC
			time ./target/release/replay-$EXE \\
				block $NETWORK $BLOCK
		END
	)
	spawn "$name" "$command" && {
		echo "Replaying $NETWORK block $BLOCK in session $name"
  }
  return 0
}

###########################################################
#                       SUBCOMMANDS                       #
###########################################################

range() {
	# Parse optional flags.
	SKIP=""
	local OPTIND
	while getopts "s:" opt; do
		case $opt in
			s) SKIP="$OPTARG" ;;
			*) echo; usage ;;
		esac
	done
	shift $((OPTIND - 1))

	# Parse positional arguments.
	if [[ $# -lt 4 ]]; then
		yell "range expects 4 positional argument\n"
		usage
	fi
	NETWORK="$1"
	START_BLOCK="$2"
	RANGE_SIZE="$3"
	N_WORKERS="$4"

	if ! [[ -a "$ENVRC" ]]; then
		yell "Failed to find $ENVRC file"
		exit 1
	fi

	step_size=$(((RANGE_SIZE + N_WORKERS - 1) / N_WORKERS))
	end_block=$((START_BLOCK + RANGE_SIZE - 1))

	# Build binaries if required.
	if [ "$SKIP" != "native" ]; then
		build_native
	fi
	if [ "$SKIP" != "vm" ]; then
		build_vm
	fi

	# Spawn executors.
	for ((i = START_BLOCK ; i <= end_block ; i += step_size )); do
		current_start_block="$i"
		current_end_block=$((i + step_size - 1))
		current_end_block=$((current_end_block > end_block ? end_block : current_end_block))

		# Spawn VM executor if required.
		if [ "$SKIP" != "vm" ]; then
			replay_range "vm" "$NAME" "$NETWORK" "$current_start_block" "$current_end_block"
		fi

		# Spawn Native executor if required.
		if [ "$SKIP" != "native" ]; then
			replay_range "native" "$NAME" "$NETWORK" "$current_start_block" "$current_end_block"
		fi
	done
}

block() {
	# Parse optional flags.
	SKIP=""
	local OPTIND
	while getopts "s:" opt; do
		case $opt in
			s) SKIP="$OPTARG" ;;
			*) echo; usage ;;
		esac
	done
	shift $((OPTIND - 1))

	# Parse positional arguments.
	if [[ $# -lt 2 ]]; then
		yell "block expects at least 2 positional argument\n"
		usage
	fi
	NETWORK="$1"

	# Build binaries if required.
	if [ "$SKIP" != "native" ]; then
		build_native
	fi
	if [ "$SKIP" != "vm" ]; then
		build_vm
	fi

	for block in "${@:2}"; do
		# Spawn VM executor if required.
		if [ "$SKIP" != "vm" ]; then
			replay_block "vm" "$NAME" "$NETWORK" "$block"
		fi

		# Spawn Native executor if required.
		if [ "$SKIP" != "native" ]; then
			replay_block "native" "$NAME" "$NETWORK" "$block"
		fi
	done
}

status() {
	{
	echo -e "status\tname\tduration\tblock\tmessage"
	
	# Iterate all sessions matching name.
	tmux ls -F '#{session_id} #{session_name} #{session_created} #{pane_current_command}' 2>/dev/null |
	while read -r _ name init_time command; do
		if ! [[ $name == $NAME-* ]]; then
	    continue;
	  fi

		if [[ $command == "bash" ]]; then
			status="STOPPED"
		else
			status="RUNNING"
		fi

		logs=$(tmux capture-pane -pJt "$name" -S 0 -E 100)

		# Find latest valid log line.
		log=""
		while IFS= read -r line; do
			if [ -n "$line" ] && echo "$line" | jq . ; then
				log="$line"
				break
			fi
		done < <( echo "$logs" | tac ) >/dev/null 2>&1
		if [ -z "${log:-}" ]; then
			echo "Failed to find logs for session $name" >&2
			printf "%s\t%s\t%s\t%s\t%s\n" "$status" "$name" "unknown" "unknown" "unknown"
			continue
		fi

		# Obtain duration by comparing last log timestamp, with initial timestmap.
		timestamp=$(echo "$log" | jq -r .timestamp | sed -E "s/\.[0-9]+//")
		case $(uname) in
			Darwin)
				timestamp_s=$(date -ujf "%Y-%m-%dT%H:%M:%SZ" "+%s" "$timestamp")
			;;
			Linux)
				timestamp_s=$(date -ud "$timestamp" "+%s")
			;;
		esac
		duration_s=$(bc <<< "$timestamp_s-$init_time")
		hours=$(bc <<< "$duration_s/3600")
		minutes=$(bc <<< "($duration_s%3600)/60")
		seconds=$(bc <<< "$duration_s%60")
		duration="$hours:$minutes:$seconds"

		# Not all logs contain the current block.
		if ! block=$( echo "$log" | jq '.spans[] | select (.name=="block execution") | .block' ); then
			block="unknown"
		fi 2>/dev/null

		message=$(echo "$log" | jq .fields.message)

		printf "%s\t%s\t%s\t%s\t%s\n" "$status" "$name" "$duration" "$block" "$message"
	done
	} |
	column -t -s $'\t'
}

stop() {
	# Parse optional flags.
	KILL_ALL=false
	local OPTIND
	while getopts "a" opt; do
		case $opt in
			a) KILL_ALL=true ;;
			*) echo; usage ;;
		esac
	done
	shift $((OPTIND - 1))

	# Iterate all sessions matching name.
	tmux ls -F '#{session_name} #{pane_current_command}' 2>/dev/null |
	while read -r name command; do
		if ! [[ $name == $NAME-* ]]; then
		  continue;
		fi

		# If the command executing is bash, then the execution has stopped.
		if [[ $command != "bash" ]]; then
			# Only kill running sessions if KILL_ALL is set.
			if [ $KILL_ALL = true ]; then
				echo "Session $name is running, killing it"
				tmux kill-session -t "$name"
			else
				echo "Session $name is running, skipping it"
			fi
		else
			# Always kill stopped sessions.
			echo "Session $name has stopped, killing it"
			tmux kill-session -t "$name"
		fi
	done
}

###########################################################
#                          MAIN                           #
###########################################################

# Parse global optional flags.
NAME="replay"
while getopts "hn:" opt; do
	case $opt in
		h) usage ;;
		n) NAME=$OPTARG ;;
		*) echo; usage ;;
	esac
done
shift $((OPTIND - 1))

# Call subcommand given by first argument.
if [[ $# -lt 1 ]]; then
	yell "expected subcommand\n"
	usage
fi
case "$1" in
	range) range "${@:2}";;
	block) block "${@:2}";;
	status) status "${@:2}";;
	stop) stop "${@:2}";;
	*) yell "unknown subcommand: $1\n"; usage ;;
esac
