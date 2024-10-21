#!/usr/bin/env bash

matching=0
diffing=0

for native_dump in state_dumps/native/*/*.json; do
  [ -f "$native_dump" ] || continue

  vm_dump="${native_dump//native/vm}"

  # Check if the corresponding vm_dump file exists, if not, skip
  if [ ! -f "$vm_dump" ]; then
    echo "Skipping: $vm_dump (file not found)"
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
