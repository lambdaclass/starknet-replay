from argparse import ArgumentParser

argument_parser = ArgumentParser('Stress Test Plotter')
argument_parser.add_argument("native_logs_path")
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
        "class hash": hex(int(compilation_span["class_hash"])),
        "size": event["fields"]["size"] / (1024 * 1024),
    }

dataset = load_dataset(arguments.native_logs_path, canonicalize)

figure, ax = plt.subplots()

sns.set_color_codes("bright")
sns.violinplot(ax=ax, x="size", data=dataset)

ax.set_xlabel("Library Size (MiB)")
ax.set_title("Library Size by Contract")

plt.show()
