import glob

import itertools
import pathlib

import matplotlib.pyplot as plt
import pandas as pd
import seaborn as sns

enabled_plots = (
    "Correlations Matrix",
    "Individual Correlations",
    "Sierra Libfunc Pie",
    "LLVM Instruction Pie",
    "MLIR by Libfunc Pie",
)

SAMPLE_LIGHT_CONTRACT = "a"
SAMPLE_HEAVY_CONTRACT = "b"

sns.set_theme()

stat_files = glob.glob("compiled_programs/*.stats.json")
stats = []
for stat_file in stat_files:
    class_hash = pathlib.Path(stat_file).name.removesuffix(".stats.json")
    stat = pd.read_json(stat_file, typ="series")
    stat["hash"] = class_hash
    stats.append(stat)

df = pd.DataFrame(stats).set_index("hash")


def group_small_entries(entries, cutoff):
    new_entries = {}
    for key, group in itertools.groupby(
        entries, lambda k: "others" if (entries[k] < cutoff) else k
    ):
        new_entries[key] = sum([entries[k] for k in list(group)])
    return new_entries


def plot_pie(light_contract, heavy_contract, attribute, title):
    sns.set_style("whitegrid")
    fig, (ax1, ax2) = plt.subplots(1, 2)
    fig.suptitle(title)

    light_libfuncs = light_contract[attribute]
    heavy_libfuncs = heavy_contract[attribute]

    cutoff = sum(light_libfuncs.values()) * 0.01
    light_libfuncs = group_small_entries(light_libfuncs, cutoff)
    ax1.pie(
        light_libfuncs.values(),
        labels=light_libfuncs.keys(),
    )
    ax1.set_title("Light Contract")

    cutoff = sum(heavy_libfuncs.values()) * 0.01
    heavy_libfuncs = group_small_entries(heavy_libfuncs, cutoff)
    ax2.pie(
        heavy_libfuncs.values(),
        labels=heavy_libfuncs.keys(),
    )
    ax2.set_title("Heavy Contract")

    sns.set_theme()


heavy_contract = df.loc[SAMPLE_HEAVY_CONTRACT]
light_contract = df.loc[SAMPLE_LIGHT_CONTRACT]

if "MLIR by Libfunc Pie" in enabled_plots:
    plot_pie(
        light_contract,
        heavy_contract,
        "mlir_operations_by_libfunc",
        "MLIR by Libfunc Pie",
    )

if "LLVM Instruction Pie" in enabled_plots:
    plot_pie(
        light_contract,
        heavy_contract,
        "llvmir_opcode_frequency",
        "LLVM Instruction Pie",
    )


if "Sierra Libfunc Pie" in enabled_plots:
    plot_pie(
        light_contract, heavy_contract, "sierra_libfunc_frequency", "Sierra Libfunc Pie"
    )

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

if "Correlations Matrix" in enabled_plots:
    fig, ax = plt.subplots()
    fig.suptitle("Correlations Matrix")

    df_corr = df.corr(numeric_only=True)
    sns.heatmap(df_corr, ax=ax)

plt.show()
