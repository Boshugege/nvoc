# RM parameter marshalling — corrections from the macOS TU116 host

> Status: read-only investigation record, 2026-10-17. Host: macOS 15.8.1 /
> x86_64 / GTX 1650 SUPER (TU116, 10de:2187) / NullMoth 2.0.4 /
> NVRM 610.57.04. No writes, no driver changes, no EFI/boot-args changes.

This is the "candidate mapping correction table" for the NVAPI→RM marshalling:
what the existing records claim, what the source actually is, and what a
reproducible read-only request looks like. It exists because two earlier notes
mixed up **coordinate systems**, and because two ids turn out not to go through
the NVOC export table at all.

## 1. The base-offset error (the "76-byte stride with fields past 76" paradox)

`power.rs` records the private VoltRails GetInfo family as *"rail-info entries:
192-byte stride indexed by rail BIT, type u32 @+76 … then SIX u32 @+76..+100
which SPAN PAST the slot stride"*, and
`voltrails-family-full-layout-r610.md` records *"GetInfo → RM 0x2080B201,
stride 76 B, type byte @+0x44, class byte @+0x77"*. A 76-byte record cannot
hold a field at +0x77 — so at least one of the two numbers is in the wrong
coordinate system. Both are.

Three different coordinate systems were being conflated:

1. **NVAPI exported struct offsets** (`+76`, `+80`, … `+132`) — offsets inside
   the 6220-byte `NV_GPU_VOLT_RAILS_INFO` the *caller* passes. Its rail slot is
   192 B, which is why the numbers run past 76.
2. **Escape-container offsets** (`0x44`, `0x6C`, `0x77`, `0x3C`, `0x40`) —
   offsets inside the 500 008-byte (`0x7A118`) escape-`0x07000191` buffer that
   the Windows NVAPI layer builds. Its header is **14 dwords = 0x38 bytes**
   (`buf[12]`=gpu, `buf[13]`=RM ctrl cmd); the RM parameter block begins at
   **container + 0x38**.
3. **RM response record offsets** — what the RM actually fills, measured here.

Subtracting the container base (`0x38` = 56) from every recorded source offset
puts *all* of them inside a 76-byte record:

```
recorded (container)  −  0x38  =  RM param offset
0x3C (mask dword)         →  4    (head +4)
0x40 (one byte)           →  8    (head +8)
0x44 (type byte)          → 12    record 0 + 0
0x48 (µV A)               → 16    record 0 + 4
0x68 (u32, "959")         → 48    record 0 + 36
0x6C (µV B)               → 52    record 0 + 40
0x77 (rail class)         → 63    record 0 + 51
0x84 (tail end)           → 76    record 0 + 64   (still < 76-byte stride)
```

The 76-byte stride itself is confirmed **independently** by the size gate on
this host: `0x2080B201` is accepted at 2444 = **12 + 32 × 76** and rejected at
2440, on both the subdevice and the device object (bundle
`stage-clkobj-20261017T074744Z`). So: **12-byte head + 32 × 76-byte records.**

## 2. Corrected `0x2080B201` (VoltRails GetInfo) layout, with live TU116 values

Read: `0x2080B201`, paramSize 2444, zero-filled, `hSubdevice`
(bundles `stage-clkdoc-20261017T074256Z`, `stage-clkobj-20261017T074744Z`,
`stage-svc-20261017T085500Z`).

Head (12 B):

| off | TU116 | reading |
| --- | --- | --- |
| +0 | `0x000000FF` | output rail bitmask; **not** an input filter — seeding `0xFFFFFFFF` returns `0xFF` unchanged, seeding 0 returns `0xFF` |
| +4 | `0x00000001` | present-rail mask, `|= 1<<bit` accumulated over the loop (matches the single populated rail) |
| +8 | `0x00` | the byte the record map calls "struct byte +8" |

Record `k` at absolute `12 + 76k`:

| rec off | width | recorded off | TU116 live | meaning (recorded) |
| --- | --- | --- | --- | --- |
| +0 | u8/u32 | 0x44 | `2` | type tag → encoder `2 → 0` (legacy format) |
| +4 | u32 | 0x48 | `800000` | µV A (recorded 4060L: 750000) |
| +8 | u16 | 0x4C | `0` | |
| +10 | u16 | 0x4E | `1` | recorded `0xFFFF` "invalid marker" |
| +12 | u16 | 0x50 | `3` | |
| +15 | u8 | 0x53 | `0` | |
| +16 | u8 | 0x54 | `1` | recorded live 1 ✓ |
| +18 | u16 | 0x56 | `2` | |
| +20 | u16 | 0x58 | `4` | |
| +22 | u8 | 0x5A | `16` | recorded live 16 ✓ |
| +26 | u8 | 0x5E | `1` | |
| +28 | u32 | 0x60 | `8` | |
| +32 | u32 | 0x64 | `1` | recorded live 1 ✓ |
| +36 | u32 | 0x68 | `959` | recorded live 959 — **exact match** |
| +40 | u32 | 0x6C | `756250` | µV B (recorded 4060L: 820000) |
| +51 | u8 | 0x77 | `1` | rail class 1..8 — recorded live 1, **exact match** |
| +54 | u8 | 0x7A | `0` | type-0 tail |
| +56 | u16 | 0x7C | `0xFFFF` | type-0 tail |
| +58 | u16 | 0x7E | `0xFFFF` | type-0 tail |
| +60 | u32 | 0x80 | `0` | type-2/3 tail |
| +64 | u8 | 0x84 | `0` | |

Five independent fields land on their recorded values (mask bit, type `2`,
`959`, `1`, rail class `1`) and the µV pair `+4`/`+40` keeps the recorded
36-byte spacing, so the −0x38 correction is not a fit: it is pinned. The two
µV readings are the only rail-domain *values* this host returns; all other 31
records are zero, i.e. this board exposes one rail.

## 3. Where these records are wrong about the transport

| claim in the record | measured on this host |
| --- | --- |
| `0x2080B201` is an RM ctrl cmd reached through escape `0x07000191` | the RM **does** answer `0x2080B201`, but only at `paramSize ≥ 2444` (2448 / 2480 / 2500 / 500008 also `NV_OK`, 2440 and below `0x1f`) — a *minimum-size* gate, which the NVOC gate in `control.c:448-458` (exact equality) cannot produce |
| the id is part of the driver's exported method table | it is **absent** from the 604-row table extracted from the loaded image |
| the carrier object is `NV20_SUBDEVICE_0` | it answers on **both** the subdevice and the device object; the client object answers `0x56` |

> **Resolved later the same day (11:50Z).** The "minimum-size gate the NVOC
> gate cannot produce" question this table raises is settled in
> `gsp-method-table-and-legacy-forwarding.md` §12/§13: the gate is on the
> **forwarded** side. The same short-size request that the host rejects with
> `0x1f` answers `0x62` while the GSP is wedged, and the client carrier — where
> the forward is skipped — answers `0x56` at *every* size. So `0x2080B201` is
> GSP-answered and needs no unexplained host path.

The `(version<<16|size)` magics (`0x10964`, `0x78604`, …) are **NVAPI-layer**
values — they are compared by the NVAPI handler, not by the RM. The RM
parameter block for `0x2080B201` carries no magic at all (dword 0 is an
*output* mask). Seeding those magics into RM parameter blocks is therefore not
evidence either way about the RM.

## 4. Reproducing

```
# on the test machine (read-only; no --write, no SET ids in the list)
probe-macos-rm/probe-macos-rm-idle --stages 0 --clkdoc-read --clkobj-read
```

`--clkobj-read` is the carrier axis (same id through hSubdevice / hDevice /
hClient), `--clkdoc-read` the documented-triple pass; both refuse SET ids before
sending. The clock-offset family has its own read-only runbook,
`probe-macos-rm/tools/clock-offset-read-run.sh`, which is the pass that seeds the
selection mask at the offset measured in section 5 (the two passes above seeded
it at the documented +8 and therefore selected nothing). The raw payload of the accepted `0x2080B201 @2444` read is two hex
lines of 16 dwords each in `clkdoc-read.txt` of the bundles above.

Bundles: `stage-clkdoc-20261017T074256Z`, `stage-clkobj-20261017T074744Z`,
`stage-clkobj-20261017T074755Z`, `stage-reg-20261017T082800Z`,
`stage-svc-20261017T085500Z`.

## 5. The clock-domain private ids: caller sizes are not RM sizes, and the mask is an input at head +4

The same conflation as section 1, in a family where it has a measurable
consequence. The clock-domain ids are private (`0x20809019` ClockClkDomainsGetInfo,
`0x2080901b` ClockClkDomainsGetControl, `0x20809025` FreqCtrlGetInfo); the
documented triple sizes come from the NVAPI caller structs, whose stamp is
literally `(version << 16) | size` and which the RM never sees.

| id | RM block size accepted (`NV_OK`) | geometry | NVAPI caller size (from its own stamp) | that size on the RM |
| --- | --- | --- | --- | --- |
| `0x20809019` | 12336 | 48 + 32 x 384 | 2520 (`0x000109d8`) | `0x1f` |
| `0x2080901b` | 2108 | 60 + 32 x 64 | 2404 (`0x00010964`), 24996 (`0x000261a4`) | `0x1f` |
| `0x20809025` | 7188 | 20 + 32 x 224 | 3148 | `0x1f` |

Each geometry is the exact division of the measured accepted size into a head
plus 32 slots (`12_336 = 48 + 32x384`, `2_108 = 60 + 32x64`,
`7_188 = 20 + 32x224`); for the control block the head length and stride are
independently pinned by the write map below, and for the directory and the range
the record stamps line up with directory entries at the same bases. The slot
count 32 is the shape that divides exactly, not a separately measured capacity.

Bundles: `stage-clkctl-hex-*` (accepted reads at the RM sizes, full byte dumps),
`stage-clkdoc-20261017T074256Z` / `T074448Z` (the documented sizes, all
`0x1f NV_ERR_INVALID_ARGUMENT`), `stage-clkdom-20261017T091500Z` (neighbouring
sizes: 2520, 2464, 2404, 2348, 2316, 2444 -- all `0x1f`). So for these ids the
gate is not "at least the struct size": every size but the RM one is refused, and
a wrong size is refused *with the mask seeded as well*, which is what makes the
two fields separable.

**The selection mask sits at head +4, and it is an input.** Measured on an `0xA5`
background: seeding `0xFF` at +4 answers `NV_OK` and writes 106 dwords
(`first @0 last @564`), and +4 is the only head dword left out of that
changed-dword set. That last observation is weaker than it looks -- the seed
overwrote the `0xA5` with `0xFF`, so a handler that *echoes* the seed and a
handler that leaves the field alone produce the same diff, and the recorded
Windows-side layout below says the driver does echo it. So: the field is read
(behaviour depends on it) and it is not modified in a way this run can see; the
two are separated only by a seed the driver would not return unchanged.
Seeding
`0xFF` into +0, +8, +12, +16, +20, +24, +28, +32 or +36 instead answers
`0x25 NV_ERR_INVALID_DATA`; seeding 0 answers `NV_OK` and writes nothing, which
is the trap: "no offset applied" and "no record requested" are the same reply.
106 decomposes exactly as 14 head dwords + 7 x 13 value dwords + 1 stamp-only
record, i.e. the mask selects entries **by directory index**, not by domain bit
(a domain-bit reading predicts 105).

Section 1 already derived this same location from the recorded container offset:
`0x3C (mask dword) -> 4 (head +4)`. Two independent lines therefore agree -- the
`-0x38` rule taken from the rail family, and a direct seed sweep on the clock
family. The *direction* differs from `0x2080B201`, whose head +4 is an output
mask the driver fills; here the driver reads it. Both are "the mask slot in the
head", so the offset agreement is meaningful, but neither may be used to infer
the other's direction.

**The 64-byte record stride is measured, not derived.** The record bases are at
`60 + 64i` (the `u16` type stamps `0x0808` at +60 and `0x0909` at +124 match
directory entries 0 and 1), so the documented 72- and 76-byte strides are both
wrong for this block. Value dwords land at record-relative
`{8,12,16,20,24,28,32,36,40,44,52,56}`; `+4`, `+48` and `+60` are never written.

**Caution when reading the earlier passes in this area.** Their per-case
"`N` non-zero B" counts included the caller's own seed -- `mask@+8=0xff`
reporting "1 non-zero B" on a case the driver *rejected* -- so a rejected row's
count says nothing about the driver. Combined with the +8 seed, that is how the
old note "mask 0 / 3 non-zero bytes, +0 +8" came to look like evidence that the
driver returns nothing, when in fact nothing had been selected.

### 5.1 The caller-side coordinate system, and what the directory table is for

The NVAPI struct for this family is `NV_GPU_CLOCK_CLIENT_CLK_DOMAINS_CONTROL_V1`
(2404 B, magic `0x10964` = version 1 | size 2404 -- the exact size the RM
refuses). Its layout is recorded in `nvapi-rs/sys/src/gpu/clock.rs` from IDA plus
a live dump on an Ada 4060 Laptop: mask at absolute +8, then 32 **bit-sparse**
records at absolute `+100 + 72*bit` -- a record for domain bit N at `+100+72*N` --
with a type byte at record+0 and named fields at record+44 (`offset_kHz`, i32),
+48 (`range_min`), +52 (`range_max`), +56 (`applied`), +60 (one more).

| | NVAPI caller struct | RM block (measured here) |
| --- | --- | --- |
| selection field | mask at +8; the Windows-side note says the driver **echoes the seed**, not the real controllable set | mask at +4, read (see above); echo unresolved |
| record base | +100 | +60 |
| record stride | 72 | 64 |
| record fields | type u8 @+0; +44 / +48 / +52 / +56 / +60 | stamp u16 @+0; value dwords @+8..+44, +52, +56; +4 / +48 / +60 never written |
| record addressing | by **domain bit** (`+100+72*bit`) | by **directory index** (`0xFF` selects indices 0..7) |

So these are two tables of the same shape (mask + 32 slots) in different
coordinates, and the caller record is **not** a field-by-field copy of the RM
record. The positional overlap is partial and is best treated as a coincidence
until something differential confirms it: the RM writes record+44, +52 and +56,
which are positionally the caller's `offset_kHz`, `range_max` and `applied`
slots, but it also writes ten dwords from +8 to +44 that the caller layout does
not name, and it does not write the caller's +48 (`range_min`) or +60. What that
buys is a *named candidate set* for three of the twelve slots -- with the caveat
that the caller's `range_min` slot has no RM counterpart in the written set, so a
straight name transfer is already contradicted once.

**The two addressings differ, and that is what the directory is for.**
Hypothesis, not fact: the caller addresses records by domain bit, the RM selects
them by directory index, and the directory block carries, per entry, both the
index (its position) and the domain bit (`1<<id`) plus the type stamp -- so it is
the table a marshalling layer needs to translate a named domain into the record
the control block expects. Evidence: the directory's `domain_bit` values are
powers of two covering bit positions 0-6 and 19-21, the same two clusters as
`NV_GPU_CLOCK_DOMAIN_ID`; the control's stamps match directory entries by index;
and the 106-dword arithmetic above fixes index addressing for the control side.
Falsification path: read the control block with a single-index mask (`0x1`, the
probe runbook's third case) -- index addressing predicts records for exactly one
entry, i.e. 14 + 13 dwords for a value-carrying entry, while bit addressing for
that same mask would select the entry stamped bit 0.

#### 5.2.1 Which rows the measured seed actually reached (this round)

The directory's own rows settle what any "the value dwords read zero" statement
covers, because a row the seed never selected had its dwords left alone. Read as
row bits, the seed the recorded run used (`0xFF` at `+4`) reaches rows 0-7 and
stops there; the rows beyond it are row 8, whose `1<<id` names id 21 (MSD), and
row 9, id 5 (HOST).

That matters for this project's own scope: the domains the backend reports are
core (id 0) and memory (id 4), which the rows place at row 0 and row 2 -- *inside*
the measured window. The two unchecked rows are peripheral, so the caveat is
bounded to them rather than to the reading as a whole. Re-running the decoder on
the recorded payload also re-confirmed the header identity (`+12` and `+20` both
`0x38007f`, the union of the ten rows' `1<<id`, against `0x3ff` for rows-as-bits),
so the placement is derived from the same rows the header check validates.

One further observation is recorded as a *candidate*, because its attribution is
proven but its quantity is not: exactly the two rows that carry a per-domain
`i16` pair at `+44` are row 0 (id 0, core) with `{-1000, 1000}` and row 2 (id 4,
memory) with `{-1000, 3000}`, and row 0 alone carries `+40 = 30000`. Unlike the
range block, whose three records cannot be attributed to named domains, these
pairs sit in rows that *are* named. Read as kHz they would be +-1 MHz and
-1/+3 MHz -- noticeably narrower than the range block's +-18750 kHz pair, so they
are probably not the same quantity. Left as a candidate and as a second, better
-attributed possible source for the limits; no unit is asserted.

### 5.3 The 0x24 coordinate constant, re-derived from the recorded bytes (field-level only)

The candidate value map in this document and in `decode-clk-containers.py`'s
`CANDIDATES` table rests on one constant: that a caller-side field coordinate
minus 0x24 gives the RM-side one. That was inherited from the earlier session;
this section re-derives it from the recorded payloads so the basis is checkable.

`0x20809025` (7188 B, head `+0 ff +4 07 +8 0 +12 64 +16 100`, records at
`20 + 224*i`) carries real values, unlike every control record at stock:

| Caller-side field (nvapi-rs `clk_freq_ctrl_info_entry`) | RM-side coordinate and value in the recorded reply |
| --- | --- |
| `REC_MAX_KHZ = 0x2C` | RM row `+0x08 = 1638400` (1638.4 MHz) |
| `REC_MIN_OFFSET = 0x38` | RM row `+0x14 = -18750` |
| `REC_MAX_OFFSET = 0x3C` | RM row `+0x18 = +18750` |

All three deltas are exactly 0x24, and the values match the caller's own recorded
comments (`live 1638400 = 1638.4 MHz`, `-18750 (freq offset floor, +-18.75 MHz
pair)`), which is what makes this a derivation rather than a coincidence: three
independent field pairs, identical values, one constant.

**What it does *not* license.** It is a *field-level* constant, not a
whole-record alignment. The same structs disagree on other coordinates: the
caller's `TYPE` is at record `+0x00` while the RM row's `+0x00` is the tag
`0x01030101`, and the caller's mask is at `+8` while the RM mask is at `+4`. The
struct-level geometry differs too -- caller base `0x4C` with stride `0x60` for
`0x20809025` against RM base `20` with stride `224`, and for the control block
caller base `100` with stride `72` against RM base `60` with stride `64`. So the
candidate map stays a candidate: the shift gives *coordinates worth testing*, and
only a same-field readback after one gated minimal-step SET can name the field
that carries the offset.

Applied to the control block, the constant predicts `OFFSET_KHZ` at RM `+0x08`,
`RANGE_MIN` at `+0x0C`, `RANGE_MAX` at `+0x10` and `APPLIED` at `+0x14` -- all
four inside the twelve value dwords the driver is measured to write (`+8 ... +24`,
`+28 ... +44`, `+52`, `+56`), which is the consistency the candidate table needs
and also the reason two different fields (+8 and +16) remain live candidates for
"the offset": the article's prose says the first slot, the Blackwell field map
says `VALUES[2]`.

### 5.2 The directory header carries both namespaces, which settles the mask's addressing

Most of the individual facts here are already recorded elsewhere --
`gsp-method-table-and-legacy-forwarding.md` §14.3/§18 has the directory's
row/type/domain-bit table, and the feature matrix's entry for that round says the
head's `+12`/`+20` are a domain-bit mask. What this section adds is that the two
identities below are now stated as arithmetic, verified against the raw dump, and
enforced by `probe-macos-rm/tools/decode-clk-containers.py` on every future dump
rather than carried as prose.

Decoded from the recorded reply to `0x20809019` (12336 B = 48 + 32x384): head
`+0 0xff, +4 0x3ff, +8 0x335, +12 0x38007f, +16 0x3fc007f, +20 0x38007f,
+24 0x85, +28 0x5, +36 0x103e8, +40 0xffff0000`, then ten non-empty rows, each
with a two-byte type pair at record+0 (0303 for row 6, 0808 for rows 0/2/7, 0909
for the rest) and a `1<<id` value at record+4.

Two of those head dwords are exact arithmetic identities over the same ten rows:

* `+4 = 0x3ff` is ten consecutive bits, i.e. bit *i* for directory row *i*.
* `+12` and `+20` = `0x38007f` = **exactly the union of the ten rows'
  `1<<id` values** (`1|2|16|4|8|0x100000|0x80000|64|0x200000|32`), i.e. bits
  0-6 and 19-21 -- the two clusters of `NV_GPU_CLOCK_DOMAIN_ID`, ids
  `0,1,4,2,3,20,19,6,21,5` as rows 0..9.

So the same header states the same ten domains in *both* namespaces, and they are
different numbers. That matters because the row-selection mask I measured on the
control block (`+4`, `0xFF` -> the eight rows at indices 0..7, 106 dwords) had
been read as index addressing on the strength of a dword count alone (106 vs the
105 a domain-bit reading predicts). The directory's own header is independent
evidence for the same conclusion: a mask of `0xFF` in domain-bit space would mean
ids 0-7, and the rows present are ids 0,1,2,3,4,5,6 only... in fact `0xFF` as
domain bits is ids 0..7, of which the card has 0,1,2,3,4,5,6 -- seven rows, 105
dwords. The measured 106 therefore requires index space, and now the header shows
the two spaces side by side.

**The driver declares ten rows, not eight.** So the runbook's `0xFF` seed reached
eight of the ten the driver itself reports; the two it missed are rows 8 and 9
(ids 21 MSD and 5 HOST). Seeding `0x3FF` -- the runbook's second case -- predicts
`14 + 9x13 + 1 = 132` written dwords (nine value-carrying rows plus the stub at
row 6), which turns that case from an open probe into a number to check.

**The two rows that carry offset-relevant data are the core and the memory.**
Rows 0 and 2 are the only 0808-type rows with extra fields, and under the domain
id order above they are id 0 (GPC, the core) and id 4 (M, the memory). They carry
a signed 16-bit pair at record+44 (`{0xfc18,0x03e8}` = {-1000,1000} for row 0,
`{0xfc18,0x0bb8}` = {-1000,3000} for row 2) and a word at record+76 (0x22A and
0x100); row 0 alone has `+40 = 30000`. Both rows are inside the `0xFF` seed that
has already been run, so their control-block records are already measured -- and
their value dwords came back zero, which is consistent with a stock offset of
zero and does not by itself name the field.

Cross-block consistency: row 6, the 0303 row, is the value-less one in the
directory *and* the stamp-only one in the control block. That is a second
independent sign that the two blocks' rows are the same rows in the same order.

The tooling consequence: `decode-clk-containers.py` now prints both masks for a
directory block, with the index-space mask derived from the rows actually
present, and reports MATCH/mismatch against the header. Its selftest rebuilds the
live ten rows and checks both directions -- the live case must MATCH, and a
header edited away from the union must be reported as a mismatch.

**Still open, deliberately:** the field identity inside the 64-byte control
record (needs a differential -- a write, or a read at a non-zero offset); the
meaning of head `+8` (0x335), `+16` (0x3fc007f, the union plus bits 18 and
22-25), `+24` (0x85), `+36` (0x103e8), `+40` (0xffff0000); and the range block's
tags, which increment in their second byte (0x01030101/0x01040101/0x01050101)
with otherwise identical payloads -- if that byte is a domain id, the range block
covers ids 3/4/5, which includes the memory but not the core, so the two blocks
may not share an id namespace and the directory stays the authority for the map.
Head `+28 = 5` is index-space bits 0 and 2, i.e. exactly the core and memory rows,
which is an attractive reading but is recorded only as a candidate: no second
observation yet distinguishes it from a version or a count.

## 6. What the forwarding table's own columns declare (this round)

The write plan for the offset channel has rested on the id arithmetic of the
twin rule (a write id is a read id with bit 14 set). This section adds the
table's own statements, measured on the corrected 1118-row dump
(`outputs/stage-gsp-table-recount-20261017T130832Z/gsp-table.tsv`, re-derivable
with `probe-macos-rm/tools/summarise-method-table.py`).

Halves, by the category byte that encodes them: 691 public, 335 legacy read
(`0x80-0xBF`), 92 legacy write (`0xC0-0xFF`). The `flags` column separates those
two halves by direction:

| half | rows | flag bit `0x08` | flag bit `0x04` | dominant flags |
| --- | --- | --- | --- | --- |
| legacy read | 335 | 332 | 2 | `0x0048` (278 rows) |
| legacy write | 92 | 10 | 82 | `0x0044` (78 rows) |

So the table itself models the read/write pairing the twin rule assumes, and it
does so where it matters for this project: the whole private clock/volt/thermal
control surface lives in those halves. The exceptions are printed by id rather
than waved at, so a driver update surfaces as a number: three read-half rows
(`0x2080854a`, `0x2080a70f`, `0x2080b703`) and ten write-half rows
(`0x2080c54d`, `0x2080c601/2/3`, `0x2080d0b3`, `0x2080d501/2`, `0x2080e0cf`,
`0x2080e802`, `0x2080f310`). The public half mixes directions (`0x00c0`, `0x0048`,
`0x0008`, `0x0044`), so the bit is a property of the legacy halves, not a
universal classifier.

The concrete case for the write plan: `0x2080901b` (read, 2108 B, flags `0x0048`)
and `0x2080d01c` (write, 2108 B, flags `0x0044`) are a **same-size sibling pair**
with complementary direction bits. That is a third, independent support for
reading the SET block as the same struct as the GET block -- the id arithmetic,
the size match, and now the table's direction flag -- and it is why "write the
coordinate the GET reports" is a defensible plan rather than a hope. Twenty-one
read ids have a present `|0x4000` twin at all, and only four of those are
same-size; the offset pair is one of the four.

One negative result is worth as much as the positive one: **`accessRight` is 0
for all 1118 rows** except a single public row (`0x2080b213`, value 2). It
therefore does *not* encode the privilege that makes the write path refuse below
root, and it must not be cited for that. The below-root refusal stays what the
runtime measurement says it is: a precondition of the forwarding path the root
owner holds, not a missing method and not a table flag.

## 7. The same commands as NVAPI wraps them: an 8-slot view of our 12-dword record, and the domain index space

Sections 5–6 established the geometry from the TU116 host side. This section records
what the *same four commands* look like from the other side of the wall, because the
in-tree `nvapi-rs` describes them and its comments carry live A/B results. It is a
cross-reference, not new measurement: everything below is either quoted from that
copy or read out of our own captured blocks (`core/tests/data/macos-offset-*.bin`).

`nvapi-rs/src/clock.rs` wraps the identical RM commands: GetControl
`0x2080901b`/ID `0xF58938F5` and SetControl `0x2080d01c` are exposed as private
NVAPI IDs behind escape `0x07000049`, and its header comment says all four were
live-verified on an Ada 4060 Laptop under R575.74. The semantics below therefore
describe **the same command on a different chip and driver version** — useful as a
hypothesis source, never as a substitute for a TU116 measurement.

### 7.1 The record geometry differs, so the value dwords must not be mapped one-to-one

| | NVAPI caller view (cited) | our measured RM block |
| --- | --- | --- |
| block | 2404 B, mask at +8, records at +100 | 2108 B, mask at +4, records at +60 |
| record stride | 72 B | 64 B |
| value slots | `values_kHz: [i32; 8]` — **8** dwords | **12** dwords written per record (record-relative `{8,12,16,20,24,28,32,36,40,44,52,56}`) |

The manager's own struct is the one that would settle this, but the two caller views
agree on nothing structural except the 32-record count. So our twelve dwords are not
the documented eight and pairing them by position is a guess until the A/B.

### 7.2 The documented slot order — cited, and marked unconfirmed by its own author

`ClkDomainControlEntry.values_kHz` carries this comment (verbatim from the copy):

> Slot semantics are driver-opaque — per the article slot 0 is the signed frequency
> offset (kHz) and neighbors are range/voltage terms, but only an A/B SET +
> MEASURE_FREQ experiment confirms which is which.

Two things follow. First, the article's claim (`melonvolt/xbar.txt`, cited in the
same header) agrees with our own recorded guess that the offset is the first value
dword, but neither source has shown it. Second, and usefully, the queued first write
in `tools/clock-offset-write-run.sh` — `BIT 0` with the value at the record's
`+8` — is *already* slot 0, so whichever way that A/B comes out it tests the
documented hypothesis first rather than a random dword.

### 7.3 Which record types carry a value write, and one discrepancy with our measurement

The copy records the driver's per-type switch as a set, and says explicitly that it
"is derived purely from the record TYPE byte, NOT a driver capability bit, and says
NOTHING about readability" — which is the same distinction our `unavailable`
wording makes. For the V1 protocol (magic `0x10964`, the size family this host
speaks) it lists the marshalled types as `{1, 3..=9}`; type `0x02` is marshalled by
neither version and a SetControl silently drops it; type `0x0A` is V2-only.

Our own stage-7.5 measurement found the write set to be type-conditional and
all-or-nothing — the type-08/09 records each took twelve value dwords while the
single type-03 record "writes its stamp only". **The V1 set includes 3**, so either
the byte we read as a type is not the protocol type, or the two versions disagree.
Recorded as an open discrepancy: the write runbook must not assume a type-03 record
accepts a value write.

### 7.4 Bit 5 names the wrong domain for *control* records

The copy's live A/B on Ada: offsetting the control record at bit 5 moved the
MSD-attributed V/F curve while the Host measure channel stayed in band, so "the
control RECORD at bit 5 drives the MSD domain, not the Host the RTSS label at index
5 names", and consumers rendering names "should relabel bit 5 to MSD". Our own
round-19 note had already used MSD for id 21 and Host for id 5; this is the warning
to carry if the payload ever names control records.

### 7.5 The index space, named from our own captured blocks

The names are RTSS's, from `nvapi-rs/sys/src/gpu/clock.rs` (`NV_GPU_CLOCK_DOMAIN_ID`:
`Gpc=0, Xbar=1, Sys=2, Hub=3, M=4, Host=5, Disp=6, …, Leg=19, Pwr=20, Msd=21`, with
the header noting the names come from RTSS and that RTSS aliases some indices).

Decoding our captured `0x20809019` block with the parser's own geometry (48 B head,
384 B rows, bit at row+4) gives ten stamped rows whose bits are
`{0x1, 0x2, 0x4, 0x8, 0x10, 0x20, 0x40, 0x80000, 0x100000, 0x200000}`, i.e. **bit
indices {0,1,2,3,4,5,6,19,20,21}** = GPC, XBAR, SYS, HUB, **M**, HOST, DISP, LEG,
PWR, MSD. That is the citable form of what the backend has been rendering
informally: bit 0 = GPC is the "core" domain and **bit 4 = M is the memory
domain**.

The rows are *not* ordered by their bits: in row order the ten bits are
`{0,1,4,2,3,20,19,6,21,5}`. That is what makes the measured `0xFF` window cover GPC
and M while leaving bits 21 and 5 — MSD and HOST — outside it, so "the first eight
rows" and "the first eight bit indices" are different sets, and the reason string's
row numbering is the one that matches the device.

The same index space appears in the `0x20809025` range block: its three records'
tags are `01 01 03 01`, `01 01 04 01`, `01 01 05 01` — third byte 3, 4, 5 — and each
carries `-18750 / +18750` at record `+20 / +24`, matching the documented
`min_offset`/`max_offset` ±18.75 MHz span. **Hypothesis, not confirmed:** those
three records are the per-domain windows for HUB(3), M(4) and HOST(5). Supporting
it: the values land inside the directory's own index space and the record count is
below the ten domains. Against it: all three records carry the same `0x190000` at
record `+8`, which is an odd thing for a per-domain quantity to do — so what that
dword means stays open, and the backend keeps reporting one agreed window rather
than three attributed ones until something other than the numbering supports it.

## 8. `0x20809019` 目录行：文档给出的字段名 ↔ RM 行里真实存在的字节（2026-10-18）

### 8.1 文档侧：这个块本来是**按域带范围**的
`nvapi-rs/src/clock.rs` L303-330 的 `RawConversion for NV_GPU_CLOCK_CLIENT_CLK_DOMAINS_INFO_ENTRY`
把这个 caller 结构的字段**逐个点名**：`disabled`、`clockType`（→ `ClockDomain`）、`rangeMax`、
`rangeMin`（→ kHz 的 `Range`）、`vfpIndexMin`、`vfpIndexMax`（→ VFP 索引 `Range`）、`unknown0`、
`unknown1`、`padding`；转换目标是 `ClockRange { domain, range, vfp_index }`（L285）。**即：公开层
认为"域目录"按域携带一段 kHz 范围与一段 VFP 索引范围**——这与我们此前"目录只有 stamp + domain bit"
的解析结论**不冲突**（RM 行 384 B ≠ 公开层行长，正如同族控制块 2108 B/64 B ≠ NVAPI 2404 B/72 B），
但它意味着：**行的其余字节不是填充，而我们此前只解析了两个字段。**

### 8.2 测量侧：行里确实有别的字段（`core/tests/data/macos-offset-directory.bin`，行 = `48+384r`）
逐行非零 dword（u16 以两字节读）：

| 行 | bit | +0 | +4 | +8 | +12 | +16 | +24 | +28 | +40 | +44 | +48 | +52 | +76 |
|----|-----|----|----|----|-----|-----|-----|-----|-----|-----|-----|-----|-----|
| 0 | 0x1 | 0x10808 | 1 | 7 | 1 | — | 1 | 0xb0b | 30000 | **−1000 / +1000** | 0x1000103 | 0xffff | 554 |
| 1 | 0x2 | 0x10909 | 2 | — | −1 | 0xff00 | 1 | 0x1717 | — | — | 0x1010204 | 0xffff | — |
| 2 | 0x10 | 0x808 | 0x10 | — | 4 | 0x100 | 1 | 0xa06 | — | **−1000 / +3000** | 0x1000602 | 0xffff | 256 |
| 3 | 0x4 | 0x10909 | 4 | — | −1 | 0xff00 | 1 | 0x1919 | — | — | 0x1020305 | 0xffff | — |

（+44 两列按 **int16** 读：`18 fc` = −1000、`e8 03` = 1000、`b8 0b` = 3000。）

**可确证的**：行里存在大量此前的解析从未读出的字段 ✔；其中 **`+44` 处一对有符号 16 位值**
只出现在 **bit 0x1（文档索引 0 = Gpc）与 bit 0x10（索引 4 = M）** 两行（本批 4 行里），
其余两行该处为零 ✔。另外每行都有 `+48`（形如 `03 01 00 01`，四个字节字段）与 `+52 = 0xffff` ✔。

**仍是假设、不得当结论的**：这对 `+44` 值是"该域的偏移窗口（kHz）"。**反向证据**：`0x20809025`
那三条记录（tag 3/4/5）给的是 ±18750，与此处的 −1000/+1000、−1000/+3000 **不一致** ⇒ 二者
要么**本就是不同量**（域窗口 vs 记录级跨度），要么其中之一读错了。**只有健康会话里再抓一次
（最好换 pstate 对比）才能判定**，故本轮**不动任何公开载荷**（不发布身份未证实的数字）。

### 8.3 顺带查证：这块**没有第二次抓取**
`outputs/stage-clkdom-20261017T091500Z/clkdom-read.txt` 属**尺寸扫描期**（全部 `0x1f`），
没有行数据；`outputs/` 里也再无 12336 B 目录块的转储 ⇒ 目前**唯一**的行字节记录就是上述 fixture。
因此给 probe 的 `--clkdom-read` 加了**逐行非零 dword 普查**（仅当 `size == 12336`，即该块文档
caller 尺寸），让下次会话自动把行尾字段记进记录，而不是再次丢失。

## 9. 写入命令的两套约定：`0x40000` 不是 SET 标记（已证伪），以及 RATED_TDP 的写入前置条件（2026-10-18）

### 9.1 被证伪的候选规则（如实记录，不留成"以为"）
猜想：方法表 flags 位 `0x40000` 标出 SET 行。**用 SDK 自己声明的 SET id 全集交叉验证后证伪**：
在 1118 行表里能找到的 SDK SET id 共 **103** 个，其中带 `0x40000` 的只有 **14** 个 ✔；而全部 103 个里
**没有一个**落在"id 位写半边"（`cmd & 0xC000 == 0xC000`）✔。反过来，`0x40000` 行（58 行）与 id 位
写半边（92 行）**交集为 0** ✔。⇒ 二者是**两套互不重叠的约定**：id 位写半边属于**私有/legacy 转发**
那一族（`0x2080D01C = 0x2080901B | 0x4000` ✔），而 SDK 公开的 SET（如 `0x2080206F` RATED_TDP_SET_CONTROL、
`0x2080205B` SET_POWERSTATE）**都带 `0x40000` 却不在写半边** ✔。**剩余问题**：`0x40000` 对那 58 行
究竟标记什么，**尚未定位**（不要当作"该行是 SET"使用）。

### 9.2 RATED_TDP：候选 id 已定、字段与前置条件均有文档
- **id 与尺寸**：`NV2080_CTRL_CMD_PERF_RATED_TDP_GET_CONTROL (0x2080206e)` ✔ 与
  `NV2080_CTRL_CMD_PERF_RATED_TDP_SET_CONTROL (0x2080206f)` ✔（`ctrl2080perf.h`）——**SDK 只声明这两个** ✔；
  两者都在方法表里，paramSize **12** ✔，与文档结构 `{client, input, vPstateType}` 的 sizeof 一致 ✔（表与头文件互相印证 ✔）。
  表里 flags：GET `0x10008`、SET `0x40048` ✔。
- **字段语义（文档）**：`client ∈ {RM=0, WAR_BUG_1785342=1, GLOBAL=2, OS=3, PROFILE=4}` ✔、
  `input/action ∈ {DEFAULT=0, FORCE_EXCEED=1, FORCE_LIMIT=2, FORCE_LOCK=3, FORCE_FLOOR=4}` ✔、`vPstateType` ✔；
  合法状态只有 `NV_OK` / `NV_ERR_NOT_SUPPORTED` ✔。
- **写入前置条件（文档原文要点）**：`SET_CONTROL` **无法设置 RM 客户端** ✔；RM 在客户端之间**仲裁**，
  并可能**回落到 `FORCE_EXCEED`** ✔。⇒ **RATED_TDP 是"仲裁请求"而不是瓦数** ✔：写入成功也可能
  *看不到* 上限变化 ⇒ 这为"不得以实时频率变化代替写入生效"提供了文档依据 ✔，**不能取代真正的偏移/VF 控制** ✔。

### 9.3 顺带核到的既有做法（未改）
probe 里派生的 `0x2080206d`（RATED_TDP **status**）：SDK 未导出 CMD 宏，只给了 message id
`NV2080_CTRL_PERF_RATED_TDP_STATUS_PARAMS_MESSAGE_ID = 0x6D` ✔，整份 SDK 里**没有** `0x2080206d` 字面量 ✔，
1118 行方法表里也**没有**该行 ✔ ⇒ 该 id 是**由 message id 约定派生**、并在源码注释中如实标注为派生 ✔
（未改动，仅记录）；因此它能否被主机表放行，由驱动而非表决定 ✔。

## 10. 写入路径核对：门控写入用的 SET 行存在且尺寸与 GET 相同；`|0x4000+1` 约定**不通用**（2026-10-18）

### 10.1 门控写入将要使用的 SET id（这是本轮最要紧的一条）
对 `0x2080901b`（GET，paramSize **2108**，flags `0x48`）查其 `|0x4000+1` 孪生：
`0x2080d01c` —— **在方法表里** ✔，paramSize **2108**（与 GET **完全相同**）✔，flags `0x44` ✔。
⇒ 计划中的"单域最小步长"写入**不会因为 payload 尺寸与 GET 不一致而失败** ✔（两侧同为 2108 ✔），
且 `0x2080d01c` 的 id 位（`0xd01c & 0xC000 = 0xC000`）确实落在 id 位写半边 ✔，与交接文档的写半边结论一致 ✔。
（GET 与 SET 的 flags 只差 `0x08`/`0x04` 一位 ✔——这一点含义**尚未定位**，仅记录。）

### 10.2 `|0x4000+1` 孪生约定**只在私有族成立、且不是普遍存在**（纠错）
四个私有 id 逐一核对孪生是否在表中：
| GET | paramSize | `|0x4000+1` 孪生 | 在表中？ |
|---|---|---|---|
| `0x2080901b` | 2108 | `0x2080d01c` | **在** ✔ paramSize 2108 |
| `0x208090a2` | 524 | `0x2080d0a3` | **在** ✔ paramSize 524 |
| `0x20809019`（目录） | 12336 | `0x2080d01a` | **不在** ✗ |
| `0x20809025`（范围块） | 7188 | `0x2080d026` | **不在** ✗ |
另外 `0x2080206e`（公开 GET）的 `|0x4000+1` = `0x2080606f` **不在**表中 ✗（它的 SET 是相邻的公开 `0x2080206f` ✔，见 §9.2）。
⇒ **不要假设任何私有 GET 都有表内 SET 孪生** ✔；要用时先在表里确认 ✔。（与既有实测"该约定仅在 47 处出现"一致 ✔。）

### 10.3 两处易混的 id（记录以免张冠李戴）
- NVAPI 副本引用的 `SetControl` RM id **`0x2080D024`** 在表中存在，paramSize **66060** ✔——它是
  **`0x20809024` 的孪生**，**不是**我们控制块的 `0x2080901b`/`0x2080d01c` ✔；尺寸也相去甚远 ⇒ 是**另一条命令** ✔。
- 若干曾被我记为 rated-TDP 相关的 id **都不在方法表**：`0x2080f031` ✗、`0x20801acf` ✗、`0x2080206d` ✗
  ⇒ 任何调用它们的路径依赖的是**派生**，不是表 ✔（`0x2080206d` 的派生在源码注释里已如实标注 ✔，见 §9.3）。

### 10.4 电压增强族（最后一个未审计的读族）：客户端侧是"volt rails control"，且不得与另三样混为一谈
客户端文档：`core_voltage_boost()` / `set_core_voltage_boost(Percentage)` ✔ 走
`NvAPI_GPU_ClientVoltRailsGetControl` / `...SetControl` ✔，载荷是 `power::undocumented::NV_GPU_CLIENT_VOLT_RAILS_CONTROL { percent, .. }` ✔
⇒ 所谓 boost 在这个族里是**一个百分比** ✔。**必须与下列三者区分**：`BaseBoostMode{Base=1, Boost=2}`（base/boost **频率表** ✔，见 `gpu.rs:946`）、
`set_dynamic_boost(bool)`（PPAB / Dynamic-Boost **使能** ✔，见 `gpu.rs:4268`）、以及 `NV2080_CTRL_CMD_PERF_BOOST`
（**硬件 boost 动作**，probe stage 3 已在发 `CMD_CLEAR`/`CMD_BOOST_1LEVEL`/`CMD_BOOST_TO_MAX` ✔）。
本族我们**实际发出过**的读是 `NV2080_CTRL_CMD_INTERNAL_PERF_GPU_BOOST_SYNC_GET_INFO (0x20800a80)`（16 B），
**实测返回 `0x56` = NOT_SUPPORTED** ✔ ⇒ 分类为**驱动明确拒绝** ✔（不是"调查未知"、更不是"命令不存在" ✔）。
承载 volt-rails 控制的 **RM id 尚未定位** ✔（该副本给的是 NVAPI 侧接口名与 escape 家族，未给 RM id）。

## 11. escape ↔ 端点映射（含一处对**我自己**亲近推断的纠正）与温度策略链的客户端定位（2026-10-18）

### 11.1 温度"策略/上限"链已在客户端定位到接口级（缺的只剩 escape/RM 端点）
在副本里查到该族的**真实调用点**（`gpu.rs:5676` 起）：
`NvAPI_GPU_ClientThermalPoliciesGetInfo` ✔（`NV_GPU_CLIENT_THERMAL_POLICIES_INFO` —— 策略集）、
`NvAPI_GPU_ClientThermalPoliciesGetStatus` ✔（`..._POLICIES_STATUS` —— 当前上限状态，即 §26 记录的 `ThermalLimit`）、
`NvAPI_GPU_ClientThermalPoliciesSetStatus` ✔（写入面），以及私有的
`NvAPI_GPU_ClientThermalPoliciesPrivateGetInfo` ✔（`..._PRIVATE_INFO`）。
私有那条的注释记录了**策略索引的权威发现规则**：*"GPS index if the VBIOS exposes one, else the acoustics index
(desktop fallback = NVML AcousticCurr), else None. ... replaces hardcoding idx 2. **RE'd from the ref-tool CLI
`GPUHandle::queryTargetTemperature` (sub_14002C410)**"* ✔。
⇒ 该族**缺的只有"这些 NVAPI 入口背后的 escape / RM 端点"** ✔，**尚未定位** ✔；定位方法与我们当年做 cooler
那条（`0x07000038 → cmd 0x20800532`）**同法**：测量 escape 号 → 与 RM 方法表 id 对齐 ✔。

### 11.2 escape 映射表（把已"命名/实测"的与"未命名"的分开）
| escape | 归属 | 依据 |
|---|---|---|
| `0x07000002` | GetAllClockFrequencies | 注释点名 ✔ |
| `0x0700004A` | VFP 曲线 | **在 391/Kepler 上内核未实现** ✔ |
| `0x0700004F` | —— | **在 `0x0700004A` 不可用的地方成功**，注释称其为"通往 VFP 曲线的可行路径" ✔ |
| `0x07000049` | 私有命令/SetControl | `SetControl (RM 0x2080D024, ID 0xFEC00D04)`，标注 **DANGEROUS V/F** ✔ |
| `0x0700004E` | **GetBarInfo（PCI BAR 拓扑）** | 未加戳、只读、无能力门、无需提权 ✔ |
| `0x07000053` / `0x0700006D` | volt / 其它只读面 | `0x07000053`（`fn(hGpu,width)`）、`0x0700006D`（0x3BD32008）✔ |
| `0x07000038` | cooler | 与 `cmd 0x20800532` 对应（本项目实测 ✔） |
**纠正**：我曾据"身处 volt 段落"推断 `0x0700004E` 属于电压族 ✗——实际读注释后确认它属于 **`GetBarInfo`** ✔。
（教训与 goal 要求一致：近邻推断必须读原文确认 ✔。）

### 11.3 电压族：**范围 / 原状态**证据已存在于副本，且是**在本机同型号卡上实测**的
`VoltVoltDevicesGetInfo`（NDA `0xA38ACF9D`，stamp `0x10F48`）的注释写：它是 `volt_rails` 的**兄弟面**
（"same escape family, different RM endpoint"），`VoltRails` 报**各轨实时墙**，而它报每个域的**可控窗口**
（min/step/max/default µV），并直接给出**实测**：*"R465 live on the **1650 SUPER**: 300 mV / 6.25 mV /
1300 mV / 1000 mV, matching the rail clamp floor"* ✔。
⇒ goal 要求的"范围 / 原状态"证据**在同型号卡上已经有过一次** ✔（**R465** 时）；但本机驱动是 **R610** ⇒
**必须重测、不得直接套用** ✔。该条已写入电压增强族的**用户可见成因串** ✔。
