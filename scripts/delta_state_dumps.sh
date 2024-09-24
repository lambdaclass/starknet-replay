#!/usr/bin/env bash

for native_dump in state_dump/native/*; do
  base_dump=$(basename -- "$native_dump")
  vm_dump="state_dump/vm/$base_dump"

  if ! cmp -s "$vm_dump" "$native_dump" ; then
    delta --paging always "$vm_dump" "$native_dump"
  fi
done
