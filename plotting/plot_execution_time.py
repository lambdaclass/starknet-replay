from argparse import ArgumentParser

import matplotlib.pyplot as plt
import matplotlib as mpl
import pandas as pd
import seaborn as sns
import json
from utils import format_hash

parser = ArgumentParser("Stress Test Plotter")
parser.add_argument("native_data")
parser.add_argument("vm_data")
parser.add_argument("-s", "--speedup", action="store_true")
parser.add_argument("-o", "--output")
args = parser.parse_args()

pd.set_option("display.max_columns", None)
pd.set_option("display.max_rows", None)

mpl.rcParams["figure.figsize"] = [16, 9]


def load_dataset(path, f):
    data = json.load(open(path))
    return (
        pd.DataFrame(data["class_executions"])
        .apply(f, axis=1)
        .dropna()
        .apply(pd.Series)
    )


def process_row(row):
    class_hash = row.class_hash
    selector = row.selector
    time = row.time["nanos"] + row.time["secs"] * 10e9

    return {
        "class_hash": class_hash,
        "selector": selector,
        "time": time,
    }


dataNative = load_dataset(args.native_data, process_row)
dataNative["executor"] = "native"
dataVM = load_dataset(args.vm_data, process_row)
dataVM["executor"] = "vm"
data = pd.concat([dataNative, dataVM])

# GROUP BY SELECTOR

# calculate mean by class hash
data_by_selector = (
    data.groupby(["executor", "class_hash", "selector"])
    .agg(
        total_time=("time", "sum"),
        mean_time=("time", "mean"),
        samples=("time", "size"),
    )
    .unstack("executor")
)
data_by_selector.columns = data_by_selector.columns.map("_".join)

if (data_by_selector["samples_native"] != data_by_selector["samples_vm"]).any():
    raise Exception("Native and VM should have the same number of samples")

# sort by decreasing time
data_by_selector.sort_values(["total_time_vm"], ascending=[False], inplace=True)  # type: ignore

if args.output:
    file_name = f"{args.output}-execution-time.csv"
    data_by_selector.to_csv(file_name)

# GROUP BY CLASS

data_by_class = (
    data.groupby(["executor", "class_hash"])
    .agg(
        total_time=("time", "sum"),
        mean_time=("time", "mean"),
        samples=("time", "size"),
    )
    .unstack("executor")
)
data_by_class.columns = data_by_class.columns.map("_".join)
data_by_class["speedup"] = (
    data_by_class["total_time_vm"] / data_by_class["total_time_native"]
)
data_by_class.sort_values(["total_time_vm"], ascending=[False], inplace=True)  # type: ignore
data_by_class = data_by_class.nlargest(50, "total_time_vm")  # type: ignore

# ======================
#        PLOTTING
# ======================

figure, axes = plt.subplots(1, 2)

ax = axes[0]

sns.barplot(
    ax=ax,
    y="class_hash",
    x="total_time_vm",
    data=data_by_class,  # type: ignore
    formatter=format_hash,
    label="VM Execution Time",
    color="r",
    alpha=0.75,
)  # type: ignore
sns.barplot(
    ax=ax,
    y="class_hash",
    x="total_time_native",
    data=data_by_class,  # type: ignore
    formatter=format_hash,
    label="Native Execution Time",
    color="b",
    alpha=0.75,
)  # type: ignore

ax.set_xlabel("Total Time (ns)")
ax.set_ylabel("Class Hash")
ax.set_title("Total time by Contract Class")
ax.set_xscale("log", base=2)

ax = axes[1]

sns.barplot(
    ax=ax,
    y="class_hash",
    x="speedup",
    data=data_by_class,  # type: ignore
    formatter=format_hash,
    label="Execution Speedup",
    color="b",
    alpha=0.75,
)  # type: ignore

ax.set_xlabel("Speedup")
ax.set_ylabel("Class Hash")
ax.set_title("Speedup by Contract Class")

if args.output:
    figure_name = f"{args.output}-execution-time.svg"
    plt.savefig(figure_name)

if args.speedup:
    fig, ax = plt.subplots()
    sns.violinplot(
        ax=ax,
        x="speedup",
        data=data_by_class,  # type: ignore
        cut=0,
    )
    ax.set_xlabel("Speedup")
    ax.set_title("Speedup Distribution")
    if args.output:
        figure_name = f"{args.output}-execution-speedup.svg"
        plt.savefig(figure_name)

if not args.output:
    plt.show()
