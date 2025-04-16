#!/usr/bin/env python

import json
import argparse
from sys import stderr, stdout

parser = argparse.ArgumentParser()
parser.add_argument("execution_info_path")
parser.add_argument("call_index", type=int)
args = parser.parse_args()


with open(args.execution_info_path, "r") as execution_info_file:
    execution_info = json.load(execution_info_file)


def find_call(call_info, call_index):
    if call_info["call_counter"] == call_index:
        return call_info

    for inner_call in call_info["inner_calls"]:
        if target_call := find_call(inner_call, call_index):
            return target_call

    return None


execute_call_info = execution_info["execution_info"]["execute_call_info"]
target_call_info = find_call(execute_call_info, args.call_index)
if not target_call_info:
    print("failed to find call", file=stderr)
    exit(1)

target_call = target_call_info["call"]
json.dump(target_call, stdout, indent=4)
