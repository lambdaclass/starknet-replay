#!/usr/bin/env bash

# If there are a lot of transactions, it can be difficult to quit the script. This prompts if the user wants to continue after each diff.
function prompt_continue {
    while true; do
      read -rp "Continue [Y/n]? " yn
      case $yn in
          [Yy]* ) break;;
          [Nn]* ) exit;;
          * ) break;;
      esac
  done
}

for block_dir in state_dumps/vm/*/; do
  [ -d "$block_dir" ] || continue
  block_base=$(basename "$block_dir")

  echo "Block ${block_base//block/}"

  for vm_dump in "$block_dir"/*.json; do
    [ -f "$vm_dump" ] || continue
    native_dump="${vm_dump//vm/native}"

    if cmp -s \
      <(sed '/"reverted": /d' "$native_dump") \
      <(sed '/"reverted": /d' "$vm_dump")
    then
      continue
    fi

    base=$(basename "$vm_dump")
    echo "Tx ${base//.json/}"

    prompt_continue

    delta "$native_dump" "$vm_dump" --side-by-side --paging always --wrap-max-lines unlimited 
  done
done
