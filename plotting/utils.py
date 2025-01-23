import pandas as pd


def format_hash(class_hash):
    return f"{class_hash[:6]}..."


def find_span(event, name):
    for span in event["spans"]:
        if name in span["name"]:
            return span
    return None


def load_compilation_logs(path):
    def canonicalize(event):
        # keep contract compilation finished logs
        if "contract compilation finished" not in event["fields"]["message"]:
            return None

        compilation_span = find_span(event, "contract compilation")
        if compilation_span is None:
            return None

        if "vm" in event["fields"]["message"]:
            executor = "vm"
        elif "native" in event["fields"]["message"]:
            executor = "native"
        else:
            raise Exception("Invalid Executor")

        return {
            "class hash": compilation_span["class_hash"],
            "time": float(event["fields"]["time"]),
            "size": float(event["fields"]["size"]) / 1024,
            "length": float(compilation_span["length"]) / 1024,
            "executor": executor,
        }

    return load_jsonl(path, canonicalize)


def load_jsonl(path, f):
    CHUNKSIZE = 100000
    dataset = pd.DataFrame()

    with pd.read_json(path, lines=True, typ="series", chunksize=CHUNKSIZE) as chunks:
        for chunk in chunks:
            chunk = chunk.apply(f).dropna().apply(pd.Series)
            if len(chunk) > 0:
                dataset = pd.concat([dataset, chunk])

    return dataset
