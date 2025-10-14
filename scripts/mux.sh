#!/usr/bin/env bash

set -euo pipefail

ENVRC=.envrc

usage() {
cat <<EOF
Usage: $0
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
	if [[ $# -lt 3 ]]; then
		yell "range expects 3 positional argument\n"
		usage
	fi
	START_BLOCK="$1"
	RANGE_SIZE="$2"
	N_WORKERS="$3"

	step_size=$(((RANGE_SIZE + N_WORKERS - 1) / N_WORKERS))
	end_block=$((START_BLOCK + RANGE_SIZE - 1))

	# Build binaries if required.
	if [ "$SKIP" != "native" ]; then
		echo "Building replay for Cairo Native"
		cargo build --release --features structured_logging,state_dump 2>/dev/null
		mv ./target/release/replay ./target/release/replay-native
	fi
	if [ "$SKIP" != "vm" ]; then
		echo "Building replay for Cairo VM"
		cargo build --release --features structured_logging,state_dump,only_cairo_vm 2>/dev/null
		mv ./target/release/replay ./target/release/replay-vm
	fi

	# Spawn executors.
	for ((i = START_BLOCK ; i <= end_block ; i += step_size )); do
		current_start_block="$i"
		current_end_block=$((i + step_size - 1))
		current_end_block=$((current_end_block > end_block ? end_block : current_end_block))

		# Spawn VM executor if required.
		if [ "$SKIP" != "vm" ]; then
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
		fi

		# Spawn Native executor if required.
		if [ "$SKIP" != "native" ]; then
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
		while IFS= read -r line; do
			if [ -n "$line" ] && echo "$line" | jq . ; then
				log="$line"
				break
			fi
		done < <( echo "$logs" | tail -r ) >/dev/null 2>&1
		if [ -z "${log:-}" ]; then
			echo "Failed to find logs for session $name" >&2
			continue
		fi

		# Obtain duration by comparing last log timestamp, with initial timestmap.
		timestamp=$(echo "$log" | jq -r .timestamp | sed -E "s/\.[0-9]+//")
		timestamp_s=$(date -ujf "%Y-%m-%dT%H:%M:%SZ" "+%s" "$timestamp")
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
			# Only kill stopped sessions if KILL_ALL is set.
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
	status) status "${@:2}";;
	stop) stop "${@:2}";;
	*) yell "unknown subcommand: $1\n"; usage ;;
esac
