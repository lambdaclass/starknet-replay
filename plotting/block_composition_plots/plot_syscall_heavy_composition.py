import sys
import os
from argparse import ArgumentParser
from utils import load_block_composition_data

parent_dir = os.path.dirname(os.path.pardir)

sys.path.append(parent_dir)

from plotting.utils import load_json_dir_data

argument_parser = ArgumentParser("Block Syscall Heavy Composition")
argument_parser.add_argument("block_execution_info")
argument_parser.add_argument("libfunc_profiling_info")
arguments = argument_parser.parse_args()


def seggregate_txs(syscalls_x_libfunc_calls):
    tx_hash = syscalls_x_libfunc_calls["tx_hash"]
    syscalls = syscalls_x_libfunc_calls["tx_syscalls"]
    libfunc_calls = syscalls_x_libfunc_calls["tx_libfuncs"]

    txs = []

    for tx_libfunc_call, tx_syscall in zip(libfunc_calls, syscalls):
        libfunc_count = tx_libfunc_call["libfunc_calls_count"]
        syscall_count = tx_syscall["syscall_count"]

        syscall_ptg = syscall_count * 100 / libfunc_count

        txs.append(
            {
                "tx_hash": tx_hash,
                "libfunc_count": libfunc_count,
                "syscalls_count": syscall_count,
                "syscall_ptg": syscall_ptg,
                "is_syscall_heavy": syscall_ptg >= 0.6,
            }
        )

    return {}


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


def process_block_composition_fn(block):
    return {
        "block_number": block["block_number"],
        "tx_syscalls": [count_syscalls(tx) for tx in block["entrypoints"]],
    }


def process_libfunc_profiles_fn(profile):
    libfunc_calls_count = sum([libfunc["samples"] for libfunc in profile["data"]])

    return {
        "block_number": profile["block_number"],
        "tx_hash": profile["tx"],
        "libfunc_calls_count": libfunc_calls_count,
    }


def aggregate_blocks(block):
    return {
        "tx_libfuncs": [
            {"tx_hash": tx[0], "libfunc_calls_count": tx[1]}
            for tx in zip(block["txs"], block["calls"])
        ]
    }


df_block_composition = load_block_composition_data(
    arguments.block_execution_info, process_block_composition_fn
)
df_libfunc_profiles = load_json_dir_data(
    arguments.libfunc_profiling_info, process_libfunc_profiles_fn
)

df_profiles_by_block_number = (
    df_libfunc_profiles.groupby(["block_number"])
    .agg(
        txs=("tx_hash", list),
        calls=("libfunc_calls_count", list),
    )
    .apply(aggregate_blocks, axis=1)
)

df = (
    df_profiles_by_block_number.to_frame()
    .merge(df_block_composition, on=["block_number", "tx_hash"])
    .apply(seggregate_txs)
)

print(df)
