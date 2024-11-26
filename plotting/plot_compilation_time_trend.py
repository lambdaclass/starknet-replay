from argparse import ArgumentParser

argument_parser = ArgumentParser("Stress Test Plotter")
argument_parser.add_argument("native_logs_path")
argument_parser.add_argument("vm_logs_path")
arguments = argument_parser.parse_args()

import matplotlib.pyplot as plt
import seaborn as sns
from utils import load_dataset, find_span

def canonicalize(event):
    # keep contract compilation finished logs
    if "contract compilation finished" not in event["fields"]["message"]:
        return None

    compilation_span = find_span(event, "contract compilation")
    if compilation_span is None:
        return None

    class_hash = compilation_span["class_hash"]
    class_length = float(compilation_span["length"])

    return {
        "class hash": class_hash,
        "length": class_length / 1024,
        "time": float(event["fields"]["time"]),
    }

dataset_native = load_dataset(arguments.native_logs_path, canonicalize)
dataset_vm = load_dataset(arguments.vm_logs_path, canonicalize)

fig, ax = plt.subplots()

sns.set_theme()
sns.set_color_codes("bright")

sns.regplot(
    x="length",
    y="time",
    label = "Native",
    data=dataset_native,
    ax = ax,
)
sns.regplot(
    x="length",
    y="time",
    label = "Casm",
    data=dataset_vm,
    ax = ax,
)

ax.set_xlabel("Sierra size (KiB)")
ax.set_ylabel("Compilation Time (ms)")
ax.set_title("Native Compilation Time Trend")
ax.legend()

plt.show()
