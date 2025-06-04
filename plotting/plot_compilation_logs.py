import pathlib

import argparse
from argparse import ArgumentParser

import matplotlib.pyplot as plt
import seaborn as sns
import pandas as pd
from pandas import DataFrame

arg_parser = ArgumentParser("Stress Test Plotter")
arg_parser.add_argument("logs_path")
arg_parser.add_argument(
    "--output",
    type=pathlib.Path,
)
arg_parser.add_argument(
    "--display", action=argparse.BooleanOptionalAction, default=True
)
args = arg_parser.parse_args()


##############
# PROCESSING #
##############


def find_span(event, name):
    for span in event["spans"]:
        if name in span["name"]:
            return span
    return None


def event_to_row(event):
    # keep contract compilation finished logs
    if "contract compilation finished" not in event["fields"]["message"]:
        return None

    compilation_span = find_span(event, "contract compilation")
    if compilation_span is None:
        return None

    if "vm" in event["fields"]["message"]:
        executor = "vm"
    elif "native" in event["fields"]["message"]:
        executor = "native"
    else:
        raise Exception("Invalid Executor")

    return {
        "class hash": compilation_span["class_hash"],
        "time": float(event["fields"]["time"]),  # ms
        "size": float(event["fields"]["size"]) / 2**10,  # KiB
        "length": float(compilation_span["length"]) / 2**10,  # KiB
        "executor": executor,
    }


def load_logs(path) -> DataFrame:
    log_df: DataFrame = pd.DataFrame()

    with pd.read_json(path, lines=True, typ="series", chunksize=100000) as chunks:
        for chunk in chunks:
            chunk_df: DataFrame = chunk.apply(event_to_row).dropna().apply(pd.Series)  # type: ignore
            if len(chunk) > 0:
                log_df = pd.concat([log_df, chunk_df])

    return log_df


# Load full dataframe. One row per log event.
df = load_logs(args.logs_path)

# Pivot table to have the executor as column
df = df.pivot_table(index="class hash", columns="executor")
df.columns = ["_".join(a) for a in df.columns.to_flat_index()]


############
# PLOTTING #
############


sns.set_color_codes("bright")


def save(name):
    if args.output:
        figure_name = f"{args.output}-{name}.svg"
        plt.savefig(figure_name)


def plot_compilation_time_regression():
    _, ax = plt.subplots()
    sns.regplot(x="length_native", y="time_native", label="Native", data=df, ax=ax)
    sns.regplot(x="length_vm", y="time_vm", label="Casm", data=df, ax=ax)
    ax.set_xlabel("Sierra size (KiB)")
    ax.set_ylabel("Compilation Time (ms)")
    ax.set_title("Native Compilation Time Trend")
    ax.legend()
    save("compilation-time-regression")


def plot_compilation_size_regression():
    _, ax = plt.subplots()
    sns.regplot(x="length_native", y="size_native", label="Native", data=df, ax=ax)
    sns.regplot(x="length_vm", y="size_vm", label="Casm", data=df, ax=ax)
    ax.set_xlabel("Sierra size (KiB)")
    ax.set_ylabel("Compiled size (KiB)")
    ax.set_title("Compilation Size Trend")
    ax.ticklabel_format(style="plain")
    ax.legend()
    save("compilation-size-regression")


def plot_compilation_size_correlation():
    _, ax = plt.subplots()
    sns.regplot(
        x="size_native",
        y="size_vm",
        data=df,
        ax=ax,
    )
    ax.set_xlabel("Native Compilation Size (KiB)")
    ax.set_ylabel("Casm Compilation Size (KiB)")
    ax.set_title("Compilation Size Correlation")
    save("compilation-size-correlation")


plot_compilation_time_regression()
plot_compilation_size_regression()
plot_compilation_size_correlation()

if args.display:
    plt.show()
