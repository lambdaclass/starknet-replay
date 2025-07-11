import itertools
import pathlib
import argparse

import matplotlib.pyplot as plt
import pandas as pd
import seaborn as sns

sns.set_theme()

arg_parser = argparse.ArgumentParser()
arg_parser.add_argument("input", nargs="*")
arg_parser.add_argument(
    "--c1",
    help="class hash for the first sample contract to use, defaults to first contract",
)
arg_parser.add_argument(
    "--c2",
    help="class hash for the second sample contract to use, defaults to second contract",
)
args = arg_parser.parse_args()

#############
# UTILITIES #
#############


# Some plots compare only two different contracts. By default, it uses the first
# and second contract, although It can be customized with the `--c1` and `--c2`
# flags.
def get_sample_contracts(df: pd.DataFrame):
    if args.c1 is None:
        c1 = df.iloc[0]
    else:
        c1 = df.loc[args.c1]
    if args.c2 is None:
        c2 = df.iloc[1]
    else:
        c2 = df.loc[args.c2]

    return c1, c2


##############
# PROCESSING #
##############


stats = []
for stat_file in args.input:
    hash = pathlib.Path(stat_file).name.split(".", maxsplit=1)[0]
    stat = pd.read_json(stat_file, typ="series")
    stat["hash"] = hash
    stats.append(stat)
df = pd.DataFrame(stats).set_index("hash")


############
# PLOTTING #
############

sns.set_theme()


def plot_correlations_matrix(df: pd.DataFrame):
    fig, ax = plt.subplots()
    fig.subplots_adjust(left=0.2, right=1, bottom=0.35)
    fig.suptitle("Correlations Matrix")

    df_corr = df.corr(numeric_only=True)
    sns.heatmap(df_corr, ax=ax)


def plot_compilation_stages(df: pd.DataFrame):
    fig, ax = plt.subplots()
    fig.suptitle("Compilation Stages")
    fig.subplots_adjust(left=0.2)

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
    df = df / df["compilation_total_time_ms"]
    df = df.drop("compilation_total_time_ms")

    sns.barplot(df, ax=ax, orient="h")


def plot_time_distribution(df: pd.DataFrame):
    fig, ax = plt.subplots()
    fig.suptitle("Compilation Time Histogram")

    sns.boxplot(df, x="compilation_total_time_ms", ax=ax, log_scale=True)


def plot_size_to_time_correlations(df: pd.DataFrame):
    fig, (ax1, ax2) = plt.subplots(1, 2)
    fig.suptitle("Size to Time Correlation")
    fig.subplots_adjust(hspace=0.3)

    outliers: pd.DataFrame = df[df["compilation_total_time_ms"] > 10 * 60 * 1000]  # type: ignore
    df = df.drop(outliers.index)  # type: ignore

    sns.regplot(df, ax=ax1, x="sierra_statement_count", y="compilation_total_time_ms")
    ax1.set_title("Sierra Size vs. Compilation Time (w/o outliers)")
    sns.regplot(df, ax=ax2, x="sierra_statement_count", y="compilation_total_time_ms")
    sns.scatterplot(
        outliers,
        ax=ax2,
        x="sierra_statement_count",
        y="compilation_total_time_ms",
        color="orange",
    )
    ax2.set_title("Sierra Size vs. Compilation Time (w/ outliers)")


def plot_pie(c1, c2, attribute):
    def group_small_entries(entries, cutoff):
        new_entries = {}
        for key, group in itertools.groupby(
            entries, lambda k: "others" if (entries[k] < cutoff) else k
        ):
            new_entries[key] = sum([entries[k] for k in list(group)])
        return new_entries

    sns.set_style("whitegrid")
    fig, (ax1, ax2) = plt.subplots(1, 2)
    fig.suptitle(attribute)

    c1_data = c1[attribute]
    c2_data = c2[attribute]

    cutoff = sum(c1_data.values()) * 0.01
    c1_data = group_small_entries(c1_data, cutoff)
    ax1.pie(
        c1_data.values(),
        labels=c1_data.keys(),
    )
    ax1.set_title(c1.name)

    cutoff = sum(c2_data.values()) * 0.01
    c2_data = group_small_entries(c2_data, cutoff)
    ax2.pie(
        c2_data.values(),
        labels=c2_data.keys(),
    )
    ax2.set_title(c2.name)

    sns.set_theme()


def plot_sierra_libfunc_pie(df: pd.DataFrame):
    c1, c2 = get_sample_contracts(df)
    plot_pie(c1, c2, "sierra_libfunc_frequency")


def plot_llvm_instruction_pie(df: pd.DataFrame):
    c1, c2 = get_sample_contracts(df)
    plot_pie(c1, c2, "llvmir_opcode_frequency")


def plot_mlir_by_libfunc_pie(df: pd.DataFrame):
    c1, c2 = get_sample_contracts(df)
    plot_pie(c1, c2, "mlir_operations_by_libfunc")


plot_mlir_by_libfunc_pie(df)
plot_llvm_instruction_pie(df)
plot_sierra_libfunc_pie(df)
plot_size_to_time_correlations(df)
plot_correlations_matrix(df)
plot_compilation_stages(df)
plot_time_distribution(df)

plt.show()
