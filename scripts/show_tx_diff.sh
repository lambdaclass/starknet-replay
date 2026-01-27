#!/usr/bin/env bash

set -e

usage() {
  cat <<EOF
Usage: $(basename "$0") <block_number> <tx_hash>

Display state diffs for a single transaction between Native and VM execution.

Arguments:
  block_number  The block number
  tx_hash       The transaction hash

Example:
  $(basename "$0") 123456 0x1234abcd...
EOF
  exit 1
}

if [ $# -ne 2 ]; then
  usage
fi

block_number="$1"
tx_hash="$2"

native_tx="state_dumps/native/block${block_number}/${tx_hash}.json"
vm_tx="state_dumps/vm/block${block_number}/${tx_hash}.json"

native_exists=false
vm_exists=false

[ -f "$native_tx" ] && native_exists=true
[ -f "$vm_tx" ] && vm_exists=true

if [ "$native_exists" = true ] && [ "$vm_exists" = true ]; then
  delta "$native_tx" "$vm_tx" --side-by-side --paging always --wrap-max-lines unlimited
elif [ "$native_exists" = false ] && [ "$vm_exists" = false ]; then
  echo "no state diffs found in ${tx_hash} from block ${block_number}"
elif [ "$native_exists" = false ]; then
  echo "native does not have state diffs stored for ${tx_hash} from block ${block_number}"
else
  echo "vm does not have state diffs stored for ${tx_hash} from block ${block_number}"
fi
