#!/usr/bin/env bash

mkdir -p good_state_dump/native
mkdir -p good_state_dump/vm

for native_dump in state_dump/native/*; do
  base_dump=$(basename -- "$native_dump")
  vm_dump="state_dump/vm/$base_dump"

  if ! cmp -s "$vm_dump" "$native_dump" ; then
    echo "diff at $base_dump"
  else
    echo "match at $base_dump"
    mv "$native_dump" good_state_dump/native
    mv "$vm_dump" good_state_dump/vm
  fi
done
