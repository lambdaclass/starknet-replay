import glob

import itertools
import pathlib

import matplotlib.pyplot as plt
import pandas as pd
import seaborn as sns

enabled_plots = (
    "Correlations Matrix",
    "Individual Correlations",
    "Libfunc Pie",
)

stat_files = glob.glob("compiled_programs/*.stats.json")

stats = []
for stat_file in stat_files:
    class_hash = pathlib.Path(stat_file).name.removesuffix(".stats.json")
    stat = pd.read_json(stat_file, typ="series")
    stat["hash"] = class_hash
    stats.append(stat)

df = pd.DataFrame(stats).set_index("hash")

if "Correlations Matrix" in enabled_plots:
    fig, ax = plt.subplots()
    fig.suptitle("Correlations Matrix")

    df_corr = df.corr(numeric_only=True)
    sns.heatmap(df_corr, ax=ax)

if "Individual Correlations" in enabled_plots:
    fig, ((ax1, ax2), (ax3, ax4)) = plt.subplots(2, 2)
    fig.suptitle("Individual Correlations")

    # HIGH CORRELATION BETWEEN
    # - sierra_statement_count
    # - mlir_operation_count
    # - llvmir_instruction_count
    # - llvmir_virtual_register_count
    sns.regplot(df, ax=ax1, x="sierra_statement_count", y="mlir_operation_count")
    ax1.set_title("Sierra Size vs. MLIR Size")

    sns.regplot(
        df, ax=ax2, x="sierra_statement_count", y="llvmir_virtual_register_count"
    )
    ax2.set_title("Sierra Size vs. LLVM Virtual Registers")

    # HIGH CORRELATION BETWEEN
    # - compilation_total_time_ms
    # - compilation_llvm_passes_time_ms
    # - compilation_llvm_to_object_time_ms
    # - object_size_bytes
    sns.regplot(df, ax=ax3, x="compilation_total_time_ms", y="object_size_bytes")
    ax3.set_title("Compilation Time vs. Object Size")

    # LOW CORRELATION BETWEEN BOTH GROUPS
    # - sierra_statement_count
    # - compilation_total_time_ms
    sns.regplot(df, ax=ax4, x="sierra_statement_count", y="compilation_total_time_ms")
    ax4.set_title("Sierra Size vs. Compilation Time")


def group_small_entries(entries, cutoff):
    new_entries = {}
    for key, group in itertools.groupby(
        entries, lambda k: "others" if (entries[k] < cutoff) else k
    ):
        new_entries[key] = sum([entries[k] for k in list(group)])
    return new_entries


if "Libfunc Pie" in enabled_plots:
    sns.set_style("whitegrid")
    fig, (ax1, ax2) = plt.subplots(1, 2)
    fig.suptitle("Libfunc Pie")

    heavy_contract = df.loc["0x45"]
    light_contract = df.loc[
        "0x7a42b40dbcfcbb8daf4741a54b2ce3880c773f65f347a01df2b02d8aae2ce33"
    ]
    heavy_libfuncs = heavy_contract["sierra_libfunc_frequency"]
    light_libfuncs = light_contract["sierra_libfunc_frequency"]

    cutoff = sum(heavy_libfuncs.values()) * 0.01
    heavy_libfuncs = group_small_entries(heavy_libfuncs, cutoff)
    ax1.pie(
        heavy_libfuncs.values(),
        labels=heavy_libfuncs.keys(),
    )
    ax1.set_title("Heavy Contract")

    cutoff = sum(light_libfuncs.values()) * 0.01
    light_libfuncs = group_small_entries(light_libfuncs, cutoff)
    ax2.pie(
        light_libfuncs.values(),
        labels=light_libfuncs.keys(),
    )
    ax2.set_title("Light Contract")

    sns.set_theme()

plt.show()
