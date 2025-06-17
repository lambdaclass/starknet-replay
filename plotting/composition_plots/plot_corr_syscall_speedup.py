import sys
import os
import pandas as pd
import matplotlib.pyplot as plt
import seaborn as sns

from argparse import ArgumentParser
from utils import load_block_composition_data, save_to_path

parent_dir = os.path.dirname(os.path.pardir)

sys.path.append(parent_dir)

from plotting.utils import load_json_dir_data, load_json_file_data

argument_parser = ArgumentParser("Syscall Percentage with Speed Correlation")
argument_parser.add_argument("native_bench_data")
argument_parser.add_argument("vm_bench_data")
argument_parser.add_argument("block_execution_info")
argument_parser.add_argument("libfunc_profiling_info")
arguments = argument_parser.parse_args()


# ==========
# PROCESSING
# ==========


def process_bench_data(tx):
    hash = tx["hash"]
    time_ns = tx["time_ns"]

    return {"tx_hash": hash, "time_ns": time_ns}


def process_speedup(native_vm_bench):
    tx_hash = native_vm_bench["tx_hash"]
    native_time_ns = native_vm_bench["native_time_ns"]
    vm_time_ns = native_vm_bench["vm_time_ns"]
    
    speedup = vm_time_ns / native_time_ns

    return {"tx_hash": tx_hash, "speedup": speedup}


def process_block_composition_fn(tx):
    syscall_count = 0

    if tx["execute_call_info"] is not None:
        syscall_count += sum(
            [entrypoint["syscall_count"] for entrypoint in tx["execute_call_info"]]
        )
    if tx["validate_call_info"] is not None:
        syscall_count += sum(
            [entrypoint["syscall_count"] for entrypoint in tx["validate_call_info"]]
        )
    if tx["fee_transfer_call_info"] is not None:
        syscall_count += sum(
            [entrypoint["syscall_count"] for entrypoint in tx["fee_transfer_call_info"]]
        )

    return {
        "block_number": tx["block_number"],
        "tx_hash": tx["tx_hash"],
        "syscall_count": syscall_count,
    }


def process_libfunc_profiles_fn(profile):
    libfunc_calls_count = sum([libfunc["samples"] for libfunc in profile["data"]])

    return {
        "block_number": profile["block_number"],
        "tx_hash": profile["tx"],
        "libfunc_calls_count": libfunc_calls_count,
    }


def process_syscall_ptg(syscalls_x_libfunc_calls):
    block_number = syscalls_x_libfunc_calls["block_number"]
    tx_hash = syscalls_x_libfunc_calls["tx_hash"]
    libfunc_count = syscalls_x_libfunc_calls["libfunc_calls"]
    syscall_count = syscalls_x_libfunc_calls["syscalls"]

    syscall_ptg = syscall_count * 100 / libfunc_count

    return {
        "tx_hash": tx_hash,
        "syscall_ptg": syscall_ptg,
    }


# Process bench data

df_native_bench = load_json_file_data(arguments.native_bench_data, process_bench_data)
df_native_bench = df_native_bench.rename(
    columns={"time_ns": "native_time_ns"}
)
df_vm_bench = load_json_file_data(arguments.vm_bench_data, process_bench_data)
df_vm_bench = df_vm_bench.rename(
    columns={"time_ns": "vm_time_ns"}
)

df_speedup = (
    df_native_bench.merge(df_vm_bench, on=["tx_hash"])
    .apply(process_speedup, axis=1)
    .apply(pd.Series)
)

# Process Syscall Percentage

df_composition_by_block = (
    load_block_composition_data(
        arguments.block_execution_info, process_block_composition_fn
    )
    .groupby(["block_number", "tx_hash"], as_index=False)
    .agg(syscalls=("syscall_count", "sum"))
)

df_profiles_by_block = (
    load_json_dir_data(arguments.libfunc_profiling_info, process_libfunc_profiles_fn)
    .groupby(["block_number", "tx_hash"], as_index=False)
    .agg(
        libfunc_calls=("libfunc_calls_count", "sum"),
    )
)

df_syscall_ptg = (
    df_profiles_by_block.merge(df_composition_by_block, on=["block_number", "tx_hash"])
    .apply(process_syscall_ptg, axis=1)
    .apply(pd.Series)
)

df_speedup_syscall = df_syscall_ptg.merge(df_speedup, on=["tx_hash"]).apply(pd.Series)

# ========
# Plotting
# ========

block_range = f"{df_composition_by_block['block_number'].min()}-{df_composition_by_block['block_number'].max()}"

figure, ax = plt.subplots(figsize=(15, 15))

sns.regplot(data=df_speedup_syscall, x="speedup", y="syscall_ptg")

ax.set_xlabel("Seepdup")
ax.set_ylabel("Syscalls (%)")
ax.set_title("Syscall Heavy Txs Composition")
save_to_path(f"syscalls_ptg_speedup_corr-{block_range}")
