#!/usr/bin/env python3
"""Audit the ids and sizes this probe sends against the driver's own table.

The probe's read batteries are arrays of `{ name, id, size, ... }` cases, and the
corrected GSP dump (outputs/stage-gsp-table-recount-*/gsp-table.tsv) declares a
paramSize for every id the driver actually dispatches. Comparing the two answers
three questions that would otherwise rest on memory:

  1. every id the probe names is one the driver's table declares (or is a
     deliberate negative control, which has to be listed as such);
  2. every id the probe *reads* is non-privileged -- the host forwarder refuses
     (cmd & 0xC000) == 0xC000 below root, so a read-half id carrying those bits
     would have failed, and none ever has: the model is self-consistent or the
     audit says where it is not;
  3. at least one case in the battery uses the table's own size for that id, so
     the struct under test is the one the driver dispatches, not a lookalike.

Sizes that differ from the table's are *not* automatically wrong: several cases
deliberately send the documented NVAPI caller size, or a truncated variant, and
name the arithmetic in the case label. They are printed for review rather than
judged.

Usage: audit-probe-ids.py <gsp-table.tsv> <source.c> [more sources...]
       audit-probe-ids.py --host <host-table.tsv> <gsp-table.tsv> <source.c> ...

A missing id is only interesting once both tables have been consulted: a public
id may be dispatched by the host side rather than forwarded to the GSP, so the
audit says "absent" only when neither table declares it. Case labels are printed
next to any size that is not the table's, because most such cases are deliberate
(caller-side struct sizes, truncations, edge cases) and the label is where that
is justified.
"""

import re
import sys
from collections import Counter

ID = re.compile(r"0x2080[0-9a-fA-F]{4}")
CASE = re.compile(r'(?:\{\s*)?"([^"]{0,60})"\s*,\s*'
                  r"(0x2080[0-9a-fA-F]{4})u?\s*,\s*(\d+)")


def load_table(path):
    table = {}
    with open(path, encoding="utf-8", errors="replace") as handle:
        for line in handle:
            line = line.strip()
            if not line or line.startswith("#"):
                continue
            field = line.split()
            if len(field) < 5:
                continue
            table[int(field[0], 16)] = (int(field[1]), int(field[2], 16))
    return table


def privileged(mid):
    return (mid & 0xC000) == 0xC000


def half(mid):
    cat = (mid >> 8) & 0xFF
    if cat >= 0xC0:
        return "write"
    if cat >= 0x80:
        return "read"
    return "public"


def main(argv):
    if "--host" in argv:
        i = argv.index("--host")
        host_table = load_table(argv[i + 1])
        argv = argv[:i] + argv[i + 2:]
    else:
        host_table = {}
    if len(argv) < 2:
        print(__doc__)
        return 2
    table = load_table(argv[0])
    named, cases = set(), []
    for path in argv[1:]:
        text = open(path, encoding="utf-8", errors="replace").read()
        named.update(int(m, 16) for m in ID.findall(text))
        cases.extend((int(b, 16), int(c), label, path)
                     for label, b, c in CASE.findall(text))

    print(f"table: {len(table)} ids"
          + (f"; host table: {len(host_table)} ids" if host_table else ""))
    print(f"probe sources: {len(argv) - 1}; ids named: {len(named)}; "
          f"(id,size) cases: {len(cases)}")
    known = table.keys() | host_table.keys()
    in_table = {m for m in named if m in known}
    absent = sorted(named - in_table)
    print()
    print("ids named, by half:", dict(Counter(half(m) for m in named)))
    print(f"  declared by a table: {len(in_table)}"
          + (f" (host side: {sum(1 for m in named if m in host_table)})"
             if host_table else ""))
    print(f"  in neither table: {len(absent)}")
    for m in absent:
        print(f"    0x{m:08x} {half(m):5s} privileged={privileged(m)}")
    print()
    print("read-path self-consistency (a privileged id can never have been read"
          " successfully below root):")
    bad = sorted(m for m in named if half(m) != "write" and privileged(m))
    print(f"  non-write ids that are privileged: {len(bad)}"
          + ("" if not bad else "  " + " ".join(f"0x{m:08x}" for m in bad)))
    w = sorted(m for m in named if half(m) == "write")
    print(f"  write-half ids the probe names: {len(w)}  "
          + " ".join(f"0x{m:08x}" for m in w))
    print()
    print("case sizes vs the table:")
    exact, other = 0, []
    for mid, size, label, path in cases:
        src = table.get(mid) or host_table.get(mid)
        tsize = src[0] if src else None
        if size == tsize:
            exact += 1
        else:
            other.append((mid, size, tsize, label, path))
    print(f"  cases using the table's own size: {exact}")
    print(f"  cases using something else: {len(other)} (labels are the"
          " justification, so read them before calling any of these wrong)")
    seen = set()
    for mid, size, tsize, label, path in sorted(other):
        key = (mid, size)
        if key in seen:
            continue
        seen.add(key)
        t = "in neither table" if tsize is None else f"table {tsize}"
        print(f"    0x{mid:08x} sends {size:<7d} ({t}): {label!r}"
              f"  [{path.split('/')[-1]}]")
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
