from yattag import Doc

import json
import pathlib
import argparse
import re
import inflection
from argparse import ArgumentParser

import matplotlib.pyplot as plt
import matplotlib as mpl
import pandas as pd
import seaborn as sns
import numpy as np

from typing import NamedTuple
from pandas import DataFrame

sns.set_palette("deep")
sns.set_color_codes("deep")
mpl.rcParams["figure.figsize"] = [16 * 0.8, 9 * 0.8]


class Args(NamedTuple):
    native_data: pathlib.Path
    vm_data: pathlib.Path
    output_dir: pathlib.Path
    display: bool


arg_parser = ArgumentParser()
arg_parser.add_argument("native_data", type=pathlib.Path)
arg_parser.add_argument("vm_data", type=pathlib.Path)
arg_parser.add_argument("--output-dir", type=pathlib.Path)
arg_parser.add_argument(
    "--display", action=argparse.BooleanOptionalAction, default=True
)


args: Args = arg_parser.parse_args()  # type: ignore


#############
# UTILITIES #
#############


def format_hash(class_hash):
    return f"{class_hash[:6]}..."


# A list of all the figures generated. Used to generate a final report.
OUTPUT_FIGURES = []


# Saves the current figure to the output directory, deriving the file name from
# the given title. Adds the figure data to `OUTPUT_FIGURES`, which can then be
# used to generate a report with all the figures.
def save_figure(title, description=""):
    if args.output_dir:
        stem = inflection.parameterize(title)
        name = f"{stem}.svg"
        OUTPUT_FIGURES.append(
            {
                title: title,
                name: name,
                description: description,
            }
        )
        plt.savefig(args.output_dir.joinpath(name))


# Saves the current dataframe to the output directory, deriving the file name
# from the given title.
def save_csv(data, title, *to_csv_args, **to_csv_kwargs):
    if args.output_dir:
        stem = inflection.parameterize(title)
        name = f"{stem}.csv"
        data.to_csv(args.output_dir.joinpath(name), *to_csv_args, **to_csv_kwargs)


# Given an info series, and the name of the field containing a Rust version,
# it parses the version string and shortens it. From example, converts from the
# full git url, to just the commit hash.
def parse_version(info: pd.Series, name: str):
    version_string: str = info[name]  # type: ignore
    match = re.search("rev=([a-z0-9]+)", version_string)
    if match:
        info[name] = match.group(1)


##############
# PROCESSING #
##############


# Loads the JSON raw data, and returns three elements:
# - Transaction dataframe.
# - Contract call dataframe.
# - Benchmark info series.
def load_data(path):
    raw_json = json.load(open(path))

    df_txs: DataFrame = pd.DataFrame(raw_json["transactions"])[
        [
            "hash",
            "block_number",
            "time_ns",
            "gas_consumed",
            "steps",
            "first_call",
        ]
    ].rename({"hash": "tx_hash"}, axis=1)  # type: ignore

    df_calls: DataFrame = pd.DataFrame(raw_json["calls"])[
        [
            "class_hash",
            "selector",
            "resource",
            "time_ns",
            "gas_consumed",
            "steps",
        ]
    ]  # type: ignore

    df_txs["gas_consumed"] += df_txs["steps"] * 100
    df_txs = df_txs.drop("steps", axis=1)
    df_calls["gas_consumed"] += df_calls["steps"] * 100
    df_calls = df_calls.drop("steps", axis=1)

    df_calls = pd.merge_asof(
        left=df_calls,
        right=df_txs[["tx_hash", "first_call"]],
        left_index=True,
        right_on="first_call",
        direction="backward",
    )
    df_txs = df_txs.drop(["first_call"], axis=1)
    df_calls = df_calls.drop(["first_call"], axis=1)

    df_txs = df_txs.groupby("tx_hash").aggregate(
        **{
            "block_number": ("block_number", "first"),
            "gas_consumed": ("gas_consumed", "first"),
            "block_number_nunique": ("block_number", "nunique"),
            "gas_consumed_nunique": ("gas_consumed", "nunique"),
            "time_ns": ("time_ns", "mean"),
        }
    )  # type: ignore
    assert (df_txs["block_number_nunique"] == 1).all()
    assert (df_txs["gas_consumed_nunique"] == 1).all()
    df_txs = df_txs.drop(["block_number_nunique", "gas_consumed_nunique"], axis=1)

    call_tx_ids = (df_calls["tx_hash"] != df_calls["tx_hash"].shift()).cumsum()
    df_calls["call_index"] = df_calls.groupby(call_tx_ids).cumcount()
    df_calls = df_calls.groupby(["tx_hash", "call_index"]).aggregate(
        **{
            "class_hash": ("class_hash", "first"),
            "selector": ("selector", "first"),
            "resource": ("resource", "first"),
            "gas_consumed": ("gas_consumed", "first"),
            "class_hash_nunique": ("class_hash", "nunique"),
            "selector_nunique": ("selector", "nunique"),
            "resource_nunique": ("resource", "nunique"),
            "gas_consumed_nunique": ("gas_consumed", "nunique"),
            "time_ns": ("time_ns", "mean"),
        }
    )  # type: ignore
    assert (df_calls["class_hash_nunique"] == 1).all()
    assert (df_calls["selector_nunique"] == 1).all()
    assert (df_calls["resource_nunique"] == 1).all()
    assert (df_calls["gas_consumed_nunique"] == 1).all()
    df_calls = df_calls.drop(
        [
            "class_hash_nunique",
            "selector_nunique",
            "resource_nunique",
            "gas_consumed_nunique",
        ],
        axis=1,
    )

    df_calls["throughput"] = df_calls["gas_consumed"] / df_calls["time_ns"]
    df_txs["throughput"] = df_txs["gas_consumed"] / df_txs["time_ns"]

    time_by_gas = (
        df_calls[df_calls["resource"] == "SierraGas"]
        .groupby("tx_hash")["time_ns"]
        .sum()
    )
    time_by_steps = (
        df_calls[df_calls["resource"] == "CairoSteps"]
        .groupby("tx_hash")["time_ns"]
        .sum()
    )
    df_txs["time_ns_gas"] = time_by_gas.reindex(df_txs.index).fillna(0)
    df_txs["time_ns_steps"] = time_by_steps.reindex(df_txs.index).fillna(0)
    df_txs["resource_ratio"] = df_txs["time_ns_gas"] / (
        df_txs["time_ns_steps"] + df_txs["time_ns_gas"]
    )

    df_txs = df_txs.reset_index()
    df_calls = df_calls.reset_index()

    # print(df_txs.info())
    # ---------------------
    # Column          Dtype
    # ---------------------
    # tx_hash         object
    # block_number    int64
    # gas_consumed    int64
    # time_ns         float64
    # time_ns_gas     float64
    # time_ns_steps   float64
    # resource_ratio  float64
    # throughput      float64

    # print(df_calls.info())
    # -------------------
    # Column        Dtype
    # -------------------
    # tx_hash       object
    # call_index    int64
    # class_hash    object
    # selector      object
    # resource      object
    # gas_consumed  int64
    # time_ns       float64
    # throughput    float64

    info = pd.Series(raw_json["info"])
    parse_version(info, "cairo_native_version")
    parse_version(info, "sequencer_version")
    info["memory"] = round(int(info["memory"]) / 2**30, 2)
    info.rename(
        {
            "date": "Date",
            "block_start": "Block Start",
            "block_end": "Block End",
            "net": "Net",
            "laps": "Laps",
            "mode": "Mode",
            "native_profile": "Native Profile",
            "rust_profile": "Rust Profile",
            "cairo_native_version": "Cairo Native Version",
            "sequencer_version": "Sequencer Version",
            "os": "OS",
            "arch": "Arch",
            "cpu": "CPU",
            "memory": "Memory (GiB)",
        },
        inplace=True,
    )

    return df_txs, df_calls, info


df_txs_native, df_calls_native, native_info = load_data(args.native_data)
df_txs_vm, df_calls_vm, vm_info = load_data(args.vm_data)

# Assert Native and VM tx execution coincide.
assert (df_txs_native["tx_hash"] == df_txs_vm["tx_hash"]).all()
assert (df_txs_native["block_number"] == df_txs_vm["block_number"]).all()
assert (df_txs_native["gas_consumed"] == df_txs_vm["gas_consumed"]).all()

# Assert Native and VM call execution coincide.
assert (df_calls_native["tx_hash"] == df_calls_vm["tx_hash"]).all()
assert (df_calls_native["call_index"] == df_calls_vm["call_index"]).all()
assert (df_calls_native["class_hash"] == df_calls_vm["class_hash"]).all()
assert (df_calls_native["selector"] == df_calls_vm["selector"]).all()
assert (df_calls_native["resource"] == df_calls_vm["resource"]).all()
assert (df_calls_native["gas_consumed"] == df_calls_vm["gas_consumed"]).all()

# merge transactions into single dataframe
df_txs: DataFrame = pd.merge(
    df_txs_native,
    df_txs_vm[
        [
            "tx_hash",
            "time_ns",
            "time_ns_gas",
            "time_ns_steps",
            "throughput",
        ]
    ],
    on="tx_hash",
    suffixes=("_native", "_vm"),
)

df_txs["speedup"] = df_txs["time_ns_vm"] / df_txs["time_ns_native"]

# print(df_txs.info())
# ---------------------------
# Column                Dtype
# ---------------------------
# tx_hash               object
# block_number          int64
# gas_consumed          int64
# resource_ratio        float64
# time_ns_native        float64
# time_ns_gas_native    float64
# time_ns_steps_native  float64
# time_ns_vm            float64
# time_ns_gas_vm        float64
# time_ns_steps_vm      float64
# speedup               float64

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

# print(df_calls.info())
# -------------------
# Column        Dtype
# -------------------
# tx_hash       object
# call_index    int64
# class_hash    object
# selector      object
# executor      object
# gas_consumed  int64
# time_ns       float64
# throughput    float64


############
# PLOTTING #
############


def plot_tx_speedup(df_txs: DataFrame):
    _, ax = plt.subplots()

    sns.boxplot(ax=ax, data=df_txs, x="speedup", showfliers=False, width=0.5)
    ax.set_xlabel("Tx Speedup Ratio")
    ax.set_title("Tx Speedup Distribution")

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

    save_figure(
        "Tx Speedup Distribution",
        "Calculates the distribution of speedup by transactions.",
    )


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

    save_figure(
        "Execution Time by Contract Class",
        "Compares execution time of most common contract classes.",
    )


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

    save_figure(
        "Execution Time by Gas Usage",
        "Correlates call execution time with gas usage. The scale is in log-log.",
    )


def plot_call_throughput(df_calls):
    fig, (ax1, ax2) = plt.subplots(1, 2)

    df_native = df_calls.loc[df_calls["executor"] == "native"]
    df_vm = df_calls.loc[df_calls["executor"] == "vm"]

    sns.boxplot(ax=ax1, data=df_native, x="throughput", showfliers=False, width=0.5)
    ax1.set_title("Native Throughput (gas/ns)")
    ax1.set_xlabel("Throughput (gas/ns)")

    sns.boxplot(ax=ax2, data=df_vm, x="throughput", showfliers=False, width=0.5)
    ax2.set_title("VM Throughput (gas/ns)")
    ax2.set_xlabel("Throughput (gas/ns)")

    native_total_throughput = (
        df_native["gas_consumed"].sum() / df_native["time_ns"].sum()
    )
    native_mean_throughput = df_native["throughput"].mean()
    native_median_throughput = df_native["throughput"].quantile(0.5)
    native_stddev_throughput = df_native["throughput"].std()

    vm_total_throughput = df_vm["gas_consumed"].sum() / df_vm["time_ns"].sum()
    vm_mean_throughput = df_vm["throughput"].mean()
    vm_median_throughput = df_vm["throughput"].quantile(0.5)
    vm_stddev_throughput = df_vm["throughput"].std()

    ax1.text(
        0.01,
        0.99,
        "\n".join(
            [
                f"Total Execution Throughput: {native_total_throughput:.2f}",
                f"Mean: {native_mean_throughput:.2f}",
                f"Median: {native_median_throughput:.2f}",
                f"Std Dev: {native_stddev_throughput:.2f}",
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
                f"Total Execution Throughput: {vm_total_throughput:.2f}",
                f"Mean: {vm_mean_throughput:.2f}",
                f"Median: {vm_median_throughput:.2f}",
                f"Std Dev: {vm_stddev_throughput:.2f}",
            ]
        ),
        transform=ax2.transAxes,
        fontsize=12,
        verticalalignment="top",
        horizontalalignment="left",
    )

    fig.suptitle("Call Throughput Distribution")

    save_figure(
        "Call Throughput Distribution",
        "Calculates the distribution of Native and VM throughput by contract calls.",
    )


def plot_pure_transactions(df_txs: DataFrame):
    fig, (ax1, ax2) = plt.subplots(1, 2)

    df_txs_mixed: DataFrame = df_txs[df_txs["resource_ratio"] != 1]  # type: ignore
    sns.scatterplot(ax=ax1, data=df_txs_mixed, x="resource_ratio", y="speedup")
    ax1.set_ylabel("Speedup")
    ax1.set_xlabel("Native/VM Ratio")
    ax1.set_title("Non Pure Native Executions")

    df_txs_only_gas: DataFrame = df_txs[df_txs["resource_ratio"] == 1]  # type: ignore
    sns.boxplot(ax=ax2, data=df_txs_only_gas, y="speedup", showfliers=False)
    ax2.set_ylabel("Speedup")
    ax2.set_title("Pure Native Executions")

    time_ns_native = df_txs["time_ns_native"].sum()
    time_ns_gas_native = df_txs["time_ns_gas_native"].sum()
    pure_native_ratio = time_ns_gas_native / time_ns_native
    ax1.text(
        0.01,
        0.99,
        "\n".join(
            [
                f"Pure Native Ratio: {pure_native_ratio * 100:.1f}%",
            ]
        ),
        transform=ax1.transAxes,
        fontsize=12,
        verticalalignment="top",
        horizontalalignment="left",
    )

    headers = ["tx_hash", "block_number", "speedup", "throughput_native"]
    pure_transactions: DataFrame = df_txs_only_gas[headers].sort_values("speedup")  # type: ignore
    save_csv(pure_transactions, "Pure Transactions", index=False)

    save_figure(
        "Pure Transactions",
        "Separates between pure (only Cairo Native) and non pure transactions, when running in Native mode.",
    )


def plot_block_speedup(df_txs: DataFrame):
    fig, ax = plt.subplots()

    df_blocks: DataFrame = df_txs.groupby("block_number").aggregate(
        **{
            "time_ns_native": ("time_ns_native", "sum"),
            "time_ns_vm": ("time_ns_vm", "sum"),
            "gas_consumed": ("gas_consumed", "sum"),
        }
    )  # type: ignore
    df_blocks["speedup"] = df_blocks["time_ns_vm"] / df_blocks["time_ns_native"]

    sns.boxplot(ax=ax, data=df_blocks, x="speedup", showfliers=False, width=0.5)
    ax.set_xlabel("Blocks Speedup Ratio")
    ax.set_title("Speedup Distribution")

    save_csv(df_blocks, "Blocks")
    save_figure(
        "Block Speedup Distribution",
        "Calculates the distribution of speedup by blocks.",
    )


args.output_dir.mkdir(parents=True, exist_ok=True)

plot_pure_transactions(df_txs)
plot_time_by_gas(df_calls)
plot_time_by_class(df_calls)
plot_call_throughput(df_calls)
plot_block_speedup(df_txs)
plot_tx_speedup(df_txs)

if args.output_dir:
    doc, tag, text = Doc().tagtext()

    def generate_info(doc, info):
        with tag("ul"):
            for k, v in info.items():
                with tag("li"):
                    doc.line("b", str(k))
                    text(": ", v)

    def generate_body(doc):
        doc, tag, text = doc.tagtext()

        doc.line("h1", "Execution Benchmark Report")

        doc.line("h2", "Cairo Native Execution Info")
        generate_info(doc, native_info)

        doc.line("h2", "Cairo VM Execution Info")
        generate_info(doc, vm_info)

        # Force line break after info
        with tag("div", style="page-break-after: always"):
            pass

        doc.line("h2", "Figures")
        OUTPUT_FIGURES.reverse()
        for title, name, description in OUTPUT_FIGURES:
            doc.line("h3", title)
            text(description)
            doc.stag("img", src=name)

    with tag("html"):
        with tag("head"):
            # Add minimal styling
            with tag("style"):
                doc.asis("""
                   body {
                        margin: 40px auto;
                        max-width: 21cm;
                        line-height: 1.6;
                        font-family: sans-serif;
                        padding: 0 10px;
                    }

                    img {
                        max-width: 100%;
                        height: auto;
                    }
                """)

        with tag("body"):
            generate_body(doc)

    args.output_dir.joinpath("report.html").write_text(doc.getvalue())

if args.display:
    plt.show()
