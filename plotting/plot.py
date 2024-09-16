from argparse import ArgumentParser

argument_parser = ArgumentParser('Stress Test Plotter')
argument_parser.add_argument("native_logs_path")
argument_parser.add_argument("vm_logs_path")
arguments = argument_parser.parse_args()

import matplotlib.pyplot as plt
import pandas as pd
import seaborn as sns

datasetNative = pd.read_json(arguments.native_logs_path, lines=True, typ="series")
datasetVM = pd.read_json(arguments.vm_logs_path, lines=True, typ="series")

def canonicalize_execution_time_by_contract_class(event):
    # skip caching logs
    if find_span(event, "benchmarking block range") == None:
        return None

    # keep contract execution finished logs
    if "contract execution finished" not in event["fields"]["message"]:
        return None

    return {
        "class hash": event["span"]["class_hash"],
        "time": float(event["fields"]["time"]),
    }

def find_span(event, name):
    for span in event["spans"]:
        if span["name"] == name:
            return span
    return None

def format_hash(class_hash):
    return f"0x{class_hash[:6]}..."

datasetNative = datasetNative.map(canonicalize_execution_time_by_contract_class).dropna().apply(pd.Series)
datasetVM = datasetVM.map(canonicalize_execution_time_by_contract_class).dropna().apply(pd.Series)

datasetNative = datasetNative.groupby("class hash").mean()
datasetVM = datasetVM.groupby("class hash").mean()

figure, ax = plt.subplots()

sns.set_color_codes("bright")

sns.barplot(ax=ax, y="class hash", x="time", data=datasetVM, formatter=format_hash, label="VM Execution Time", color="r", alpha = 0.75) # type: ignore
sns.barplot(ax=ax, y="class hash", x="time", data=datasetNative, formatter=format_hash, label="Native Execution Time", color="b", alpha = 0.75) # type: ignore

ax.set(xlabel="Mean Time (ms)", ylabel="Class Hash")

plt.show()
