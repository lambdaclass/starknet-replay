import glob

import pathlib

import matplotlib.pyplot as plt
import pandas as pd
import seaborn as sns

enabled_plots = (
    "Correlations Matrix",
)

stat_files = glob.glob("compiled_programs/*.stats.json")

stats = []
for stat_file in stat_files:
    class_hash = pathlib.Path(stat_file).name.removesuffix(".stats.json")
    stat = pd.read_json(stat_file, typ="series")
    stat["hash"] = class_hash
    stats.append(stat)

df = pd.DataFrame(stats).set_index("hash")

if "Correlations Matrix" in enabled_plots:
    fig, ax = plt.subplots()
    fig.suptitle("Correlations Matrix")

    df_corr = df.corr(numeric_only=True)
    sns.heatmap(df_corr, ax=ax)

