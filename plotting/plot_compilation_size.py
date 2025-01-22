from argparse import ArgumentParser
import matplotlib.pyplot as plt
import seaborn as sns
from utils import load_log, find_span

argument_parser = ArgumentParser("Stress Test Plotter")
argument_parser.add_argument("native_logs_path")
arguments = argument_parser.parse_args()


def canonicalize(event):
    if "native contract compilation finished" not in event["fields"]["message"]:
        return None

    compilation_span = find_span(event, "contract compilation")
    if compilation_span is None:
        return None

    return {
        "class hash": compilation_span["class_hash"],
        "size": event["fields"]["size"] / (1024 * 1024),
    }


dataset = load_log(arguments.native_logs_path, canonicalize)

figure, ax = plt.subplots()

sns.set_color_codes("bright")
sns.violinplot(ax=ax, x="size", data=dataset, cut=0)

ax.set_xlabel("Library Size (MiB)")
ax.set_title("Library Size by Contract")

plt.show()
