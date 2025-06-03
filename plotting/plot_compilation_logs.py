from argparse import ArgumentParser

import matplotlib.pyplot as plt
import seaborn as sns
from utils import find_span, load_jsonl
from pandas import DataFrame

argument_parser = ArgumentParser("Stress Test Plotter")
argument_parser.add_argument("logs_path")
argument_parser.add_argument("-o", "--output")
arguments = argument_parser.parse_args()

sns.set_color_codes("bright")


def save(name):
    if arguments.output:
        figure_name = f"{arguments.output}-{name}.svg"
        plt.savefig(figure_name)


def canonicalize(event):
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


dataset = load_jsonl(arguments.logs_path, canonicalize)
dataset_native: DataFrame = dataset[dataset["executor"] == "native"]  # type: ignore
dataset_vm: DataFrame = dataset[dataset["executor"] == "vm"]  # type: ignore
dataset_pivoted = dataset.pivot_table(index="class hash", columns="executor")
dataset_pivoted.columns = ["_".join(a) for a in dataset_pivoted.columns.to_flat_index()]


def plot_compilation_time():
    _, ax = plt.subplots()
    sns.regplot(x="length", y="time", label="Native", data=dataset_native, ax=ax)
    sns.regplot(x="length", y="time", label="Casm", data=dataset_vm, ax=ax)
    ax.set_xlabel("Sierra size (KiB)")
    ax.set_ylabel("Compilation Time (ms)")
    ax.set_title("Native Compilation Time Trend")
    ax.legend()

    save("compilation-time-trend")


def plot_compilation_size():
    _, ax = plt.subplots()
    sns.regplot(x="length", y="size", label="Native", data=dataset_native, ax=ax)
    sns.regplot(x="length", y="size", label="Casm", data=dataset_vm, ax=ax)
    ax.set_xlabel("Sierra size (KiB)")
    ax.set_ylabel("Compiled size (KiB)")
    ax.set_title("Compilation Size Trend")
    ax.ticklabel_format(style="plain")
    ax.legend()
    save("compilation-size-trend")


def plot_compilation_size_correlation():
    _, ax = plt.subplots()
    sns.regplot(
        x="size_native",
        y="size_vm",
        data=dataset_pivoted,
        ax=ax,
    )
    ax.set_xlabel("Native Compilation Size (KiB)")
    ax.set_ylabel("Casm Compilation Size (KiB)")
    ax.set_title("Compilation Size Correlation")
    save("compilation-size-correlation")


plot_compilation_time()
plot_compilation_size()
plot_compilation_size_correlation()

if not arguments.output:
    plt.show()
