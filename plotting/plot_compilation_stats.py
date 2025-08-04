import json
import re
import pathlib
import argparse
import inflection
from yattag import Doc
from typing import NamedTuple

import matplotlib.pyplot as plt
import matplotlib as mpl
import matplotlib.ticker as mticker
import pandas as pd
import seaborn as sns

sns.set_palette("deep")
sns.set_color_codes("deep")
mpl.rcParams["figure.figsize"] = [16 * 0.8, 9 * 0.8]


class Args(NamedTuple):
    compilation_data: pathlib.Path
    output_dir: pathlib.Path
    display: bool


arg_parser = argparse.ArgumentParser()
arg_parser.add_argument("compilation_data")
arg_parser.add_argument("--output-dir", type=pathlib.Path)
arg_parser.add_argument(
    "--display",
    action=argparse.BooleanOptionalAction,
    default=True,
)
args: Args = arg_parser.parse_args()  # type: ignore

if args.output_dir:
    args.output_dir.mkdir(parents=True, exist_ok=True)

#############
# UTILITIES #
#############

# A list of all the figures generated. Used to generate a final report.
OUTPUT_FIGURES = []


# Saves the current figure to the output directory, deriving the file name from
# the given title. Adds the figure data to `OUTPUT_FIGURES`, which can then be
# used to generate a report with all the figures.
def save_figure(title, description=""):
    if args.output_dir:
        stem = inflection.parameterize(title)
        name = f"{stem}.svg"
        OUTPUT_FIGURES.append(
            {
                title: title,
                name: name,
                description: description,
            }
        )
        plt.savefig(args.output_dir.joinpath(name))


# Given an info series, and the name of the field containing a Rust version,
# it parses the version string and shortens it. From example, converts from the
# full git url, to just the commit hash.
def parse_version(info: pd.Series, name: str):
    version_string: str = info[name]  # type: ignore
    match = re.search("rev=([a-z0-9]+)", version_string)
    if match:
        info[name] = match.group(1)


##############
# PROCESSING #
##############


def class_entry_to_series(entry):
    return pd.Series(
        {
            "class_hash": entry["class_hash"],
        }
        | entry["statistics"]
    )


raw_json = json.load(open(args.compilation_data))

info = pd.Series(raw_json["info"])
parse_version(info, "cairo_native_version")
info["memory"] = round(int(info["memory"]) / 2**30, 2)
info.rename(
    {
        "date": "Date",
        "block_start": "Block Start",
        "block_end": "Block End",
        "net": "Net",
        "native_profile": "Native Profile",
        "rust_profile": "Rust Profile",
        "cairo_native_version": "Cairo Native Version",
        "os": "OS",
        "arch": "Arch",
        "cpu": "CPU",
        "memory": "Memory (GiB)",
    },
    inplace=True,
)
df = pd.DataFrame(map(class_entry_to_series, raw_json["classes"]))

############
# PLOTTING #
############


def plot_correlations_matrix(df: pd.DataFrame):
    fig, ax = plt.subplots()
    fig.subplots_adjust(left=0.2, right=1, top=0.9, bottom=0.3)
    fig.set_figheight(10)

    def transform_column_label(label: str):
        label = label.removesuffix("_ms")
        label = label.removesuffix("_bytes")
        label = inflection.titleize(label)
        label = label.replace("Mlir", "MLIR")
        label = label.replace("Llvmir", "LLVMIR")
        label = label.replace("Llvm", "LLVM")
        return label

    df = df.rename(columns=transform_column_label)

    df_corr = df.corr(numeric_only=True)
    sns.heatmap(df_corr, ax=ax)

    ax.set_title("Correlation Matrix")

    save_figure(
        "Compilation Correlations",
        "Calculates a correlation matrix with different compilation statistics.",
    )


def plot_stages(df: pd.DataFrame):
    _, ax = plt.subplots()

    time_variables = [
        "compilation_total_time_ms",
        "compilation_sierra_to_mlir_time_ms",
        "compilation_mlir_passes_time_ms",
        "compilation_mlir_to_llvm_time_ms",
        "compilation_llvm_passes_time_ms",
        "compilation_llvm_to_object_time_ms",
        "compilation_linking_time_ms",
    ]

    df = df[time_variables].sum().sort_values(ascending=False)
    df = df / df["compilation_total_time_ms"] * 100
    df = df.drop("compilation_total_time_ms")

    df.rename(
        {
            "compilation_sierra_to_mlir_time_ms": "Sierra to MLIR",
            "compilation_mlir_passes_time_ms": "MLIR passes",
            "compilation_mlir_to_llvm_time_ms": "MLIR to LLVM",
            "compilation_llvm_passes_time_ms": "LLVM passes",
            "compilation_llvm_to_object_time_ms": "LLVM to object",
            "compilation_linking_time_ms": "Linking",
        },
        inplace=True,
    )

    sns.barplot(df, ax=ax, orient="h")

    ax.set_title("Compilation Stages")
    ax.xaxis.set_major_formatter(mticker.PercentFormatter())

    save_figure(
        "Contract Compilation Time by Stage",
        "Calculates the total time percentage for each compilation stage.",
    )


def plot_time(df: pd.DataFrame):
    _, ax = plt.subplots()

    time = df["compilation_total_time_ms"] / 1000

    sns.boxplot(df, x=time, ax=ax, showfliers=False, width=0.5)
    ax.set_title("Compilation Time Distribution")
    ax.set_xlabel("Compilation Time (s)")

    count = time.count()
    mean_time = time.mean()
    median_time = time.quantile(0.5)
    stddev_time = time.std()

    ax.text(
        0.01,
        0.99,
        "\n".join(
            [
                f"Count: {count}",
                f"Mean: {mean_time:.2f} s",
                f"Median: {median_time:.2f} s",
                f"Std Dev: {stddev_time:.2f}",
            ]
        ),
        transform=ax.transAxes,
        fontsize=12,
        verticalalignment="top",
        horizontalalignment="left",
    )

    save_figure(
        "Contract Compilation Time Distribution",
        "Calculates the distribution of contract compilation time.",
    )


def plot_size(df: pd.DataFrame):
    _, ax = plt.subplots()

    size = df["object_size_bytes"] / 2**10

    sns.boxplot(df, x=size, ax=ax, showfliers=False, width=0.5)
    ax.set_title("Compiled Contract Size Distribution")
    ax.set_xlabel("Compiled Contract Size (KiB)")

    count = size.count()
    mean_time = size.mean()
    median_time = size.quantile(0.5)
    stddev_time = size.std()

    ax.text(
        0.01,
        0.99,
        "\n".join(
            [
                f"Count: {count}",
                f"Mean: {mean_time:.2f} KiB",
                f"Median: {median_time:.2f} KiB",
                f"Std Dev: {stddev_time:.2f}",
            ]
        ),
        transform=ax.transAxes,
        fontsize=12,
        verticalalignment="top",
        horizontalalignment="left",
    )

    save_figure(
        "Compiled Contract Size Distribution",
        "Calculates the distribution of compiled contract size.",
    )


def plot_size_to_time(df: pd.DataFrame):
    fig, ax = plt.subplots()
    fig.subplots_adjust(hspace=0.3)

    sierra_statement_count: pd.Series = df["sierra_statement_count"]  # type: ignore
    compilation_total_time: pd.Series = df["compilation_total_time_ms"] / 1e3  # type: ignore

    sns.regplot(df, ax=ax, x=sierra_statement_count, y=compilation_total_time)
    ax.set_title("Sierra Size vs. Compilation Time")
    ax.set_xlabel("Sierra Statement Count")
    ax.set_ylabel("Compilation Time (s)")

    save_figure(
        "Sierra Size vs. Compilation Time",
        "Correlates the Sierra size with the total compilation time.",
    )


def plot_size_to_size(df: pd.DataFrame):
    fig, ax = plt.subplots()
    fig.subplots_adjust(hspace=0.3)

    sierra_statement_count: pd.Series = df["sierra_statement_count"]  # type: ignore
    object_size: pd.Series = df["object_size_bytes"] / 2**10  # type: ignore

    sns.regplot(df, ax=ax, x=sierra_statement_count, y=object_size)
    ax.set_title("Sierra Size vs. Compiled Contract Size")
    ax.set_xlabel("Sierra Statement Count")
    ax.set_ylabel("Compiled Contract Size (KiB)")

    save_figure(
        "Sierra Size vs. Compiled Contract Size",
        "Correlates the Sierra size with the compiled contract size.",
    )


plot_size_to_size(df)
plot_size_to_time(df)
plot_correlations_matrix(df)
plot_stages(df)
plot_size(df)
plot_time(df)

if args.output_dir:
    doc, tag, text = Doc().tagtext()

    def generate_info(doc, info):
        with tag("ul"):
            for k, v in info.items():
                with tag("li"):
                    doc.line("b", str(k))
                    text(": ", v)

    def generate_body(doc):
        doc, tag, text = doc.tagtext()

        doc.line("h1", "Execution Benchmark Report")

        doc.line("h2", "Execution Info")
        generate_info(doc, info)

        # Force line break after info
        with tag("div", style="page-break-after: always"):
            pass

        doc.line("h2", "Figures")
        OUTPUT_FIGURES.reverse()
        for title, name, description in OUTPUT_FIGURES:
            doc.line("h3", title)
            text(description)
            doc.stag("img", src=name)

    with tag("html"):
        with tag("head"):
            # Add minimal styling
            with tag("style"):
                doc.asis("""
                   body {
                        margin: 40px auto;
                        max-width: 21cm;
                        line-height: 1.6;
                        font-family: sans-serif;
                        padding: 0 10px;
                    }
                    img {
                        max-width: 100%;
                        height: auto;
                    }
                """)

            with tag("body"):
                generate_body(doc)

    args.output_dir.joinpath("report.html").write_text(doc.getvalue())


if args.display:
    plt.show()
