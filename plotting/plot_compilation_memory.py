from argparse import ArgumentParser

argument_parser = ArgumentParser('Stress Test Plotter')
argument_parser.add_argument("native_logs_path")
arguments = argument_parser.parse_args()

import matplotlib.pyplot as plt
import pandas as pd
import seaborn as sns

dataset = pd.read_json(arguments.native_logs_path, lines=True, typ="series")

def canonicalize_compilation_time(event):
    if "contract compilation finished" not in event["fields"]["message"]:
        return None

    compilation_span = find_span(event, "contract compilation")
    if compilation_span is None:
        return None

    return {
        "class hash": compilation_span["class_hash"],
        "size": event["fields"]["size"] / (1024 * 1024),
    }

def find_span(event, name):
    for span in event["spans"]:
        if name in span["name"]:
            return span
    return None

def format_hash(class_hash):
    return f"0x{class_hash[:6]}..."


dataset = dataset.apply(canonicalize_compilation_time).dropna().apply(pd.Series)

figure, ax = plt.subplots()

sns.set_color_codes("bright")
sns.barplot(ax=ax, y="class hash", x="size", data=dataset, formatter=format_hash) # type: ignore

ax.set_xlabel("Library Size (MiB)")
ax.set_ylabel("Class Hash")
ax.set_title("Library Size by Contract")

plt.show()
