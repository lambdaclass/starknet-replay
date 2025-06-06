import pandas as pd
from pandas import DataFrame
import json
import os


def format_hash(class_hash):
    return f"{class_hash[:6]}..."


def find_span(event, name):
    for span in event["spans"]:
        if name in span["name"]:
            return span
    return None


def load_jsonl(path, f):
    CHUNKSIZE = 100000
    dataset = pd.DataFrame()

    with pd.read_json(path, lines=True, typ="series", chunksize=CHUNKSIZE) as chunks:
        for chunk in chunks:
            chunk_df: DataFrame = chunk.apply(f).dropna().apply(pd.Series)  # type: ignore
            if len(chunk) > 0:
                dataset = pd.concat([dataset, chunk_df])

    return dataset


def load_json_dir_data(path, f):
    df = pd.DataFrame()

    # iter blocks
    for block_dir in os.listdir(path):
        for tx_file_name in os.listdir(f"{path}/{block_dir}"):
            tx_file = f"{path}/{block_dir}/{tx_file_name}"

            data = pd.DataFrame(json.load(open(tx_file)))

            df = pd.concat([df, data])

    df = df.apply(f, axis=1).dropna().apply(pd.Series)

    return df
