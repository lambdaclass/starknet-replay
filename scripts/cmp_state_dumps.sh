#!/usr/bin/env bash

matching=0
diffing=0

for vm_dump in state_dumps/vm/*/*.json; do
  [ -f "$vm_dump" ] || continue

  native_dump="${vm_dump//vm/native}"

  base=$(basename "$native_dump")

  if ! cmp -s "$native_dump" "$vm_dump" ; then
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
