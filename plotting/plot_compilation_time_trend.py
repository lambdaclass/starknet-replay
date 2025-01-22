from argparse import ArgumentParser

import matplotlib.pyplot as plt
import seaborn as sns
from utils import load_log, find_span

argument_parser = ArgumentParser("Stress Test Plotter")
argument_parser.add_argument("logs_path")
arguments = argument_parser.parse_args()


def canonicalize(event):
    # keep contract compilation finished logs
    if "contract compilation finished" not in event["fields"]["message"]:
        return None

    compilation_span = find_span(event, "contract compilation")
    if compilation_span is None:
        return None

    class_hash = compilation_span["class_hash"]
    class_length = float(compilation_span["length"])

    if "vm" in event["fields"]["message"]:
        executor = "vm"
    elif "native" in event["fields"]["message"]:
        executor = "native"
    else:
        raise Exception("Invalid Executor")

    return {
        "class hash": class_hash,
        "length": class_length / 1024,
        "time": float(event["fields"]["time"]),
        "executor": executor,
    }


dataset = load_log(arguments.logs_path, canonicalize)

fig, ax = plt.subplots()

sns.set_theme()
sns.set_color_codes("bright")

sns.regplot(
    x="length",
    y="time",
    label="Native",
    data=dataset[dataset["executor"] == "native"],
    ax=ax,
)
sns.regplot(
    x="length",
    y="time",
    label="Casm",
    data=dataset[dataset["executor"] == "vm"],
    ax=ax,
)

ax.set_xlabel("Sierra size (KiB)")
ax.set_ylabel("Compilation Time (ms)")
ax.set_title("Native Compilation Time Trend")
ax.legend()

plt.show()
