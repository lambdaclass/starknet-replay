#!/usr/bin/env bash

# If there are a lot of transactions, it can be difficult to quit the script. This prompts if the user wants to diff the tx, allowing Ctrl+C.
function prompt_continue {
    while true; do
      read -rp "Diff [Y/n]? " yn
      case $yn in
          [Nn]* ) return 1;;
          * ) return 0;;
      esac
  done
}

for block in state_dumps/vm/*/; do
  [ -d "$block" ] || continue
  block_name=$(basename "$block")

  echo "Block ${block_name//block/}"

  # Compares the files in ascending order, by creation date
  IFS=$'\n'
  for tx_name in $(ls -tr1 $block); do
    native_tx="state_dumps/native/$block_name/$tx_name"
    vm_tx="state_dumps/vm/$block_name/$tx_name"

    # Check if the corresponding native_tx file exists, if not, skip
    if [ ! -f "$native_tx" ]; then
      echo "Skipping: $native_tx (file not found)"
      continue
    fi

    if cmp -s \
      <(sed '/"revert_error": /d' "$native_tx") \
      <(sed '/"revert_error": /d' "$vm_tx")
    then
      continue
    fi

    echo "Tx ${tx_name//.*/}"

    prompt_continue && {
      delta "$native_tx" "$vm_tx" --side-by-side --paging always --wrap-max-lines unlimited
    }
  done
done
