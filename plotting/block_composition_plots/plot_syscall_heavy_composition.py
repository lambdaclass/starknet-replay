from argparse import ArgumentParser
from utils import load_data
from pprint import pprint

SYSCALL_ENTRYPOINTS = []

argument_parser = ArgumentParser("Block Syscall Heavy Composition")
argument_parser.add_argument("block_execution_info")
arguments = argument_parser.parse_args()


def count_syscalls(tx):
    syscall_count = 0
    gas = sum(tx["gas"].values())

    if tx["validate_call_info"] is not None:
        syscall_count += sum(
            [entrypoint["syscall_count"] for entrypoint in tx["validate_call_info"]]
        )
    if tx["execute_call_info"] is not None:
        syscall_count += sum(
            [entrypoint["syscall_count"] for entrypoint in tx["execute_call_info"]]
        )
    if tx["fee_transfer_call_info"] is not None:
        syscall_count += sum(
            [entrypoint["syscall_count"] for entrypoint in tx["fee_transfer_call_info"]]
        )

    return {"tx_hash": tx["tx_hash"], "syscall_count": syscall_count, "total_gas": gas}


def process_fn(block):
    return {
        "block_number": block["block_number"],
        "txs": [count_syscalls(tx) for tx in block["entrypoints"]],
    }


df = load_data(arguments.block_execution_info, process_fn)
