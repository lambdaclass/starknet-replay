import argparse
import pathlib
import inflection
import json

import pandas as pd
import seaborn as sns
import matplotlib.pyplot as plt
import matplotlib.ticker

parser = argparse.ArgumentParser()
parser.add_argument("input", type=pathlib.Path)
parser.add_argument("output", type=pathlib.Path)
args = parser.parse_args()


def save_artifact(metadata):
    slug = inflection.parameterize(metadata["title"])

    plt.savefig(f"{slug}.svg")
    with open(f"{slug}.meta.json", "w") as f:
        json.dump(metadata, f)


nanosecond_to_second_formatter = matplotlib.ticker.FuncFormatter(
    lambda x, _: f"{x / 1e9}"
)

df = pd.read_csv(args.input)
print(df.info())

_, ax = plt.subplots()
sns.boxplot(df, ax=ax, x="native_time_ns", showfliers=False)
ax.xaxis.set_major_formatter(nanosecond_to_second_formatter)
ax.xaxis.set_label_text("Time (s)")
ax.set_title("Compilation Time Distribution")
save_artifact(
    {
        "title": "Compilation Time Distribution",
        "description": "Calculates the distribution of the contract compilation time.",
    }
)

plt.show()
