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
        "size": event["fields"]["size"] / 1024,
    }

dataset_native = load_dataset(arguments.native_logs_path, canonicalize)
dataset_vm = load_dataset(arguments.vm_logs_path, canonicalize)

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
