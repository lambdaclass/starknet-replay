from argparse import ArgumentParser

import matplotlib.pyplot as plt
import seaborn as sns
from matplotlib.ticker import PercentFormatter
from utils import load_log, find_span


argument_parser = ArgumentParser("Stress Test Plotter")
argument_parser.add_argument("logs_path")
arguments = argument_parser.parse_args()


def canonicalize(event):
    # keep contract compilation finished logs
    compilation_span = find_span(event, "contract compilation")
    if compilation_span is None:
        return None

    class_hash = compilation_span["class_hash"]
    class_length = compilation_span["length"]

    if "native contract compilation finished" in event["fields"]["message"]:
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


dataset = load_log(arguments.logs_path, canonicalize)

dataset = dataset.pivot(index=["class hash"], columns="type", values="time")
dataset = dataset.sum()

sections = [
    "Linking",
    "LLVM to object",
    "LLVM passes",
    "MLIR to LLVM",
    "MLIR passes",
    "Sierra to MLIR",
]
for section in sections:
    dataset[section] = dataset[section] / dataset["Total"] * 100

dataset = dataset.drop("Total")
dataset = dataset.sort_values(ascending=False)

figure, ax = plt.subplots()

sns.barplot(data=dataset, orient="y")  # type: ignore

plt.title("Mean Compilation Time by Step")
ax.xaxis.set_major_formatter(PercentFormatter(decimals=0))

ax.set_xlabel("Step")

plt.show()
