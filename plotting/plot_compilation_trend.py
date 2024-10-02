from argparse import ArgumentParser
import matplotlib.pyplot as plt
import pandas as pd
import seaborn as sns

argument_parser = ArgumentParser("Stress Test Plotter")
argument_parser.add_argument("native_logs_path")
arguments = argument_parser.parse_args()


dataset = pd.read_json(arguments.native_logs_path, lines=True, typ="series")


def canonicalize_compilation_time(event):
    # keep contract compilation finished logs
    if "contract compilation finished" not in event["fields"]["message"]:
        return None

    compilation_span = find_span(event, "contract compilation")
    if compilation_span is None:
        return None

    class_hash = compilation_span["class_hash"]
    class_length = compilation_span["length"]

    return {
        "class hash": class_hash,
        "length": class_length,
        "type": "Total",
        "time": float(event["fields"]["time"]),
    }


def find_span(event, name):
    for span in event["spans"]:
        if span["name"] == name:
            return span
    return None


def format_hash(class_hash):
    return f"0x{class_hash[:6]}..."


dataset = dataset.apply(canonicalize_compilation_time).dropna().apply(pd.Series)

sns.set_theme()
sns.set_color_codes("bright")

g = sns.lmplot(
    x="length",
    y="time",
    data=dataset,
)

g.set_xlabels("Sierra Length (statements)")
g.set_ylabels("Compilation Time (ms)")
g.set_titles("Native Compilation Time")

plt.show()
