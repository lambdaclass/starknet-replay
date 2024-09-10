#!/usr/bin/env bash

echo -n "$1" | hexdump -e '32/1 "%x" "\n"'
