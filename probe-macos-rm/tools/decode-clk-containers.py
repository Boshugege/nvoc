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
  type bytes at +60 and +124, and `write_set_check()` turns that observation into
  a per-row verdict: it reads the written slots off the bytes and compares them
  with the per-type rule (13 slots for a type 08/09 record, its stamp alone for
  type 03), states the head count the bytes imply, and prints where the last
  changed dword actually is.  On an `0xA5` block the generic record printer is
  misleading on its own -- it lists non-zero dwords, so the dwords the handler
  wrote as zero are the ones it hides -- which is why the write-set line, and not
  the record line, is what to read there.
* `0x20809025` -> 20 / 224: records at +20, +244, +468, and the named fields
  match the independently recorded Windows/NVAPI record of the same tree (§5.1)
* `0x20809061` -> 8 / 24, `0x2080b201` -> 12 / 76, `0x2080b213` -> 12 / 32:
  the previously recorded payloads put the header at exactly those lengths

`check_geometry()` enforces the arithmetic for every entry, and a directory
block additionally gets `mask_consistency()`: the header names the same ten rows
as both row-bits (0x3ff) and domain bits (0x38007f), so the tool checks its two
halves against the rows actually present instead of trusting prose.

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

# 0x2080901D sits in GEOMETRY but NOT in VERIFIED, so its (40, 1020) split is
# arithmetic only -- and the arithmetic has four solutions, not one: 40+32*1020,
# 168+32*1016, 296+32*1012 and 424+32*1008 all equal 32680.  The 2026-10-17
# 0xA5 run supplies the first *measured* structure for this id: its changed-dword
# list repeats 128 bytes apart between the group at +36..+56 and the group at
# +164..+184 (three dwords byte-identical, two differing in a single byte), which
# is what a per-domain record table looks like.  Two consequences worth keeping
# straight:
#   * a 128-byte period cannot tile any of the four arithmetic splits (1012-1020
#     bytes is 7x128 + 116..124), so a 128 B stride would be evidence the
#     arithmetic alone could not produce;
#   * both observed groups are consecutive under (header 36, stride 128), while
#     under (40, 1020) the first group lands in the head and the second at
#     rec0+124.
# The discriminating observation is therefore the *third* group, and it is
# pre-registered here: a full payload with an 0xA5 background decides it.
MEASURED_REPETITION = {
    0x2080901D: {
        "artifact": "stage-gsp-clkctl-20261017T092536Z/clkctl-offsweep-run1-A5.txt",
        "group_starts": (36, 164),
        "observed_stride": 128,
        "prediction_if_stride_128": "a third group at +292",
        "prediction_if_head_1020": "the next group starts at +1060 (40 + 1020)",
        "pending": "a full 0x2080901d payload with an 0xA5 background",
    },
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
#
# That +8..+24 mapping comes from one constant, 0x24, re-derived from the
# recorded 0x20809025 payload: RM row +0x08/+0x14/+0x18 hold 1638400 / -18750 /
# +18750, and the caller's named REC_MAX_KHZ/REC_MIN_OFFSET/REC_MAX_OFFSET sit at
# +0x2C/+0x38/+0x3C -- three pairs, identical values, one delta.  It is a
# FIELD-level constant only: caller TYPE(+0) vs the RM tag(+0) and caller
# mask(+8) vs the RM mask(+4) do not follow it, and the struct geometry differs
# (caller 100/72 against RM 60/64).  Hence candidates, not names.
CANDIDATES = {
    0x2080901B: {
        8: "offset_khz?",
        12: "range_min?",
        16: "freq_offset_or_range_max?",
        20: "applied?",
        24: "extra?",
    },
}

BLOCK_RE = re.compile(r"0x([0-9a-fA-F]{8})\s+@(\d+)\s+status\s+0x([0-9a-fA-F]{8})")
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


# The selection mask the recorded 0x2080901b run seeded at byte +4 (the field the
# reply's write set showed to be an input). Row coverage is computed from this,
# not assumed to be contiguous.
MEASURED_SEED = 0xFF


def mask_consistency(buf: bytes, header: int, stride: int) -> list[str]:
    """Cross-check a directory header against the rows it actually lists.

    The point of this check: the same ten domains can be named two ways -- as
    directory rows (bit i for row i) and as the `1<<id` values the rows carry --
    and the header states both. Because the two numbers differ (0x3ff vs
    0x38007f on this TU116), the block itself is the evidence that the
    caller-supplied selection mask at `+4` counts *rows*, which is the reading
    the 0x2080901b write-set arithmetic also requires. Prose can drift; this
    cannot.
    """
    rows = []
    for index in range(32):
        base = header + index * stride
        record = buf[base : base + stride]
        if len(record) < stride or not any(record):
            continue
        rows.append((index, struct.unpack_from("<I", record, 4)[0]))
    if not rows:
        return ["  mask check: no populated row, nothing to cross-check"]
    index_mask = 0
    domain_mask = 0
    for index, bit in rows:
        index_mask |= 1 << index  # row bit must correspond to the row's place
        domain_mask |= bit
    head = [
        struct.unpack_from("<I", buf, off)[0] if len(buf) >= off + 4 else 0
        for off in (4, 12, 20)
    ]
    out = [
        f"  mask check: {len(rows)} rows; rows-as-bits 0x{index_mask:08x}, "
        f"union of their domain bits 0x{domain_mask:08x}"
    ]
    for off, value in zip((4, 12, 20), head):
        want = index_mask if off == 4 else domain_mask
        verdict = "MATCH" if value == want else f"mismatch (0x{value:08x})"
        out.append(f"    head +{off} = 0x{value:08x}: {verdict}")
    out.append(
        "    rows: " + " ".join(f"{i}:id{bit.bit_length() - 1}" for i, bit in rows)
    )
    # Which rows the seed the recorded run used actually reached, read as row
    # bits -- the reading the 0x2080901b write-set arithmetic requires and the
    # header cross-check above confirms. This is what bounds any "zero measured"
    # claim: the dwords of a row the seed did not reach were never written.
    covered = [i for i, _ in rows if (MEASURED_SEED >> i) & 1]
    beyond = [
        (i, bit.bit_length() - 1) for i, bit in rows if not (MEASURED_SEED >> i) & 1
    ]
    reach = (
        f"rows {covered[0]}..{covered[-1]}"
        if covered and covered == list(range(covered[0], covered[-1] + 1))
        else f"rows {covered}"
    )
    if beyond:
        far = "; ".join(f"row {i} id {bit}" for i, bit in beyond)
        out.append(f"    seed {MEASURED_SEED:#04x} reaches {reach}, beyond it: {far}")
    else:
        out.append(
            f"    seed {MEASURED_SEED:#04x} reaches {reach} and every listed row"
        )
    return out


# --- the write-set model, as executable arithmetic ---------------------------
#
# 0x2080901b is the one block whose *write set* is observable end to end: on an
# 0xA5 background a dword the handler never touched stays a5a5a5a5 while one it
# wrote shows what it wrote -- and at stock the value dwords it writes are zero.
# The model that explains the one measured run ("106 of 527 dwords changed, last
# @564") is:
#
#   changed(mask) = HEAD + sum over *selected* directory rows of per-type(rec)
#   per-type(0x08) = per-type(0x09) = 13 slots (rec+0 stamp, rec+8..+44, rec+52,
#   rec+56), per-type(0x03) = 1 (its stamp alone); rec+4, rec+48 and rec+60 are
#   never written.
#
# Two properties keep this a model rather than a fit: the same HEAD and the same
# per-type rule must reproduce all three masks the probe sends (0x1, 0xFF,
# 0x3FF), and the last changed offset must land on the last selected
# value-writing row's rec+56.  RECORDED_WRITE_SET holds the three numbers as they
# were recorded/predicted, so editing the rule or the row types breaks
# `--selftest` instead of silently rewriting history.
#
# The row list is the recorded ten in directory order with their own type words,
# because the per-type term is exactly what makes the 0xFF case differ from a
# uniform "8 records x 13": row 6 carries type 0x03 and writes its stamp only.
# The two type bytes of a stamp are the only part of rec+0 that is written, which
# is why a written stamp still leaves the upper half of that dword at a5a5.
RECORDED_ROWS = (
    (1, 0x0808),
    (2, 0x0909),
    (16, 0x0808),
    (4, 0x0909),
    (8, 0x0909),
    (1_048_576, 0x0909),
    (524_288, 0x0303),
    (64, 0x0808),
    (2_097_152, 0x0909),
    (32, 0x0909),
)
WRITE_SET_HEAD = 14
WRITE_SET_PER_TYPE = {0x08: 13, 0x09: 13, 0x03: 1}
# mask -> (changed dwords, last changed offset).  0xFF is measured (2026-10-17
# offsetsweep); 0x1 and 0x3FF are the probe's pre-registered predictions for the
# same rule, which is why they are pinned here rather than recomputed freely.
RECORDED_WRITE_SET = {0x1: (27, 116), 0xFF: (106, 564), 0x3FF: (132, 692)}


def write_set_model(mask, rows=RECORDED_ROWS, header=60, stride=64):
    """The dwords the handler writes for `mask`, and the last one of them.

    Returns (None, None) when a selected row carries a type the model has no
    per-type rule for: an unknown type is never silently counted as zero.
    """
    selected = [(i, stamp) for i, (_bit, stamp) in enumerate(rows) if (mask >> i) & 1]
    count = WRITE_SET_HEAD
    last = None
    for index, stamp in selected:
        rule = WRITE_SET_PER_TYPE.get(stamp & 0xFF)
        if rule is None:
            return None, None
        count += rule
        if rule > 1:
            last = header + index * stride + 56
    if last is None and selected:
        last = header + selected[-1][0] * stride
    return count, last


def a5_block(mask, header=60, stride=64, size=2108, rows=RECORDED_ROWS):
    """The model written as bytes: a synthetic 0xA5 block after one masked call.

    Used by `--selftest` so the byte-level checker below is tested without a
    device.  The head's 14 dwords are placed at +0..+52; the *count* is what the
    model claims, the identity of the head slots is a separate, still open
    question, and this builder deliberately does not pretend otherwise.
    """
    buf = bytearray(b"\xa5" * size)
    for off in range(0, 4 * WRITE_SET_HEAD, 4):
        struct.pack_into("<I", buf, off, 0)
    for index, (_bit, stamp) in enumerate(rows):
        if not (mask >> index) & 1:
            continue
        base = header + index * stride
        struct.pack_into("<H", buf, base, stamp)  # the two type bytes only
        if WRITE_SET_PER_TYPE.get(stamp & 0xFF, 0) > 1:
            for off in (8, 12, 16, 20, 24, 28, 32, 36, 40, 44, 52, 56):
                struct.pack_into("<I", buf, base + off, 0)
    return bytes(buf)


def write_set_check(buf: bytes, header: int, stride: int) -> list[str]:
    """Check the A5 reply's actual write set against the model, per row.

    This is the check that settles what the handler writes without a second
    device call: it reads the written slots off the bytes and compares them with
    the per-type rule, reports the head count the bytes imply, and states where
    the last changed dword is instead of where it was predicted to be.

    It is only meaningful on the 0xA5 case, and it says so rather than guessing:
    on a zeroed block the dwords the handler writes are zero too, so the model
    would report a short write set that is really just "zero background".  The
    detector is that some dword still holds a5a5a5a5, which the A5 case cannot
    exhaust -- the model's own largest mask writes 132 of 527 dwords.
    """
    seed = 0xA5A5A5A5
    if not any(
        struct.unpack_from("<I", buf, off)[0] == seed
        for off in range(0, len(buf) - 3, 4)
    ):
        return [
            "  write-set: no dword still holds 0xa5a5a5a5, so this is not "
            "the 0xA5 case and the write set is not observable here -- on a "
            "zeroed block a written zero looks exactly like an untouched "
            "dword; read the record line above instead"
        ]
    changed = [
        off
        for off in range(0, len(buf) - 3, 4)
        if struct.unpack_from("<I", buf, off)[0] != seed
    ]
    if not changed:
        return [
            "  write-set: the A5 background is intact, so the handler wrote "
            "nothing for this mask"
        ]
    out = [
        f"  write-set: {len(changed)} of {len(buf) // 4} dwords differ from "
        f"the 0xA5 seed, first @{changed[0]} last @{changed[-1]}"
    ]
    head_written = [off for off in changed if off < header]
    out.append(
        f"    head: {len(head_written)} written dwords"
        + (" at " + " ".join(f"+{off}" for off in head_written) if head_written else "")
        + f" -- model HEAD={WRITE_SET_HEAD}: "
        + ("MATCH" if len(head_written) == WRITE_SET_HEAD else "MISMATCH")
    )
    rows = []
    for index in range(32):
        base = header + index * stride
        record = buf[base : base + stride]
        if len(record) < stride or struct.unpack_from("<I", record, 0)[0] == seed:
            continue
        stamp = struct.unpack_from("<H", record, 0)[0]
        written = [
            off
            for off in range(0, stride - 3, 4)
            if struct.unpack_from("<I", record, off)[0] != seed
        ]
        rows.append((index, stamp, base, written))
        rule = WRITE_SET_PER_TYPE.get(stamp & 0xFF)
        verdict = (
            "no per-type rule: type unknown"
            if rule is None
            else f"rule {rule} slot(s): "
            + ("MATCH" if rule == len(written) else "MISMATCH")
        )
        out.append(
            f"    row {index} (type 0x{stamp:04x}) @+{base}: written rel "
            + " ".join(f"+{off}" for off in written)
            + f" -- {verdict}"
        )
    if not rows:
        out.append("    no record carries a stamp, so the mask selected nothing")
        return out
    by_rule = [WRITE_SET_PER_TYPE.get(stamp & 0xFF) for _i, stamp, _b, _w in rows]
    if any(rule is None for rule in by_rule):
        out.append(
            "    the arithmetic is NOT applied while a selected type is "
            "unknown -- an unruled type is not assumed to equal any case"
        )
        return out
    total = WRITE_SET_HEAD + sum(by_rule)
    out.append(
        f"    model: {len(rows)} rows ({sum(by_rule)} row dwords) + "
        f"HEAD={WRITE_SET_HEAD} = {total} vs observed {len(changed)}: "
        + ("MATCH" if total == len(changed) else "MISMATCH")
    )
    value_rows = [
        base + 56
        for _i, stamp, base, _w in rows
        if WRITE_SET_PER_TYPE.get(stamp & 0xFF, 0) > 1
    ]
    if value_rows:
        out.append(
            f"    model: the last changed dword should be a value row's "
            f"rec+56 = {max(value_rows)} vs observed {changed[-1]}: "
            + ("MATCH" if max(value_rows) == changed[-1] else "MISMATCH")
        )
    return out


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
        print(
            f"0x{mid:08x} @{size}: no geometry entry; ids: "
            + " ".join(f"0x{k:08x}" for k in sorted(GEOMETRY)),
            file=out,
        )
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
        note = (
            "derived, UNVERIFIED: the arithmetic admits header+128/stride-4 "
            "as well -- do not treat the field offsets as a layout"
        )
    print(
        f"0x{mid:08x} @{size}: header {header} B, 32 x {stride} B records @+{header}",
        file=out,
    )
    print(f"  ({note})", file=out)
    print("  head: " + (" ".join(_dwords(buf, header)) or "(all zero)"), file=out)
    named = NAMED.get(mid, {})
    populated = 0
    for index in range(32):
        base = header + index * stride
        record = buf[base : base + stride]
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
            record = buf[base : base + stride]
            if len(record) < stride or not any(record):
                continue
            entries.append(
                "%d:%d:0x%04x"
                % (
                    index,
                    struct.unpack_from("<I", record, 4)[0],
                    struct.unpack_from("<H", record, 0)[0],
                )
            )
        if entries:
            print(
                "  directory (entry:domain_bit:type) = " + " ".join(entries), file=out
            )
            for line in mask_consistency(buf, header, stride):
                print(line, file=out)
    if mid in CANDIDATES:
        # Mask-driven: the echoed mask at +4 IS the set of records the caller
        # asked for, so this prints one candidate row per requested domain even
        # when every value is zero (the stock case).
        mask = struct.unpack_from("<I", buf, 4)[0] if len(buf) >= 8 else 0
        fields = sorted(CANDIDATES[mid].items())
        print(
            f"  candidate slots (mask@+4=0x{mask:x}); the ORDER inside them is",
            file=out,
        )
        print("  NOT resolved: the layer above names five value dwords at RM", file=out)
        print(
            "  rec+8/12/16/20/24 (offset_khz, range_min, range_max, applied,", file=out
        )
        print(
            "  extra) via the measured 0x24 shift, but its own article says", file=out
        )
        print(
            "  slot 0 is the signed kHz offset while its Blackwell field map", file=out
        )
        print(
            "  says the frequency delta is VALUES[2] -- so the current offset", file=out
        )
        print(
            "  is one of +8/+12/+16, and ONLY a same-field readback after one", file=out
        )
        print("  minimal-step SET decides which. Realtime MHz never does.", file=out)
        for bit in range(32):
            if not (mask >> bit) & 1:
                continue
            base = header + bit * stride
            record = buf[base : base + stride]
            if len(record) < stride:
                continue
            stamp = struct.unpack_from("<H", record, 0)[0]
            cells = []
            for off, _label in fields:
                if off + 4 > len(record):
                    continue
                cells.append(f"+{off}={struct.unpack_from('<i', record, off)[0]}")
            print(
                f"    bit {bit:<2} rec @+{base} type=0x{stamp:04x}: " + " ".join(cells),
                file=out,
            )
    if mid == 0x2080901B or INHERITS.get(mid) == 0x2080901B:
        # Only a non-zero background can show a write set: on zeros the dwords
        # the handler writes are zero as well, so "wrote zero" and "never
        # touched" are indistinguishable there.  This is also the check that
        # answers "which record-relative slots does the handler write" from the
        # recorded bytes instead of from the caller-layer field names.
        for line in write_set_check(buf, header, stride):
            print(line, file=out)
    return 0


def selftest() -> int:
    check_geometry()
    # The 0x2080901d candidate above claims a measured 128-byte period over a
    # 32680-byte payload.  Assert the two facts that make it checkable at all:
    # the arithmetic split has four solutions (so it cannot be evidence by
    # itself), and none of them admits a 128-byte period.
    solutions = [
        (head, (32680 - head) // 32)
        for head in range(0, 2048)
        if (32680 - head) % 32 == 0 and (32680 - head) // 32 in (1008, 1012, 1016, 1020)
    ]
    assert sorted(solutions) == [(40, 1020), (168, 1016), (296, 1012), (424, 1008)], solutions
    for head, stride in solutions:
        assert stride % 128 != 0, (head, stride)
    rep = MEASURED_REPETITION[0x2080901D]
    assert rep["group_starts"][1] - rep["group_starts"][0] == rep["observed_stride"]
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
        [
            "[OK] clkctl freq-ctrl-info 0x20809025 @7188  status 0x00000000 (NV_OK)"
            "  cmd 0x20809025 size 7188 -> NV_OK, hex follows"
        ]
        + [
            f"    {off:05d}: " + " ".join(f"{b:02x}" for b in buf[off : off + 16])
            for off in range(0, len(buf), 16)
        ]
    )
    blocks = parse_blocks(text)
    assert blocks[(0x20809025, 7188)] == bytes(buf), "round-trip failed"
    import io

    sink = io.StringIO()
    assert decode(0x20809025, 7188, bytes(buf), sink) == 0
    report = sink.getvalue()
    for needle in (
        "max_vf_khz",
        "1638400",
        "offset_floor_khz",
        "-18750",
        "offset_ceiling_khz",
        "18750",
        "rec2",
    ):
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
    # The live ten rows, and the header's own two answers for them: 0x3ff as
    # rows-as-bits and 0x38007f as the union of the rows' 1<<id values.  The
    # check has to MATCH the live shape and to catch a header edited away from
    # it, otherwise it is decoration.
    live_bits = (0x1, 0x2, 0x10, 0x4, 0x8, 0x100000, 0x80000, 0x40, 0x200000, 0x20)
    bufdir = bytearray(GEOMETRY[0x20809019][0])
    head19, stride19 = GEOMETRY[0x20809019][1:]
    struct.pack_into("<I", bufdir, 4, 0x3FF)
    struct.pack_into("<I", bufdir, 12, 0x38007F)
    struct.pack_into("<I", bufdir, 20, 0x38007F)
    for index, bit in enumerate(live_bits):
        base = head19 + index * stride19
        struct.pack_into("<H", bufdir, base, 0x0808)
        struct.pack_into("<I", bufdir, base + 4, bit)
    sink = io.StringIO()
    assert decode(0x20809019, GEOMETRY[0x20809019][0], bytes(bufdir), sink) == 0
    live = sink.getvalue()
    assert "10 rows; rows-as-bits 0x000003ff" in live, live
    assert "union of their domain bits 0x0038007f" in live, live
    assert live.count("MATCH") == 3, live
    # ... and the placement line has to say which rows the recorded seed reached,
    # because that is what bounds every "the value dwords read zero" claim.  On
    # the live ten rows the 0xFF seed stops at row 7, leaving ids 21 and 5.
    beyond = "seed 0xff reaches rows 0..7, beyond it: row 8 id 21; row 9 id 5"
    assert beyond in live, live
    bent = bytearray(bufdir)
    struct.pack_into("<I", bent, 12, 0x38003F)  # forget one row's bit
    sink = io.StringIO()
    assert decode(0x20809019, GEOMETRY[0x20809019][0], bytes(bent), sink) == 0
    assert "head +12 = 0x0038003f: mismatch" in sink.getvalue(), sink.getvalue()
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
    # The write-set model, checked twice: as arithmetic against the three
    # recorded/predicted numbers, and as bytes against a block the model itself
    # builds.  Changing the per-type rule, HEAD, or a recorded row type has to
    # break here -- that is what keeps 106/27/132 a model instead of a memory.
    for mask, want in RECORDED_WRITE_SET.items():
        assert write_set_model(mask) == want, (mask, write_set_model(mask), want)
    for mask in sorted(RECORDED_WRITE_SET):
        want_count, want_last = RECORDED_WRITE_SET[mask]
        sink = io.StringIO()
        assert decode(0x2080901B, 2108, a5_block(mask), sink) == 0
        report = sink.getvalue()
        assert f"{want_count} of 527 dwords differ" in report, report
        assert f"last @{want_last}" in report, report
        assert f"= {want_count} vs observed {want_count}: MATCH" in report, report
        assert f"vs observed {want_last}: MATCH" in report, report
        assert "MISMATCH" not in report, report
    # A zero-background block must be declined, not misread: its stamps are
    # non-zero and its written zeros are not, so the model would see a 2-dword
    # write set where 14 head dwords were written.
    zeroed = bytearray(GEOMETRY[0x2080901B][0])
    struct.pack_into("<H", zeroed, GEOMETRY[0x2080901B][1], 0x0808)
    assert write_set_check(bytes(zeroed), 60, 64)[0].startswith(
        "  write-set: no dword still holds 0xa5a5a5a5"
    ), write_set_check(bytes(zeroed), 60, 64)
    # A type the rule does not know is never counted as zero: the arithmetic has
    # to be withheld, not guessed.
    odd = a5_block(0x1, rows=((1, 0x0B0B),) + RECORDED_ROWS[1:])
    assert "type unknown" in "\n".join(write_set_check(odd, 60, 64))
    assert write_set_model(0x1, rows=((1, 0x0B0B),) + RECORDED_ROWS[1:]) == (None, None)
    print("selftest ok: geometry + round-trip + field decode + write-set model")
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
        for mid, size in sorted(blocks):
            status |= decode(mid, size, blocks[(mid, size)])
    return status


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
