from argparse import ArgumentParser
import pandas as pd

argument_parser = ArgumentParser('Block composition')
argument_parser.add_argument("block_execution_info")
arguments = argument_parser.parse_args()

dataset = pd.read_json(arguments.block_execution_info)
print(dataset)

data_by_block = (
    dataset.group_by([]).agg(
        total_txs=,
        total_tranfers=,
        total_swaps=,
    )
)
