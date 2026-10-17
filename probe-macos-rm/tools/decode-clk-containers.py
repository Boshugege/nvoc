#!/usr/bin/env python3
"""Decode the private clock containers captured by `probe-macos-rm --clkctl-hex`.

These ids are answered by the GSP firmware, which validates the parameter block
against an exact `paramSize` taken from its own export table (see
`docs/reverse-engineering/nvapi/gsp-method-table-and-legacy-forwarding.md` §3).
Each container is a fixed header plus a bit-sparse table of 32 per-domain
records, so the layout is fully described by `(header, stride)`.

The geometries below are *derived*, not guessed: for a given id the pair is the
unique split with `header + 32*stride == paramSize` whose stride is a multiple
of 4.  For `0x2080901b` (record base +60, stride 64) and `0x20809025` (base
+20, stride 224) they are additionally confirmed by what the driver writes into
a 0xA5A5A5A5 background, and for `0x20809025` the named fields agree, field for
field, with the independently recorded Windows/NVAPI record of the same
controller tree (see §5.1).  `check_geometry()` enforces the arithmetic.

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

# Field names that are established by evidence (see the doc §5.1 / §5.3).
NAMED = {
    0x20809025: {
        8: "max_vf_khz",
        20: "offset_floor_khz",
        24: "offset_ceiling_khz",
        28: "flags",
    },
    0x20809019: {4: "domain_bit", 8: "count_or_mask", 28: "version_pair"},
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

    print(f"0x{mid:08x} @{size}: header {header} B, 32 x {stride} B records @+{header}",
          file=out)
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
        print(f"  rec{index:<2} @+{base}: " + " ".join(words), file=out)
    if not populated:
        print("  (no populated record)", file=out)
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
