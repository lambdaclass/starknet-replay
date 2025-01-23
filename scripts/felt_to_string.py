#!/usr/bin/env python

import sys

hex = sys.argv[1]
if hex[:2] == "0x":
    hex = hex[2:]

bytes = bytes.fromhex(hex)
string = bytes.decode()

print(string)
