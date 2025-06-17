import json
import pathlib
import argparse
import math
from argparse import ArgumentParser

import matplotlib.pyplot as plt
import matplotlib as mpl
import pandas as pd
import seaborn as sns
import numpy as np

from pandas import DataFrame

sns.set_palette("deep")
sns.set_color_codes("deep")

arg_parser = ArgumentParser()
arg_parser.add_argument("native_data")
arg_parser.add_argument("vm_data")
arg_parser.add_argument(
    "--output",
    type=pathlib.Path,
)
arg_parser.add_argument(
    "--display", action=argparse.BooleanOptionalAction, default=True
)
args = arg_parser.parse_args()

#############
# UTILITIES #
#############


def format_hash(class_hash):
    return f"{class_hash[:6]}..."


def save(name, ext="svg"):
    if args.output:
        figure_name = f"{args.output}-{name}.{ext}"
        plt.savefig(figure_name)


##############
# PROCESSING #
##############


def load_data(path):
    raw_json = json.load(open(path))

    df_txs = pd.DataFrame(raw_json["transactions"])
    df_calls = pd.DataFrame(raw_json["calls"])

    return df_txs, df_calls


df_txs_native, df_calls_native = load_data(args.native_data)
df_txs_vm, df_calls_vm = load_data(args.vm_data)

# Assert Native and VM tx execution coincide.
assert (df_txs_native.index == df_txs_vm.index).all()
assert (df_txs_native["hash"] == df_txs_vm["hash"]).all()
assert (df_txs_native["first_call"] == df_txs_vm["first_call"]).all()
assert (df_txs_native["gas_consumed"] == df_txs_vm["gas_consumed"]).all()
assert (df_txs_native["steps"] == df_txs_vm["steps"]).all()
assert (df_txs_native["block_number"] == df_txs_vm["block_number"]).all()

# Assert Native and VM call execution coincide.
assert (df_calls_native.index == df_calls_vm.index).all()
assert (df_calls_native["class_hash"] == df_calls_vm["class_hash"]).all()
assert (df_calls_native["selector"] == df_calls_vm["selector"]).all()
assert (df_calls_native["gas_consumed"] == df_calls_vm["gas_consumed"]).all()
assert (df_calls_native["steps"] == df_calls_vm["steps"]).all()

# merge transactions into single dataframe
df_txs: DataFrame = pd.merge(
    df_txs_native,
    df_txs_vm.drop(
        ["hash", "first_call", "gas_consumed", "steps", "block_number"], axis=1
    ),
    left_index=True,
    right_index=True,
    suffixes=("_native", "_vm"),
)
# merge steps into gas_consumed
df_txs["gas_consumed"] += df_txs["steps"] * 100
df_txs = df_txs.drop("steps", axis=1)
# calculate speedup
df_txs["speedup"] = df_txs["time_ns_vm"] / df_txs["time_ns_native"]

# print(df_txs.info())
# -------------------------
# Column              Dtype
# -------------------------
# hash                object
# gas_consumed        int64
# first_call          int64
# block_number        int64
# time_ns_native      int64
# time_ns_vm          int64
# speedup             float64

# use resource to determine executor
df_calls_native.replace("SierraGas", "native", inplace=True)
df_calls_native.replace("CairoSteps", "vm", inplace=True)
df_calls_native.rename(columns={"resource": "executor"}, inplace=True)
df_calls_vm.rename(columns={"resource": "executor"}, inplace=True)
df_calls_vm["executor"] = "vm"
# merge calls into single dataframe
df_calls: DataFrame = pd.concat([df_calls_native, df_calls_vm])
# drop calls with no time
df_calls = df_calls[df_calls["time_ns"] != 0]  # type: ignore
# merge steps into gas_consumed
df_calls["gas_consumed"] += df_calls["steps"] * 100
df_calls = df_calls.drop("steps", axis=1)
df_calls["speed"] = df_calls["gas_consumed"] / df_calls["time_ns"]

# print(df_calls.info())
# -------------------
# Column        Dtype
# -------------------
# class_hash    object
# selector      object
# time_ns       int64
# gas_consumed  int64
# executor      object
# speed         float64


def separate_gas_and_sierra_time(tx, native_calls):
    first_call = int(tx["first_call"])
    next_first_call = tx["next_first_call"]

    if math.isnan(next_first_call):
        calls = native_calls.iloc[first_call:]
    else:
        next_first_call = int(next_first_call)
        calls = native_calls.iloc[first_call:next_first_call]

    gas_calls: DataFrame = calls[calls["executor"] == "native"]  # type: ignore
    sierra_calls: DataFrame = calls[calls["executor"] == "vm"]  # type: ignore

    time_gas = gas_calls["time_ns"].sum()
    time_sierra = sierra_calls["time_ns"].sum()

    tx["time_ns_native_gas"] = time_gas
    tx["time_ns_native_sierra"] = time_sierra
    tx["time_ns_native_ratio"] = time_gas / (time_gas + time_sierra)

    return tx


df_txs["next_first_call"] = df_txs["first_call"].shift(-1)
df_txs = df_txs.apply(
    lambda tx: separate_gas_and_sierra_time(tx, df_calls_native), axis=1
)  # type: ignore
df_txs = df_txs.drop("next_first_call", axis=1)

# print(df_txs.info())
# ----------------------------
# Column                 Dtype
# ----------------------------
# hash                   object
# gas_consumed           int64
# first_call             int64
# block_number           int64
# time_ns_native         int64
# time_ns_vm             int64
# speedup                float64
# time_ns_native_gas     int64
# time_ns_native_sierra  int64

############
# PLOTTING #
############


def plot_speedup(df_txs: DataFrame):
    _, ax = plt.subplots()

    sns.boxplot(ax=ax, data=df_txs, x="speedup", showfliers=False, width=0.5)
    ax.set_xlabel("Tx Speedup Ratio")
    ax.set_title("Speedup Distribution")

    total_speedup = df_txs["time_ns_vm"].sum() / df_txs["time_ns_native"].sum()
    mean_speedup = df_txs["speedup"].mean()
    median_speedup = df_txs["speedup"].quantile(0.5)
    stddev_speedup = df_txs["speedup"].std()

    ax.text(
        0.01,
        0.99,
        "\n".join(
            [
                f"Total Execution Speedup: {total_speedup:.2f}",
                f"Mean: {mean_speedup:.2f}",
                f"Median: {median_speedup:.2f}",
                f"Std Dev: {stddev_speedup:.2f}",
            ]
        ),
        transform=ax.transAxes,
        fontsize=12,
        verticalalignment="top",
        horizontalalignment="left",
    )

    save("speedup")


def plot_time_by_class(df_calls: DataFrame):
    df: DataFrame = (
        df_calls.groupby(["executor", "class_hash"])
        .aggregate(
            mean_time=("time_ns", "mean"),
            total_time=("time_ns", "mean"),
        )
        .unstack("executor")
    )  # type: ignore

    # flatten multi index
    df.columns = df.columns.map("_".join)

    # drop rows for which we don't have any Native samples
    df = df.dropna(axis=0, subset=[("mean_time_native"), ("mean_time_vm")])

    # sort so that the legend doesn't cover the bars
    df = df.nlargest(columns="total_time_vm", n=40)
    df.sort_values(["mean_time_vm"], ascending=[False], inplace=True)

    df["speedup"] = df["mean_time_vm"] / df["mean_time_native"]

    _, (ax1, ax2) = plt.subplots(1, 2)
    sns.barplot(
        ax=ax1,
        y="class_hash",
        x="mean_time_vm",
        data=df,
        formatter=format_hash,
        label="VM Execution Time",
        color="r",
    )
    sns.barplot(
        ax=ax1,
        y="class_hash",
        x="mean_time_native",
        data=df,
        formatter=format_hash,
        label="Native Execution Time",
        color="b",
    )
    ax1.set_xscale("log", base=2)
    ax1.set_xlabel("Mean Time (ns)")
    ax1.set_ylabel("Class Hash")
    ax1.set_title("Mean time by Contract Class")

    sns.barplot(
        ax=ax2,
        y="class_hash",
        x="speedup",
        data=df,
        formatter=format_hash,
        label="Speedup",
        color="b",
    )
    ax2.set_title("Speedup by Contract Class")
    ax2.set_ylabel("")
    ax2.set_xscale("log", base=2)
    ax2.set_xlabel("Speedup Ratio")

    save("time-by-class")


def plot_time_by_gas(df_calls: DataFrame):
    _, ax = plt.subplots()

    df_native = df_calls.loc[df_calls["executor"] == "native"]
    df_vm = df_calls.loc[df_calls["executor"] == "vm"]

    native_gas_consumed = df_native["gas_consumed"]
    native_time_ns = df_native["time_ns"]
    vm_gas_consumed = df_vm["gas_consumed"]
    vm_time_ns = df_vm["time_ns"]

    # The range is to wide, so we apply log to see the full range.
    native_gas_consumed = np.log10(native_gas_consumed)
    native_time_ns = np.log10(native_time_ns)
    vm_gas_consumed = np.log10(vm_gas_consumed)
    vm_time_ns = np.log10(vm_time_ns)

    sns.histplot(
        ax=ax,
        x=native_gas_consumed,
        y=native_time_ns,
        color="b",
        binwidth=1 / 8,
    )
    sns.regplot(
        ax=ax,
        x=native_gas_consumed,
        y=native_time_ns,
        scatter=False,
        color="b",
        label="Native",
    )
    sns.histplot(
        ax=ax,
        x=vm_gas_consumed,
        y=vm_time_ns,
        color="r",
        binwidth=1 / 8,
    )
    sns.regplot(
        ax=ax,
        x=vm_gas_consumed,
        y=vm_time_ns,
        scatter=False,
        color="r",
        label="VM",
    )

    ax.legend()

    # Format the axis to show the normal values, not the log ones.
    def unlog10(x, _):
        return f"{10**x:.0e}"

    ax.set_xlabel("Gas Consumed")
    ax.set_ylabel("Execution Time (ns)")
    ax.get_xaxis().set_major_formatter(unlog10)
    ax.get_yaxis().set_major_formatter(unlog10)

    ax.set_title("Execution Time by Gas Usage")

    save("time-by-gas")


def plot_speed(df_calls):
    fig, (ax1, ax2) = plt.subplots(1, 2)

    df_native = df_calls.loc[df_calls["executor"] == "native"]
    df_vm = df_calls.loc[df_calls["executor"] == "vm"]

    sns.boxplot(ax=ax1, data=df_native, x="speed", showfliers=False, width=0.5)
    ax1.set_title("Native Speed (gas/ns)")
    ax1.set_xlabel("Speed (gas/ns)")

    sns.boxplot(ax=ax2, data=df_vm, x="speed", showfliers=False, width=0.5)
    ax2.set_title("VM Speed (gas/ns)")
    ax2.set_xlabel("Speed (gas/ns)")

    native_total_speed = df_native["gas_consumed"].sum() / df_native["time_ns"].sum()
    native_mean_speed = df_native["speed"].mean()
    native_median_speed = df_native["speed"].quantile(0.5)
    native_stddev_speed = df_native["speed"].std()

    vm_total_speed = df_vm["gas_consumed"].sum() / df_vm["time_ns"].sum()
    vm_mean_speed = df_vm["speed"].mean()
    vm_median_speed = df_vm["speed"].quantile(0.5)
    vm_stddev_speed = df_vm["speed"].std()

    ax1.text(
        0.01,
        0.99,
        "\n".join(
            [
                f"Total Execution Speed: {native_total_speed:.2f}",
                f"Mean: {native_mean_speed:.2f}",
                f"Median: {native_median_speed:.2f}",
                f"Std Dev: {native_stddev_speed:.2f}",
            ]
        ),
        transform=ax1.transAxes,
        fontsize=12,
        verticalalignment="top",
        horizontalalignment="left",
    )
    ax2.text(
        0.01,
        0.99,
        "\n".join(
            [
                f"Total Execution Speed: {vm_total_speed:.2f}",
                f"Mean: {vm_mean_speed:.2f}",
                f"Median: {vm_median_speed:.2f}",
                f"Std Dev: {vm_stddev_speed:.2f}",
            ]
        ),
        transform=ax2.transAxes,
        fontsize=12,
        verticalalignment="top",
        horizontalalignment="left",
    )

    fig.suptitle("Speed by Contract Call")
    save("speed")


mpl.rcParams["figure.figsize"] = [16 * 0.8, 9 * 0.8]


def plot_executors(df_txs: DataFrame):
    fig, (ax1, ax2) = plt.subplots(1, 2)

    df_txs_mixed: DataFrame = df_txs[df_txs["time_ns_native_ratio"] != 1]  # type: ignore
    sns.scatterplot(ax=ax1, data=df_txs_mixed, x="time_ns_native_ratio", y="speedup")
    ax1.set_ylabel("Speedup")
    ax1.set_xlabel("Native/VM Ratio")
    ax1.set_title("Non Pure Native Executions")

    df_txs_only_gas: DataFrame = df_txs[df_txs["time_ns_native_ratio"] == 1]  # type: ignore
    sns.boxplot(ax=ax2, data=df_txs_only_gas, y="speedup", showfliers=True)
    ax2.set_ylabel("Speedup")
    ax2.set_title("Pure Native Executions")

    if args.output:
        df_txs_only_gas.sort_values("speedup").to_csv(f"{args.output}-executors.csv")
    save("executors")


# plot_speed(df_calls)
# plot_time_by_gas(df_calls)
# plot_time_by_class(df_calls)
# plot_speedup(df_txs)
plot_executors(df_txs)

if args.display:
    plt.show()
