import argparse
import pathlib
import json
import yattag

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
parser.add_argument("inputs", nargs="*", type=pathlib.Path)
parser.add_argument("output", type=pathlib.Path)
args = parser.parse_args()

doc, tag, text, line = yattag.Doc().ttl()


def add_head():
    with tag("head"):
        with tag("style"):
            doc.asis(STYLESHEET)


def add_body():
    with tag("body"):
        line("h1", "Report")
        add_artifacts()


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

            with tag("ul"):
                for key, value in metadata["statistics"].items():
                    with tag("li"):
                        line("b", f"{key}: ")
                        text(value)

        if artifact_path.suffix == ".svg":
            doc.stag("img", src=str(artifact_path))


doc.asis("<!DOCTYPE html>")
with tag("html"):
    add_head()
    add_body()

with open(args.output, "w") as f:
    f.write(yattag.indent(doc.getvalue()))
