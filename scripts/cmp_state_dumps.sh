#!/usr/bin/env bash

# Compares state dump files between two directories: 'state_dumps/vm' and 'state_dumps/native'.
# It iterates over all JSON files in the 'state_dumps/vm' directory and checks if the corresponding
# file exists in 'state_dumps/native'.
# If the corresponding file does not exist, it skips the comparison and counts the missing files.
# For existing pairs, it compares the contents, ignoring the lines containing the "reverted" field, because of error message diference in Native and VM.
# If invoked with the -x option, then matching files are removed
# It counts and displays the number of matching, differing, and missing state dumps.

matching=0
diffing=0
missing=0

while getopts "x" option; do
	case "$option" in
		x)
			REMOVE=true
			;;
		*)
	esac
done

echo "Starting comparison"

# Iterate over state_dumps/vm dumps
for vm_dump in state_dumps/vm/*/*.json; do
	[ -f "$vm_dump" ] || continue

	native_dump="${vm_dump//vm/native}"

	tx_name=$(basename "$vm_dump")
	tx=${tx_name//.*/}
	block_name=$(basename "$(dirname "$vm_dump")")
	block=${block_name//block/}

	# Check if the corresponding native_dump file exists, if not, skip
	if [ ! -f "$native_dump" ]; then
		echo "Miss at block $block, tx $tx (file not found)"
		missing=$((missing+1))
		continue
	fi

	if ! cmp -s \
		<(sed '/"revert_error": /d' "$native_dump") \
		<(sed '/"revert_error": /d' "$vm_dump")
	then
		echo "Diff at block $block, tx $tx"
		diffing=$((diffing+1))
	else
		matching=$((matching+1))

		# remove the files only with REMOVE flag
		if [ "$REMOVE" = true ] ; then
			echo "block $block tx $tx" >> "state_dumps/matching.log"
			rm "$vm_dump" "$native_dump"
		fi
	fi
done

echo
echo "Finished comparison"
echo "- Matching: $matching"
echo "- Diffing:  $diffing"
echo "- Missing:  $missing"

# remove empty block directories
if [ "$REMOVE" = true ] ; then
	find state_dumps -type d -empty -delete
fi
