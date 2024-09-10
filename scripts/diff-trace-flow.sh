#!/usr/bin/env bash

index=$1
emu_trace="./traces/emu/trace_$index.json"
native_trace="./traces/native/trace_$index.json"

delta <(grep statementIdx "$emu_trace") <(grep statementIdx "$native_trace") --side-by-side
