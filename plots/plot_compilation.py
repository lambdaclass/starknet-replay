import argparse
import pathlib
import inflection
import json

import pandas as pd
import seaborn as sns
import matplotlib.pyplot as plt

parser = argparse.ArgumentParser()
parser.add_argument("input", type=pathlib.Path)
parser.add_argument("output", type=pathlib.Path)
args = parser.parse_args()


def save_artifact(metadata):
    slug = inflection.parameterize(metadata["title"])

    plt.savefig(f"{args.output}/{slug}.svg")
    with open(f"{args.output}/{slug}.meta.json", "w") as f:
        json.dump(metadata, f)


args.output.mkdir(parents=True, exist_ok=True)

df = pd.read_csv(args.input)
df["native_time_s"] = df["native_time_ns"] / 1e9
df["object_size_kb"] = df["object_size_bytes"] / 2**10
print(df.info())

_, ax = plt.subplots()
sns.boxplot(df, ax=ax, x="native_time_s", showfliers=False)
ax.set_xlabel("Time (s)")
ax.set_title("Compilation Time Distribution")
save_artifact(
    {
        "title": "Compilation Time Distribution",
        "description": "Calculates the distribution of the contract compilation time.",
        "statistics": df["native_time_s"].describe().to_dict(),
    }
)

_, ax = plt.subplots()
sns.boxplot(df, ax=ax, x="object_size_kb", showfliers=False)
ax.set_xlabel("Size (KiB)")
ax.set_title("Compiled Contract Size Distribution")
save_artifact(
    {
        "title": "Compiled Contract Size Distribution",
        "description": "Calculates the distribution of the compiled contract size.",
        "statistics": df["object_size_kb"].describe().to_dict(),
    }
)


_, ax = plt.subplots()
sns.regplot(df, ax=ax, x="sierra_statement_count", y="object_size_kb")
ax.set_title("Sierra Size vs. Compiled Contract Size")
ax.set_xlabel("Sierra Statement Count")
ax.set_ylabel("Compiled Contract Size (KiB)")
save_artifact(
    {
        "title": "Sierra Size vs. Compiled Contract Size",
        "description": "Correlates the Sierra size with the compiled contract size.",
    }
)

plt.show()
