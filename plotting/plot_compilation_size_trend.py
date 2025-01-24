from argparse import ArgumentParser
import matplotlib.pyplot as plt
import seaborn as sns
from utils import load_compilation_logs

argument_parser = ArgumentParser("Stress Test Plotter")
argument_parser.add_argument("logs_path")
arguments = argument_parser.parse_args()


dataset = load_compilation_logs(
    arguments.logs_path,
)

figure, ax = plt.subplots()

sns.set_color_codes("bright")

sns.regplot(
    x="length",
    y="size",
    label="Native",
    data=dataset[dataset["executor"] == "native"],
    ax=ax,
)
sns.regplot(
    x="length",
    y="size",
    label="Casm",
    data=dataset[dataset["executor"] == "vm"],
    ax=ax,
)

ax.set_xlabel("Sierra size (KiB)")
ax.set_ylabel("Compiled size (KiB)")
ax.set_title("Compilation Size Trend")
ax.ticklabel_format(style="plain")

ax.legend()

plt.show()
