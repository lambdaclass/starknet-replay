from argparse import ArgumentParser

argument_parser = ArgumentParser('Stress Test Plotter')
argument_parser.add_argument("native_logs_path")
argument_parser.add_argument("vm_logs_path")
arguments = argument_parser.parse_args()

import matplotlib.pyplot as plt
import seaborn as sns
from utils import load_dataset, find_span

def canonicalize(event):
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

dataset_native = load_dataset(arguments.native_logs_path, canonicalize)
dataset_vm = load_dataset(arguments.vm_logs_path, canonicalize)

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
