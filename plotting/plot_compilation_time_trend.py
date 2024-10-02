from argparse import ArgumentParser
import matplotlib.pyplot as plt
import pandas as pd
import seaborn as sns

argument_parser = ArgumentParser("Stress Test Plotter")
argument_parser.add_argument("native_logs_path")
argument_parser.add_argument("vm_logs_path")
arguments = argument_parser.parse_args()


dataset_native = pd.read_json(arguments.native_logs_path, lines=True, typ="series")
dataset_vm = pd.read_json(arguments.vm_logs_path, lines=True, typ="series")


def canonicalize_compilation_time(event):
    # keep contract compilation finished logs
    if "contract compilation finished" not in event["fields"]["message"]:
        return None

    compilation_span = find_span(event, "contract compilation")
    if compilation_span is None:
        return None

    class_hash = compilation_span["class_hash"]
    class_length = float(compilation_span["length"])

    return {
        "class hash": class_hash,
        "length": class_length / 1024,
        "time": float(event["fields"]["time"]),
    }


def find_span(event, name):
    for span in event["spans"]:
        if name in span["name"]:
            return span
    return None


def format_hash(class_hash):
    return f"0x{class_hash[:6]}..."


dataset_native = dataset_native.apply(canonicalize_compilation_time).dropna().apply(pd.Series)
dataset_vm = dataset_vm.apply(canonicalize_compilation_time).dropna().apply(pd.Series)

fig, ax = plt.subplots()

sns.set_theme()
sns.set_color_codes("bright")

sns.regplot(
    x="length",
    y="time",
    label = "Native",
    data=dataset_native,
    ax = ax,
)
sns.regplot(
    x="length",
    y="time",
    label = "Casm",
    data=dataset_vm,
    ax = ax,
)

ax.set_xlabel("Sierra size (KiB)")
ax.set_ylabel("Compilation Time (ms)")
ax.set_title("Native Compilation Time Trend")
ax.legend()

plt.show()
