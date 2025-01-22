from argparse import ArgumentParser
import matplotlib.pyplot as plt
import seaborn as sns
from utils import load_log, find_span


argument_parser = ArgumentParser("Stress Test Plotter")
argument_parser.add_argument("logs_path")
arguments = argument_parser.parse_args()


def canonicalize(event):
    # keep contract compilation finished logs
    if "native contract compilation finished" not in event["fields"]["message"]:
        return None

    compilation_span = find_span(event, "contract compilation")
    if compilation_span is None:
        return None

    return {
        "class hash": compilation_span["class_hash"],
        "time": float(event["fields"]["time"]),
    }


dataset = load_log(arguments.logs_path, canonicalize)

figure, ax = plt.subplots()

sns.set_color_codes("bright")
sns.violinplot(ax=ax, x="time", data=dataset, cut=0)

ax.set_xlabel("Compilation Time (ms)")
ax.set_title("Native Compilation Time")

plt.show()
