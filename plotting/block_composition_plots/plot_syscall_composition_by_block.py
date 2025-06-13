import sys
import os
import pandas as pd
import numpy as np
import matplotlib.pyplot as plt
import seaborn as sns

from argparse import ArgumentParser
from utils import load_block_composition_data, chunks, save_to_path

parent_dir = os.path.dirname(os.path.pardir)

sys.path.append(parent_dir)

from plotting.utils import load_json_dir_data

argument_parser = ArgumentParser("Syscall Heavy Composition")
argument_parser.add_argument("block_execution_info")
argument_parser.add_argument("libfunc_profiling_info")
arguments = argument_parser.parse_args()


def process_block_composition_fn(tx):
    syscall_count = 0
    gas = tx["gas.l1_gas"] + tx["gas.l2_gas"] + tx["gas.l1_data_gas"]

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
        "total_gas": gas,
    }


def process_libfunc_profiles_fn(profile):
    libfunc_calls_count = sum([libfunc["samples"] for libfunc in profile["data"]])

    return {
        "block_number": profile["block_number"],
        "tx_hash": profile["tx"],
        "libfunc_calls_count": libfunc_calls_count,
    }


def seggregate_txs(syscalls_x_libfunc_calls):
    block_number = syscalls_x_libfunc_calls["block_number"]
    tx_hash = syscalls_x_libfunc_calls["tx_hash"]
    libfunc_count = syscalls_x_libfunc_calls["libfunc_calls"]
    syscall_count = syscalls_x_libfunc_calls["syscalls"]
    syscall_count = syscalls_x_libfunc_calls["syscalls"]
    total_gas = syscalls_x_libfunc_calls["total_gas"]

    syscall_ptg = syscall_count * 100 / libfunc_count

    return {
        "block_number": block_number,
        "tx_hash": tx_hash,
        "libfunc_count": libfunc_count,
        "syscalls_count": syscall_count,
        "syscall_ptg": syscall_ptg,
        "is_syscall_heavy": syscall_ptg >= 60,
        "total_gas": total_gas,
    }


# ==========
# PROCESSING
# ==========

# Process Block Composition data

df_composition_by_block = (
    load_block_composition_data(
        arguments.block_execution_info, process_block_composition_fn
    )
    .groupby(["block_number", "tx_hash"], as_index=False)
    .agg(syscalls=("syscall_count", "sum"), total_gas=("total_gas", "sum"))
)

# Process Libfunc Profiles data

df_profiles_by_block = (
    load_json_dir_data(arguments.libfunc_profiling_info, process_libfunc_profiles_fn)
    .groupby(["block_number", "tx_hash"], as_index=False)
    .agg(
        libfunc_calls=("libfunc_calls_count", "sum"),
    )
)

# Seggregate Transactions (syscall/libfunc) heavy

df_seggregation = (
    df_profiles_by_block.merge(df_composition_by_block, on=["block_number", "tx_hash"])
    .apply(seggregate_txs, axis=1)
    .apply(pd.Series)
)


# ========
# PLOTTING
# ========

# Boxplot syscall percentages quantiles per block

for blocks_chunk in chunks("block_number", df_seggregation, 10):
    figure, ax = plt.subplots(figsize=(10, 10))

    block_range = (
        f"{blocks_chunk['block_number'].min()}-{blocks_chunk['block_number'].max()}"
    )

    sns.boxplot(data=blocks_chunk, x="block_number", y="syscall_ptg")

    ax.set_xlabel("Block")
    ax.set_ylabel("Syscalls (%)")
    ax.set_title("Syscall Heavy Txs Composition")
    save_to_path(f"syscalls_quantiles-blocks-{block_range}")


# Plot an histogram with syscall percentages
block_range = f"{df_composition_by_block['block_number'].min()}-{df_composition_by_block['block_number'].max()}"

cut_bins = np.arange(0, 30, 0.5)

labels = [f"{i}" for i in cut_bins[:-1]]

df_seggregation["ptg_group"] = pd.cut(
    df_seggregation["syscall_ptg"], bins=cut_bins, labels=labels
)

figure, ax = plt.subplots(figsize=(15, 15))

sns.histplot(
    data=df_seggregation,
    x="ptg_group",
    stat="count",
)

ax.set_xlabel("Percentages")
ax.set_ylabel("Tx Count")
ax.set_title(f"Syscall percentages in Block Range {block_range}")
save_to_path(f"syscalls_ptg_hist-blocks-{block_range}")
