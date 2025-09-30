import argparse
import pathlib
import json
import yattag

parser = argparse.ArgumentParser(
    description="""
        Combines all benchmarking artifacts into a single HTML report.
    """,
)
parser.add_argument(
    "info",
    type=pathlib.Path,
    help="""
        Path to a JSON file containing general benchmark information.
        This information will be included at the top of the report.
    """,
)
parser.add_argument(
    "artifacts",
    nargs="*",
    type=pathlib.Path,
    help="""
        Artifacts to be included in the report. For each artifact, a sidecar
        metadata file with extension "meta.json" is expected. This sidecar file
        includes information on the artifact, and how it should be presented.
    """,
)
parser.add_argument(
    "output",
    type=pathlib.Path,
    help="""
        Output path for the generated HTML report.
    """,
)
args = parser.parse_args()

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
    for artifact_path in args.artifacts:
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
            relative_artifact_path = artifact_path.relative_to(args.output.parent)
            doc.stag("img", src=str(relative_artifact_path))


if __name__ == "__main__":
    add_document()
    with open(args.output, "w") as f:
        f.write(yattag.indent(doc.getvalue()))
