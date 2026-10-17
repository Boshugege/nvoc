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

`0x2080901d` stays out of that list on purpose.  Its only measured structure is a
repetition inside the first 184 bytes of an `0xA5` diff, which constrains the
record interior without deciding where a record ends; `check_measured_repetition()`
re-derives that repetition from `fixtures/tu116-1d-32680-A5.txt` (md5 checked, so
the bytes cannot drift) and prints both candidate predictions for the third
occurrence, which is the observation that would settle it.

Usage:
    probe-macos-rm-idle --clkctl-hex > clkctl-hex.txt
    python3 tools/decode-clk-containers.py clkctl-hex.txt [more.txt ...]
    python3 tools/decode-clk-containers.py --selftest
"""

from __future__ import annotations

import hashlib
import os
import re
import struct
import sys
from pathlib import Path

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
# 168+32*1016, 296+32*1012 and 424+32*1008 all equal 32680.  Three things are
# true of the 2026-10-17 0xA5 run, and they are worth keeping apart:
#   * measured: inside its printed window the twelve changed dwords +12..+56
#     reappear at +140..+184, one 128-byte shift later (a byte-identical copy of
#     the whole 12-dword run -- the values still carry 0xA5 bytes, so the driver
#     writes sub-dword fields).  That is a repetition, not yet a stride.
#   * arithmetic: a 128-byte period cannot tile any of the four splits above
#     (1012..1020 B is 7*128 + 116..124).  If the records really are 128 B apart,
#     the size admits exactly one split, 40 + 255*128 -- 255 records, not the 32
#     domains 0x2080901b carries, which is why the measurement and not this
#     sentence has to decide.
#   * unobserved: the printer stops after 40 changed dwords (the summary says 352
#     of 8170, first @0, last @3376), so nothing past +184 is in the artifact and
#     the (40, 1020) reading cannot be excluded from it.
# The discriminating observation is therefore the *third* occurrence, and it is
# pre-registered below as a distance from the second one, so no group-start
# convention has to be agreed first.  check_measured_repetition() re-derives the
# measured line above from the committed fixture; the rest stays on the record.
MEASURED_REPETITION = {
    0x2080901D: {
        "artifact": "stage-gsp-clkctl-20261017T092536Z/clkctl-offsweep-run1-A5.txt",
        "fixture": "probe-macos-rm/fixtures/tu116-1d-32680-A5.txt",
        "fixture_md5": "bdb3db742ae9bd44990efbffa6eda8e0",
        "subblock_start": 12,
        "subblock_dwords": 12,
        "second_occurrence": 140,
        "observed_shift": 128,
        "window_ends_at": 184,
        "summary": "352/8170 dwords changed (first @0 last @3376)",
        "prediction_if_period_128": "a third occurrence at +268 (the second, +140, plus 128)",
        "prediction_if_stride_1020": "a third occurrence at +1160 (the second, +140, plus 1020)",
        "also_recorded": (
            "an earlier reading of this same window named the group starts (36, 164) -- the "
            "same repetition seen 24 bytes further into the record; the convention-free "
            "statement is the +140 one.  Still pending: a full 0x2080901d payload with an "
            "0xA5 background, which is what would settle the stride"
        ),
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
# 16/20/24.
#
# WHICH SLOT, for the signed offset: rec+8 is the supported reading, and the
# one counter-citation does not apply to this block.
#   * the repo's own V1 map names the first of the five value dwords
#     (record+44) OFFSET_KHZ and appends range min / range max / applied after
#     it (nvapi-rs/sys/src/gpu/clock.rs, `clk_ctrl_entry`); the records this
#     driver reports carry types 3, 8 and 9, all inside `v1_marshalable`'s set,
#     so the V1 naming is the one that applies to them.
#   * the VALUES[2] counter-citation is a V2/Blackwell fact -- BASE 292,
#     STRIDE 772, VALUES@268, FREQ_KHZ = 0x114 == VALUES[2] -- i.e. a different
#     struct generation, and this driver rejects the V2 caller size (24996) at
#     0x1f, so the V2 value map is not evidence about this block.
#   * the measured write set agrees: 13 contiguous dwords per 0x08/0x09 record
#     (WRITE_SET_PER_TYPE below; rec+8 + 4*12 == rec+56 is where the last
#     changed dword of the last row lands), so value slot k sits at rec+8+4k and
#     "the caller's field k" and "RM slot k" are the same index.
# Still printed as CANDIDATES: only a same-field readback after a single-step
# SET can promote them to names.
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


# (id, size) -> the case label the dump printed, so a report says which run it
# came from when one capture holds the same block twice.
BLOCK_LABELS: dict[tuple[int, int], str] = {}


def parse_blocks(text: str) -> dict[tuple[int, int], bytes]:
    """Pull every `(id, size) -> payload` pair out of a --clkctl-hex dump.

    Two cases of the same (id, size) -- like 0x2080901d's zero background and its
    0xA5 one -- land in the same slot and the later dump wins, so the case label
    is kept alongside in BLOCK_LABELS and decode() prints it: which of the two
    runs a report came from is not something a reader should have to infer from
    the order of a file.
    """
    blocks: dict[tuple[int, int], bytearray] = {}
    current: tuple[int, int] | None = None
    for line in text.splitlines():
        match = BLOCK_RE.search(line)
        if match and "hex follows" in line:
            current = (int(match.group(1), 16), int(match.group(2)))
            label = re.search(r"V2\.clkctl\s+(.*?)\s+status", line)
            if label:
                BLOCK_LABELS[current] = label.group(1).strip()
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


A5_UNTOUCHED = 0xA5A5A5A5


def written_offsets(buf: bytes) -> set[int]:
    """Dwords the handler left something in.

    On an 0xA5 background every dword it did not touch still reads 0xa5a5a5a5, so
    the written set is visible end to end (including the dwords it wrote as
    zero).  On a zero background only the non-zero writes are visible, which is
    why this returns the set and the caller reports the background it saw.
    """
    out = set()
    for off in range(0, len(buf) - 3, 4):
        value = struct.unpack_from("<I", buf, off)[0]
        if value not in (0, A5_UNTOUCHED):
            out.add(off)
    return out


def report_one_d_stride(buf: bytes, out=sys.stdout) -> None:
    """Say which 0x2080901d reading the bytes in front of it support.

    Two readings are on the table (doc section 51.2): records 128 B apart (255 of
    them) or 1020 B apart (the 32 the GEOMETRY split assumes).  Both copies of the
    measured +12..+56 run are already known -- +12 and +140 -- so the decisive
    measurement is the *third* occurrence: +268 follows from 128, +1160 from 1020.

    An earlier version of this function scored how many written dwords have a
    written neighbour at each lag.  Its own selftest caught it tying 24/36 to
    24/36 with the third run sitting at +1160, so it now measures the one thing
    the candidates disagree about instead of a proxy that can tie.
    """
    background = (
        "0xA5 (untouched dwords stay 0xa5a5a5a5)"
        if any(
            struct.unpack_from("<I", buf, off)[0] == A5_UNTOUCHED
            for off in range(0, len(buf) - 3, 4)
        )
        else "zero (only non-zero writes are visible)"
    )
    written = written_offsets(buf)
    print(f"  stride: background {background}; {len(written)} dwords written", file=out)
    if not written:
        print(
            "  stride: nothing written, so this dump cannot argue either way", file=out
        )
        return
    first_run = [12 + 4 * i for i in range(12)]  # the measured +12..+56 dwords
    anchor_run = [off + 128 for off in first_run]  # the same run again at +140..+184
    anchor_visible = all(off in written for off in anchor_run)
    if not anchor_visible:
        print(
            "  stride: the +140..+184 run is not fully visible in this dump, so the"
            " third occurrence cannot be read off it -- this is the zero-background"
            " case's blind spot, not evidence",
            file=out,
        )
    for third, meaning in (
        (268, "records 128 B apart (the run at +140 plus 128)"),
        (1160, "records 1020 B apart (the +40 split plus 1020)"),
    ):
        state = "written" if third in written else "untouched/or zero"
        print(f"  stride probe: +{third} is {state} -- {meaning}", file=out)
    verdict = []
    if 268 in written and 1160 not in written:
        verdict.append("+268 alone => the 128 B period holds")
    if 1160 in written and 268 not in written:
        verdict.append("+1160 alone => the 1020 B record stride holds")
    if 268 in written and 1160 in written:
        verdict.append("both => one of them is a coincidence, not a table")
    if not verdict:
        verdict.append("neither => both readings are wrong, go back to the bytes")
    print(
        "  stride verdict: "
        + "; ".join(verdict)
        + f" (anchor run visible: {anchor_visible})",
        file=out,
    )


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
    label = BLOCK_LABELS.get((mid, size))
    if label:
        print(f"  case: {label}", file=out)
    if mid == 0x2080901D:
        report_one_d_stride(buf, out)
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


# The slot claim above is only as good as the Rust constants it quotes, and those
# constants live in another file that nothing here reads.  So read them: this
# turns "the repo's V1 map names slot 0 the offset" from prose into a check that
# fails loudly if nvapi-rs changes its mind.  Missing file (the tool copied out
# of the tree) is a skip, not a failure.
RUST_CLOCK = "nvapi-rs/sys/src/gpu/clock.rs"
# The marshal gates live in the high-level crate, not the sys one -- a second
# file, checked too, because "v1_marshalable covers the types this driver
# reports" is the other half of the argument.
RUST_MARSHAL = "nvapi-rs/src/clock.rs"


def cross_check_rust_field_map(verbose=True) -> bool:
    import re
    from pathlib import Path

    here = Path(__file__).resolve()
    for root in (here.parents[1], here.parents[2], Path.cwd()):
        path = root / RUST_CLOCK
        if path.is_file():
            break
    else:
        if verbose:
            print(f"  (skipped: {RUST_CLOCK} not found relative to {here.parents[1]})")
        return True
    text = path.read_text()
    here_parents = [here.parents[1], here.parents[2], Path.cwd()]
    for root in here_parents:
        marshal_path = root / RUST_MARSHAL
        if marshal_path.is_file():
            break
    else:
        if verbose:
            print(f"  (skipped: {RUST_MARSHAL} not found)")
        return True
    marshal = marshal_path.read_text()

    def const(module: str, name: str) -> int:
        body = text.split(f"pub mod {module} {{", 1)[1].split("\n    }", 1)[0]
        hit = re.search(rf"pub const {name}: usize = ([0-9]+);", body)
        assert hit, (module, name)
        return int(hit.group(1))

    slots = {
        "OFFSET_KHZ": const("clk_ctrl_entry", "OFFSET_KHZ"),
        "RANGE_MIN": const("clk_ctrl_entry", "RANGE_MIN"),
        "RANGE_MAX": const("clk_ctrl_entry", "RANGE_MAX"),
        "APPLIED": const("clk_ctrl_entry", "APPLIED"),
    }
    assert slots == {
        "OFFSET_KHZ": 44,
        "RANGE_MIN": 48,
        "RANGE_MAX": 52,
        "APPLIED": 56,
    }, slots
    # The five printed candidate slots are those four plus the next one, moved by
    # the one measured field delta -- so the tool cannot drift from the map.
    delta = 0x24
    expected = [v - delta for v in (*slots.values(), slots["APPLIED"] + 4)]
    printed = sorted(CANDIDATES[0x2080901B])
    assert printed == expected, (printed, expected)
    # The types this driver reports are inside the V1 marshal set, which is why
    # the V1 naming applies to them at all.
    assert re.search(
        r"fn v1_marshalable.*?matches!\(entry_type, 1 \| 3 \| 4 \| 5 \| 6 \| 7 \| 8 \| 9\)",
        marshal,
        re.S,
    ), "v1_marshalable no longer covers 3/8/9 -- the V1 naming would stop applying"
    # And the VALUES[2] counter-citation is a different generation: its base and
    # stride are not this family's, and its value block starts past them.
    assert const("clk_ctrl_entry_v2", "STRIDE") == 772, const(
        "clk_ctrl_entry_v2", "STRIDE"
    )
    assert const("clk_ctrl_entry_v2", "BASE") == 292, const("clk_ctrl_entry_v2", "BASE")
    assert const("clk_ctrl_entry", "STRIDE") == 72
    if verbose:
        print(
            "  rust field map: V1 offset slot"
            f" {slots['OFFSET_KHZ']} -> RM rec+{slots['OFFSET_KHZ'] - delta}"
            f" ; V2 stride {const('clk_ctrl_entry_v2', 'STRIDE')} is another generation"
        )
    return True


FIXTURE_ENV = "NVOC_CLKCTL_FIXTURES"


def fixture_path(rel: str) -> Path | None:
    """Resolve a fixture, preferring a local override over the committed copy.

    The committed fixture is what makes a measurement checkable away from the
    machine that produced it, so a missing file is reported, never assumed.
    """
    candidates = []
    if os.environ.get(FIXTURE_ENV):
        candidates.append(Path(os.environ[FIXTURE_ENV]) / Path(rel).name)
    root = Path(__file__).resolve().parents[2]
    candidates.append(root / rel)
    candidates.append(Path(rel))
    for candidate in candidates:
        if candidate.is_file():
            return candidate
    return None


def parse_a5_diff(text: str, mid: int) -> dict[int, int]:
    """Changed dwords the 0xA5 background diff printed for one command id.

    The printer emits `+OFFSET:a5a5a5a5>VALUE` groups after the id's summary
    line, so the lines are filtered by id first -- both batteries in the capture
    use the same diff format.
    """
    changed: dict[int, int] = {}
    for line in text.splitlines():
        if f"{mid:#010x}" not in line:
            continue
        for match in re.finditer(r"\+(\d+):[0-9a-f]{8}>([0-9a-f]{8})", line):
            changed[int(match.group(1))] = int(match.group(2), 16)
    return changed


def check_measured_repetition(verbose: bool = False) -> bool:
    """Re-derive the 0x2080901d repetition from the fixture, and keep it scoped.

    Asserts only what the artifact shows: the twelve dwords +12..+56 reappear
    verbatim one 128-byte shift later at +140..+184, the printed window ends at
    +184, and the summary's last changed dword (+3376) is far outside that window
    -- which is why the record stride is still unclaimed.  Returns False (with a
    printed SKIP) when the fixture is absent, so a missing artifact can never
    look like a passing check.
    """
    rep = MEASURED_REPETITION[0x2080901D]
    path = fixture_path(rep["fixture"])
    if path is None:
        print(
            f"SKIP measured repetition: no fixture at {rep['fixture']}"
            f" (set {FIXTURE_ENV} to the directory holding it; the captured copy is"
            f" outputs/{rep['artifact']})"
        )
        return False
    data = path.read_bytes()
    digest = hashlib.md5(data).hexdigest()
    assert digest == rep["fixture_md5"], (
        f"fixture drifted: {digest} != {rep['fixture_md5']}"
    )
    text = data.decode("utf-8", errors="replace")
    changed = parse_a5_diff(text, 0x2080901D)
    assert changed, "no 0x2080901d diff lines in the fixture"
    assert rep["summary"] in text, (
        "the summary line this entry quotes is not in the fixture"
    )

    start, count, second = (
        rep["subblock_start"],
        rep["subblock_dwords"],
        rep["second_occurrence"],
    )
    assert second == start + rep["observed_shift"], "the shift and the offsets disagree"
    run = [start + 4 * i for i in range(count)]
    copy_run = [off + rep["observed_shift"] for off in run]
    assert copy_run == [second + 4 * i for i in range(count)]
    missing = [off for off in run + copy_run if off not in changed]
    assert not missing, f"the fixture does not show the repetition at {missing}"
    # The window is the printer's first-40 slice, not the whole diff.
    assert max(changed) == rep["window_ends_at"] == run[-1] + rep["observed_shift"], (
        sorted(changed)
    )
    last_changed = int(re.search(r"last @(\d+)", rep["summary"]).group(1))
    assert max(changed) < last_changed, (
        "the printed window would contain the whole diff"
    )
    for key in ("prediction_if_period_128", "prediction_if_stride_1020"):
        assert str(rep["observed_shift"] if "128" in key else 1020) in rep[key]
    assert (32680 - 40) % 128 == 0 and (32680 - 40) // 128 == 255, (
        "the 128 B split arithmetic moved"
    )
    if verbose:
        print(
            f"  measured repetition: 0x2080901d dwords +{run[0]}..+{run[-1]} reappear at"
            f" +{copy_run[0]}..+{copy_run[-1]} (shift {rep['observed_shift']}); window ends at"
            f" +{max(changed)} while the diff reaches +{last_changed}, so the stride stays open:"
            f" {rep['prediction_if_period_128']} vs {rep['prediction_if_stride_1020']}"
        )
    return True


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
    assert sorted(solutions) == [(40, 1020), (168, 1016), (296, 1012), (424, 1008)], (
        solutions
    )
    for head, stride in solutions:
        assert stride % 128 != 0, (head, stride)
    measured = check_measured_repetition(verbose=True)
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
    # The stride question the next full dump has to answer, exercised both ways on
    # the artifact a real one would look like (0xA5 background, so untouched dwords
    # stay 0xa5a5a5a5 and every written one is visible).  A third occurrence at
    # +268 must read as the 128 B period and one at +1160 as the 1020 B stride; a
    # block that shows neither, or shows both, must say so instead of picking one.
    import io

    def one_d(thirds: tuple[int, ...] = (), background: int = A5_UNTOUCHED) -> bytes:
        buf = bytearray(32680)
        if background:
            for off in range(0, len(buf) - 3, 4):
                struct.pack_into("<I", buf, off, background)
        for start in (12, 140, *thirds):
            for index in range(12):
                struct.pack_into("<I", buf, start + 4 * index, 0x00850800 + index)
        return bytes(buf)

    expectations = {
        268: "+268 alone => the 128 B period holds",
        1160: "+1160 alone => the 1020 B record stride holds",
    }
    for third, verdict in expectations.items():
        sink = io.StringIO()
        assert decode(0x2080901D, 32680, one_d((third,)), sink) == 0
        text = sink.getvalue()
        assert verdict in text, text
        assert "(anchor run visible: True)" in text, text
        assert f"+{268 if third == 1160 else 1160} is untouched/or zero" in text, text
    for label, block, verdict in (
        ("neither", one_d(), "both readings are wrong"),
        ("both", one_d((268, 1160)), "one of them is a coincidence"),
    ):
        sink = io.StringIO()
        decode(0x2080901D, 32680, block, sink)
        assert verdict in sink.getvalue(), (label, sink.getvalue())
    zero_bg = io.StringIO()
    decode(0x2080901D, 32680, bytes(32680), zero_bg)
    assert "nothing written, so this dump cannot argue either way" in zero_bg.getvalue()
    # The blind spot of a zero background has to be named, not read as evidence:
    # drop the anchor run and the third occurrence stops being readable.
    hidden = bytearray(one_d((268,)))
    for index in range(12):
        struct.pack_into("<I", hidden, 140 + 4 * index, 0)
    sink = io.StringIO()
    decode(0x2080901D, 32680, bytes(hidden), sink)
    assert "not fully visible in this dump" in sink.getvalue(), sink.getvalue()
    cross_check_rust_field_map()
    # The committed fixture is the whole point of the check above, so a missing
    # file is a failure here even though check_measured_repetition() treats it as
    # a printed SKIP when the tool runs away from the repo.
    assert measured, (
        "the committed 0x2080901d fixture is missing; run without --selftest to skip it"
    )
    # Negative path: the check has to fail when the bytes stop supporting the
    # claim -- once at the hash gate, and once at the repetition itself (the hash
    # is repointed for that second half, because a mutant is not the fixture).
    import tempfile

    rep = MEASURED_REPETITION[0x2080901D]
    original = fixture_path(rep["fixture"])
    assert original is not None
    bytes_at_start = original.read_bytes()
    mutant = bytes_at_start.decode().replace("+140:", "+140X:")
    assert mutant != bytes_at_start.decode(), "the mutation did not change anything"
    with tempfile.TemporaryDirectory() as tmp:
        Path(tmp, Path(rep["fixture"]).name).write_text(mutant)
        os.environ[FIXTURE_ENV] = tmp
        try:
            try:
                check_measured_repetition()
                raise AssertionError("the hash gate did not fire on a mutated fixture")
            except AssertionError as err:
                assert "fixture drifted" in str(err), err
            rep["fixture_md5"] = hashlib.md5(mutant.encode()).hexdigest()
            try:
                check_measured_repetition()
                raise AssertionError("a missing repetition was not caught")
            except AssertionError as err:
                assert "does not show the repetition" in str(err), err
        finally:
            rep["fixture_md5"] = hashlib.md5(bytes_at_start).hexdigest()
            os.environ.pop(FIXTURE_ENV, None)
    print(
        "selftest ok: geometry + round-trip + field decode + write-set model + rust field map"
        " + measured 1d repetition"
    )
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
