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
sns.violinplot(ax=ax, x="size", data=dataset[dataset["executor"] == "native"], cut=0)

ax.set_xlabel("Library Size (KiB)")
ax.set_title("Library Size by Contract")

plt.show()
