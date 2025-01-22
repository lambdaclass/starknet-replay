from argparse import ArgumentParser
import pandas as pd

TRANSFER_ENTRYPOINT_HASH = (
    '0x83afd3f4caedc6eebf44246fe54e38c95e3179a5ec9ea81740eca5b482d12e'
)
SWAP_ENTRYPOINT_HASH = (
    '0x15543c3708653cda9d418b4ccd3be11368e40636c10c44b18cfe756b6d88b29'
)
MULTI_ROUTE_SWAP_ENTRYPOINT_HASH = (
    '0x1171593aa5bdadda4d6b0efde6cc94ee7649c3163d5efeb19da6c16d63a2a63'
)

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

    return count


def count_swaps(transactions):
    def is_swap(entrypoint):
        return (
            entrypoint['selector'] == SWAP_ENTRYPOINT_HASH
            or entrypoint['selector'] == MULTI_ROUTE_SWAP_ENTRYPOINT_HASH
        )

    count = 0

    for tx in transactions:
        if tx == None:
            continue

        if 'execute_call_info' in tx:
            if any(
                is_swap(entrypoint) for entrypoint in tx['execute_call_info']
            ):
                count += 1

    return count


def count_tx(transactions):
    txs_without_none = [tx for tx in transactions if tx is not None]
    return len(txs_without_none)


dataset = pd.read_json(
    arguments.block_execution_info, orient='index'
).transpose()

transfers_by_block = dataset.agg(count_transfers)
swaps_by_block = dataset.agg(count_swaps)
tx_by_block = dataset.agg(count_tx)

print('AVERAGE TRANSFER IN BLOCK:', transfers_by_block.mean().astype(int))
print('AVERAGE SWAPS IN BLOCK:', swaps_by_block.mean().astype(int))
print('AVERAGE TX IN BLOCK:', tx_by_block.mean().astype(int))
