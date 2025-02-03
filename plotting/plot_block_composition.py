from argparse import ArgumentParser
import os
import pandas as pd
import seaborn as sns
import matplotlib.pyplot as plt
from utils import flatmap
import json

TRANSFER_ENTRYPOINT_HASH = (
    '0x83afd3f4caedc6eebf44246fe54e38c95e3179a5ec9ea81740eca5b482d12e'
)
SWAP_ENTRYPOINT_HASHES = [
    # SWAP_ENTRYPOINT_HASH
    '0x15543c3708653cda9d418b4ccd3be11368e40636c10c44b18cfe756b6d88b29',
    # SWAP_EXACT_TOKEN_TO_ENTRYPOINT_HASH
    '0xe9f3b52dc560050c4c679481500c1b1e2ba7496b6a0831638c1acaedcbc6ac',
    # MULTI_ROUTE_SWAP_ENTRYPOINT_HASH
    '0x1171593aa5bdadda4d6b0efde6cc94ee7649c3163d5efeb19da6c16d63a2a63',
    # SWAP_EXACT_TOKENS_FOR_TOKENS
    '0x3276861cf5e05d6daf8f352cabb47df623eb10c383ab742fcc7abea94d5c5cc',
    # SWAP_EXACT_TOKENS_FOR_TOKENS
    '0x2c0f7bf2d6cf5304c29171bf493feb222fef84bdaf17805a6574b0c2e8bcc87',
]


argument_parser = ArgumentParser('Block composition')
argument_parser.add_argument('block_execution_info')
arguments = argument_parser.parse_args()


def count_transfers(transactions):
    count = 0

    for tx in transactions:
        if tx == None:
            continue

        # in general, a pure transfer is made of two entrypoints: __execute__, transfer
        if (
            tx['execute_call_info'] != None
            and len(tx['execute_call_info']) <= 2
        ):
            for entrypoint in tx['execute_call_info']:
                if entrypoint['selector'] == TRANSFER_ENTRYPOINT_HASH:
                    count += 1

    return count


def count_transfers_ptg(transactions):
    return (
        count_transfers(transactions) / len(transactions) * 100
        if len(transactions) > 0
        else 0
    )


def count_swaps(transactions):
    def is_swap(entrypoint):
        return entrypoint['selector'] in SWAP_ENTRYPOINT_HASHES

    count = 0

    for tx in transactions:
        if tx == None:
            continue

        if tx['execute_call_info'] != None and any(
            is_swap(entrypoint) for entrypoint in tx['execute_call_info']
        ):
            count += 1

    return count


def count_swaps_ptg(transactions):
    return (
        count_swaps(transactions) / len(transactions) * 100
        if len(transactions) > 0
        else 0
    )


def count_tx(transactions):
    txs_without_none = [tx for tx in transactions if tx is not None]
    return len(txs_without_none)


def load_data(path):
    def process(block):
        # 'entrypoints' is an dict of groups of entrypoints (each with objectives)
        # since each group is a tree of calls (an entrypoint can be called during the execution
        # of another entrypoin) we need to flatten them make them process friendly
        block['entrypoints'] = list(
            map(flatten_call_trees, block['entrypoints'])
        )

        return {
            'block': block['block_number'],
            'timestamp': pd.Timestamp(block['block_timestamp']),
            'txs': count_tx(block['entrypoints']),
            'transfers': count_transfers(block['entrypoints']),
            'swaps': count_swaps(block['entrypoints']),
            'transfers_ptg': count_transfers_ptg(block['entrypoints']),
            'swaps_ptg': count_swaps_ptg(block['entrypoints']),
        }

    df = pd.DataFrame()

    for filename in os.listdir(path):
        blocks = json.load(open(path + '/' + filename))

        block_df = pd.DataFrame(blocks)

        df = pd.concat([df, block_df])
    df = df.apply(process, axis=1).dropna().apply(pd.Series)

    return df


def flatten_call_trees(entrypoints):
    if entrypoints['validate_call_info'] != None:
        entrypoints['validate_call_info'] = flatten_call_tree(
            entrypoints['validate_call_info']
        )

    if entrypoints['execute_call_info'] != None:
        entrypoints['execute_call_info'] = flatten_call_tree(
            entrypoints['execute_call_info']
        )

    if entrypoints['fee_transfer_call_info'] != None:
        entrypoints['fee_transfer_call_info'] = flatten_call_tree(
            entrypoints['fee_transfer_call_info']
        )

    return entrypoints


def flatten_call_tree(call_tree):
    calls = list(flatmap(flatten_call_tree, call_tree['inner']))

    calls.append(call_tree['root'])

    return calls


df = load_data(arguments.block_execution_info)

df_by_timestamp = df.groupby(pd.Grouper(key='timestamp', freq='D')).agg(
    avg_txs=('txs', 'mean'),
    avg_transfers=('transfers', 'mean'),
    avg_swaps=('swaps', 'mean'),
    avg_percentage_transfers=('transfers_ptg', 'mean'),
    avg_percentage_swaps=('swaps_ptg', 'mean'),
)

fig, axs = plt.subplots(2, figsize=(10, 7))

sns.lineplot(
    data=df_by_timestamp,
    x='timestamp',
    y='avg_txs',
    ax=axs[0],
    label='average txs',
)
sns.lineplot(
    data=df_by_timestamp,
    x='timestamp',
    y='avg_transfers',
    ax=axs[0],
    label='average transfers',
)
sns.lineplot(
    data=df_by_timestamp,
    x='timestamp',
    y='avg_swaps',
    ax=axs[0],
    label='average swaps',
)
sns.lineplot(
    data=df_by_timestamp,
    x='timestamp',
    y='avg_percentage_transfers',
    ax=axs[1],
    label='average transfers',
)
sns.lineplot(
    data=df_by_timestamp,
    x='timestamp',
    y='avg_percentage_swaps',
    ax=axs[1],
    label='average swaps',
)

axs.flat[0].set(xlabel='day', ylabel='average')
axs.flat[1].set(xlabel='day', ylabel='average (%)')

fig.subplots_adjust(wspace=1, hspace=0.5)

axs[0].set_title('Average txs, transfers and swaps in a block')
axs[1].set_title('Average percentage of swaps and tranfers in a block')


plt.show()
