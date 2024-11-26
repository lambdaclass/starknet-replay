from argparse import ArgumentParser

argument_parser = ArgumentParser('Stress Test Plotter')
argument_parser.add_argument("native_logs_path")
argument_parser.add_argument("vm_logs_path")
arguments = argument_parser.parse_args()

import matplotlib.pyplot as plt
import pandas as pd
import seaborn as sns

dataset_native = pd.read_json(arguments.native_logs_path, lines=True, typ="series")
dataset_vm = pd.read_json(arguments.vm_logs_path, lines=True, typ="series")

def canonicalize_compilation_time(event):
    if "contract compilation finished" not in event["fields"]["message"]:
        return None

    compilation_span = find_span(event, "contract compilation")
    if compilation_span is None:
        return None

    return {
        "class hash": compilation_span["class_hash"],
        "length": compilation_span["length"] / 1024,
        "size": event["fields"]["size"] / 1024,
    }

def find_span(event, name):
    for span in event["spans"]:
        if name in span["name"]:
            return span
    return None

def format_hash(class_hash):
    return f"0x{class_hash[:6]}..."


dataset_native = dataset_native.apply(canonicalize_compilation_time).dropna().apply(pd.Series)
dataset_vm = dataset_vm.apply(canonicalize_compilation_time).dropna().apply(pd.Series)

figure, ax = plt.subplots()

sns.set_color_codes("bright")

sns.regplot(
    x="length",
    y="size",
    label = "Native",
    data=dataset_native,
    ax = ax,
)
sns.regplot(
    x="length",
    y="size",
    label = "Casm",
    data=dataset_vm,
    ax = ax,
)

ax.set_xlabel("Sierra size (KiB)")
ax.set_ylabel("Compiled size (KiB)")
ax.set_title("Compilation Size Trend")
ax.ticklabel_format(style="plain")


ax.legend()

plt.show()
