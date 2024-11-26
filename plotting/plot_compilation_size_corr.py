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

dataset_native = dataset_native.set_index("class hash")
dataset_vm = dataset_vm.set_index("class hash")

dataset = dataset_native.join(dataset_vm, lsuffix="_native", rsuffix="_casm")

figure, ax = plt.subplots()

sns.set_color_codes("bright")

sns.regplot(
    x="size_native",
    y="size_casm",
    data=dataset,
    ax = ax,
)

ax.set_xlabel("Native Compilation Size (KiB)")
ax.set_ylabel("Casm Compilation Size (KiB)")
ax.set_title("Compilation Size Correlation")

plt.show()
