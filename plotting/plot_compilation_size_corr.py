from argparse import ArgumentParser

import matplotlib.pyplot as plt
import seaborn as sns
from utils import load_compilation_logs

argument_parser = ArgumentParser("Stress Test Plotter")
argument_parser.add_argument("logs_path")
arguments = argument_parser.parse_args()


dataset = load_compilation_logs(arguments.logs_path)
dataset = dataset.pivot_table(index="class hash", columns="executor")
dataset.columns = ["_".join(a) for a in dataset.columns.to_flat_index()]

figure, ax = plt.subplots(figsize=(50, 5))

sns.set_color_codes("bright")

sns.regplot(
    x="size_native",
    y="size_vm",
    data=dataset,
    ax=ax,
)

ax.set_xlabel("Native Compilation Size (KiB)")
ax.set_ylabel("Casm Compilation Size (KiB)")
ax.set_title("Compilation Size Correlation")

plt.show()
