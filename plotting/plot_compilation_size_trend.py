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
        "length": compilation_span["length"] / 1024,
        "size": event["fields"]["size"] / 1024,
        "executor": executor,
    }


dataset = load_log(arguments.logs_path, canonicalize)

figure, ax = plt.subplots()

sns.set_color_codes("bright")

sns.regplot(
    x="length",
    y="size",
    label="Native",
    data=dataset[dataset["executor"] == "native"],
    ax=ax,
)
sns.regplot(
    x="length",
    y="size",
    label="Casm",
    data=dataset[dataset["executor"] == "vm"],
    ax=ax,
)

ax.set_xlabel("Sierra size (KiB)")
ax.set_ylabel("Compiled size (KiB)")
ax.set_title("Compilation Size Trend")
ax.ticklabel_format(style="plain")

ax.legend()

plt.show()
