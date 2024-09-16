from argparse import ArgumentParser

argument_parser = ArgumentParser('Stress Test Plotter')
argument_parser.add_argument("logs_path")
arguments = argument_parser.parse_args()

import matplotlib.pyplot as plt
import pandas as pd
import seaborn as sns

dataset = pd.read_json(arguments.logs_path, lines=True, typ="series")


def canonicalize_execution_time_by_contract_class(event):
    # skip: caching logs
    if find_span(event, "caching_block_range") != None:
        return None

    # keep: native contract execution finished logs
    if event["fields"]["message"] != "native contract execution finished":
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
    return f"0x{class_hash[:5]}..."

dataset = dataset.map(canonicalize_execution_time_by_contract_class).dropna().apply(pd.Series)

figure, ax = plt.subplots()

sns.boxplot(ax=ax, y="class hash", x="time", data=dataset, formatter=format_hash) # type: ignore

plt.show()
