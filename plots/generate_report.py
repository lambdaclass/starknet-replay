import argparse
import pathlib
import json
import textwrap

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

document = f"""\
<html>
<head><style>{STYLESHEET}</style></head>
<body>
<h1>Report</h1>
"""

for artifact_path in args.inputs:
    artifact_path: pathlib.Path = artifact_path
    metadata_path = artifact_path.with_suffix(".meta.json")

    with open(f"{metadata_path}", "r") as f:
        metadata = json.load(f)

    document += textwrap.dedent(f"""
        <h2>{metadata["title"]}</h2>
    """)

    if "description" in metadata:
        document += textwrap.dedent(f"""
            <p>{metadata["description"]}</p>
        """)

    if "statistics" in metadata:
        document += textwrap.dedent("""
            <p><b>Statistics:</b></p>
            <ul>
        """)
        for key, value in metadata["statistics"].items():
            document += textwrap.dedent(f"""
                <li><b>{key}:</b> {value}</li>
            """)
            pass
        document += textwrap.dedent("""
            </ul>
        """)

    if artifact_path.suffix == ".svg":
        document += textwrap.dedent(f"""
            <img src={artifact_path}></img>
        """)

document += """\
</body>
</html>
"""

with open(args.output, "w") as f:
    f.write(document)
