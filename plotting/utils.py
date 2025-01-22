import pandas as pd


def format_hash(class_hash):
    return f"{class_hash[:6]}..."


def find_span(event, name):
    for span in event["spans"]:
        if name in span["name"]:
            return span
    return None


CHUNKSIZE = 100000


def load_log(path, f):
    dataset = pd.DataFrame()

    with pd.read_json(path, lines=True, typ="series", chunksize=CHUNKSIZE) as chunks:
        for chunk in chunks:
            chunk = chunk.apply(f).dropna().apply(pd.Series)
            if len(chunk) > 0:
                dataset = pd.concat([dataset, chunk])

    return dataset
