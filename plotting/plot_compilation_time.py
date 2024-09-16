from argparse import ArgumentParser

argument_parser = ArgumentParser('Stress Test Plotter')
argument_parser.add_argument("native_logs_path")
arguments = argument_parser.parse_args()

import matplotlib.pyplot as plt
import pandas as pd
import seaborn as sns

dataset = pd.read_json(arguments.native_logs_path, lines=True, typ="series")

def canonicalize_compilation_time(event):
    # keep contract compilation finished logs
    if "native compilation finished" not in event["fields"]["message"]:
        return None

    return {
        "class hash": event["fields"]["class_hash"],
        "time": float(event["fields"]["time"]),
    }

def find_span(event, name):
    for span in event["spans"]:
        if span["name"] == name:
            return span
    return None

def format_hash(class_hash):
    return f"0x{class_hash[:6]}..."

dataset = dataset.map(canonicalize_compilation_time).dropna().apply(pd.Series)

figure, ax = plt.subplots()

sns.set_color_codes("bright")
sns.barplot(ax=ax, y="class hash", x="time", data=dataset, formatter=format_hash) # type: ignore

ax.set_xlabel("Compilation Time (ms)")
ax.set_ylabel("Class Hash")
ax.set_title("Native Compilation Time")

plt.show()
