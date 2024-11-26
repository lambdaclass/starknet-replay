from argparse import ArgumentParser

argument_parser = ArgumentParser('Stress Test Plotter')
argument_parser.add_argument("native_logs_path")
argument_parser.add_argument("vm_logs_path")
arguments = argument_parser.parse_args()

import matplotlib.pyplot as plt
import pandas as pd
import seaborn as sns
import numpy as np
import matplotlib.ticker as ticker
from utils import find_span, format_hash, load_dataset, keep_common_classes

pd.set_option('display.max_colwidth', None)
sns.set_color_codes("bright")

def canonicalize(event):
    # skip caching logs
    if find_span(event, "caching block range") != None:
        return None

    # keep contract execution finished logs
    if "contract execution finished" not in event["fields"]["message"]:
        return None

    class_hash = hex(int(event["span"]["class_hash"]))
    time = float(event["fields"]["time"])
    
    return {
        "class hash": class_hash,
        "time": time,
    }

datasetNative = load_dataset(arguments.native_logs_path, canonicalize)
datasetVM = load_dataset(arguments.vm_logs_path, canonicalize)

datasetNative = keep_common_classes(datasetNative, 75)
datasetVM = keep_common_classes(datasetVM, 75)

# CALCULATE MEAN
datasetNative = datasetNative.groupby("class hash").agg(["mean","size"])
datasetVM = datasetVM.groupby("class hash").agg(["mean","size"])
dataset = datasetNative.join(datasetVM, lsuffix="_native", rsuffix="_vm")
dataset.columns = dataset.columns.map('_'.join)

# CALCULATE SPEEDUP
dataset["speedup"] = dataset["time_vm_mean"] / dataset["time_native_mean"]

# SORT BY TIME
dataset.sort_values(['time_vm_mean'], ascending=[False], inplace=True)

print("Average Speedup: ", dataset["speedup"].mean())
print(dataset)

figure, axes = plt.subplots(1, 2)

ax=axes[0]

sns.barplot(ax=ax, y="class hash", x="time_vm_mean", data=dataset, formatter=format_hash, label="VM Execution Time", color="r", alpha = 0.75) # type: ignore
sns.barplot(ax=ax, y="class hash", x="time_native_mean", data=dataset, formatter=format_hash, label="Native Execution Time", color="b", alpha = 0.75) # type: ignore

ax.set_xlabel("Mean Time (ns)")
ax.set_ylabel("Class Hash")
ax.set_title("Mean time by Contract Class")
ax.set_xscale("log", base=2)

ax=axes[1]

sns.barplot(ax=ax, y="class hash", x="speedup", data=dataset, formatter=format_hash, label="Execution Speedup", color="b", alpha = 0.75) # type: ignore

ax.set_xlabel("Speedup")
ax.set_ylabel("Class Hash")
ax.set_title("Speedup by Contract Class")

fig, ax = plt.subplots()
sns.violinplot(ax=ax, x="speedup", data=dataset, cut=0)
ax.set_xlabel("Speedup")
ax.set_title("Speedup Distribution")
ax.xaxis.set_major_locator(ticker.MultipleLocator(2, 1)) # type: ignore

plt.show()
