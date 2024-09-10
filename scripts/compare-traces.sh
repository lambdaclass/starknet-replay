#!/usr/bin/env bash

index=0
while : ; do
  emu_trace="./traces/emu/trace_$index.json"
  native_trace="./traces/native/trace_$index.json"

  if ! [ -f $emu_trace ] && ! [ -f $native_trace ]; then
    exit
  fi

  if ! [ -f $emu_trace ]; then
    echo "missing file: $emu_trace"
    exit
  fi
  if ! [ -f $native_trace ]; then
    echo "missing file: $native_trace"
    exit
  fi

  if ! cmp --silent $emu_trace $native_trace; then
    echo "difference: $emu_trace $native_trace"
  fi

  index=$((index+1))
done
