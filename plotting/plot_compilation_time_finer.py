from argparse import ArgumentParser
import matplotlib.pyplot as plt
import pandas as pd
import seaborn as sns
import numpy as np

argument_parser = ArgumentParser("Stress Test Plotter")
argument_parser.add_argument("native_logs_path")
arguments = argument_parser.parse_args()


dataset = pd.read_json(arguments.native_logs_path, lines=True, typ="series")


def canonicalize_compilation_time(event):
    # keep contract compilation finished logs
    compilation_span = find_span(event, "contract compilation")
    if compilation_span is None:
        return None

    class_hash = compilation_span["class_hash"]
    class_length = compilation_span["length"]

    if "contract compilation finished" in event["fields"]["message"]:
        return {
            "class hash": class_hash,
            "length": class_length,
            "type": "Total",
            "time": float(event["fields"]["time"]),
        }
    elif "sierra to mlir compilation finished" in event["fields"]["message"]:
        return {
            "class hash": class_hash,
            "length": class_length,
            "type": "Sierra to MLIR",
            "time": float(event["fields"]["time"]),
        }
    elif "mlir passes finished" in event["fields"]["message"]:
        return {
            "class hash": class_hash,
            "length": class_length,
            "type": "MLIR passes",
            "time": float(event["fields"]["time"]),
        }
    elif "mlir to llvm finished" in event["fields"]["message"]:
        return {
            "class hash": class_hash,
            "length": class_length,
            "type": "MLIR to LLVM",
            "time": float(event["fields"]["time"]),
        }
    elif "llvm passes finished" in event["fields"]["message"]:
        return {
            "class hash": class_hash,
            "length": class_length,
            "type": "LLVM passes",
            "time": float(event["fields"]["time"]),
        }
    elif "llvm to object compilation finished" in event["fields"]["message"]:
        return {
            "class hash": class_hash,
            "length": class_length,
            "type": "LLVM to object",
            "time": float(event["fields"]["time"]),
        }
    elif "linking finished" in event["fields"]["message"]:
        return {
            "class hash": class_hash,
            "length": class_length,
            "type": "Linking",
            "time": float(event["fields"]["time"]),
        }
    return None


def find_span(event, name):
    for span in event["spans"]:
        if name in span["name"]:
            return span
    return None


def format_hash(class_hash):
    return f"0x{class_hash[:6]}..."


dataset = dataset.apply(canonicalize_compilation_time).dropna().apply(pd.Series)
dataset = dataset.pivot(index = ["class hash"], columns = "type", values = "time")

pd.set_option('display.max_columns', None)

figure, ax = plt.subplots()

sns.set_color_codes("pastel")
sns.barplot(data=dataset, y="class hash", x="Total", label="Other", ax=ax, formatter=format_hash)

bottom = np.zeros(len(dataset))
sections = ["Linking", "LLVM to object", "LLVM passes", "MLIR to LLVM", "MLIR passes", "Sierra to MLIR"]

for section in sections:
    bottom += dataset[section]

for section in sections:
    sns.barplot(y=dataset.index, x=bottom, ax=ax, label=section, formatter=format_hash, orient="h")
    bottom -= dataset[section]

ax.set_xlabel("Compilation Time (ms)")
ax.set_ylabel("Class Hash")
ax.set_title("Native Compilation Time")
ax.legend()

plt.show()
