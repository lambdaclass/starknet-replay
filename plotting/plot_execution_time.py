import json
import pathlib
import argparse
from argparse import ArgumentParser

import matplotlib.pyplot as plt
import pandas as pd
import seaborn as sns
import numpy as np
import scipy

from pandas import DataFrame

arg_parser = ArgumentParser("Stress Test Plotter")
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


def save(name):
    if args.output:
        figure_name = f"{args.output}-{name}.svg"
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
df_txs = pd.merge(
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
# first_call         int64
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
df_calls = pd.concat([df_calls_native, df_calls_vm])
# merge steps into gas_consumed
df_calls["gas_consumed"] += df_calls["steps"] * 100
df_calls = df_calls.drop("steps", axis=1)

# print(df_calls.info())
# -------------------
# Column        Dtype
# -------------------
# class_hash    object
# selector      object
# time_ns       int64
# gas_consumed  int64
# executor      object

############
# PLOTTING #
############


def plot_calls_by_class_hash(df_calls: DataFrame):
    df: DataFrame = (
        df_calls.groupby(["executor", "class_hash"])
        .aggregate(mean_time=("time_ns", "mean"))
        .unstack("executor")
    )  # type: ignore

    # flatten multi index
    df.columns = df.columns.map("_".join)

    # drop rows for which we don't have any Native samples
    df = df.dropna(axis=0, subset=[("mean_time_native"), ("mean_time_vm")])

    # sort so that the legend doesn't cover the bars
    df.sort_values(["mean_time_vm"], ascending=[False], inplace=True)

    df["speedup"] = df["mean_time_vm"] / df["mean_time_native"]

    # print(df.describe())
    # ------------------------------------------------
    #        mean_time_native  mean_time_vm    speedup
    # mean       3.798728e+05  3.747944e+06  13.183916
    # std        6.406297e+05  1.429918e+07  16.153037
    # min        5.104050e+03  5.758320e+04   1.260679
    # max        2.209098e+06  8.232545e+07  66.898440

    _, (ax1, ax2) = plt.subplots(1, 2)
    sns.barplot(
        ax=ax1,
        y="class_hash",
        x="mean_time_vm",
        data=df,
        formatter=format_hash,
        label="VM Execution Time",
        color="r",
        alpha=0.75,
    )
    sns.barplot(
        ax=ax1,
        y="class_hash",
        x="mean_time_native",
        data=df,
        formatter=format_hash,
        label="Native Execution Time",
        color="b",
        alpha=0.75,
    )
    ax1.set_xscale("log", base=2)
    ax1.set_title("Mean time by Contract Class")

    sns.barplot(
        ax=ax2,
        y="class_hash",
        x="speedup",
        data=df,
        formatter=format_hash,
        label="Speedup",
        color="b",
        alpha=0.75,
    )
    ax2.set_title("Speedup by Contract Class")

    save("calls_by_class_hash")


def plot_tx_speedup(df_txs: DataFrame):
    _, ax = plt.subplots()
    sns.violinplot(ax=ax, data=df_txs, x="speedup")
    ax.set_title("Speedup by Transaction")

    total_speedup = df_txs["time_ns_vm"].sum() / df_txs["time_ns_native"].sum()
    mean_speedup = df_txs["speedup"].mean()

    ax.text(
        0.01,
        0.99,
        f"Total speedup: {total_speedup:.2f}\nMean speedup: {mean_speedup:.2f}",
        transform=ax.transAxes,
        fontsize=12,
        verticalalignment="top",
        horizontalalignment="left",
    )

    save("tx-speedup")


def plot_calls_by_gas_usage(df_calls: DataFrame):
    _, ax = plt.subplots()

    df_native = df_calls.loc[df_calls["executor"] == "native"]
    df_vm = df_calls.loc[df_calls["executor"] == "vm"]

    # remove outliers
    df_native = df_native[np.abs(scipy.stats.zscore(df_native["time_ns"])) < 3]
    df_vm = df_vm[np.abs(scipy.stats.zscore(df_vm["time_ns"])) < 3]
    df_native = df_native[np.abs(scipy.stats.zscore(df_native["gas_consumed"])) < 3]
    df_vm = df_vm[np.abs(scipy.stats.zscore(df_vm["gas_consumed"])) < 3]

    sns.regplot(data=df_native, x="gas_consumed", y="time_ns")
    sns.regplot(data=df_vm, x="gas_consumed", y="time_ns")
    ax.set_title("Execution Time by Gas Usage")

    save("calls-by-gas-usage")


def plot_calls_by_gas_unit(df_calls):
    fig, (ax1, ax2) = plt.subplots(1, 2)

    df_calls["speed"] = df_calls["gas_consumed"] / df_calls["time_ns"]

    df_native = df_calls.loc[df_calls["executor"] == "native"]
    df_vm = df_calls.loc[df_calls["executor"] == "vm"]

    df_native_clean = df_native[np.abs(scipy.stats.zscore(df_native["speed"])) < 2]
    df_vm_clean = df_vm[np.abs(scipy.stats.zscore(df_vm["speed"])) < 2]
    sns.violinplot(
        ax=ax1,
        data=df_native_clean,
        x="speed",
    )
    ax1.set_title("Native Speed (gas/ns)")
    ax1.set_xlabel("Speed (gas/ns)")
    sns.violinplot(
        ax=ax2,
        data=df_vm_clean,
        x="speed",
    )
    ax2.set_title("VM Speed (gas/ns)")
    ax2.set_xlabel("Speed (gas/ns)")

    native_mean_speed = df_native["speed"].mean()
    vm_mean_speed = df_vm["speed"].mean()

    native_total_speed = df_native["gas_consumed"].sum() / df_native["time_ns"].sum()
    vm_total_speed = df_vm["gas_consumed"].sum() / df_vm["time_ns"].sum()

    ax1.text(
        0.01,
        0.99,
        f"Mean: {native_mean_speed:.2f}\nTotal: {native_total_speed:.2f}",
        transform=ax1.transAxes,
        fontsize=12,
        verticalalignment="top",
        horizontalalignment="left",
    )
    ax2.text(
        0.01,
        0.99,
        f"Mean: {vm_mean_speed:.2f}\nTotal: {vm_total_speed:.2f}",
        transform=ax2.transAxes,
        fontsize=12,
        verticalalignment="top",
        horizontalalignment="left",
    )

    fig.suptitle("Speed by Call")
    save("speed-by-call")


def plot_txs_by_gas_unit(df_txs):
    fig, (ax1, ax2) = plt.subplots(1, 2)

    df_txs["speed_native"] = df_txs["gas_consumed"] / df_txs["time_ns_native"]
    df_txs["speed_vm"] = df_txs["gas_consumed"] / df_txs["time_ns_vm"]

    sns.violinplot(
        ax=ax1,
        data=df_txs,
        x="speed_native",
    )
    ax1.set_title("Native Speed (gas/ns)")
    ax1.set_xlabel("Speed (gas/ns)")
    sns.violinplot(
        ax=ax2,
        data=df_txs,
        x="speed_vm",
    )
    ax2.set_title("VM Speed (gas/ns)")
    ax2.set_xlabel("Speed (gas/ns)")

    native_mean_speed = df_txs["speed_native"].mean()
    vm_mean_speed = df_txs["speed_vm"].mean()
    native_total_speed = df_txs["gas_consumed"].sum() / df_txs["time_ns_native"].sum()
    vm_total_speed = df_txs["gas_consumed"].sum() / df_txs["time_ns_vm"].sum()

    ax1.text(
        0.01,
        0.99,
        f"Mean: {native_mean_speed:.2f}\nTotal: {native_total_speed:.2f}",
        transform=ax1.transAxes,
        fontsize=12,
        verticalalignment="top",
        horizontalalignment="left",
    )
    ax2.text(
        0.01,
        0.99,
        f"Mean: {vm_mean_speed:.2f}\nTotal: {vm_total_speed:.2f}",
        transform=ax2.transAxes,
        fontsize=12,
        verticalalignment="top",
        horizontalalignment="left",
    )

    fig.suptitle("Speed by Transaction")
    save("speed-by-tx")


def plot_block_speedup(df_txs: DataFrame):
    _, ax = plt.subplots()

    df_blocks: DataFrame = df_txs.groupby("block_number").aggregate(
        time_ns_native=("time_ns_native", "sum"),
        time_ns_vm=("time_ns_vm", "sum"),
        tx_speedup=("speedup", "mean"),
    )  # type: ignore

    df_blocks["block_speedup"] = df_blocks["time_ns_vm"] / df_blocks["time_ns_native"]

    total_speedup = df_blocks["time_ns_vm"].sum() / df_blocks["time_ns_native"].sum()

    melted = df_blocks[["tx_speedup", "block_speedup"]].melt(
        var_name="metric", value_name="value"
    )
    sns.violinplot(
        ax=ax,
        data=melted,
        x="value",
        hue="metric",
    )
    ax.set_title("Speedup by Block")
    ax.text(
        0.01,
        0.99,
        f"Total: {total_speedup:.2f}",
        transform=ax.transAxes,
        fontsize=12,
        verticalalignment="top",
        horizontalalignment="left",
    )
    save("block-speedup")


plot_calls_by_class_hash(df_calls)
plot_tx_speedup(df_txs)
plot_calls_by_gas_usage(df_calls)
plot_calls_by_gas_unit(df_calls)
plot_txs_by_gas_unit(df_txs)
plot_block_speedup(df_txs)

if args.display:
    plt.show()
