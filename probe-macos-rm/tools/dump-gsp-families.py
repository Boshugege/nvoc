#!/usr/bin/env python3
"""Family map of the private GSP control surface (read-only).

Why this exists
---------------
The GSP's own export table lists 1118 methods (an earlier extraction reported
only 889, because a `flags < 0x10000` guard silently dropped 229 real rows -- see
`extract-gsp-method-table.py`, which now finds all 1118 with two scan
alignments).  The id encoding puts the
interface (category) byte at bits 15:8 of the low half, and the two top bits of
that byte are the legacy/privilege marker:

    bit 15 = 0x8000  legacy non-privileged  (client-reachable, forwarded)
    bit 14 = 0x4000  privileged             (SET half of the same interface)

Read as a pair, because bit 14 alone is not privilege: the forwarder's test is
`(cmd & 0xC000) == 0xC000`, and on the 889-row dump exactly its 89 write-half rows
pass it -- while 19 rows carry bit 14 with bit 15 clear, all of them in the public
0x40-0x44 category range (`0x20804006@776`, `0x20804101@8`, ...), and the test
selects none of those.

so `0x2080_901b` (CLK read) and `0x2080_d01c` (CLK write) are the same
interface, one nibble apart.  Grouping the table by that byte is how the
private families (CLK / VOLT / PMGR / THERMAL / PERF / internal) become visible
without guessing ids -- and it is what showed that the earlier "169 CLK /
151 VOLT" figure in the doc was a family *mislabel* (169 is the 0x0a internal
block, 151 is the 0x20 PERF block).

Usage:
    dump-gsp-families.py <gsp-table.tsv> [--host NVRM-kext] [--family 0xb2]

Read-only: reads the tsv and, if given, the kext.
"""
import argparse
import collections
import struct
import sys

ENTRY = 40                      # NVOC_EXPORTED_METHOD_DEF x86_64

# interface id -> family, derived from the *public* SDK: for every
# "#define NV2080_CTRL_CMD_<NAME> (0x2080xxxx)" the category is (id >> 8) & 0x3f.
# Categories with no public define are deliberately left unnamed.
CATEGORY_NAMES = {
    0x00: "NULL/VGPU", 0x01: "GET/GPU/GSP/HSHUB/INTERNAL/RUSD", 0x03: "EVENT",
    0x04: "TIMER", 0x05: "THERMAL", 0x06: "I2C", 0x08: "BIOS",
    0x0A: "INTERNAL/KGR", 0x0B: "INTERNAL", 0x10: "CLK", 0x11: "FIFO",
    0x12: "GR", 0x13: "FB/GMMU/GSP", 0x17: "MC", 0x18: "BUS", 0x20: "PERF",
    0x22: "RC/TDR", 0x23: "INTERNAL", 0x24: "NVD", 0x25: "DMA", 0x26: "PMGR",
    0x27: "GC6/INTERNAL", 0x28: "LPWR", 0x2A: "CE/INTERNAL", 0x30: "NVLINK",
    0x31: "FLCN", 0x34: "ECC", 0x35: "FLA", 0x36: "GSP", 0x38: "GRMGR",
    0x3A: "DMABUF", 0x3B: "BIF", 0x3D: "OS",
}
U32 = struct.Struct("<I")
U64 = struct.Struct("<Q")


def load_gsp(path):
    rows = {}
    for line in open(path):
        if line.startswith("#"):
            continue
        f = line.rstrip("\n").split("\t")
        rows[int(f[0], 16)] = (int(f[1]), int(f[2], 16), f[3])
    return rows


def load_host(path):
    """Host NVOC export table: 40-byte records, ``methodId`` at +0x10.

    This mirrors ``extract-nvrm-method-table.py`` on purpose (2026-10-17): the
    two tools disagreed (589 vs 608 rows) because this one used to walk the
    binary as *contiguous runs* that additionally required ``0 < flags <=
    0xFFFFFFF`` and ``access & ~0x3FFFF == 0``.  That dropped every row whose
    ``flags`` is 0 -- e.g. 0x20801350/0x20801351, two adjacent real rows -- and
    any row that sits next to a non-conforming gap.  A flat 4-byte scan with
    the structural predicate below finds the same 608 rows the extractor
    reports, and finds **no** row in any private clock/voltage family (which is
    the only claim that matters here).  Keep the two tools in step.
    """
    data = open(path, "rb").read()
    limit = len(data)
    host = {}
    for off in range(0, limit - 40, 4):
        mid = U32.unpack_from(data, off + 16)[0]
        psize = U32.unpack_from(data, off + 20)[0]
        cptr = U64.unpack_from(data, off + 24)[0]
        if (mid & 0xFFFF0000) != 0x20800000:
            continue
        if not 0 < psize < 8_000_000:
            continue
        if not 0x1000 <= cptr < limit:
            continue
        host.setdefault(mid, psize)
    return host


def main():
    ap = argparse.ArgumentParser(description=__doc__,
                                 formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("gsp_table")
    ap.add_argument("--host", help="NVRM kext executable, to flag host-registered ids")
    ap.add_argument("--family", help="print every row of one family byte, e.g. 0xb2")
    args = ap.parse_args()

    gsp = load_gsp(args.gsp_table)
    host = load_host(args.host) if args.host else {}

    fam = collections.defaultdict(list)
    for m in gsp:
        fam[(m >> 8) & 0xFF].append(m)

    if args.family:
        b = int(args.family, 16)
        print("# family 0x%02x: %d rows" % (b, len(fam.get(b, []))))
        print("# %-12s %-9s %-8s %-6s %s" % ("id", "paramSize", "flags", "host?", "priv?"))
        for m in sorted(fam.get(b, [])):
            psize, flags, _ar = gsp[m]
            print("0x%08x  %-9d 0x%04x   %-6s %s"
                  % (m, psize, flags, "yes" if m in host else "-",
                     "SET" if (m & 0x4000) else "GET"))
        return 0

    cat = collections.defaultdict(list)
    for m in gsp:
        cat[(m >> 8) & 0x3F].append(m)

    print("# GSP private-surface family map")
    print("# id layout (low 16 bits): bit15 0x8000 = legacy non-privileged (read half),")
    print("# bit14 0x4000 = privileged (write half).  So a category c appears as the byte")
    print("# 0x80|c (read) and 0xC0|c (write); c alone is the public (non-legacy) form.")
    print("#")
    print("# %-6s %-28s %6s %6s %6s" % ("cat", "family", "GET", "SET", "host"))
    for c in sorted(cat):
        ids = cat[c]
        gets = sorted(m for m in ids if not (m & 0x4000))
        sets = sorted(m for m in ids if (m & 0x4000))
        nh = sum(1 for m in ids if m in host)
        print("  0x%02x %-28s %6d %6d %6d"
              % (c, CATEGORY_NAMES.get(c, "(no public define)"), len(gets), len(sets), nh))
    print("# totals: %d GSP rows, %d host rows, %d ids shared, %d GSP-only"
          % (len(gsp), len(host), len(set(gsp) & set(host)),
             len(set(gsp) - set(host))))
    print("#")
    print("# %-8s %-20s %6s %6s %6s   %s"
          % ("byte", "role", "rows", "GET", "SET", "examples"))
    for b in sorted(fam):
        # the legacy pair lives in bits 15:14: 10 = read (0x80|c), 11 = write
        # (0xc0|c).  Testing bit 15 first (as this did) labels every 0xc0|c
        # row a "read half" -- 0xd0 (the clock SET family) included.
        role = ("write half of cat 0x%02x" % (b & 0x3F) if (b & 0xC0) == 0xC0 else
                "read half of cat 0x%02x" % (b & 0x3F) if (b & 0xC0) == 0x80 else
                "public form of cat 0x%02x" % b)
        ids = sorted(fam[b])
        gets = [m for m in ids if not (m & 0x4000)]
        sets = [m for m in ids if (m & 0x4000)]
        ex = ", ".join("0x%08x@%d%s" % (m, gsp[m][0], "H" if m in host else "")
                       for m in ids[:3])
        print("  0x%02x   %-20s %6d %6d %6d   %s"
              % (b, role, len(ids), len(gets), len(sets), ex))
    return 0


if __name__ == "__main__":
    sys.exit(main())
