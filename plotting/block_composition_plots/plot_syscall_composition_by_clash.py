import sys
import os
import numpy as np
import pandas as pd
import matplotlib.pyplot as plt
import seaborn as sns

from argparse import ArgumentParser
from utils import load_block_composition_data, save_to_path

parent_dir = os.path.dirname(os.path.pardir)

sys.path.append(parent_dir)

from plotting.utils import load_json_dir_data

argument_parser = ArgumentParser("Syscall Heavy Composition")
argument_parser.add_argument("block_execution_info")
argument_parser.add_argument("libfunc_profiling_info")
arguments = argument_parser.parse_args()

CLASS_HASHES_ERC20 = [
    "0x6afa2f21a611f8b4a77ef681a9eb0c7cd6e52aa918e7f8b4b8142b4ca1bde49",
    "0x74aad3c412b1d7c05f720abfd39adc709b8bf8a8c7640e50505a9436a6ff0cf",
    "0x7f3777c99f3700505ea966676aac4a0d692c2a9f5e667f4c606b51ca1dd3420",
    "0x79561bce61f39a0dfab9413cee86f6cfe7d9112b96abce545c6e929b20081eb",
    "0x5ffbcfeb50d200a0677c48a129a11245a3fc519d1d98d76882d1c9a1b19c6ed",
    "0x4ad3c1dc8413453db314497945b6903e1c766495a1e60492d44da9c2a986e4b",
    "0x29fd83b01f02b45987dfb9652633cd0f1f64a0f36403ab1fed7bd99642fa474",
]
CLASS_HASHES_SWAP = [
    "0x7f3331378862ed0a10f8c3d49f4650eb845af48f1c8120591a43da8f6f12679",
    "0x7197021c108b0cc57ae354f5ad02222c4b3d7344664e6dd602a0e2298595434",
    "0x514718bb56ed2a8607554c7d393c2ffd73cbab971c120b00a2ce27cc58dd1c1",
    "0x40b83509bc9cebd1af068b7d32e8b04cda394db1aedacb512f321d8a825e683",
]


def count_syscalls_by_entrypoint(entrypoint):
    return {
        "class_hash": entrypoint["class_hash"],
        "selector": entrypoint["selector"],
        "syscall_count": entrypoint["syscall_count"],
    }


def process_class_hash(tx):
    entrypoints = []

    if tx["execute_call_info"] is not None:
        entrypoints.extend(
            [
                count_syscalls_by_entrypoint(entrypoint)
                for entrypoint in tx["execute_call_info"]
            ]
        )
    if tx["validate_call_info"] is not None:
        entrypoints.extend(
            [
                count_syscalls_by_entrypoint(entrypoint)
                for entrypoint in tx["validate_call_info"]
            ]
        )
    if tx["fee_transfer_call_info"] is not None:
        entrypoints.extend(
            [
                count_syscalls_by_entrypoint(entrypoint)
                for entrypoint in tx["fee_transfer_call_info"]
            ]
        )

    return entrypoints


def process_classhes(tx):
    return {
        "block_number": tx["block_number"],
        "tx_hash": tx["tx_hash"],
        "selector_by_classh": process_class_hash(tx),
    }


def process_selector_profiles(profile):
    libfunc_calls_count = sum([libfunc["samples"] for libfunc in profile["data"]])

    return {
        "block_number": profile["block_number"],
        "class_hash": profile["class_hash"],
        "tx_hash": profile["tx"],
        "selector": profile["selector"],
        "libfunc_calls_count": libfunc_calls_count,
    }


def get_syscall_percentages(syscalls_x_libfunc_calls):
    class_hash = syscalls_x_libfunc_calls["class_hash"]
    libfunc_count = syscalls_x_libfunc_calls["libfunc_calls_count"]
    syscall_count = syscalls_x_libfunc_calls["syscall_count"]

    syscall_ptg = syscall_count * 100 / libfunc_count

    return {
        "class_hash": class_hash,
        "syscall_ptg": syscall_ptg,
    }


# ==========
# PROCESSING
# ==========

# Process block composition class hashes

df_block_composition = load_block_composition_data(
    arguments.block_execution_info, process_classhes
)

df_block_composition = df_block_composition.explode("selector_by_classh")

df_block_composition = pd.concat(
    [
        df_block_composition.drop(columns="selector_by_classh").reset_index(drop=True),
        pd.json_normalize(df_block_composition["selector_by_classh"]).reset_index(
            drop=True
        ),
    ],
    axis=1,
)

df_block_composition_by_clash = df_block_composition.groupby(
    ["class_hash"], as_index=False
).agg(syscall_count=("syscall_count", "sum"))

df_block_composition_by_clash = df_block_composition_by_clash[
    df_block_composition_by_clash["class_hash"].isin(
        CLASS_HASHES_ERC20 + CLASS_HASHES_SWAP
    )
]


# Process libfunc profiles

df_profiles_by_clash = (
    load_json_dir_data(arguments.libfunc_profiling_info, process_selector_profiles)
    .groupby(["class_hash"], as_index=False)
    .agg(libfunc_calls_count=("libfunc_calls_count", "sum"))
)

df_clashes_syscall_ptg = (
    df_block_composition_by_clash.merge(df_profiles_by_clash, on=["class_hash"])
    .apply(get_syscall_percentages, axis=1)
    .apply(pd.Series)
)
df_clashes_syscall_ptg["type"] = df_clashes_syscall_ptg["class_hash"].apply(
    lambda hash: "SWAP" if hash in CLASS_HASHES_SWAP else "ERC20"
)

# ========
# PLOTTING
# ========

block_range = f"{df_block_composition['block_number'].min()}-{df_block_composition['block_number'].max()}"

cut_bins = np.arange(0, 70, 0.5)

labels = [f"{i}" for i in cut_bins[:-1]]

df_clashes_syscall_ptg["ptg_group"] = pd.cut(
    df_clashes_syscall_ptg["syscall_ptg"], bins=cut_bins, labels=labels
)

figure, ax = plt.subplots(figsize=(15, 10))

sns.histplot(
    data=df_clashes_syscall_ptg,
    x="ptg_group",
    stat="count",
    hue="type",
    multiple="dodge",
)

ax.set_xlabel("Percentages")
ax.set_ylabel("Class Hash Count")
ax.set_title("Syscall Percentages by Class Hash")
save_to_path(f"syscalls_ptg_hist_blocks-{block_range}")
