import numpy as np
import pandas as pd
import matplotlib.pyplot as plt
import seaborn as sns
import os
from pathlib import Path
from collections import Counter
import json
from functools import reduce


from argparse import ArgumentParser

argument_parser = ArgumentParser("Libfunc Counts")
argument_parser.add_argument("libfunc_counts")
arguments = argument_parser.parse_args()

LIBFUNCS_FILTER = [
    "branch_align",
    "enable_ap_tracking",
    "disable_ap_tracking",
    "jump",
    "revoke_ap_tracking",
    "unwrap_non_zero",
    "bounded_int_wrap_non_zero",
    "box_forward_snapshot"
]

def load_json_dir(path):
    def walk_dir(path):
        df = pd.DataFrame()

        for filename in os.listdir(path):
            new_path = Path(path).joinpath(filename)

            data = pd.read_json(new_path) if new_path.is_file() else walk_dir(new_path)

            df = pd.concat([df, data])

        return df

    df = walk_dir(path)
    df = df.dropna().apply(pd.Series)

    return df


def aggregate_dicts(entrypoints):
    ret = {}

    for entrypoint in entrypoints:
        for libfunc, count in entrypoint["entrypoint_counters"].items():
            if libfunc in ret:
                ret[libfunc] += count
            else:
                ret[libfunc] = count

    return ret


# Process libfunc counts

entrypoints_libfuncs_counts = load_json_dir(arguments.libfunc_counts)["entrypoints"]

total_counts = Counter()

for entrypoint in entrypoints_libfuncs_counts:
    total_counts.update(entrypoint["entrypoint_counters"])

total_counts = (
    pd.DataFrame(
        [
            {"libfunc": func_name, "count": count}
            for func_name, count in total_counts.items()
        ]
    )
    .sort_values(by=["count"], ascending=False)
    # .head(10)
    .apply(pd.Series)
)

total_counts_filtered = total_counts[
    ~total_counts["libfunc"].isin(
        LIBFUNCS_FILTER
    )
]

print(total_counts)

# Plotting

_, ax = plt.subplots()

sns.barplot(data=total_counts, x="libfunc", y="count", ax=ax)
ax.set_xlabel("Libfunc")
ax.set_ylabel("Calls Count")
ax.set_title("Libfuncs Counts")

plt.show()
