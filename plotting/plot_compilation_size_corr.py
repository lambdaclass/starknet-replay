from argparse import ArgumentParser

import matplotlib.pyplot as plt
import seaborn as sns
from utils import load_log, find_span

argument_parser = ArgumentParser("Stress Test Plotter")
argument_parser.add_argument("logs_path")
arguments = argument_parser.parse_args()


def canonicalize(event):
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
        "size": event["fields"]["size"] / 1024,
        "executor": executor,
    }


dataset = load_log(arguments.logs_path, canonicalize)
dataset = dataset.pivot_table(index="class hash", columns="executor")
dataset.columns = ["_".join(a) for a in dataset.columns.to_flat_index()]

figure, ax = plt.subplots()

sns.set_color_codes("bright")

sns.regplot(
    x="size_native",
    y="size_vm",
    data=dataset,
    ax=ax,
)

ax.set_xlabel("Native Compilation Size (KiB)")
ax.set_ylabel("Casm Compilation Size (KiB)")
ax.set_title("Compilation Size Correlation")

plt.show()
