import sys
import os
import pandas as pd
import matplotlib.pyplot as plt
import seaborn as sns

from argparse import ArgumentParser
from utils import load_block_composition_data

parent_dir = os.path.dirname(os.path.pardir)

sys.path.append(parent_dir)

from plotting.utils import load_json_dir_data

argument_parser = ArgumentParser("Block Syscall Heavy Composition")
argument_parser.add_argument("block_execution_info")
argument_parser.add_argument("libfunc_profiling_info")
argument_parser.add_argument("output_path")
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
    syscall_count = syscalls_x_libfunc_calls["syscall_count"]
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


def save_to_path(name):
    path = f"{arguments.output_path}-{name}.svg"
    plt.savefig(path)


# Process Block Composition data

df_block_composition = load_block_composition_data(
    arguments.block_execution_info, process_block_composition_fn
)

df_composition_by_block = df_block_composition.groupby(["block_number", "tx_hash"]).agg(
    syscalls=("syscall_count", "sum"),
)

# Process Libfunc Profiles data

df_libfunc_profiles = load_json_dir_data(
    arguments.libfunc_profiling_info, process_libfunc_profiles_fn
)

df_profiles_by_block = df_libfunc_profiles.groupby(["block_number", "tx_hash"]).agg(
    libfunc_calls=("libfunc_calls_count", "sum"),
)

# Seggregate Transactions

df_seggregation = (
    df_profiles_by_block.merge(df_block_composition, on=["block_number", "tx_hash"])
    .apply(seggregate_txs, axis=1)
    .apply(pd.Series)
)

df_seggregation_by_block = df_seggregation.groupby(["block_number", "tx_hash"]).agg(
    syscall_ptg=("syscall_ptg", "sum")
)
# Plotting
figure, axs = plt.subplots()
block_range = f"{df_block_composition['block_number'].min()}-{df_block_composition['block_number'].max()}"

# Boxplot syscall percentages quantiles per block

figure, ax = plt.subplots()
sns.boxplot(data=df_seggregation_by_block, x="block_number", y="syscall_ptg")
ax.set_xlabel("Block")
ax.set_ylabel("Syscalls (%)")
ax.set_title("Syscall Heavy Txs Composition")

if arguments.output_path:
    save_to_path(f"syscalls_quantiles-blocks-{block_range}")

# Plot an histogram with syscall percentages

cut_bins = list(range(0, 110, 5))

labels = [f"{i}%" for i in cut_bins[:-1]]

df_seggregation_by_block["ptg_group"] = pd.cut(
    df_seggregation_by_block["syscall_ptg"], bins=cut_bins, labels=labels, right=False
)
df_seggregation_by_ptg_group = df_seggregation_by_block.groupby(
    ["block_number", "ptg_group"]
).agg("count")

figure, ax = plt.subplots()
sns.histplot(data=df_seggregation_by_block, x="ptg_group", kde=True)
ax.set_xlabel("Percentages")
ax.set_ylabel("Syscalls count")
ax.set_title("Syscall percentages in a Block Range")

if arguments.output_path:
    save_to_path(f"syscalls_ptg_hist-blocks-{block_range}")
