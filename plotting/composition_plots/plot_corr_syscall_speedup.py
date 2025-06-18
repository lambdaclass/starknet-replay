import json
import pandas as pd
import matplotlib.pyplot as plt
import seaborn as sns
from pprint import pprint
from argparse import ArgumentParser
from utils import load_json_dir, save_to_path

argument_parser = ArgumentParser("Runtime Percentage with Speed Correlation")
argument_parser.add_argument("native_bench_data")
argument_parser.add_argument("vm_bench_data")
argument_parser.add_argument("libfunc_profiling_info")
arguments = argument_parser.parse_args()

RUNTIME_LIBFUNCS = [
    "debug_print",
    "pedersen_hash",
    "hades_permutation",
    "ec_state_finalize",
    "ec_state_init",
    "ec_state_add_mul",
    "ec_state_add",
    "ec_try_new",
    "ec_point_from_x",
    "felt252dict_new",
    "felt252dict_get",
    "get_builtin_costs",
    "class_hash_const",
    "class_hash_try_from_felt252",
    "class_hash_to_felt252",
    "contract_address_const",
    "contract_address_try_from_felt252",
    "contract_address_to_felt252",
    "storage_read",
    "storage_write",
    "storage_base_address_const",
    "storage_base_address_from_felt252",
    "storage_address_from_base",
    "storage_address_from_base_and_offset",
    "storage_address_to_felt252",
    "storage_address_try_from_felt252",
    "emit_event",
    "get_block_hash",
    "get_exec_info_v1",
    "get_exec_info_v2",
    "deploy",
    "keccak",
    "replace_class",
    "send_message_to_l1",
    "cheatcode",
    "secp256k1_new",
    "secp256k1_add",
    "secp256k1_mul",
    "secp256k1_get_point_from_x",
    "secp256k1_get_xy",
    "secp256r1_new",
    "secp256r1_add",
    "secp256r1_mul",
    "secp256r1_get_point_from_x",
    "secp256r1_get_xy",
    "sha256_process_block",
    "sha256_state_handle_init",
    "sha256_state_handle_digest",
    "get_class_hash_at_syscall",
    "meta_tx_v0",
]


def load_bench_data(path, f):
    data = json.load(open(path))
    df = pd.DataFrame(data["transactions"])
    df = df.apply(f, axis=1).dropna().apply(pd.Series)

    return df


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


def process_libfunc_profiles_fn(profile):
    libfuncs_removed = ["contract_call", "library_call"]
    libfunc_total_time = sum(
        [
            libfunc["total_time"]
            for libfunc in profile["data"]
            if libfunc["libfunc_name"] not in libfuncs_removed
        ]
    )
    runtime_total_time = sum(
        [
            libfunc["total_time"]
            for libfunc in profile["data"]
            if libfunc["libfunc_name"] in RUNTIME_LIBFUNCS
        ]
    )

    return {
        "block_number": profile["block_number"],
        "tx_hash": profile["tx"],
        "libfunc_total_time": libfunc_total_time,
        "runtime_total_time": runtime_total_time,
    }


def process_runtime_ptg(tx):
    block_number = tx["block_number"]
    tx_hash = tx["tx_hash"]
    runtime_ptg = tx["runtime_total_time"] * 100 / tx["libfunc_total_time"]

    return {
        "block_number": block_number,
        "tx_hash": tx_hash,
        "runtime_ptg": runtime_ptg,
    }


# ==========
# PROCESSING
# ==========

# Process bench data

df_native_bench = load_bench_data(arguments.native_bench_data, process_bench_data)
df_native_bench = df_native_bench.rename(columns={"time_ns": "native_time_ns"})
df_vm_bench = load_bench_data(arguments.vm_bench_data, process_bench_data)
df_vm_bench = df_vm_bench.rename(columns={"time_ns": "vm_time_ns"})

df_speedup = (
    df_native_bench.merge(df_vm_bench, on=["tx_hash"])
    .apply(process_speedup, axis=1)
    .apply(pd.Series)
)

# Process Syscall Percentage

df_profiles = load_json_dir(
    arguments.libfunc_profiling_info, process_libfunc_profiles_fn
)
df_profiles_by_block = (
    df_profiles.groupby(["block_number", "tx_hash"], as_index=False)
    .agg(
        libfunc_total_time=("libfunc_total_time", "sum"),
        runtime_total_time=("runtime_total_time", "sum"),
    )
    .apply(process_runtime_ptg, axis=1)
    .apply(pd.Series)
)
print(df_profiles_by_block)

df_speedup_runtime = df_profiles_by_block.merge(df_speedup, on=["tx_hash"]).apply(
    pd.Series
)


# ========
# Plotting
# ========

block_range = f"{df_profiles['block_number'].min()}-{df_profiles['block_number'].max()}"

figure, ax = plt.subplots(figsize=(15, 15))

sns.regplot(data=df_speedup_runtime, x="speedup", y="runtime_ptg")

ax.set_xlabel("Seepdup")
ax.set_ylabel("Runtime (%)")
ax.set_title("Runtime Heavy Txs Composition")
save_to_path(f"runtime_ptg_speedup_corr-{block_range}")
