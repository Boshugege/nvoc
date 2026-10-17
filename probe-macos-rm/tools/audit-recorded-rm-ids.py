#!/usr/bin/env python3
"""Intersect every RM control id referenced in this repo with the driver's table.

Companion to ``extract-nvrm-method-table.py``.  The question this answers is
"is the family we are chasing actually implemented by the kernel RM on this
host?", which a header grep cannot answer and a marshalling experiment can only
answer one id at a time.

    audit-recorded-rm-ids.py method-table.tsv [repo-root]

Scans *.rs / *.md / *.c / *.h / *.py / *.txt / *.toml / *.json under the repo
root for ``0x2080xxxx`` literals, parses the NVOC method id out of the table
(``class 31:16``, legacy 15:14, ``category 13:8``, ``index 7:0``) and prints a
per-category histogram plus the family-level lists.

Read-only: it only reads files.
"""

import argparse
import collections
import pathlib
import re
import sys

SUFFIXES = (".rs", ".md", ".c", ".h", ".py", ".txt", ".toml", ".json")
SKIP_PARTS = ("target", ".git", "node_modules", "third_party")

CATEGORY = {
    0x01: "GPU",
    0x03: "EVENT",
    0x04: "TIMER",
    0x05: "THERMAL",
    0x06: "I2C",
    0x08: "BIOS",
    0x0A: "INTERNAL",
    0x10: "CLK",
    0x11: "FIFO",
    0x12: "GR",
    0x13: "FB",
    0x17: "MC",
    0x18: "BUS",
    0x20: "PERF",
    0x22: "RC",
    0x26: "PMGR",
    0x27: "POWER",
    0x2A: "CE",
    0x30: "NVLINK",
    0x32: "VOLT",
    0x34: "ECC",
    0x36: "GSP",
    0x40: "DISPLAY",
}
# Families the overclocking work actually cares about, reported individually.
FAMILIES = (0x05, 0x10, 0x20, 0x26, 0x27, 0x32)


def load_table(path):
    live = {}
    for line in pathlib.Path(path).read_text().splitlines():
        if not line.strip() or line.startswith("#"):
            continue
        method_id, size, _class, _off, name = line.split("\t")
        live[int(method_id, 16)] = (int(size), name)
    return live


def scan_repo(root):
    found = collections.defaultdict(set)
    pattern = re.compile(r"0x(2080[0-9A-Fa-f]{4})\b")
    for path in pathlib.Path(root).rglob("*"):
        if not path.is_file() or path.suffix not in SUFFIXES:
            continue
        if any(part in SKIP_PARTS for part in path.parts):
            continue
        try:
            text = path.read_text(errors="ignore")
        except OSError:
            continue
        for match in pattern.finditer(text):
            found[int(match.group(1), 16)].add(str(path))
    return found


def category(method_id):
    return (method_id >> 8) & 0x3F


def main():
    ap = argparse.ArgumentParser(
        description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter
    )
    ap.add_argument("table", help="method table TSV from extract-nvrm-method-table.py")
    ap.add_argument("root", nargs="?", default=".", help="repo root (default: .)")
    args = ap.parse_args()

    live = load_table(args.table)
    found = scan_repo(args.root)

    registered = sum(1 for i in found if i in live)
    print("distinct 0x2080xxxx ids referenced : %d" % len(found))
    print("  present in the loaded table     : %d" % registered)
    print("  absent                          : %d" % (len(found) - registered))
    print()

    hist = collections.defaultdict(lambda: [0, 0])
    for method_id in found:
        hist[category(method_id)][0 if method_id in live else 1] += 1
    print("cat   name                in-table  absent")
    for cat in sorted(hist):
        print(
            "0x%02x  %-19s %5d %8d"
            % (cat, CATEGORY.get(cat, "?"), hist[cat][0], hist[cat][1])
        )
    print()

    for cat in FAMILIES:
        ids = sorted(i for i in found if category(i) == cat)
        if not ids:
            continue
        print(
            "0x%02x %s: %s"
            % (
                cat,
                CATEGORY.get(cat, "?"),
                " ".join("0x%08x%s" % (i, "*" if i in live else "") for i in ids),
            )
        )
    print("\n(* = present in the loaded method table)")
    return 0


if __name__ == "__main__":
    sys.exit(main())
