#!/usr/bin/env bash

index=$1
emu_trace="./traces/emu/trace_$index.json"
native_trace="./traces/native/trace_$index.json"

delta "$emu_trace" "$native_trace" --side-by-side
