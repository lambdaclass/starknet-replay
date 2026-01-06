import inflection
import json

import matplotlib.pyplot as plt
import seaborn as sns
import pandas as pd


def pretty_describe(row):
    return row.describe().rename(
        {
            "count": "Number of Samples",
            "mean": "Mean",
            "std": "Standard Deviation",
            "min": "Minimum",
            "25%": "25th Percentile",
            "50%": "50th Percentile",
            "75%": "75th Percentile",
            "max": "Maximum",
        }
    )


def save_fig_artifact(dir, fig, metadata):
    slug = inflection.parameterize(metadata["title"])
    fig.savefig(f"{dir}/{slug}.svg")
    with open(f"{dir}/{slug}.meta.json", "w") as f:
        json.dump(metadata, f, indent=4)


def save_df_artifact(dir, data, metadata):
    slug = inflection.parameterize(metadata["title"])
    data.to_csv(f"{dir}/{slug}.csv")
    with open(f"{dir}/{slug}.meta.json", "w") as f:
        json.dump(metadata, f, indent=4)


def plot_distribution(artifact_dir, data, label, title, description, log=False):
    fig, ax = plt.subplots(2)
    sns.boxplot(ax=ax[0], x=data, showfliers=False)
    ax[0].set_xlabel(label)
    sns.stripplot(ax=ax[1], x=data, alpha=0.25, jitter=0.4)
    ax[1].set_xlabel(label)
    if log:
        ax[1].set_xscale("log", base=2)
    fig.subplots_adjust(hspace=0.30)
    fig.suptitle(title)
    save_fig_artifact(
        artifact_dir,
        fig,
        {
            "title": title,
            "description": description,
            "statistics": pretty_describe(data).to_dict(),
        },
    )


def plot_relation(artifact_dir, x_data, y_data, x_label, y_label, title, description):
    fig, ax = plt.subplots()
    sns.regplot(ax=ax, x=x_data, y=y_data)
    ax.set_title(title)
    ax.set_xlabel(x_label)
    ax.set_ylabel(y_label)
    save_fig_artifact(
        artifact_dir,
        fig,
        {
            "title": title,
            "description": description,
        },
    )


def save_edge_cases(artifacts_dir, data, title, description):
    best = data.nsmallest(10)
    worst = data.nlargest(10)
    edge = pd.concat([best, worst]).sort_values()  # type: ignore
    save_df_artifact(
        artifacts_dir,
        edge,
        {
            "title": title,
            "description": description,
        },
    )
    pass
