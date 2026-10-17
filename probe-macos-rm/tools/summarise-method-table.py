#!/usr/bin/env python3
"""Summarise a GSP/host method-table dump: what its own columns declare.

Why this exists: the reverse-engineering claims about *this driver's* dispatch
surface -- which ids are reads, which are writes, whether the table marks a
privileged half -- have to be re-derivable after a driver update instead of
living in prose. The dump is produced by the recount stage (see
outputs/stage-gsp-table-recount-*/gsp-table.tsv); this script reads that file.

What it reports, each with counts rather than adjectives:

  * id-space halves, by the byte that encodes them (a 0x80-0xBF category byte is
    the legacy non-privileged read half, 0xC0-0xFF the write half);
  * the `flags` column's low byte against direction: read-half rows carry 0x48,
    write-half rows 0x44, with the exceptions printed by id so an update shows
    up as a number rather than as a surprise;
  * the `accessRight` column, which on this driver is 0 for every row but one --
    i.e. it does not encode the privilege that makes the write path refuse
    below root, so it must not be cited for that;
  * read/write sibling pairs found by the id arithmetic (write = read | 0x4000),
    split by whether the two paramSizes match, because a same-size sibling is
    what makes "the SET block is the same struct" a supported reading.

Usage: summarise-method-table.py <table.tsv> [--pairs]
"""

import collections
import sys


def half(mid):
    cat = (mid >> 8) & 0xFF
    if cat >= 0xC0:
        return "write"
    if cat >= 0x80:
        return "read"
    return "public"


def load(path):
    rows = []
    with open(path, encoding="utf-8", errors="replace") as handle:
        for line in handle:
            line = line.strip()
            if not line or line.startswith("#"):
                continue
            field = line.split()
            if len(field) < 5:
                continue
            rows.append((int(field[0], 16), int(field[1]), int(field[2], 16),
                         int(field[3])))
    return rows


def main(argv):
    if not argv:
        print(__doc__)
        return 2
    rows = load(argv[0])
    counts = collections.Counter(half(mid) for mid, _, _, _ in rows)
    print(f"rows: {len(rows)}")
    print("halves:", ", ".join(f"{name} {n}" for name, n in counts.most_common()))
    for name in ("public", "read", "write"):
        sub = [r for r in rows if half(r[0]) == name]
        if not sub:
            continue
        flags = collections.Counter(f for _, _, f, _ in sub)
        access = collections.Counter(a for _, _, _, a in sub)
        top = ", ".join(f"0x{k:04x}:{v}" for k, v in flags.most_common(5))
        print(f"  {name:6s} n={len(sub):<4d} accessRight={dict(access.most_common(3))}")
        print(f"         flags: {top}")
        read_bit = sum(1 for _, _, f, _ in sub if f & 0x08)
        write_bit = sum(1 for _, _, f, _ in sub if f & 0x04)
        print(f"         flag bit 0x08 set {read_bit}/{len(sub)}, "
              f"bit 0x04 set {write_bit}/{len(sub)}")
    print()
    print("direction-flag exceptions (a write-half row flagged as a read, or the"
          " reverse):")
    for mid, size, flags, _ in rows:
        side = half(mid)
        want = 0x08 if side == "read" else 0x04 if side == "write" else None
        if want is not None and not flags & want:
            print(f"  0x{mid:08x} {side:5s} size {size:<7d} flags 0x{flags:04x}")
    print()
    by_id = {mid: (size, flags, access) for mid, size, flags, access in rows}
    pairs = []
    for mid, size, flags, access in rows:
        if half(mid) != "read":
            continue
        twin = mid | 0x4000
        if twin in by_id:
            pairs.append((mid, size, flags, twin, by_id[twin]))
    same = [p for p in pairs if p[1] == p[4][0]]
    print(f"read ids with a present |0x4000 twin: {len(pairs)}; "
          f"same paramSize: {len(same)}")
    if "--pairs" in argv:
        for mid, size, flags, twin, (tsize, tflags, _) in pairs:
            mark = "same" if size == tsize else "DIFF"
            print(f"  0x{mid:08x} size {size:<7d} flags 0x{flags:04x} -> "
                  f"0x{twin:08x} size {tsize:<7d} flags 0x{tflags:04x}  [{mark}]")
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
