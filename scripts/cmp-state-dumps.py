#!/usr/bin/env python

import argparse
import glob
import re
from multiprocessing import Pool
from collections import defaultdict


POOL_SIZE = 8


def main():
    files = glob.glob("state_dumps/vm/*/*.json")

    print(f"Starting comparison with {POOL_SIZE} workers")

    with Pool(POOL_SIZE) as pool:
        results = pool.map(compare, files)

    print("Finished comparison")

    stats = defaultdict(int)
    for status, _, _ in results:
        stats[status] += 1

    print()
    for key, count in stats.items():
        print(key, count)


def compare(vm_dump_path: str):
    native_dump_path = vm_dump_path.replace("vm", "native")

    if not (m := re.findall(r"/(0x.*).json", vm_dump_path)):
        raise Exception("bad path")
    tx = m[0]

    if not (m := re.findall(r"block(\d+)", vm_dump_path)):
        raise Exception("bad path")
    block = m[0]

    try:
        with open(native_dump_path) as f:
            native_dump = f.read()
        with open(vm_dump_path) as f:
            vm_dump = f.read()
    except:  # noqa: E722
        return ("MISS", block, tx)

    native_dump = re.sub(r".*revert_error.*", "", native_dump, 1)
    vm_dump = re.sub(r".*revert_error.*", "", vm_dump, 1)

    if native_dump == vm_dump:
        return ("MATCH", block, tx)
    else:
        return ("DIFF", block, tx)


if __name__ == "__main__":
    parser = argparse.ArgumentParser(prog="cmp-state-dumps")
    parser.add_argument("-d", "--delete", action="store_true")
    global args
    args = parser.parse_args()

    main()
