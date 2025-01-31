from argparse import ArgumentParser
import os
import pandas as pd
import seaborn as sns
import matplotlib.pyplot as plt
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
        if 'execute_call_info' in tx and len(tx['execute_call_info']) <= 2:
            for exec_call in tx['execute_call_info']:
                if exec_call['selector'] == TRANSFER_ENTRYPOINT_HASH:
                    count += 1

    return count / len(transactions) * 100 if len(transactions) > 0 else 0


def count_swaps(transactions):
    def is_swap(entrypoint):
        return entrypoint['selector'] in SWAP_ENTRYPOINT_HASHES

    count = 0

    for tx in transactions:
        if tx == None:
            continue

        if 'execute_call_info' in tx and any(
            is_swap(entrypoint) for entrypoint in tx['execute_call_info']
        ):
            count += 1

    return count / len(transactions) * 100 if len(transactions) > 0 else 0


def count_tx(transactions):
    txs_without_none = [tx for tx in transactions if tx is not None]
    return len(txs_without_none)


def load_data(path):
    def process(block):
        return {
            'block': block['block_number'],
            'timestamp': pd.Timestamp(block['block_timestamp']),
            'txs': count_tx(block['entrypoints']),
            'transfers': count_transfers(block['entrypoints']),
            'swaps': count_swaps(block['entrypoints']),
        }

    df = pd.DataFrame()

    for filename in os.listdir(path):
        blocks = json.load(open(path + "/" + filename))

        block_df = pd.DataFrame(blocks)

        df = pd.concat([df, block_df])

    df = df.apply(process, axis=1).dropna().apply(pd.Series)
    print(df)
    return df


df = load_data(arguments.block_execution_info)

df_by_timestamp = df.groupby(pd.Grouper(key='timestamp', freq='D')).agg(
    avg_percentaje_txs=('txs', 'mean'),
    avg_percentaje_transfers=('transfers', 'mean'),
    avg_percentaje_swaps=('swaps', 'mean'),
)

figure, ax = plt.subplots(figsize=(10, 5))

print(df_by_timestamp)
sns.lineplot(
    data=df_by_timestamp,
    x='timestamp',
    y='avg_percentaje_txs',
    ax=ax,
    label='average txs',
)
sns.lineplot(
    data=df_by_timestamp,
    x='timestamp',
    y='avg_percentaje_transfers',
    ax=ax,
    label='average transfers (%)',
)
sns.lineplot(
    data=df_by_timestamp,
    x='timestamp',
    y='avg_percentaje_swaps',
    ax=ax,
    label='average swaps (%)',
)

ax.set_xlabel('day')
ax.set_ylabel('average')
ax.set_title('Block Composition')

plt.show()
