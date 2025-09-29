import json
import datetime
import platform
import psutil
import subprocess
import re


def get_version(crate):
    metadata_string = subprocess.check_output(
        ["cargo", "metadata", "--no-deps", "--format-version", "1"], text=True
    )
    metadata = json.loads(metadata_string)

    replay = next((p for p in metadata["packages"] if p["name"] == "replay"))
    crate = next((p for p in replay["dependencies"] if p["name"] == crate))

    path = crate.get("path", None)
    req = crate.get("req", None)
    source = crate.get("source", None)

    if path:
        # If path is not null, it is a path dependency
        # and we save the version by taking the current git revision
        return subprocess.check_output(
            ["git", "rev-parse", "HEAD"], cwd=path, text=True
        ).strip()
    if req and req != "*":
        # If req is not *, it is crate.io dependency and
        # we just return the used version.
        return req
    elif source:
        # If the source is a git URL, we find the pinned rev
        # and return it.
        match = re.search(r"rev=([a-f0-9]+)", source)
        if match:
            return match.group(1)

    raise Exception("Unsupported dependency version")


def get_cpu():
    system = platform.system()
    if system == "Darwin":
        return subprocess.check_output(
            ["sysctl", "-n", "machdep.cpu.brand_string"], text=True
        ).strip()
    elif system == "Linux":
        # TODO: I did not test it in
        with open("/proc/cpuinfo") as cpuinfo:
            for line in cpuinfo:
                if "model name" in line:
                    return line.strip().split(":")[1].strip()
    else:
        raise Exception("Unsupported system")


info = {
    "Version of cairo-native": get_version("cairo-native"),
    "Version of blockifier": get_version("blockifier"),
    "Date": datetime.datetime.today().strftime("%Y-%m-%d"),
    "OS": platform.system() + " " + platform.release(),
    "Memory": f"{psutil.virtual_memory().total / 2**30:.2f} GiB",
    "CPU": get_cpu(),
}

print(json.dumps(info, indent=4))
