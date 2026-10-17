#!/usr/bin/env python3
"""Decode the private clock containers captured by `probe-macos-rm --clkctl-hex`.

These ids are answered by the GSP firmware, which validates the parameter block
against an exact `paramSize` taken from its own export table (see
`docs/reverse-engineering/nvapi/gsp-method-table-and-legacy-forwarding.md` §3).
Each container is a fixed header plus a bit-sparse table of 32 per-domain
records, so the layout is fully described by `(header, stride)`.

The arithmetic is only a *filter*, not a proof: `header + 32*stride ==
paramSize` has four solutions per id (they differ by 128 bytes of header and 4
bytes of stride), so a split is only trusted once the payload or an `0xA5`
background shows where the records actually start.  VERIFIED below lists the
ids whose split has that evidence; everything else is reported as "derived,
unverified" so nobody mistakes a decomposition for a layout.

* `0x20809019` -> 48 / 384: the record field pattern repeats at +48, +432, +816
  and the lossless 12336 B dump fills exactly 10 records in directory order,
  each carrying its own domain bit at rec+4 -- the same order and the same type
  stamps the `0x2080901b` directory uses, which is why 901b's +4 mask is an
  *entry selector* and not a domain-bit mask (see the doc §14.1/§14.3)
* `0x2080901b` -> 60 / 64: an `0xA5` background shows the driver writing record
  type bytes at +60 and +124
* `0x20809025` -> 20 / 224: records at +20, +244, +468, and the named fields
  match the independently recorded Windows/NVAPI record of the same tree (§5.1)
* `0x20809061` -> 8 / 24, `0x2080b201` -> 12 / 76, `0x2080b213` -> 12 / 32:
  the previously recorded payloads put the header at exactly those lengths

`check_geometry()` enforces the arithmetic for every entry.

Usage:
    probe-macos-rm-idle --clkctl-hex > clkctl-hex.txt
    python3 tools/decode-clk-containers.py clkctl-hex.txt [more.txt ...]
    python3 tools/decode-clk-containers.py --selftest
"""

from __future__ import annotations

import re
import struct
import sys

# id -> (paramSize, header, stride).  paramSize is the GSP table value.
GEOMETRY = {
    0x20809019: (12336, 48, 384),
    0x2080901A: (2052, 4, 64),
    0x2080901B: (2108, 60, 64),
    0x2080901C: (808, 40, 24),
    0x2080901D: (32680, 40, 1020),
    0x20809025: (7188, 20, 224),
    0x20809026: (5332, 84, 164),
    0x20809061: (776, 8, 24),
    0x2080B201: (2444, 12, 76),
    0x2080B213: (1036, 12, 32),
    0x2080D01C: (2108, 60, 64),
}

# Ids whose (header, stride) split has payload/background evidence.
VERIFIED = {
    0x20809019: "record field pattern repeats at +48/+432/+816",
    0x2080901B: "0xA5 background: driver writes record types at +60 and +124",
    0x20809025: "records at +20/+244/+468 and the named fields match nvapi-rs",
    0x20809061: "recorded payload: 8 B head + 32x24 B",
    0x2080B201: "recorded payload: 12 B head + one 76 B record",
    0x2080B213: "recorded payload: 12 B head + 32x32 B",
}

# Record dwords that are really a signed 16-bit PAIR (measured pattern, not a
# name): 0x20809019's rec+44 reads 0x03e8fc18 / 0x0bb8fc18 on two domains, i.e.
# (-1000,+1000) and (-1000,+3000) as two i16s, while the same slots read as a
# nonsensical 65600536 / 196672536 as one i32.  Printed as a second column so
# the raw dword stays visible.  Unit and field meaning are NOT established.
PAIRS = {
    0x20809019: {44: "i16 pair"},
}

# Containers whose records are packed in directory order and carry their own
# domain bit, so the printable directory is itself the evidence for the mask
# semantics (doc §14.3).
DIRECTORY_IDS = {0x20809019}

# Same RM struct as another id (the GET/SET twin shares paramSize, §5.2).
INHERITS = {0x2080D01C: 0x2080901B}

# Field names that are established by evidence (see the doc §5.1 / §5.3).
NAMED = {
    0x20809025: {
        8: "max_vf_khz",
        20: "offset_floor_khz",
        24: "offset_ceiling_khz",
        28: "flags",
    },
    0x20809019: {
        0: "type_stamp_u16x2",
        2: "flag_u16",
        4: "domain_bit",
        8: "count_or_mask",
        28: "version_pair",
        40: "khz_candidate_gpc_only",
        52: "ffff_or_zero",
    },
}

# Candidate value slots, printed for every domain the block's own echoed input
# mask names -- INCLUDING the all-zero case, which the generic record printer
# skips and which is exactly the stock reading we have to interpret.
#
# 0x2080901b: the RM record is the driver-side struct; the layer above
# (nvapi-rs clk_ctrl_entry) names five value dwords at record+44/+48/+52/+56/
# +60, and the one measured RM<->nvapi coordinate constant (0x24, pinned on
# 0x20809025's max_khz/offset_floor/offset_ceiling) puts them at RM rec+8/12/
# 16/20/24.  The order inside that five is NOT resolved: the article's own text
# says slot 0 is the signed kHz frequency offset, while the same layer's
# Blackwell field map says the frequency delta is VALUES[2].  So the current
# offset is one of rec+8 / rec+12 / rec+16, and only a same-field readback
# after a single-step SET can decide.  Printed as CANDIDATES, never as names.
CANDIDATES = {
    0x2080901B: {
        8: "offset_khz?",
        12: "range_min?",
        16: "freq_offset_or_range_max?",
        20: "applied?",
        24: "extra?",
    },
}

BLOCK_RE = re.compile(
    r"0x([0-9a-fA-F]{8})\s+@(\d+)\s+status\s+0x([0-9a-fA-F]{8})"
)
DUMP_RE = re.compile(r"\s(\d{5}):((?:\s+[0-9a-f]{2}){1,16})\s*$")


def check_geometry() -> None:
    """Fail loudly if a (header, stride) pair does not reconstruct paramSize."""
    for mid, (size, header, stride) in GEOMETRY.items():
        assert stride % 4 == 0, (hex(mid), stride)
        assert header % 4 == 0, (hex(mid), header)
        assert header + 32 * stride == size, (hex(mid), size, header, stride)


def parse_blocks(text: str) -> dict[tuple[int, int], bytes]:
    """Pull every `(id, size) -> payload` pair out of a --clkctl-hex dump."""
    blocks: dict[tuple[int, int], bytearray] = {}
    current: tuple[int, int] | None = None
    for line in text.splitlines():
        match = BLOCK_RE.search(line)
        if match and "hex follows" in line:
            current = (int(match.group(1), 16), int(match.group(2)))
            blocks.setdefault(current, bytearray())
            continue
        if current is None:
            continue
        dump = DUMP_RE.search(line)
        if not dump:
            continue
        offset = int(dump.group(1))
        payload = bytes(int(b, 16) for b in dump.group(2).split())
        buf = blocks[current]
        if len(buf) < offset:  # a zeroed gap is legal; a rewritten one is not
            buf.extend(bytes(offset - len(buf)))
        elif len(buf) > offset:
            del buf[offset:]
        buf.extend(payload)
    return {k: bytes(v) for k, v in blocks.items()}


def _dwords(buf: bytes, limit: int) -> list[str]:
    out = []
    for off in range(0, limit - 3, 4):
        value = struct.unpack_from("<I", buf, off)[0]
        if value:
            out.append(f"+{off}=0x{value:08x}")
    return out


def decode(mid: int, size: int, buf: bytes, out=sys.stdout) -> int:
    """Print the header words and every non-zero record word of one block."""
    if mid not in GEOMETRY:
        print(f"0x{mid:08x} @{size}: no geometry entry; ids: "
              + " ".join(f"0x{k:08x}" for k in sorted(GEOMETRY)), file=out)
        return 2
    want_size, header, stride = GEOMETRY[mid]
    if size != want_size:
        print(f"0x{mid:08x} @{size}: not the GSP paramSize ({want_size})", file=out)
        return 2

    if mid in VERIFIED:
        note = f"verified: {VERIFIED[mid]}"
    elif mid in INHERITS:
        note = f"inherits the geometry of 0x{INHERITS[mid]:08x}"
    else:
        note = ("derived, UNVERIFIED: the arithmetic admits header+128/stride-4 "
                "as well -- do not treat the field offsets as a layout")
    print(f"0x{mid:08x} @{size}: header {header} B, 32 x {stride} B records @+{header}",
          file=out)
    print(f"  ({note})", file=out)
    print("  head: " + (" ".join(_dwords(buf, header)) or "(all zero)"), file=out)
    named = NAMED.get(mid, {})
    populated = 0
    for index in range(32):
        base = header + index * stride
        record = buf[base:base + stride]
        if len(record) < stride or not any(record):
            continue
        populated += 1
        words = []
        for off in range(0, stride - 3, 4):
            value = struct.unpack_from("<I", record, off)[0]
            if not value:
                continue
            name = named.get(off)
            signed = struct.unpack_from("<i", record, off)[0]
            label = f"+{off}="
            if name:
                label = f"+{off}({name})="
            shown = f"{value}" if not name else f"{signed}"
            words.append(f"{label}0x{value:08x}/{shown}")
        if mid in PAIRS:
            for off, label in sorted(PAIRS[mid].items()):
                if off + 4 > len(record):
                    continue
                lo, hi = struct.unpack_from("<hh", record, off)
                words.append(f"<+{off} {label} = [{lo}, {hi}]>")
        print(f"  rec{index:<2} @+{base}: " + " ".join(words), file=out)
    if not populated:
        print("  (no populated record)", file=out)
    if mid in DIRECTORY_IDS:
        # The directory is the *point* of this container: the records are packed
        # in directory order and each one states its own domain bit, so the
        # caller's mask is an entry selector.  Print it as one checkable line --
        # it is what makes "mask 0xFF fills 8 records although only 7 of its
        # bits name a domain" a measurement rather than an inference.
        entries = []
        for index in range(32):
            base = header + index * stride
            record = buf[base:base + stride]
            if len(record) < stride or not any(record):
                continue
            entries.append("%d:%d:0x%04x" % (
                index,
                struct.unpack_from("<I", record, 4)[0],
                struct.unpack_from("<H", record, 0)[0],
            ))
        if entries:
            print("  directory (entry:domain_bit:type) = " + " ".join(entries),
                  file=out)
    if mid in CANDIDATES:
        # Mask-driven: the echoed mask at +4 IS the set of records the caller
        # asked for, so this prints one candidate row per requested domain even
        # when every value is zero (the stock case).
        mask = struct.unpack_from("<I", buf, 4)[0] if len(buf) >= 8 else 0
        fields = sorted(CANDIDATES[mid].items())
        print(f"  candidate slots (mask@+4=0x{mask:x}); the ORDER inside them is",
              file=out)
        print("  NOT resolved: the layer above names five value dwords at RM",
              file=out)
        print("  rec+8/12/16/20/24 (offset_khz, range_min, range_max, applied,",
              file=out)
        print("  extra) via the measured 0x24 shift, but its own article says",
              file=out)
        print("  slot 0 is the signed kHz offset while its Blackwell field map",
              file=out)
        print("  says the frequency delta is VALUES[2] -- so the current offset",
              file=out)
        print("  is one of +8/+12/+16, and ONLY a same-field readback after one",
              file=out)
        print("  minimal-step SET decides which. Realtime MHz never does.",
              file=out)
        for bit in range(32):
            if not (mask >> bit) & 1:
                continue
            base = header + bit * stride
            record = buf[base:base + stride]
            if len(record) < stride:
                continue
            stamp = struct.unpack_from("<H", record, 0)[0]
            cells = []
            for off, _label in fields:
                if off + 4 > len(record):
                    continue
                cells.append(f"+{off}={struct.unpack_from('<i', record, off)[0]}")
            print(f"    bit {bit:<2} rec @+{base} type=0x{stamp:04x}: "
                  + " ".join(cells), file=out)
    return 0


def selftest() -> int:
    check_geometry()
    # Synthetic 0x20809025 payload: mask 0x7, three NAFLL-style records with the
    # values this TU116 actually returned (1638400 kHz, -18750/+18750 kHz).
    header, stride = GEOMETRY[0x20809025][1:]
    buf = bytearray(GEOMETRY[0x20809025][0])
    struct.pack_into("<I", buf, 4, 0x7)
    for index, dtype in enumerate((3, 4, 5)):
        base = header + index * stride
        struct.pack_into("<I", buf, base, 0x01010101 | (dtype << 8))
        struct.pack_into("<I", buf, base + 8, 1638400)
        struct.pack_into("<i", buf, base + 20, -18750)
        struct.pack_into("<i", buf, base + 24, 18750)
        struct.pack_into("<I", buf, base + 28, 0x00FF0100)
    text = "\n".join(
        [f"[OK] clkctl freq-ctrl-info 0x20809025 @7188  status 0x00000000 (NV_OK)"
         f"  cmd 0x20809025 size 7188 -> NV_OK, hex follows"]
        + [f"    {off:05d}: " + " ".join(f"{b:02x}" for b in buf[off:off + 16])
           for off in range(0, len(buf), 16)]
    )
    blocks = parse_blocks(text)
    assert blocks[(0x20809025, 7188)] == bytes(buf), "round-trip failed"
    import io
    sink = io.StringIO()
    assert decode(0x20809025, 7188, bytes(buf), sink) == 0
    report = sink.getvalue()
    for needle in ("max_vf_khz", "1638400", "offset_floor_khz", "-18750",
                   "offset_ceiling_khz", "18750", "rec2"):
        assert needle in report, needle
    # 0x20809019 rec+44 is a signed i16 pair on the live payloads (-1000/+1000
    # and -1000/+3000); the pair column must render it as such, not as i32.
    buf19 = bytearray(GEOMETRY[0x20809019][0])
    base19 = GEOMETRY[0x20809019][1]
    struct.pack_into("<HH", buf19, base19, 0x0808, 0x0001)
    struct.pack_into("<I", buf19, base19 + 4, 0x1)
    struct.pack_into("<i", buf19, base19 + 44, -1000)
    struct.pack_into("<H", buf19, base19 + 46, 1000)
    sink = io.StringIO()
    assert decode(0x20809019, GEOMETRY[0x20809019][0], bytes(buf19), sink) == 0
    report19 = sink.getvalue()
    assert "i16 pair = [-1000, 1000]" in report19, report19
    assert "directory (entry:domain_bit:type) = 0:1:0x0808" in report19, report19
    assert "+0(type_stamp_u16x2)=" in report19, report19
    assert "65600536" in report19, report19
    # 0x2080901b candidate table: the stock case is an ALL-ZERO record whose
    # value slots the driver did write, so the mask-driven rows must appear even
    # though the generic record printer sees nothing.  mask@+4 = 0x2 selects
    # exactly record 1 (bit -> index, 1:1), which is what the live battery saw.
    buf1b = bytearray(GEOMETRY[0x2080901B][0])
    struct.pack_into("<I", buf1b, 4, 0x2)
    base1b = GEOMETRY[0x2080901B][1] + 1 * GEOMETRY[0x2080901B][2]
    struct.pack_into("<H", buf1b, base1b, 0x0808)
    sink = io.StringIO()
    assert decode(0x2080901B, GEOMETRY[0x2080901B][0], bytes(buf1b), sink) == 0
    report1b = sink.getvalue()
    assert "candidate slots" in report1b, report1b
    assert "ONLY a same-field readback" in report1b, report1b
    assert f"bit 1  rec @+{base1b} type=0x0808: " in report1b, report1b
    assert "+8=0 +12=0 +16=0 +20=0 +24=0" in report1b, report1b
    assert "bit 0 " not in report1b, report1b
    assert decode(0x20809025, 2108, bytes(buf), sink) == 2, "size guard"
    assert decode(0x20801234, 4, b"\0" * 4, sink) == 2, "unknown id guard"
    print("selftest ok: geometry + round-trip + field decode")
    return 0


def main(argv: list[str]) -> int:
    check_geometry()
    if not argv or argv[0] in ("-h", "--help"):
        print(__doc__)
        return 0 if argv else 2
    if argv[0] == "--selftest":
        return selftest()
    status = 0
    for path in argv:
        if path == "-":
            text = sys.stdin.read()
        else:
            with open(path, "r", encoding="utf-8", errors="replace") as handle:
                text = handle.read()
        blocks = parse_blocks(text)
        if not blocks:
            print(f"{path}: no candidate blocks found", file=sys.stderr)
            status = 1
        for (mid, size) in sorted(blocks):
            status |= decode(mid, size, blocks[(mid, size)])
    return status


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
