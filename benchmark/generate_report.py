import argparse
import pathlib
import json
import yattag
import base64

STYLESHEET = """
body {
    margin: 40px auto;
    max-width: 21cm;
    line-height: 1.6;
    font-family: sans-serif;
    padding: 0 10px;
}
img {
    max-width: 100%;
    height: auto;
    margin: auto;
}
"""

parser = argparse.ArgumentParser()
parser.add_argument("info", type=pathlib.Path)
parser.add_argument("inputs", nargs="*", type=pathlib.Path)
parser.add_argument("output", type=pathlib.Path)
parser.add_argument("--self-contained", action="store_true")
args = parser.parse_args()

doc, tag, text, line = yattag.Doc().ttl()


def add_document():
    doc.asis("<!DOCTYPE html>")
    with tag("html"):
        add_head()
        add_body()


def add_head():
    with tag("head"):
        with tag("style"):
            doc.asis(STYLESHEET)


def add_body():
    with open(f"{args.info}", "r") as f:
        info = json.load(f)

    with tag("body"):
        title = info.get("Title", "Benchmark")
        line("h1", title)
        add_dictionary(info)
        add_artifacts()


def add_dictionary(data):
    with tag("ul"):
        for key, value in data.items():
            with tag("li"):
                line("b", f"{key}: ")
                text(value)


def add_artifacts():
    for artifact_path in args.inputs:
        artifact_path: pathlib.Path = artifact_path
        metadata_path = artifact_path.with_suffix(".meta.json")

        with open(f"{metadata_path}", "r") as f:
            metadata = json.load(f)

        line("h2", metadata["title"])

        if "description" in metadata:
            line("p", metadata["description"])

        if "statistics" in metadata:
            with tag("p"):
                line("b", "Statistics:")

            add_dictionary(metadata["statistics"])

        if artifact_path.suffix == ".svg":
            if args.self_contained:
                artifact = artifact_path.read_bytes()
                artifact_base64 = str(base64.b64encode(artifact), "utf-8")
                doc.stag("img", src=f"data:image/svg+xml;base64,{artifact_base64}")
            else:
                relative_artifact_path = artifact_path.relative_to(args.output.parent)
                doc.stag("img", src=str(relative_artifact_path))


if __name__ == "__main__":
    add_document()
    with open(args.output, "w") as f:
        f.write(yattag.indent(doc.getvalue()))
