#!/usr/bin/env python3
"""Extract the GSP firmware's own RM control method table (read-only).

Why this exists
---------------
On this host the private clock/voltage ids (0x20809061, 0x2080B201, ...) are
answered even though the *host* NVRM's NVOC export table has no row for them and
the literal id appears nowhere in the host binaries.  The responder is the GSP
firmware: any control whose id has the legacy-non-privileged bit
``RM_GSS_LEGACY_MASK = 0x00008000`` set is handed to the GSP by
``RmGssLegacyRpcCmd()`` (src/nvidia/interface/deprecated/rmapi_gss_legacy_control.c)
before the normal table dispatch, and the GSP validates the paramSize against
its own table.  So the authoritative size for those ids comes from the firmware
image, not from the host table and not from the NVAPI caller struct.

The firmware file is a normal ELF64 file (RISC-V, e_machine 243) whose
``.fwimage`` section holds the loaded image.  Its per-class export arrays are
arrays of 32-byte records with the methodId first::

    +0x00  methodId     u32
    +0x04  paramSize    u32
    +0x08  pClassInfo   u64  (image virtual address, base 0x4000000)
    +0x10  pFunc        u64
    +0x18  flags        u32   (NOT bounded by 0xFFFF -- see the note in extract())
    +0x1c  accessRight  u32

The ``flags`` field is not a small enum: live rows carry e.g. 0x48, 0x448,
0x10248, 0x81074a, 0x82004a, 0x83034a.  A ``flags < 0x10000`` guard (which
this tool used to apply) is therefore a silent row-dropper, not a validity
test.  Counts from the corrected extraction: 1118 rows total, CLK legacy
0x90 = 74 + 0xd0 = 19, VOLT 0xb2 = 13 + 0xf2 = 4.

Usage::

    extract-gsp-method-table.py /Users/Shared/nvfw/nvidia/<ver>/gsp_tu10x.bin \
        > outputs/gsp-table-tu10x-<ver>.tsv

Read-only: the tool only reads the file that the driver already loaded.
"""
import argparse
import collections
import struct
import sys

REC = 32
IMG_BASE = 0x4000000          # load base of .fwimage on this driver
NULL_PTRS = (0,)              # pClassInfo/pFunc are never null in a live row


def load_image(path):
    b = open(path, "rb").read()
    if b[:4] != b"\x7fELF":
        sys.exit("not an ELF file")
    e_machine, = struct.unpack_from("<H", b, 18)
    e_shoff, = struct.unpack_from("<Q", b, 40)
    e_shentsize, = struct.unpack_from("<H", b, 58)
    e_shnum, = struct.unpack_from("<H", b, 60)
    secs = [struct.unpack_from("<IIQQQQIIQQ", b, e_shoff + i * e_shentsize)
            for i in range(e_shnum)]
    img = [s for s in secs if s[1] == 1 and s[5] > 0x100000]
    if not img:
        sys.exit("no large PROGBITS image section found")
    return b, e_machine, img[0][4], img[0][5]


def extract(path):
    b, e_machine, start, size = load_image(path)
    lo, hi = IMG_BASE, IMG_BASE + size
    rows = {}
    off = start
    while off + REC <= start + size:
        mid, psz = struct.unpack_from("<II", b, off)
        pcls, pfn = struct.unpack_from("<QQ", b, off + 8)
        fl, ar = struct.unpack_from("<II", b, off + 24)
        # NOTE (2026-10-17): there used to be two more guards here --
        # ``fl < 0x10000 and ar < 0x10000``.  They silently dropped 229 real
        # rows (889 reported instead of 1118), because the ``flags`` field of
        # a live row routinely carries bits above 16: e.g. 0x20809006 has
        # flags 0x81074a and 0x20809062 has 0x82004a.  Both of those ids were
        # therefore reported "absent from the GSP table" while they are in
        # fact rows with paramSize 8 and 648.  The structural checks below
        # (id class 0x2080, plausible size, both pointers inside the loaded
        # image) already reject every non-row: with the guards removed, a
        # 4-byte-aligned scan and a 32-byte-strided scan find the *same*
        # 1118 rows, i.e. the relaxed predicate adds no false positives.
        if ((mid >> 16) == 0x2080 and 0 < psz < 8_000_000
                and lo <= pcls < hi and lo <= pfn < hi
                and pcls not in NULL_PTRS and pfn not in NULL_PTRS):
            rows.setdefault(mid, (psz, fl, ar, off))
            off += REC
            continue
        off += 4
    return b, e_machine, rows


def main():
    ap = argparse.ArgumentParser(description=__doc__,
                                 formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("firmware")
    ap.add_argument("--family", action="store_true",
                    help="print only the clock/voltage/thermal/perf categories")
    args = ap.parse_args()

    _b, e_machine, rows = extract(args.firmware)
    # NOTE: the category keys are printed as *hex strings*.  They used to be
    # printed as bare ints, and a "10: 169" key was then read as "cat 0x10
    # (CLK) = 169" -- it is actually category 0x0a (internal).  Do not change
    # this back to a plain dict of ints.
    cats = collections.Counter(((m >> 8) & 0x3F) for m in rows)
    sys.stderr.write("# %s: %u rows, e_machine=%u, categories={%s}\n"
                     % (args.firmware, len(rows), e_machine,
                        ", ".join("0x%02x: %d" % (c, cats[c])
                                  for c in sorted(cats))))

    def cat(m):
        return (m >> 8) & 0x3F

    print("# methodId\tparamSize\tflags\taccessRight\tfileOff")
    for m in sorted(rows):
        psz, fl, ar, off = rows[m]
        if args.family and cat(m) not in (0x05, 0x0A, 0x10, 0x20, 0x26, 0x27,
                                          0x32, 0x90, 0xB2, 0xD0, 0xF2):
            continue
        print("0x%08x\t%d\t0x%02x\t%d\t0x%x" % (m, psz, fl, ar, off))
    return 0


if __name__ == "__main__":
    sys.exit(main())
