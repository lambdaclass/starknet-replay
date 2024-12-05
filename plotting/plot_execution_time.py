from argparse import ArgumentParser

import matplotlib.pyplot as plt
import pandas as pd
import seaborn as sns
from utils import format_hash

parser = ArgumentParser("Stress Test Plotter")
parser.add_argument("native_data")
parser.add_argument("vm_data")
parser.add_argument("-s", "--speedup", action="store_true")
args = parser.parse_args()


def load_dataset(path, f):
    return pd.read_json(path).apply(f, axis=1).dropna().apply(pd.Series)


def process_row(row):
    class_hash = row.class_hash
    time = row.time["nanos"] + row.time["secs"] * 10e9

    return {
        "class_hash": class_hash,
        "time": time,
    }


dataNative = load_dataset(args.native_data, process_row)
dataNative["executor"] = "native"
dataVM = load_dataset(args.vm_data, process_row)
dataVM["executor"] = "vm"
data = pd.concat([dataNative, dataVM])

# calculate mean by class hash
data = (
    data.groupby(["executor", "class_hash"])
    .agg(
        total_time=("time", "sum"),
        mean_time=("time", "mean"),
    )
    .unstack("executor")
)
data.columns = data.columns.map("_".join)

# calculate speedup
data["speedup"] = data["total_time_vm"] / data["total_time_native"]

total_native = data["total_time_native"].sum() / 10e9
total_vm = data["total_time_vm"].sum() / 10e9
print(f"Total Native: {total_native} seconds")
print(f"Total VM: {total_vm} seconds")
print("Total Speedup:", total_vm / total_native)

# sort by decreasing time
data.sort_values(["total_time_vm"], ascending=[False], inplace=True)  # type: ignore

print(data)

# ======================
#        PLOTTING
# ======================

figure, axes = plt.subplots(1, 2)

ax = axes[0]

sns.barplot(
    ax=ax,
    y="class_hash",
    x="total_time_vm",
    data=data,  # type: ignore
    formatter=format_hash,
    label="VM Execution Time",
    color="r",
    alpha=0.75,
)  # type: ignore
sns.barplot(
    ax=ax,
    y="class_hash",
    x="total_time_native",
    data=data,  # type: ignore
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
    data=data,  # type: ignore
    formatter=format_hash,
    label="Execution Speedup",
    color="b",
    alpha=0.75,
)  # type: ignore

ax.set_xlabel("Speedup")
ax.set_ylabel("Class Hash")
ax.set_title("Speedup by Contract Class")

if args.speedup:
    fig, ax = plt.subplots()
    sns.violinplot(
        ax=ax,
        x="speedup",
        data=data,  # type: ignore
        cut=0,
    )
    ax.set_xlabel("Speedup")
    ax.set_title("Speedup Distribution")

plt.show()
