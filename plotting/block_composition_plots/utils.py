import pandas as pd
import json
import os
import itertools


def flatmap(f, iterable):
    return itertools.chain.from_iterable(map(f, iterable))


def load_block_composition_data(path, process_fn):
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

    df = (
        df.apply(apply_flattening, axis=1)
        .apply(process_fn, axis=1)
        .dropna()
        .apply(pd.Series)
    )

    return df


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
