from argparse import ArgumentParser

import matplotlib.pyplot as plt
import seaborn as sns
from utils import load_compilation_logs

argument_parser = ArgumentParser("Stress Test Plotter")
argument_parser.add_argument("logs_path")
arguments = argument_parser.parse_args()


dataset = load_compilation_logs(arguments.logs_path)

fig, ax = plt.subplots()

sns.set_theme()
sns.set_color_codes("bright")

sns.regplot(
    x="length",
    y="time",
    label="Native",
    data=dataset[dataset["executor"] == "native"],
    ax=ax,
)
sns.regplot(
    x="length",
    y="time",
    label="Casm",
    data=dataset[dataset["executor"] == "vm"],
    ax=ax,
)

ax.set_xlabel("Sierra size (KiB)")
ax.set_ylabel("Compilation Time (ns)")
ax.set_title("Native Compilation Time Trend")
ax.legend()

plt.show()
