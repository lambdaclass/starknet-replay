#!/usr/bin/env bash

matching=0
diffing=0
skipping=0

# Iterate over state_dumps/vm dumps
for vm_dump in state_dumps/vm/*/*.json; do
  [ -f "$vm_dump" ] || continue

  native_dump="${vm_dump//vm/native}"

  # Check if the corresponding native_dump file exists, if not, skip
  if [ ! -f "$native_dump" ]; then
    echo "Skipping: $native_dump (file not found)"
    skipping=$((skipping+1))
    continue
  fi

  base=$(basename "$vm_dump")

  if ! cmp -s \
      <(sed '/"reverted": /d' "$native_dump") \
      <(sed '/"reverted": /d' "$vm_dump")
  then
    echo "diff:  $base"
    diffing=$((diffing+1))
  else
    echo "match: $base"
    matching=$((matching+1))
  fi
done

echo
echo "Finished comparison"
echo "- Matching: $matching"
echo "- Diffing:  $diffing"
echo "- Skipping: $skipping"
