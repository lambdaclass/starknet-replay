import pandas as pd
import json
import os
import itertools
import more_itertools
import matplotlib.pyplot as plt


def flatmap(f, iterable):
    return itertools.chain.from_iterable(map(f, iterable))


def chunks(by, df, chunk_size):
    unique_blocks = sorted(df[by].unique())

    chunks = more_itertools.chunked(unique_blocks, chunk_size)

    return [df[df[by].isin(chunk)] for chunk in chunks]


def chunks(by, df, chunk_size):
    unique_blocks = sorted(df[by].unique())

    chunks = more_itertools.chunked(unique_blocks, chunk_size)

    return [df[df[by].isin(chunk)] for chunk in chunks]


def load_block_composition_data(path, process_fn=None):
    def apply_flattening(block):
        # An entrypoint is a dict of groups of entrypoints (each with objectives)
        # since each group is a tree of calls (an entrypoint can be called during the execution
        # of another) we need to flatten them to make them process friendly
        block["entrypoints"] = list(map(flatten_call_trees, block["entrypoints"]))

        return block

    df = pd.DataFrame()

    for filename in os.listdir(path):
        blocks = json.load(open(path + "/" + filename))

        block_df = pd.DataFrame(blocks)

        df = pd.concat([df, block_df])

    df = df.apply(apply_flattening, axis=1)

    df_exploded = df.explode("entrypoints")

    df_expanded = pd.concat(
        [
            df_exploded.drop(columns="entrypoints").reset_index(drop=True),
            pd.json_normalize(df_exploded["entrypoints"]).reset_index(drop=True),
        ],
        axis=1,
    )
    df_expanded = (
        df_expanded.apply(process_fn, axis=1) if process_fn is not None else df
    )
    df_expanded = df_expanded.dropna().apply(pd.Series)

    return df_expanded


def save_to_path(name):
    output_dir = f"{os.getcwd()}/block_composition_plots"

    if not os.path.exists(output_dir):
        os.mkdir(output_dir)

    file_path = f"{output_dir}/{name}.svg"

    plt.grid(True)
    plt.savefig(file_path)


def flatten_call_trees(entrypoints):
    if entrypoints["validate_call_info"] is not None:
        entrypoints["validate_call_info"] = flatten_call_tree(
            entrypoints["validate_call_info"]
        )

    if entrypoints["execute_call_info"] is not None:
        entrypoints["execute_call_info"] = flatten_call_tree(
            entrypoints["execute_call_info"]
        )

    if entrypoints["fee_transfer_call_info"] is not None:
        entrypoints["fee_transfer_call_info"] = flatten_call_tree(
            entrypoints["fee_transfer_call_info"]
        )

    return entrypoints


def flatten_call_tree(call_tree):
    calls = list(flatmap(flatten_call_tree, call_tree["inner"]))

    calls.append(call_tree["root"])

    return calls
