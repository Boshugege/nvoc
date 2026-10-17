#!/usr/bin/env python3
"""Map GSP firmware control handlers: GET/SET twins, sizes and handler code.

Why this exists
---------------
The private clock/voltage/thermal ids on this host are answered by the GSP
firmware, not by the host NVRM, so the authoritative paramSize and the
authoritative marshalling logic both live in ``gsp_tu10x.bin``.  Two earlier
mistakes came from *not* reading that file properly:

1. a ``flags < 0x10000`` guard silently dropped 229 of the 1118 table rows
   (fixed in ``extract-gsp-method-table.py``);
2. the SET twin of a GET id was assumed to carry the same ``paramSize``.  It
   does not: ``0x20809040 @4808`` pairs with ``0x2080d041 @648``.  A SET id is
   resolved by the GSP against its *own* row, so inheriting the GET size is a
   real, measurable way to get ``NV_ERR_INVALID_ARGUMENT``.

Id encoding (the part that is easy to get wrong)
------------------------------------------------
The control id's second byte carries the interface in its low 6 bits and two
marker bits above it -- the same encoding ``dump-gsp-families.py`` documents:

    bit 15 (0x80) = legacy non-privileged   (client-reachable, forwarded)
    bit 14 (0x40) = privileged              (the SET half of the interface)

So the relation between a GET id and its SET twin is **flip bit 14 and advance
the index by one**, with the interface and bit 15 unchanged::

    GET 0x2080_901b  (iface 0x10, idx 27)  <->  SET 0x2080_d01c (iface 0x10, idx 28)

This is a *general* rule over the table, and it is bidirectional: applying it
to every bit14-clear row yields exactly the same 63 pairs as applying the
inverse to every bit14-set row.  Three earlier formulations were wrong, and all
three produced plausible-looking ids:

* ``(mid >> 8) & 0x3F`` read as a *category index*, then adding 0x40 to it as
  if it were a per-family step.  That works for CLK/VOLT/THERMAL/PMGR/PERF by
  coincidence (their interface bytes really are 0x10/0x32/0x05/0x26/0x20 and
  bit 14 really is 0x40), but it cannot express the other 816 rows.
* masking only the interface byte with ``~0x3F00`` leaves bit 7 of the
  interface standing, so 0x90 survives into the "write" id.
* OR-ing the new low byte onto the old one (``0x1b | 0x1c == 0x1f``) invents an
  id that also exists in the table, so the error survives a spot check.  The
  same OR bug on the *interface* byte produced the **false pair**
  ``0x2080852a -> 0x2080852b``: both rows exist (so it looks like a hit), but
  both are bit14-clear *read* controls, and the real SET twin is the absent
  ``0x2080c52b``.  A twin id that merely exists is not evidence the rule is
  right; ``--selftest`` asserts the counter-example.

Handler code
------------
This tool reads each row's ``pFunc`` -- the RISC-V entry point the GSP actually
calls for that control -- and its ``pClassInfo``.  Findings on 610.57.04:

* all 1118 rows carry a **distinct**, non-null ``pFunc``, always inside the
  file-backed part of ``.fwimage``, so every handler is statically
  disassemblable (no firmware-blind row);
* every row shares **one** ``pClassInfo`` (``0x406ce48``), i.e. that pointer is
  a class descriptor, not a per-method struct;
* the handlers are thin trampolines: they walk a runtime object graph
  (``*(ctx+0x158) -> +0xE0``, then a fixed object), load a vtable pointer and
  ``jalr`` through a virtual slot, forwarding the caller's parameter block.
  So the *static* part of a handler tells you which object kind and which
  virtual operation implement the control; the *runtime* part fills the fields.
  That split is why "GET succeeded" cannot be read as "SET will succeed" --
  see ``WRITE-VERIFICATION-PLAN.md`` S7.2 (G6), which enumerates the extra
  pre-conditions only the SET thunk tests.

Usage::

    gsp-handler-map.py /path/to/gsp_tu10x.bin --families
    gsp-handler-map.py /path/to/gsp_tu10x.bin --twins
    gsp-handler-map.py /path/to/gsp_tu10x.bin --id 0x2080901b --disasm 40
    gsp-handler-map.py /path/to/gsp_tu10x.bin --selftest

Read-only: this tool only reads the firmware file the driver already loaded.
"""

import argparse
import collections
import struct
import sys

REC = 32
IMG_BASE = 0x4000000  # load base of .fwimage (verified on 610.57.04)
ELF_HDR = 0x40  # file offset of .fwimage == image vaddr base

PRIV_BIT = 0x4000  # bit 14: the privileged / SET half
LEGACY_BIT = 0x8000  # bit 15: legacy non-privileged
IFACE_MASK = 0x3F  # interface index within the second byte

# Interface -> family, carried from dump-gsp-families.py (derived from the
# public SDK: for every "#define NV2080_CTRL_CMD_<NAME> (0x2080xxxx)" the
# interface is (id >> 8) & 0x3f).  Names are only attached where the public
# SDK actually defines the interface -- naming one from the table alone would
# be a guess.
#
# 0x32 is the one interface that matters most here and is deliberately marked:
# it carries the whole private VOLT family (0x2080b201/b202/b213, SET half
# 0x2080f214) and OGKM 610.57.04's ctrl2080volt.h contains **zero**
# "0x2080xxxx" defines, while ctrl2080clk.h contains one (0x20801037) and
# ctrl2080perf.h fifteen.  So "VOLT has no public name" is a measured absence,
# not a gap in this table.
IFACE_NAMES = {
    0x00: "NULL/VGPU",
    0x01: "GET/GPU/GSP/HSHUB/INTERNAL/RUSD",
    0x03: "EVENT",
    0x04: "TIMER",
    0x05: "THERMAL",
    0x06: "I2C",
    0x08: "BIOS",
    0x0A: "INTERNAL/KGR",
    0x0B: "INTERNAL",
    0x10: "CLK",
    0x11: "FIFO",
    0x12: "GR",
    0x13: "FB/GMMU/GSP",
    0x17: "MC",
    0x18: "BUS",
    0x20: "PERF",
    0x22: "RC/TDR",
    0x23: "INTERNAL",
    0x24: "NVD",
    0x25: "DMA",
    0x26: "PMGR",
    0x27: "GC6/INTERNAL",
    0x28: "LPWR",
    0x2A: "CE/INTERNAL",
    0x30: "NVLINK",
    0x31: "FLCN",
    0x32: "VOLT (no public define)",
    0x34: "ECC",
    0x35: "FLA",
    0x36: "GSP",
    0x38: "GRMGR",
    0x3A: "DMABUF",
    0x3B: "BIF",
    0x3D: "OS",
}


def load_image(path):
    b = open(path, "rb").read()
    if b[:4] != b"\x7fELF":
        sys.exit("not an ELF file")
    (e_machine,) = struct.unpack_from("<H", b, 18)
    (e_shoff,) = struct.unpack_from("<Q", b, 40)
    (e_shentsize,) = struct.unpack_from("<H", b, 58)
    (e_shnum,) = struct.unpack_from("<H", b, 60)
    secs = [
        struct.unpack_from("<IIQQQQIIQQ", b, e_shoff + i * e_shentsize)
        for i in range(e_shnum)
    ]
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
        # Pure structural predicate.  Do NOT reintroduce a bound on ``flags``:
        # live rows carry 0x81074a / 0x82004a / 0x10248, so ``flags < N``
        # silently drops real rows (229 of them on this image).
        if (
            (mid >> 16) == 0x2080
            and 0 < psz < 8_000_000
            and lo <= pcls < hi
            and lo <= pfn < hi
        ):
            rows.setdefault(mid, (psz, fl, ar, off, pcls, pfn))
            off += REC
            continue
        off += 4
    # highest vaddr actually backed by a file byte
    file_hi = IMG_BASE + (len(b) - start)
    return b, e_machine, rows, file_hi


def interface_of(mid):
    """The interface index: low 6 bits of the id's second byte.

    Masked with 0x3F on purpose.  This is an interface *index*, not a family
    step -- the 0x40 above it is bit 14, the privileged marker, and the twin
    rule treats it as a bit rather than as arithmetic on the interface.
    """
    return (mid >> 8) & IFACE_MASK


def family_of(mid):
    return IFACE_NAMES.get(interface_of(mid), "?")


def is_privileged(mid):
    """True for the SET half of an interface (bit 14)."""
    return bool(mid & PRIV_BIT)


def is_legacy(mid):
    """True for the client-reachable legacy forwarding form (bit 15)."""
    return bool(mid & LEGACY_BIT)


def twin_of(mid):
    """The opposite half of this control's GET/SET pair, or None.

    Flip bit 14 and advance/retreat the index by one, leaving bit 15 and the
    interface index untouched::

        0x2080901b (iface 0x10 idx 27, read)  -> 0x2080d01c (idx 28, write)
        0x2080d01c (iface 0x10 idx 28, write) -> 0x2080901b (idx 27, read)

    Never OR the new low byte onto the old one -- ``0x1b | 0x1c == 0x1f`` is a
    different, existing id.
    """
    idx = (mid & 0xFF) + (-1 if is_privileged(mid) else 1)
    if not 0 <= idx <= 0xFF:
        return None
    return (mid & ~0xFFFF) | (((mid >> 8) & 0xFF) ^ (PRIV_BIT >> 8)) << 8 | idx


def handler_head(b, file_hi, pfn, n=24):
    """Disassemble the first instructions of a handler, if capstone is there.

    Returns (list_of_text, note).  Absence of capstone is not an error: the
    rest of the tool (twins and sizes) needs no disassembler.
    """
    if pfn >= file_hi:
        return [], "pFunc not file-backed"
    try:
        import capstone
    except ImportError:
        return [], "capstone not installed"
    md = capstone.Cs(capstone.CS_ARCH_RISCV, capstone.CS_MODE_RISCV64)
    fo = ELF_HDR + (pfn - IMG_BASE)
    out = []
    for ins in md.disasm(b[fo : fo + 4 * n], pfn):
        out.append(
            "%#010x  %-10s %s"
            % (ins.address, ins.bytes.hex(), ins.mnemonic + " " + ins.op_str)
        )
    return out, ""


def twin_pairs(rows):
    """Every GET/SET pair with both rows present, keyed by the GET id.

    Enumerated from the bit14-clear side only, so each pair appears once, and
    cross-checked against the inverse direction: a rule that is right in one
    direction and wrong in the other would show up as a mismatch.
    """
    fwd = {}
    for mid in sorted(rows):
        if is_privileged(mid):
            continue
        tw = twin_of(mid)
        if tw is not None and tw in rows:
            fwd[mid] = tw
    rev = set()
    for mid in sorted(rows):
        if not is_privileged(mid):
            continue
        tw = twin_of(mid)
        if tw is not None and tw in rows:
            rev.add((tw, mid))
    if rev != set(fwd.items()):
        sys.exit("twin rule is not bidirectional: fwd=%d rev=%d" % (len(fwd), len(rev)))
    return fwd


def cmd_twins(args):
    b, em, rows, file_hi = extract(args.firmware)
    pairs = twin_pairs(rows)
    print("# GSP GET/SET twin map (firmware %s, e_machine=%u)" % (args.firmware, em))
    print("# rule: flip bit14 (privileged) and idx+1; interface and bit15 unchanged")
    print(
        "# %-12s %-7s %-22s %-12s %-7s %-9s %s"
        % ("get_id", "size", "interface", "set_id", "size", "size_same", "note")
    )
    mismatched = 0
    for mid in sorted(pairs):
        tw = pairs[mid]
        gs, ss = rows[mid][0], rows[tw][0]
        same = gs == ss
        if not same:
            mismatched += 1
        print(
            "%-14s %-7d %-22s %-12s %-7d %-9s %s"
            % (
                "0x%08x" % mid,
                gs,
                family_of(mid),
                "0x%08x" % tw,
                ss,
                "yes" if same else "NO",
                "" if same else "SIZE DIFFERS - resolve SET by its own row",
            )
        )
    # a SET row with no GET row is a write-only control and matters just as much
    write_only = [
        m for m in sorted(rows) if is_privileged(m) and twin_of(m) not in rows
    ]
    print(
        "# pairs present on both sides: %d, of which size-mismatched: %d"
        % (len(pairs), mismatched)
    )
    print("# write-only SET rows (no GET twin): %d" % len(write_only))
    for m in write_only[: args.write_only]:
        print("#   0x%08x %-22s size %d" % (m, family_of(m), rows[m][0]))
    if len(write_only) > args.write_only:
        print("#   ... %d more" % (len(write_only) - args.write_only))
    return 0


def cmd_family(args):
    b, em, rows, file_hi = extract(args.firmware)
    print("# GSP family map (firmware %s)" % args.firmware)
    print(
        "# %-24s %-5s %-12s %-9s %-10s %-12s %-12s %-6s %s"
        % ("family", "iface", "id", "size", "flags", "pFunc", "twin", "rw", "disasm")
    )
    counts = collections.Counter()
    for mid in sorted(rows):
        iface = interface_of(mid)
        # --family-clk keeps only the interfaces that have a public name, i.e.
        # the ones this survey can attribute.  Use --families --all to see the
        # unnamed remainder (and note that 0x32 now carries one).
        if args.family_clk and not args.all and iface not in IFACE_NAMES:
            continue
        counts[iface] += 1
        tw = twin_of(mid)
        head, note = handler_head(b, file_hi, rows[mid][5], args.disasm)
        print(
            "%-24s 0x%02x  0x%08x %-9d 0x%08x %#012x %-12s %-6s %s"
            % (
                family_of(mid),
                iface,
                mid,
                rows[mid][0],
                rows[mid][1],
                rows[mid][5],
                ("0x%08x" % tw) if tw else "-",
                "SET" if is_privileged(mid) else "GET",
                ("ok" if head else note),
            )
        )
        if args.verbose and head:
            for line in head:
                print("        %s" % line)
    print(
        "# counts: %s"
        % ", ".join("0x%02x=%d" % (i, k) for i, k in sorted(counts.items()))
    )
    return 0


def cmd_one(args):
    b, em, rows, file_hi = extract(args.firmware)
    mid = int(args.id, 0)
    if mid not in rows:
        print("0x%08x: NO ROW in the GSP table" % mid)
        return 1
    psz, fl, ar, off, pcls, pfn = rows[mid]
    tw = twin_of(mid)
    print("id          0x%08x" % mid)
    print(
        "interface   %s/0x%02x idx %d" % (family_of(mid), interface_of(mid), mid & 0xFF)
    )
    print(
        "half        %s%s"
        % (
            "SET" if is_privileged(mid) else "GET",
            " (legacy 0x8000)" if is_legacy(mid) else "",
        )
    )
    print("paramSize   %d" % psz)
    print("flags       0x%08x   accessRight %d" % (fl, ar))
    print("row fileOff 0x%x" % off)
    print("pClassInfo  0x%x" % pcls)
    print(
        "pFunc       0x%x  (%s)"
        % (pfn, "file-backed" if pfn < file_hi else "NOT file-backed")
    )
    if tw is not None:
        if tw in rows:
            print(
                "twin        0x%08x paramSize %d%s"
                % (tw, rows[tw][0], "" if rows[tw][0] == psz else "   <-- SIZE DIFFERS")
            )
        else:
            print(
                "twin        0x%08x  no row (%s-only control)"
                % (tw, "GET" if is_privileged(mid) else "write")
            )
    head, note = handler_head(b, file_hi, pfn, args.disasm)
    print("handler:")
    if head:
        for line in head:
            print("  %s" % line)
    else:
        print("  (%s)" % note)
    return 0


def selftest(args):
    """Invariants that a silent regression in this tool would break.

    Every one of these is a bug that actually happened in this file's history
    (or in extract-gsp-method-table.py), so they are asserted, not described.
    """
    b, em, rows, file_hi = extract(args.firmware)
    fails = []

    def ck(cond, what):
        if not cond:
            fails.append(what)
        print("  %-4s %s" % ("ok" if cond else "FAIL", what))

    # 1. the row count must be the corrected one; a re-added flags bound drops
    #    it to 889.
    ck(len(rows) == 1118, "row count is 1118 (got %d)" % len(rows))
    # 2. the two ids whose absence was an artifact of that bug are present.
    ck(
        0x20809006 in rows and rows[0x20809006][0] == 8,
        "0x20809006 present with paramSize 8",
    )
    ck(
        0x20809062 in rows and rows[0x20809062][0] == 648,
        "0x20809062 present with paramSize 648",
    )
    # 3. every row's handler is file-backed, i.e. statically disassemblable.
    out = [m for m in rows if rows[m][5] >= file_hi]
    ck(not out, "all %d handlers are file-backed (out: %s)" % (len(rows), out[:4]))
    # 4. the interface is the low 6 bits of the second byte; the 0x40 above it
    #    is bit 14, not a family step.
    ck(interface_of(0x2080901B) == 0x10, "interface_of(0x2080901b) == 0x10 (CLK)")
    ck(interface_of(0x2080D01C) == 0x10, "interface_of(0x2080d01c) == 0x10 (CLK)")
    ck(
        is_privileged(0x2080D01C) and not is_privileged(0x2080901B),
        "0x2080d01c is the privileged half and 0x2080901b is not",
    )
    # 5. the one pair that was measured live: 0x2080901b@2108 <-> 0x2080d01c@2108.
    ck(twin_of(0x2080901B) == 0x2080D01C, "twin_of(0x2080901b) == 0x2080d01c")
    ck(
        twin_of(0x2080D01C) == 0x2080901B,
        "twin_of(0x2080d01c) == 0x2080901b (bidirectional)",
    )
    ck(
        twin_of(0x2080901B) in rows and rows[0x2080D01C][0] == 2108,
        "the SET twin 0x2080d01c has paramSize 2108",
    )
    # 6. a size-mismatched pair must still be reported as size-mismatched:
    #    0x20809040@4808 -> 0x2080d041@648.  Assuming the GET size for the SET
    #    is exactly the error this tool exists to catch.
    ck(twin_of(0x20809040) == 0x2080D041, "twin_of(0x20809040) == 0x2080d041")
    ck(
        rows[0x20809040][0] != rows[0x2080D041][0],
        "0x20809040 and its twin carry different paramSize",
    )
    # 7. the FALSE PAIR that the byte-OR rule produced: 0x2080852a's real twin
    #    0x2080c52b is absent, while 0x2080852b exists but is itself a read
    #    control.  A rule that returns 0x2080852b is wrong even though that id
    #    is in the table.
    ck(
        twin_of(0x2080852A) == 0x2080C52B,
        "twin_of(0x2080852a) == 0x2080c52b (the absent SET half)",
    )
    ck(
        twin_of(0x2080852A) not in rows and 0x2080852B in rows,
        "0x2080852b is NOT the twin: it exists but is a read control",
    )
    ck(
        not is_privileged(0x2080852B),
        "0x2080852b has bit14 clear, so it cannot be a SET half",
    )
    # 8. 0x2080901c is a sibling READ control (808 B), not the SET of
    #    0x2080901b -- its bit14 is clear.  It is in the host build's wedge list.
    ck(
        not is_privileged(0x2080901C),
        "0x2080901c is a GET row, not the write half of 0x2080901b",
    )
    ck(
        0x2080901C in rows and rows[0x2080901C][0] == 808,
        "0x2080901c present with paramSize 808",
    )
    # 9. a read id must not be given a twin row: 0x20809025 has no SET row.
    ck(
        0x20809025 in rows and twin_of(0x20809025) not in rows,
        "0x20809025 is read-only (no row for its twin)",
    )
    # 9b. the private VOLT family really is interface 0x32, and the public SDK
    #     really has no name for it (ctrl2080volt.h defines zero 0x2080 ids,
    #     unlike ctrl2080clk.h with one and ctrl2080perf.h with fifteen).  So
    #     the name is present here as a labelled absence, not as a guess.
    ck(interface_of(0x2080B201) == 0x32, "interface_of(0x2080b201) == 0x32")
    ck(
        0x2080B201 in rows and rows[0x2080B201][0] == 2444,
        "0x2080b201 present with paramSize 2444",
    )
    ck(
        twin_of(0x2080B213) == 0x2080F214,
        "twin_of(0x2080b213) == 0x2080f214 (the VOLT SET half)",
    )
    ck(
        0x2080F214 in rows and rows[0x2080F214][0] == 1036,
        "0x2080f214 present with paramSize 1036",
    )
    # 10. the pair enumeration must be non-trivial and bidirectional (it
    #     sys.exit()s if the two directions disagree).
    pairs = twin_pairs(rows)
    ck(len(pairs) == 63, "63 GET/SET pairs on both sides (got %d)" % len(pairs))
    ck(pairs.get(0x2080901B) == 0x2080D01C, "the CLK offset pair is in the enumeration")
    print("# %d check(s) failed" % len(fails))
    return 1 if fails else 0


def main():
    ap = argparse.ArgumentParser(
        description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter
    )
    ap.add_argument("firmware")
    ap.add_argument(
        "--twins",
        action="store_true",
        help="print every GET/SET twin pair with both sizes",
    )
    ap.add_argument(
        "--families",
        dest="family_clk",
        action="store_true",
        help="print rows per named interface",
    )
    ap.add_argument(
        "--all",
        action="store_true",
        help="with --families, also print interfaces with no public name",
    )
    ap.add_argument("--id", help="dump one id's row and handler head")
    ap.add_argument(
        "--selftest",
        action="store_true",
        help="assert the invariants this tool depends on",
    )
    ap.add_argument(
        "--disasm",
        type=int,
        default=24,
        help="instructions to disassemble per handler (default 24)",
    )
    ap.add_argument(
        "--write-only",
        type=int,
        default=12,
        help="write-only SET rows to list (default 12)",
    )
    ap.add_argument(
        "--verbose",
        action="store_true",
        help="print the disassembly for every row in --families",
    )
    args = ap.parse_args()

    if args.selftest:
        return selftest(args)
    if args.id:
        return cmd_one(args)
    if args.twins:
        return cmd_twins(args)
    return cmd_family(args)


if __name__ == "__main__":
    sys.exit(main())
