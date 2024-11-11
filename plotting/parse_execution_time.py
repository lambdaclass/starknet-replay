from argparse import ArgumentParser

argument_parser = ArgumentParser('Stress Test Plotter')
argument_parser.add_argument("input")
arguments = argument_parser.parse_args()

import pandas as pd

CHUNKSIZE = 10000

def canonicalize(event):
    # skip caching logs
    if find_span(event, "caching block range") != None:
        return None

    # keep contract execution finished logs
    if "contract execution finished" not in event["fields"]["message"]:
            return None

    # filter target classes
    class_hash = hex(int(event["span"]["class_hash"]))
    time = float(event["fields"]["time"])
    return {
        "class hash": class_hash,
        "time": time,
    }

def find_span(event, name):
    for span in event["spans"]:
        if name in span["name"]:
            return span
    return None

def process(input):
    output = f"{input}-execution"
    with open(output, "w"):
        pass

    with pd.read_json(input, lines=True, typ="series", chunksize=CHUNKSIZE) as chunks:
        for chunk in chunks:
            chunk = chunk.apply(canonicalize).dropna().apply(pd.Series)
            if len(chunk) > 0:
                chunk.to_json(output, orient='records', lines=True, mode='a')

process(arguments.input)
