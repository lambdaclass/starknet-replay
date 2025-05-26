import pandas as pd
import itertools
from pandas import DataFrame


def flatmap(f, iterable):
    return itertools.chain.from_iterable(map(f, iterable))


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
