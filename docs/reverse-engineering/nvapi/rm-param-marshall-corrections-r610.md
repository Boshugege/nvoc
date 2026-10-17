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

## 12. 风扇/散热族的客户端分发链：分成"cooler 面"与"**arbiter（仲裁）面**"，且我们可用的私有行**没有恢复孪生**（2026-10-18）

### 12.1 副本里风扇族的真实接口清单（这是"实际分发链"，不是头文件字面量）
**读**：`NvAPI_GPU_GetTachReading`（转速/RPM 表 ✔）、`NvAPI_GPU_GetCoolerSettings` ✔、`NvAPI_GPU_GetCoolerPolicyTable` ✔、
`NvAPI_GPU_ClientFanArbitersGetInfo` ✔、`NvAPI_GPU_ClientFanArbitersGetStatus` ✔。
**写**：`NvAPI_GPU_SetCoolerLevels` ✔、`NvAPI_GPU_SetCoolerPolicyTable` ✔、
**`NvAPI_GPU_RestoreCoolerSettings`** ✔、**`NvAPI_GPU_RestoreCoolerPolicyTable`** ✔、`NvAPI_GPU_ClientFanArbiterControl`（见下）✔。
⇒ 客户端**明确提供两个 restore 入口** ✔（goal 要求的"恢复"在客户端侧是有名分的 ✔）。注：副本里**没有** `src/cooler.rs` ✔（cooler 面挂在 `gpu.rs` 与 `sys/gpu/cooler.rs`）。

### 12.2 arbiter 三件套的精确布局（"取控制权"是一套**请求/仲裁**机制）
`NV_GPU_CLIENT_FAN_ARBITER_INFO_V1 { unknown, flags{FAN_STOP=1, FAN_STOP_DEFAULT=2}, arbiter_index, padding[40 B] }` ✔、
`NV_GPU_CLIENT_FAN_ARBITERS_INFO_V1` = 36 B 头 + 32×40 = **1316** ✔；
`NV_GPU_CLIENT_FAN_ARBITER_STATUS_V1 { unknown0, unknown1 }`（`fan_stop_active()` ⇔ `unknown1 != 0`）✔、
`..._ARBITERS_STATUS_V1` = 36 + 32×8 = **292** ✔；
`NV_GPU_CLIENT_FAN_ARBITER_CONTROL_V1 { arbiter_index, flags{FAN_STOP=1} }` ✔。
⇒ **客户端侧的风扇"控制"主体是"谁可以让风扇停"的仲裁请求**（与 RATED_TDP、热策略同形 ✔），
**不是**直接的占空比/目标转速设定 ✗。⇒ **读到 policy 表 ≠ 持有控制权** ✔（与 §9.2 的 RATED_TDP 教训同构，勿混）。

### 12.3 本族**缺口**：我们可用的私有行没有"恢复"孪生
我们测得的候选是私有 `0x20808532@336`（在表 ✔）与其公开孪生 `0x20800532`（**不在表** ✗，见 §8/上轮记录）。
核对"`|0x4000+1` 孪生"约定（§10.2）后：该行**没有**表内恢复/写孪生 ✗；而 arbiter 三件套的尺寸（40/1316/8/292）
与 336 **都不吻合** ⇒ 该私有行属于 **cooler/policy 面** ✔，但它**精确对应哪个客户端结构尚未定位** ✗。
⇒ 因此**风扇的恢复计划只能定为"先成功读到原值，再写回原值"** ✔——**不得假设存在一条恢复命令** ✔（这与客户端
确有两个 `Restore*` 入口并不矛盾：那是 NVAPI 面，我们的 RM 私有行没有对应孪生 ✔）。

## 13. 复制资料的封送规则正对本命令：**值 dword 只对固定的记录类型集合拷贝**；并给出一个可判定的槽位预测（2026-10-18）

### 13.1 副本对 `0x2080901b` 的记载（与本项目读到的同一条命令）
`nvapi-rs/src/clock.rs`（`ClkDomainControlEntry`）：*"One controllable clock-domain entry from the private
ClockClient **GetControl (RM 0x2080901b, ID 0xF58938F5)**"* ✔；`bit` = 可控掩码位 = 域索引 = MEASURE_FREQ 的
`domain_index` ✔；文章里的 XBAR 域 = `ClockDomainId::Xbar`（掩码位 `0x2`）✔。
**最要紧的一条**：`value_modifiable` —— *"derived purely from the record **TYPE byte**, NOT a driver capability bit …
the driver-side remap (`sub_18015BB30/BD20`) maps protocol ↔ internal types and the per-record switch **only copies
value dwords for a fixed internal-type set**（V2 magic `0x261A4` → protocol `{1,3..=10,15}`；V1 `0x10964` → `{1,3..=9}`）。
Type `0x02`（若干代际的 Disp/Host、A100 的 M）**两个版本都不封送** —— SetControl 会**静默丢弃** —— 但这些时钟
仍可能**可读**（MEASURE_FREQ/GetAllClocks）… **Readability and writability are independent per domain.**"* ✔
⇒ 这就是"**值可能被静默丢弃**"的机制性依据 ✔（与本项目既有的"不得假设 type-03 记录接受写入"一致 ✔）。

### 13.2 由该规则推出的**可判定预测**（供健康会话第一读直接判读）
副本还写了两个版本读值 dword 的窗口：V2 从 `rec+268..296`（8 dwords）✔，**V1 回退从 `rec+44..60` 填 slot 0..4** ✔。
本项目在 macOS 上**实测**的是十二个值 dword，位于记录相对偏移 `{8,12,16,20,24,28,32,36,40,44,52,56}` ✔。
⇒ 若副本的 `rec` 与我们这个 64 B 记录同基，则 **V1 窗口 `44..60` 恰好覆盖我们那十二个里的最后四个 `{44,52,56}`（及 48）** ✔，
而 **V2 的 `268..296` 完全落在 64 B 之外** ✗。由此得到**一条可在首次健康读取时立刻判定的预测**：
**非零值只出现在末四个槽 ⇒ 主机走的是 V1 值窗口；前段槽位出现非零 ⇒ 另有布局** ✔。
（注意前提：副本的 `rec` 是 NVAPI **调用方**记录（72 B），与我们的 64 B RM 记录**未必同基** ⇒ 该预测是**待验证**的，不作为结论 ✔。）

### 13.3 一处**张力**（如实记录，不强行调和）
副本的 `v1_marshalable` **包含类型 3** ✔，但本项目实测注释记载"**type 03 记录只写自己的 stamp、不写值 dword**" ✔
（`probe-macos-rm/src/nvx_suite.c` 实测注释）⇒ 二者要么说的不是同一层的"type"，要么拷贝规则在另一层生效 ✗ ——
**尚未定位** ✔。但结论方向一致：**收到 stamp 变化 ≠ 值被接受** ✔。

### 13.4 写入门控的现状核对（goal 第 4/5 条）
`settings_write` / `settings_restore` 在 macOS 后端定义 ✔、导出到 Python（`nvoc-python/src/lib.rs:1497-1499`，
含等价的 `performance_writes` ✔）、并有测试断言 ✔（全树 19 处命中）。但**`cli/`、`auto-optimizer/`、`srv/` 里
没有任何分支读它们** ✔ ⇒ 目前它是**被如实上报的能力标志**，**强制执行点尚未接线** ✔ ——这与"自动优化（阶段 10）
尚未接到 macOS 后端"一致 ✔；**接线时就该在这里短路性能写入** ✔（本轮不提前改代码，遵循 goal 的
"未完成代码不必为切换方向仓促提交" ✔）。

## 14. `0x20809019` 的 **RM 侧 12336 B 目录**：两处权威资料都没有声明；实测布局与"文档化的 NVAPI 版"**结构性不同**（2026-10-18）

### 14.1 核对结果（两处都查了，不是"搜不到"）
- **副本**：只声明 NVAPI 侧 `NV_GPU_CLOCK_CLIENT_CLK_DOMAINS_INFO_ENTRY`（`sys/src/gpu/clock.rs:437-447`）：
  `+0 disabled u32`/`+4 clockType`/`+8 unknown0[8]`/`+40 rangeMax i32`/`+44 rangeMin i32`/`+48 vfpIndexMin u8`/`+49 vfpIndexMax u8`/`+50 pad[2]`/`+52 unknown1[5]`
  ⇒ 条目 **72 B**，块 `V1 = 2344`（= 40 B 头 + 32×72 ✔）；且该 API 注明 **"Pascal only"** ✔。
- **OGKM R610.57.04 参考树**：`grep -rln "CLK_DOMAINS_INFO\|CLK_DOMAIN_INFO\|ClkDomainsInfo" --include="*.h"` ⇒ **0 个文件命中** ✔
  （`_CLK_DOMAIN_INFO` 结构声明亦 0 命中 ✔）⇒ **RM 侧那条 12336 B 指令的载荷在参考头文件里没有声明** ✔。
⇒ 结论：我们手上这条 `0x20809019@12336`（48 + 32×384 ✔）**没有任何公开结构可套** ✔；它的字段名只能来自实测/回调链或 NVRM 封送逻辑 ✔。
⇒ **不得**把 72 B NVAPI 版的字段名硬套到 384 B 行上（下节的实测就是反例）✔。

### 14.2 实测布局（从仓库内夹具 `core/tests/data/macos-offset-directory.bin` **重新推导**，12336 = 48 + 32×384 ✔）
按 `+4`（**掩码位/域索引**：1、2、4、16 ✔）标行，**不要按行号标**（本轮先把行号当成了位，已纠正 ✔）：
- bit `0x1`：`+0=0x00010808` `+4=1` `+40=30000` `+44=int16{−1000,+1000}` `+48=0x01000103`(字节 `[3,1,0,1]`) `+52=0xffff` `+76=554`
- bit `0x2`：`+0=0x00010909` `+4=2` `+48=0x01010204`(`[4,2,1,1]`) `+52=0xffff`，`+40/+44/+76` 全零
- bit `0x10`：`+0=0x00000808` `+4=16` `+44=int16{−1000,+3000}` `+48=0x01000602`(`[2,6,0,1]`) `+52=0xffff` `+76=256`
- bit `0x4`：`+0=0x00010909` `+4=4` `+48=0x01020305`(`[5,3,2,1]`) `+52=0xffff`
**与文档化版本的差别是结构性的**：文档版 `+4` 是 `clockType`，而我们这四行的 `+4` **恰是掩码位本身**（1/2/4/16 ✔）⇒ **布局不同已被证实** ✔（也与副本"bit = 掩码位 = 域索引"的说法一致 ✔）。

### 14.3 有价值的正面线索（**未证明**，不许当结论发布）
- **只有 `0x1` 与 `0x10` 两行带 int16 增量窗口** ✔；按副本的位↔域表，`0x1`=Gpc(0)、`0x10`=M(4) ✔ ⇒ **恰好是核心与显存**——正是本轮主攻通道所在的两个域 ✔。窗口为 `{−1000,+1000}`（Gpc）与 `{−1000,+3000}`（M）✔，**单位尚未定位** ✗（若为 kHz 则仅 ±1 MHz、偏小；若为 100 kHz 则 ±100 MHz、较合理；两者都不足以定论 ✔）。
- `+48` 的字节形态（前两字节成对 + 尾字节非零）与文档版 `vfpIndexMin/Max` 的 `u8,u8` **形态相似** ✔，但模板里的 `padding[2]==0` 在我们这里是 `{0,1}`/`{1,1}` ✗ ⇒ **线索，不是证明** ✔。
- 四行 `+52` 恒为 `0xffff` ⇒ 疑似"无上限"哨兵 ✗（尚未定位 ✔）；`+76=554/256` 含义 ✗（尚未定位 ✔）。

**验证范围**：本轮**无代码改动**；依据为副本源码、OGKM 参考树 grep（0 命中）与仓库内夹具的重新推导 ✔。**新增可执行动作**：按**掩码位**而非行号记录目录行 ✔。

## 15. 私有时钟域通道**只存在于 GSP 方法表**：三份产物交叉核对（2026-10-18）

### 15.1 核对结果（逐文件、带行号，全部本轮实测）
对六个私有 id（`0x20809019`、`0x2080901b`、`0x20809025`、`0x20808532`、`0x2080d01c`、`0x2080d024`）
做**大小写不敏感**的全文件搜索：
- `nvoc-analysis/tables/gsp-1118.tsv`：**6/6 命中** ✔ —— 行 712 `0x20808532 336 0x48`、行 747 `0x20809019 12336 0x48`、
  行 749 `0x2080901b 2108 0x48`、行 757 `0x20809025 7188 0x48`、行 1040 `0x2080d01c 2108 0x44`、行 1042 `0x2080d024 66060 0x44` ✔
  （即此前记录的尺寸/权限位本轮**已逐条复核** ✔）。
- `nvoc-analysis/tables/host-608.tsv`：**0 命中** ✗。
- 测试机 `outputs/method-table-nvrm-610.57.04.tsv`（605 行）：**0 命中** ✗，且该表 `0x20809*`/`0x2080d*` 前缀计数为 **0** ✗（`0x20808*` 也仅 3 行 ✔）。
⇒ **结论：这六个 id 只出现在 GSP 表里**，即**私有时钟域通道走 GSP/legacy 转发一侧** ✔，主机（NVAPI 侧）分发表与 NVRM-605 表都不认识它们 ✗。
⇒ 这同时从**第三个独立产物**上支撑了上一轮的判断：**该载荷没有公开结构可套**（副本无、OGKM 头文件无、两张表都不含 ✗）。

### 15.2 一处方法学更正（避免把"表里没有"当成"没有名字"）
`method-table-nvrm-610.57.04.tsv` **确有 `name` 列**（列头为 `methodId paramSize classPtr offset name` ✔），
但其名字基本为空，全表仅两条与时钟相关 ✔：`0x20800a93 subdeviceCtrlCmdInternalNvlinkGetLinkAndClockInfo`、
`0x20801037 subdeviceCtrlCmdClkPmumonClkDomainsGetSamples` ✔。
⇒ 命名风格是 `subdeviceCtrlCmd…` ✔，但**我们的私有 id 不在该表内** ⇒ **不得据该表说"没有名字"**，只能说"该表不含它们" ✔。
另：本轮 `ls -l …/class/ctrl2080clk.h` 返回非零（该快照在该路径下无此文件 ✔），与上一轮**全树 grep 0 命中**一致 ✔（后者才是主要证据 ✔）。

### 15.3 对主攻通道的意义
- 私有偏移通道的可达路径被限定为 **GSP 一侧** ✔ —— 与探针实际使用的私有 RM 控制路径一致 ✔。
- 因此"**读得到**"这件事本身已经由 GSP 表 + 实测 `NV_OK` 支撑 ✔；仍缺的只是**值是否被封送**（见 §13 的按记录普查 ✔）。
- 未决项不变：记录类型字节、值窗口归属、目录 `+44`/`+76` 的单位与含义 ✔。

## 16. 私有家族在方法表里的**形状**：同尺寸配对与 `+0x4001` 孪生规则的交叉验证（2026-10-18）

### 16.1 方法与其**唯一**验证点
两个可判定手段：**(a) 同 `paramSize` 配对**；**(b) 已测得的 `+0x4001` 孪生关系**（`0x2080901b`→`0x2080d01c` ✔）。
在 1118 行 GSP 表上，唯一可用来验证的**实测对**是：`paramSize == 2108` ⇒ 表里**恰好两行**——
`0x2080901b`（flags `0x48`）与 `0x2080d01c`（flags `0x44`）✔ ⇒ **两个手段在已知对上同时成立** ✔（这使下面的否定结果有意义 ✔）。

### 16.2 否定结果（逐条，本机可复算）
- 目录 `0x20809019`（`paramSize 12336`）：**表里只有这一行** ✔；其 `+0x4001` 孪生 `0x2080d01a` **不存在** ✗。
- 逐记录范围 `0x20809025`（`7188`）：**只有这一行** ✔；孪生 `0x2080d026` **不存在** ✗。
- 风扇私有 `0x20808532`（`336`）：**只有这一行** ✔；孪生 `0x2080c533` **不存在** ✗。
- `0x2080d024`（`66060`，flags `0x44` ✔）：孪生 `0x2080d025` **不存在** ✗。
⇒ **整个私有簇里，方法表只声明了"一对"读写**（就是我们的偏移对 `0x2080901b`/`0x2080d01c`）✔ —— 同时说明：`0x20809019`/`0x20809025`/`0x20808532` 在表层面是**只读行** ✔。

### 16.3 公开风扇 id 在**两张表里都不存在**（与实测一致）
`0x20800532`（及其近邻 `0x20800533`/`0x2080d533`/`0x2080c533`）在 **GSP 表与主机表都 0 命中** ✗。
⇒ 与实测"公开风扇控制返回 `NV_ERR_NOT_SUPPORTED`"一致 ✔（"命令未被表声明"是**可服务性**问题，不是权限问题 ✔）。
私有行 `0x20808532` 虽在表内，但**无写孪生** ⇒ **风扇的写命令在方法表层面未被声明** ✔；至于"0x48 行是否仍接受写入"，
**表不能回答，只能实测** ✗（不越权下结论 ✔）。

### 16.4 方法边界（如实记录，防止把启发式当规则）
- `flags & 0x04` 与写半区的相关性：写半区 92 行中 **82 行**带 `0x04` ✔，**10 行带 `0x48`** ✗（如 `0x2080c54d`、`0x2080d0b3`、`0x2080e0cf`），故**只是启发式** ✗。
- `flags == 0x44` 共 **121 行** ✗（并非写半区专用 ✔）；另有 1 行 flags 为复合值 `0x5a14438`（`0x2080f310`）✗ ⇒ 该列不宜当作纯位域解读 ✔。
- 结构性判据仍是**既有 id 位规则**（`(id & 0xC000) == 0xC000` ⇒ 92 行 ✔）。

### 16.5 主机表覆盖面（补充事实）
`host-608.tsv` 的 id 前缀分布：`0x20800*` 299、`0x20801*` 144、`0x20802*` 56、`0x20803*` 88、`0x20804*` 17、`0x20808*` 3、`0x2080a*` 1
⇒ **完全不含 `0x20809*` 与 `0x2080d*`** ✗ ⇒ 主机侧分发在私有区间之前就结束了 ✔（与上一轮"私有通道只在 GSP 一侧"一致 ✔）。

## 17. 目录**按条目标引**而不是按域位排序：两份独立捕获互相印证（2026-10-18）

### 17.1 两份捕获**完全一致**（此前担心的分歧，是排序假设造成的错觉）
- 探针源码记录的**实测**列表（掩码 `0xFF` ⇒ 条目 0..7 类型）为 `[08,09,08,09,09,09,03,08]` ✔。
- 仓库夹具 `core/tests/data/macos-offset-directory.bin` **按行序**解出的类型（`+0` 的 u16 高字节）为
  `08,09,08,09,09,09,03,08,09,09`（10 个已填充条目 ✔，其 `+4` 域位依次为
  `0x1,0x2,0x10,0x4,0x8,0x100000,0x80000,0x40,0x200000,0x20` ✗ **并非升序**）。
⇒ 前八个**逐项吻合** ✔ —— 分歧消失的原因是：**掩码位选的是"条目索引"，不是"域位"** ✔。
⇒ 由此**独立复算**出探针的写集算术：前八条里 08/09 共 **7** 条（08 三条、09 四条）、03 一条 ⇒ `14 + 7*13 + 1 = 106` ✔，
与实测"106/527 dwords 改变"**一致** ✔。

### 17.2 代码侧无缺陷（核对了，不是假设）
`core/src/macos_offset.rs` 已把这件事写在注释里（L68-73：把掩码当域位读会选到 7 个带值条目并预测 105 ✗），
并把 `index` 与 `domain_bit` 分开解析（L208-214 ✔），`domain_bit` "原样上报"（未映射到公开域 ✔）⇒ **无需改动** ✔。
另：`rec+0` 的 u16 印章与目录条目的类型一致（实测掩码 `0x2` ⇒ 记录 1、印章 `0x00000909`＝类型 09 ✔，与夹具条目 1 的类型 09 一致 ✔）。

### 17.3 给下一次健康会话的**可证伪预测**（由现有数据推导，无需新访问）
夹具给出**全部 10 个已填充条目**的类型（`08,09,08,09,09,09,03,08,09,09` ⇒ 带值的 08/09 **9** 条、03 **1** 条）
⇒ 掩码 `0xFFFFFFFF` 时预测改变 dword 数为：
- 若 22 个**空**槽位不写任何 dword ⇒ `14 + 9*13 + 1 = **132**` ✔；
- 若每个空槽位只写自己的印章（1 dword）⇒ `132 + 22 = **154**` ✔。
**两分支的取值就是判据**：实得 132 ⇒ 空槽不写；实得 154 ⇒ 空槽只写印章；两者都不是 ⇒ 该算术模型本身需要修正 ✔。
（对照：实测掩码 `0xFF` ⇒ 106 ✔ 已由本节独立复算 ✔。）

## 18. 偏移**范围块**（`0x20809025`）的逐字节实况：三条记录、头 `+4` 是条目掩码、单位口径与目录 `+44` 不一致（2026-10-18）

数据源＝仓库夹具 `core/tests/data/macos-offset-range.bin`（7188 B ✔，与 `core/src/macos_offset.rs` 的解析常量一致：头 20 B＋32×224 B ✔）。

### 18.1 实测字节（不是推测）
- **头 20 B**：`+0=0x000000ff`、`+4=0x00000007`、`+8=0`、`+12=0x64 (100)`、`+16=0x100 (256)` ✔。
- **填充的记录恰为 3 条**，条目 **0、1、2** ✔，其余 29 条全零 ✔：
  | 条目 | `+0` tag | `+4` | `+8` | `+20` | `+24` | `+28` |
  |---|---|---|---|---|---|---|
  | 0 | `0x01030101` | 1 | 1638400 | −18750 | +18750 | `0xff0100` |
  | 1 | `0x01040101` | 1 | 1638400 | −18750 | +18750 | `0xff0100` |
  | 2 | `0x01050101` | 1 | 1638400 | −18750 | +18750 | `0xff0100` |
  `+0` 只在**第三个字节**递增（03/04/05），其余字节恒为 `0x0101` ✗ **含义尚未定位** ✔。
  三条的 `min/max` 与 `+8` **完全相同** ✗ ⇒ 这三个值不是"每域各异"的限值，更像**同一口径的全局上界**（`+8`＝1638400 kHz＝1638.4 MHz ✔ 与副本 `max_vf_khz` 的命名一致 ✔）。

### 18.2 头 `+4 = 0x7` 是**条目掩码**（推断，依据写在下面）
`0x7` 的 3 个置位 **恰好**对应 3 条被填充的记录 ✔，且该字段位置与**控制块**的输入掩码（`+4`）一致 ✔ ⇒ 推断：**范围块的头 `+4` 也是"选中哪些条目"的掩码** ✔。
⇒ **这可由下一次健康读取直接证伪** ✔：头掩码与填充条目数必须始终一致；只要出现"掩码位 ≠ 被填充条目"即该推断作废 ✗。
`+0=0xff` 与目录块/控制块的头 `+0` 相同 ⇒ 是这三个块共同的**常量族标记** ✔（控制块的 `+0` 实测被写入 u8 `0xFF` ✔）。

### 18.3 跨块对齐：范围记录用的是**条目索引**，与目录同序
范围块填充的条目是 **0、1、2**（索引序 ✔），与控制块"掩码位选条目索引"的约定一致 ✔ ⇒ 范围块也是**按条目索引**寻址 ✔。
⇒ **"10 个域里只有 3 个带范围"是实测事实** ✔；但这 3 个条目对应哪些**公开域**仍**尚未定位** ✔（需要位↔域映射，而映射只能由写/回读实验确立 ✔）。

### 18.4 单位口径**不一致**，如实记录
- 范围块给出 `±18750`（按副本命名即 kHz，±18.75 MHz ✔）——三条相同 ✔。
- 目录块里只有条目 0（位 `0x1`）与条目 2（位 `0x10`）在 `+44` 带 int16 对（`{−1000,+1000}` / `{−1000,+3000}` ✔）。
⇒ 两者**不是同一个量**（同一行上 18750 与 1000/3000 并存 ✗）⇒ **哪一个才是"可设置窗口"尚未定位** ✔，只能由写/回读实验判定 ✔。
⇒ 对写实验的直接指导：**优先选范围块声明过的条目 0**，并同时观察目录 `+44` 是否随写变化 ✔ —— 两者若同时变化，则它们是同一量的不同编码 ✗；若只有其一变化，即定位到窗口字段 ✔。

### 18.5 代码侧
`core/src/macos_offset.rs` 已按 `+0` tag、`+8`、`+20`、`+24` 解析并只保留非零记录 ✔ ⇒ 与本实况一致，**无需改动** ✔；但副本未记录头的 `+4` 掩码语义 ✗ ⇒ 见本节（推断＋证伪条件 ✔）。

## 19. 温度族的层级定位：驱动只声明**一个**温度命令；macOS 客户端侧**没有**策略面（2026-10-18）

### 19.1 RM/驱动层：只有一个温度命令，且**没有**限额/策略操作码
测试机上 NVRM 610.57.04 的**带名字**命令表（`outputs/method-table-nvrm-610.57.04.tsv`，605 行，列名含真实 RM 命令名）里，温度相关**只有一行** ✔：
```
0x20800513  1432  subdeviceCtrlCmdThermalSystemExecuteV2
```
`0x2080 06xx` 段其余全是 I²C（`I2cReadBuffer`/`WriteBuffer`/`ReadReg`/`WriteReg`，276–532 B）✗ ⇒ **驱动自己的声明表里不存在"温度上限/策略"操作码** ✔。
方法表侧（`nvoc-analysis/tables/gsp-1118.tsv`）：`0x20800513 @1432 flags 0x48` ✔ 与**非特权孪生 `0x20808513 @1432 flags 0x10248`** ✔（后者是 **GSP 表独有** ✗，见 19.2）。

### 19.2 修正一条**过度解读**：host-608 表是"公开面清单"，不是穷尽表
本机 `host-608.tsv` 里有 `0x20800513` ✗ 却**没有** `0x20808513` ✗ —— 也就是说这张表**漏掉的恰是非特权孪生** ✔。
⇒ 由此得到一个独立例证（此前只有时钟域私有族这一个例子 ✔）：**该表里"查不到"不能当作"不存在"** ✔ —— 它是公开面清单 ✗。这正是 goal 要求的"不能用'头文件搜不到/二进制无字面量'代替核对"在**表层面**的对应结论 ✔。

### 19.3 macOS 客户端侧：**没有任何文件提到 thermal**
测试机 `reference/nvidia-macos-driver`（即 `gpu-port` 移植检出，含 `app/`、`kexts/`、`gpu-port/`）全目录 `grep -rli thermal` 只命中 **1 个文件**：`gpu-port/xkpi/bsdcompat/include/sys/systm.h` ✗（BSD 兼容层头文件，与 GPU 温度策略无关 ✗）。
⇒ **macOS 侧没有实现 NVAPI 的 thermal-policy 面** ✔（`nvapi-rs` 里该面有 240 处实现 ✔，但那是跨平台 NVAPI 层，macOS 无 NVAPI ✗）。
**范围声明**：这是对**移植检出源码**的核对 ✔，不等于已加载 kext 的全部符号 ✗（如实标注 ✔）。

### 19.4 结论与下一步（不写"只能固件逆向"）
- **可读路径的候选**：`0x20808513 @1432`（非特权孪生 ✔，**从未发送过** ✗）⇒ 列入下次健康会话的**只读**候选 ✔，用于判定 `ThermalSystemExecuteV2` 是否携带温度/限额信息 ✔。
- **策略语义（限额、跳变点）**：位于 NVAPI 客户端抽象 ✗，macOS 侧缺失 ⇒ 属**驱动实现/客户端层缺口** ✔，**不是**"命令不存在"、**不是**固件限制 ✔。
- 因此温度族本轮状态＝**调查未知（已定位到层）** ✔ —— 与风扇/功耗族分开记录，不互相牵连 ✔。

## 20. 目录记录的 `range` 字段：以仓库内 IDA 实测偏移为准，纠正我先前"`+40/+44` 即窗口"的推断（2026-10-18）

**触发**：核对仓库**已有封送资料**（而非再扫尺寸/ID）时，`nvapi-rs/sys/src/gpu/clock.rs:423-448` 的 IDA 注释给出了**权威的 RM 记录相对偏移**：

> RM record dwords @+80/+84 → entry+0x28/+0x2C (rangeMax/Min); TWO SEPARATE BYTES @+88/+89 → entry+0x30/+0x31 (VFP 索引上下界); `disabled`@entry+0 == **取反后的 RM 类型字节@+64**：**1 = 域存在但无 range 信息（type 0）**，**0 = range 与 vfp 索引字段已填（type 1）**；`clockType`@entry+4 由 **RM 域 id@+68** 填。

**要点（易错）**：该类型字节是**取反**的 ⇒ **RM `+64 == 0` 表示"没有 range 信息"** ✔（我先前按字面读成"0 = 已填"是反的 ✗）。

**对我的 12336 B 目录 fixture 重新解码（客户端 python，无 GPU）** ✔：`+64` 在**每一行已填行**上都是 **0** ⇒ 按上述 => **这些域都没有 range 信息** ⇒ **`+80/+84`（rangeMax/Min）与 `+88/+89`（VFP 上下界）全为零是自洽的** ✔，不是数据缺失 ✔。
**纠正**：我先前（§14/§18）把目录里看起来像区间的 `+40/+44`＋`+48` 当作**可设偏移窗口** ✗ —— 按实测偏移，调用方可见的 `rangeMin/rangeMax` 在 **RM+80/+84**，本块这些域**根本没有**它们 ⇒ **`+40/+44` 只是旁路字段，不是窗口证据** ✗（保留字节事实 ✔，撤掉"这是窗口"的解读 ✗）。
**单位（新定位，来自仓库封送代码）**：`nvapi-rs/src/types.rs:260-296` 中 `From<i32> for Kilohertz2Delta` 为**恒等** ✔，`×2` 只出现在 `KilohertzDelta → Kilohertz2Delta` ✔；`clock.rs:303-331` 直接把 `rangeMax/rangeMin` 包进该类型 ⇒ **调用方 `rangeMin/rangeMax` 的单位就是 kHz 原样** ✔。故若 `0x20809025@7188` 的 `±18750` 属同族字段，即 **±18.75 MHz** ✗ —— 幅度量级可疑 ⇒ **该块字段是否同族仍记为尚未定位** ✔（不再用目录字段替代它 ✗）。
**表内核对（1118 行修正表）** ✔：`0x20809019@12336`、`0x20809025@7188`、`0x2080901b@2108` / `0x2080d01c@2108`、`0x2080b201@2444`、`0x20808532@336`、`0x2080901d@32680`、`0x20809026@5332`；**表内没有任何 2344 字节的 id** ✔ ⇒ `0x928` 是 **NVAPI 调用方结构**（`nvversion! ... V1 = 2344`）的大小 ✔，`12336` 是 **RM 块**的大小 ✔ ⇒ **两层结构本来就不同** ✔，不必再去找一个 2344 的私有 id ✗。
**验证范围**：全部为**静态**（仓库源码 + 已抓取的 fixture） ✔；**未跑 GPU**，设备访问 1 次（anchor）✔。IDA 偏移应用于本 fixture 属**拟合成功**（每行 type 0 ⇒ 无 range ⇒ 相关字段空 ✔）而非已证明 ✔。

## 21. 客户端原生层审计：本仓库只发 GET，且把"更宽掩码未测"写在注释里（2026-10-18）

**审的是本仓库自己的**发送侧源码（即"实际封送逻辑"在树内的那一份）：`core/native/macos_rm.c:586-637` 与 `core/native/macos_rm_abi.h`。

**(1) 该层没有任何 SET id** ✔ —— 在 `core/native/` 内搜 `0x2080d0*`/`d01c`/`d024`/`f214`/`write` **零命中** ✗。该层的偏移通道只有三个 GET：`{0x20809019,12336}`、`{0x20809025,7188}`、`{0x2080901b,2108}` ✔，且**发送前 `memset(out,0,size)`**、仅对控制块在 `+4` 种入 `0xFF` ✔。
⇒ 这是"**偏移写入缺哪一层**"的**第二份独立支撑**（第一份是进程特权 ✗）：**客户端这一层根本没有写命令** ✔，与"未提权 ⇒ `0x1b`"是两个各自充分的阻断点 ✔。**不是协议缺失，也不是固件问题** ✔。
**(2) 该注释自己声明了未测量范围** ✔：`NVOC_RM_OFFSET_CONTROL_MASK 0xff`、`+4` 掩码 ✔，并写明 *"The device lists ten entries, so this selection deliberately stops at index 7 — that extent is what has been measured; **a wider mask has not**"* ✔。⇒ 控制块第 8/9 两个条目**从未被读过** ✗ ⇒ 这正是**只读**即可补的缺口 ✔（写入门控无关 ✗），且 runbook 早已包含 `0x3FF` 那一档 ✔。
**(3) 与两层结构互证** ✔：`core/src/macos_rm.rs:328` 记录调用方尺寸 **2520/2404 被 `0x1f` 拒绝** ✔，而 RM 侧 12336/7188/2108 被接受 ✔ ⇒ 与本轮 §20 的"NVAPI 调用方层 vs RM 块层"结论一致 ✔ ⇒ 不必再尝试用调用方尺寸发文 ✗。

**下一健康会话的预登记判据（只读，`0x3FF` = 目录 0..9 全部十条）** ✔：按 `0xFF` 已测算术（106 = 14 头 + 7×13 + 1 个 type-03 只写戳记 ✔）与 fixture 实测类型序列 `[08,09,08,09,09,09,03,08,09,09]`（非 03 条目共 **9** 个 ✔）⇒ **预测写入 dword 数 = 14 + 9×13 + 1 = 132** ✔。
- 若 **=132** ⇒ 掩码位↔条目索引映射与整套算术同时被证实 ✔，第 8/9 条目的写入集**无需任何写入**即可知 ✔。
- 若 **=154** ⇒ 每条目 dword 数与该算术不符 ⇒ 该算术作废并重推（不是"掩码变了"）✔。
- 若为其它值 ⇒ 逐条目 census 定位差异，禁止猜 ✗。
**验证范围**：全部静态（仓库源码 + 既有 fixture）✔；**未跑 GPU**，设备访问 1 次（anchor）✔。

## 22. `0x20809025` 的 `-18750/+18750` 含义与单位：由仓库内 NVAPI 副本的 IDA 注记定位（2026-10-18，**关闭 §20 的未决项**）

**来源（仓库内封送资料，非猜测）** ✔：`nvapi-rs/sys/src/gpu/clock.rs:5408-5428`
- `ClkFreqController directory (ID 0x58F4F4C1, **RM 0x20809025**, magic 0x10C4C = (1<<16)|3148 = 0x4C + 32x0x60). No seed. Live TU116: mask 0x7, records carry {1638400 max_kHz, -18750/+18750 offset pair}. 4060 Laptop/R610: mask 0` ✔
- `rec+0x2C: u32 max VF frequency kHz (live 1638400 = 1638.4 MHz)`、**`rec+0x38: i32 -18750 (freq offset floor, ±18.75 MHz pair)`**、**`rec+0x3C: i32 +18750 (offset ceiling)`** ✔
另有 `nvapi-rs/src/clock.rs:2246`：`min_offset/max_offset ±18750 (the ±18.75 MHz control span)` ✔，以及 `sys/src/gpu/clock.rs:4814-4821` 的 IDA 实测：该处驱动器写入 **16 位** `0xB6C2 = -18750`，并从**恰好那 2 字节**读回 ✔（与 §18 实测的 **int16** `∓18750` 宽度一致 ✔）。

**结论（定位）** ✔：该对字段是**频率偏移的下界/上界（offset floor/ceiling）**，即注释所称**±18.75 MHz 控制跨度** ✔ ⇒ **单位为 kHz 且是"偏移跨度"语义，不是绝对频率上限** ✔。§20 中"若同族则为 ±18.75 MHz、量级可疑、记为尚未定位"的疑虑**由此解除** ✔ —— 该幅度就是驱动声明的偏移控制跨度 ✔。
**尚未reconcile的一点**（如实保留）✗：副本的**调用方**几何为 `0x4C + 32×0x60`（3148）✔，而本驱动实测 **RM** 块为 `20 + 32×224`（7188）✔ ⇒ 两者是**两层**（同 §20/§21 结论 ✔），记录以**内容**匹配（`max_kHz 1638400` ✔ 与 `∓18750` int16 ✔ 都逐一吻合），**尚无逐字段映射证明** ✗ ⇒ 字段归属记为已定位、字段映射记为尚未证明 ✔。
**验证范围**：静态（仓库源码 + 既有 fixture）✔。**未跑 GPU**，设备访问 1 次（anchor）✔。

## 23. 与仓库内 NVAPI 副本逐项对表：range 块的 `+4` 掩码获独立佐证；`0x20809026` 状态块的两层尺寸（2026-10-18）

**对表对象**：`nvapi-rs/sys/src/gpu/clock.rs:5408-5431`（`NV_GPU_CLOCK_CLK_FREQ_CONTROLLER_INFO_V1`，自述 `RM 0x20809025`，magic `0xC4C = 0x4C + 32×0x60`，字段常量 `MASK`/`REC_TYPE`/`REC_STEPS`/`REC_MAX_KHZ`/`REC_MIN_OFFSET`/`REC_MAX_OFFSET`）。

| 副本声明（调用方层） | 本机实测（RM 层） | 判定 |
|---|---|---|
| `MASK` 在 `+4`、**"MASK-SEEDED at +4"** ✔ | range 块 head `+4 = 0x7` ✔ | **吻合** ⇒ 我此前"`+4` 是每条目掩码位"的**推断获得独立佐证** ✔ |
| `Live TU116: mask 0x7` ✔ | 掩码 `0x7` = 位 0..2，恰为**3 条有值记录** ✔ | **吻合** ✔ |
| 记录含 `1638400 max_kHz` ✔ | `+8 = 1638400` ✔ | **吻合** ✔ |
| `rec+0x38` floor `-18750`／`rec+0x3C` ceiling `+18750` ✔ | `+20/+24` = `∓18750`（int16）✔ | **吻合（值/宽度）** ✔ |
| 几何 `0x4C + 32×0x60`（76+3072=3148）✗ | `20 + 32×224`（7188）✗ | **两层不同**，映射未证 ✔ |
| `rec+4` 类型枚举（raw 0..8 恒等，9→0xF、10→0x10）✗ | 目录行的 `+4` 是**掩码位**而非类型 ✗ | **不同块，勿混用** ✔ |

**结论**：range 块 head `+4` 的**掩码语义**与 `0x7` 的选择范围，由仓库内副本独立佐证 ✔（此前仅有实测推断 ✔）；副本的**字段常量表**可用于命名，但其基址是调用方层，**逐字段映射仍未证明** ✗ ⇒ 只按**内容**对应 ✔。

**`0x20809026`（ClkFreqController status）**：副本给 `NV_GPU_CLOCK_CLK_FREQ_CONTROLLER_STATUS_V1`（ID `0x45C064D5`，**RM 0x20809026**，magic `0x9CC = 0x4C + 32×0x4C` = **2508** ✔，`MASK-SEEDED at +4` ✔），并声明 *"Live TU116: all records {6, 0, 0} static on the v1 path (470-generation data unfilled — re-evaluate with the v3 stamp 0x30970 on 610+ drivers)"* ✔。
**修正我自己记录里的一处粗疏** ✗：该 id 并非"从未读过"——探测程序有 **2508**（调用方尺寸 ✔）的用例表，且解码器记有 **5332** 的几何 `(5332, 84, 164)` ✔（与一次成功读回相符 ✔）；offset 电池**拒发的是 5332 那一形式** ✗，因为**正是那次调用**在 2026-10-17 闩住了 GSP ✗ ⇒ "这个 id 危险"说过头了 ✔，准确说法是"**RM 尺寸那一形式**这次不发" ✔。
**待办（只读、低成本）**：健康会话加发一次 **`0x20809026@2508`**（副本的调用方尺寸），并带上副本的预期——TU116 上该 v1 路径**全静态 {6,0,0}** ✗ ⇒ 即便 NV_OK 也别指望它给出实时值 ✔；若要拿 470 代的数据需换 v3 戳记 `0x30970` ✔。
**验证范围**：静态（仓库源码 + 既有 fixture）✔；`sh -n` 通过 ✔。**未跑 GPU**，设备访问 1 次（anchor）✔。

## 24. 候选映射纠错表（截至 2026-10-18）＋读写 id 映射规则与偏移族的"唯一可写对"（2026-10-18）

### 24.1 读写 id 映射规则（两份独立来源）
- 本地 GSP 孪生表 `/home/lyy/work/nvoc-analysis/twin-map-610.57.04.txt` 头部**原文**：
  `# rule: flip bit14 (privileged) and idx+1; interface and bit15 unchanged` ✔
  （该文件在 `nvoc-analysis/` 下，不在 `tables/` 下 ✗ —— 我此前的路径记录有误，此处更正 ✔。）
- 对修正后的 1118 行 GSP 导出表（`nvoc-analysis/tables/gsp-1118.tsv`）**穷举核对** ✔：读半区（`0x20800000-0x2080bfff`）**1026 行**，写半区 **92 行** ✔；`id+0x4001` 且 **paramSize 完全相同**的有 **47** 对 ✔（例：`0x2080852c→0x2080c52d`、`0x20808530→0x2080c531`、`0x2080853c→0x2080c53d`、`0x2080854c→0x2080c54d` ✔）；`id+0x4000` 且尺寸相同的 **4** 对 ✔（`0x2080a06f→0x2080e06f`、`0x2080a09c→0x2080e09c`、`0x2080a0a6→0x2080e0a6`、`0x2080a63a→0x2080e63a` ✔）。⇒ 规则成立且**以 +1 为主（47 对）**，另有 4 对本规则下**异常**（记异常，不强行解释 ✗）。

### 24.2 偏移族的可写性：**只有控制块有声明孪生** ✔
| 读 id | 尺寸 | flags | 写孪生 | 写尺寸 | 判定 |
|---|---|---|---|---|---|
| `0x2080901b` | 2108 | `0x48` | **`0x2080d01c`** ✔ | 2108 | **存在**（flags `0x44`）✔ |
| `0x20809019` | 12336 | `0x48` | `0x2080d019` | — | **不存在** ✗ |
| `0x20809025` | 7188 | `0x48` | `0x2080d026` | — | **不存在** ✗ |
| `0x20809026` | 5332 | `0x48` | `0x2080d027` | — | **不存在** ✗ |
| `0x20808532`（风扇候选） | 336 | `0x48` | `0x2080c532` | — | **不存在** ✗ |
| `0x20808513`（热，非特权孪生） | 1432 | `0x10248` | `0x2080c513` | — | **不存在** ✗ |
两处独立来源一致 ✔：本地孪生表**只有一行**属于该族 —— `0x2080901b 2108 CLK 0x2080d01c 2108 yes` ✔。
⇒ **目录、range、status 三者按驱动自己的声明都是只读** ✔；**偏移族唯一声明的写通道就是控制块那一对** ✔ ⇒ 已排定的写实验（bit 0、值在 record+8）打的正是**唯一**的声明写路径 ✔ ✔。

### 24.3 纠错表（本会话中被**撤回或收窄**的判断）
| 曾记录的说法 | 现状 | 依据 |
|---|---|---|
| 目录 `+40/+44` 是"可设窗口" ✗ | **撤回**（字节事实保留） | RM 记录的 `+64` 类型字节全为 0 ⇒ 无 range 信息（RM+80/+84 全零自洽）✔ |
| `±18750` 单位"尚未定位" ✗ | **已定位**：偏移下界/上界，单位 kHz（±18.75 MHz 跨度）✔ | 仓库内 NVAPI 副本 IDA 注记 ✔（§22） |
| "方法表只声明了一对私有读/写" ✗ | **收窄**：仅指我探测的那六个 id ✔；整表实测 **51 对**（47+4）✔ | 24.1 穷举 ✔ |
| "客户端层没有 SET" ✔ | **保持**，并明确限定为**客户端层** ✗：驱动侧写半区有 92 行 ✔ | §21＋24.1 ✔ |
| "`0x20809026` 从未读过" ✗ | **更正**：探测程序有 2508 用例表、解码器有 5332 几何 ✔，拒发的只是 5332 那一形式 ✔ | §23 ✔ |
| `0x2080d024`（66060）归属不明 ✗ | **已定位**：它是 `0x20809023`（66060）的孪生 ✔，不属偏移族 ✔ | 24.1 尺寸配对 ✔ |
| `+4` 是掩码位（推断）✔ | **已由仓库内副本独立佐证** ✔ | §23 ✔ |
**验证范围**：静态（表 + 仓库源码）✔。**未跑 GPU**，设备访问 1 次（anchor）✔。

## 25. 热/风扇族的**声明面**审计：HWFS 是"已被实测可读的近期控制"，但本机从未发过（2026-10-18）

**方法与依据** ✔：查仓库内 NVAPI 副本对 RM id 的注记（穷举 `RM 0x2080…` 字面量 ✔）、修正后的两份 id 表、本地孪生表，以及探测程序自身到底发过哪些 id（穷举 `nvx_suite.c` ✔）。

### 25.1 副本给 RM id 打过的全部注记（31 个）✗
`0x20808540`、`0x20809006/19/1a/1b/21/22/23/25/26/33/34/35/61/62/79/7b/83/a0/a1/b0/b1`、`0x2080a0b1/a0b3/a0b5/a0b6`、`0x2080d01c/d024/d036`、`0x2080e0b0/e0b7` ✔。
⇒ **其中没有任何风扇/散热器（cooler/fan/arbiter）id** ✗ ⇒ 客户端的风扇面（`GetCoolerSettings` 族、仲裁器族）在副本里**没有 RM id 注记** ✗ ⇒ 这解释了为何风扇族至今只有一个**探测猜出的候选** `0x20808532` ✗ 而从未被确认 ✔。**"散热器/仲裁器背后的 RM id"记为尚未定位** ✔（不是"找不到"，而是副本与两份表都没给它注记 ✔）。
**可反驳的例外**：`0x20808540`（HWFS）**有**注记，且其名 `NV_GPU_THERMAL_HWFS_CONTROL_V1` 属热族 ✔ ⇒ 见 25.2。

### 25.2 HWFS：声明面完整、且副本记有**在本代 GPU 上实测过的实时值** ✔
- 副本注记（原文）：`GET RM 0x20808540/0x20800540, escape 0x07000190/0x07000191`，*"Flat 52-byte struct, no mask/records: +4 u8 in, +5 u8 out, +0x24 u32 selector (jump-table 0..0x1D), +0x28/+0x2C/+0x30 u32 in/out. **Live sel=2 → {out=10, 0x7FFFFFFF, 64, 256(Q8 1.0?)} on BOTH TU116/462.96 and 4060/R610**; sel 0/1/3 → -104 both; sel 4/5 → -104 on TU116 but -5 on 4060"* ✔
- 表中的声明 ✔：`0x20808540 24 0x48`（**GSP 导出**，非特权半区 ✔）；本地孪生表：`0x20808540 24 THERMAL → 0x2080c541 24 yes` ✔ ⇒ **有声明写孪生** ✔（`0x2080c541` 在表中 flags `0x44` ✔）。
- **注意两层** ✗：副本的 52 字节是**其自身/escape**的尺寸（magic `0x34`=52 ✔），而 GSP 导出的该 id 是 **24** ✔ ⇒ 与 §20/§26 一致的两层结构 ⇒ **向该 id 发 52 字节没有依据** ✗，稳妥的只读试发是**按声明 24** ✔。
- **本机从未发过** ✗：`probe-macos-rm/` 全树 grep `hwfs|HWFS|0x20808540|0x20800540` **零命中** ✔；探测程序实际只发过两个 `0x208085xx` id（`0x20808513`、`0x20808532` ✔）。
⇒ **登记（只读、低成本、待健康会话）**：试发一次 `0x20808540@24`（无掩码、平面结构 ✔），带上副本的预期（`sel=2` 时读数形如 `{10, 0x7FFFFFFF, 64, 256}` ✔；`sel 0/1/3` 两代都回 `-104` ✔）。**权限判定只按规则说**：该 id 在非特权半区（bit14 清 ✔），但**宿主是否转发它无法由 host-608 表证明** ✗（该表是公共面清单 ✔，缺行不证明不存在 ✔）⇒ 以实测为准 ✔。

### 25.3 热族状态**按 id 分列**（纠正此前"整族未知"的说法 ✗）
| id | 声明 | 状态 |
|---|---|---|
| `0x20800513` ThermalSystemExecuteV2@1432 | host 表有 ✔（flags 未列）；GSP 表 flags `0x48` ✔ | 已实现待实测（本机是否发过需按 25.4 核对 ✔）|
| `0x20808513`@1432 flags `0x10248` | GSP-only ✔（host 表无 ✔）| 未实现待实测（非特权孪生，从未发过 ✔）|
| `0x20808540` HWFS@24 ↔ `0x2080c541` ✔ | 两表均无 `0x20800540` ✗ | **已实现待实测（本机从未发 ✔）**；副本记有本代实测值 ✔ |
| 热限/热策略 opcode | 驱动自行命名的表里只有 `0x20800513` 一条热命令 ✔ | **调查未知（有具名层次：客户端/驱动实现缺口）** ✔ |
⇒ **散热/温度族不是"整族不可读"** ✔：HWFS 有完整声明面与跨代实测值 ✔，只是本机没发过 ✗。
**验证范围**：静态（副本 + 两份表 + 孪生表 + 探测程序穷举 ✔）。**未跑 GPU**，设备访问 1 次（anchor）✔。

## 26. 功耗族声明面 + 特权面计数更正（2026-10-18）

### 26.1 更正：上一轮的"写半区 92 行"是**门控计数**，不是特权面总数 ✗
按位重新穷举修正后的 1118 行导出表 ✔：**bit14 置位（0x4000）共 112 行** ✗，其中 bit15 置位者 **92 行** ✔（= `(cmd & 0xC000) == 0xC000` 的那个既有门控计数 ✔，与我长期记录的 92 一致 ✔），**bit15 未置位者另有 20 行** ✗ —— 即我上一轮把它当成"写半区总数"是**命名错误** ✗（当时用的 `>= 0xc000` 分割把 bit15 一起当条件 ✗）。⇒ **特权面是 112 行，其中 92 行属"legacy 转发（bit15 置位）"形式，20 行不是** ✔。上一轮据此得出的**孪生对数（47 + 4）不受影响** ✔（那是按规则逐对核出的 ✔）。
**推论**：§24.1 的"flip bit14 and idx+1"规则描述的是**legacy 转发族** ✔；把"92"当作写面全集会漏掉那 20 行 ✗。

### 26.2 功耗族：RATED_TDP 的 GET/SET 都在**非特权半区**，且配对方式与 legacy 族**不同** ✗
导出表实测 ✔：`0x2080206e` @12 flags `0x10008` ✔（SDK 名 `NV2080_CTRL_CMD_PERF_RATED_TDP_GET_CONTROL` ✔）与 `0x2080206f` @12 flags `0x40048` ✔（`..._SET_CONTROL` ✔）；两者 bit14 **皆清** ✗。
- 按 §24.1 规则预测 `0x2080206f` 的孪生应为 `0x20806070`/`0x20806071` ✗ —— **表中皆不存在** ✗；对 `0x2080200a/0b/5b/5d/68/69/6e/6f/87/92/93` 逐一代入该规则，**一个孪生都找不到** ✗。
⇒ **存在第二种配对约定** ✔：导出命令用 **`…ne`（GET，flags `0x10008`）/`…nf`（SET，flags `0x40048`）相邻 id** 表示读写对，而不是 legacy 族的 bit14+idx+1 ✔。这与 §24.2 的偏移族结论**不冲突** ✔（偏移族的两处独立证据仍成立 ✔），但**"通用孪生规则"的说法必须收窄为"legacy 转发族"** ✗。
- 同号非特权半区行：`0x2080a06e` @**12** ✔（与 RATED_TDP GET 同尺寸）flags `0x48` ✗（与 `0x10008` 不同类 ✗）⇒ **是否同一条命令尚未定位** ✔（尺寸相同、flag 类别不同，证据不足，不强行认定 ✗）。
- 表内其它功耗邻域行（供后续核对 ✔）：`0x2080200a`(8,`0x50048`)、`0x2080200b`(780,`0x40048`)、`0x2080205b`(4,`0x50048`)、`0x2080205d`(4,`0x50048`)、`0x20802068`(4,`0x48`)、`0x20802069`(8,`0x4a`)、`0x20802087`(12,`0x08`)、`0x20802092`(4,`0x18`)、`0x20802093`(1,`0x50008`) ✔。
### 26.3 flag 位 `0x40000` 的含义：**证据两边都有，记为尚未定位** ✗
- 支持"需要提权"：`0x2080206f`（SET）flags `0x40048` ✔ 含该位；`0x2080206e`（GET）`0x10008` ✔ 不含 ✔。
- **反例**：偏移 SET `0x2080d01c` flags **`0x44`** ✗ 不含该位，但它已被实测在非 root 下回 `0x1b`（提权门控 ✔）；而 `0x2080200b`（一个 GET）flags `0x40048` ✗ 却含该位。
⇒ **不成立为"必经提权"的判据** ✗；记为尚未定位 ✔，并把上述正反例一并留存 ✔。
**验证范围**：静态（导出表 + 孪生表 + 仓库源码 ✔）。**未跑 GPU**，设备访问 1 次（anchor）✔。

## 27. 控制块的**逐字段权威版式**（副本内 IDA 记录）＋被拒尺寸的命名，及对写实验候选的排序更正（2026-10-18）

### 27.1 副本给出的 id↔NVAPI 对照与记录版式（原文位置 `nvapi-rs/sys/src/gpu/clock.rs`）
- **对照**（L1367-1370 ✔，作者注为 IDA 确认 R575.74 ✓）：`CLK_CLK_DOMAINS_GET_INFO (0x20809019) → NvAPI_GPU_ClockClkDomainsGetInfo` ✔、`GET_CONTROL (0x2080901b) → NvAPI_GPU_ClockClkDomainsGetControl` ✔、**`SET_CONTROL (0x2080d01c) → NvAPI_GPU_ClockClkDomainsSetControl`** ✔、`CLK_MEASURE_FREQ (0x20809006) → NvAPI_GPU_ClockCounterMeasureAvgFreq` ✔ ⇒ **正是本会话那三个 GET ＋ 那个唯一写对的对应关系** ✔，且**写命令的名称与 §24.2"唯一声明写对"完全吻合** ✔。
- **记录版式**（L1376-1409 ✔，注为 IDA ＋ 实机 dump）：`+0` 版本魔数、**`+8` controllable_mask(u32)**、`+12..+99` 不透明头、**`+100` 起 32×72B 的按位稀疏记录（domain bit N ⇒ +100+72*N）**；每条记录：`+0` u8 类型（实机 `0x0A`）、随后五个 u32 在 **`+44..+60`**，语义依序为 **`offset_khz(i32)`、`range_min`、`range_max`、`applied`、`extra`** ✔ ⇒ 常量表 `OFFSET_KHZ=44`、`RANGE_MIN=48`、`RANGE_MAX=52`、`APPLIED=56` ✔。
- 实机对照：该层 `mask` 读作 `0x000000FF`，注释点名 **GPC(bit0)|XBAR(bit1)|SYS(bit2)|MCLK(bit4)** ✔ —— 与本机"掩码位选择**记录号**"的实测**不冲突**（两层的位语义可能不同 ✗，见下）。
- V2（L1776-1794 ✔）：魔数 `0x261A4` ＝ 版本 2 ｜ 尺寸 `0x61A4`=24996 ✔，`mask@+8`、记录 `@+292` 步长 772 ✔（作者还留了一条自查注记：早先误记为 `0x26154`，handler 的 `cmp eax,261A4h` 才是权威 ✔）。

### 27.2 被驱动**明确拒绝**的两个尺寸，现在有名有姓 ✔
本机 `core/src/macos_rm.rs` 记录过：调用者层的 **2520** 与 **2404** 两个尺寸都被 `0x1f` 拒绝 ✔。副本给出：**2404 = `0x964` = `NV_GPU_CLOCK_CLIENT_CLK_DOMAINS_CONTROL_V1`** ✔、**2520 = `0x9D8` = `..._CLK_DOMAINS_INFO_PRIVATE_V1`** ✔ ⇒ 被拒的不是随便的尺寸，而是**副本那一层的 V1 控制块与私有信息块** ✔。而本机**能通**的是 **2108**（控制）与 **12336/7188/5332**（目录/range/status ✔）⇒ **两层确实不同** ✔（§20/§26 的一贯结论 ✔）。

### 27.3 由此对写实验候选**重新排序**（这是本轮唯一的行为改动）✔
- 推理链：副本的记录里 **`+44` 是"偏移(kHz)"、`+56` 是"已应用值"** ✔；本机实测的十二个可写 dword 集合 **包含 44 与 56** ✔，但**不含 48** ✗ —— 而本机早先实测就是"GET 从不写 `rec+48`" ✔ ⇒ 与"48＝range_min 属输入字段"**自洽** ✔。
- **边界（不得越界宣称）** ✗：副本的记录基址 100/步长 72（V1）与 292/772（V2），本机 RM 块是 **60/64** ✗ ⇒ 字段版式**不能直接搬运** ✗ ⇒ "本机 `rec+44` 承载偏移"是**推断** ✔，其可反驳点已明确 ✔（写 `+44` 后 `+56` 是否随动、以及同记录其它 dword 是否保持）。
- **因此**：把排定的写实验第一例改为 **`0:44:250`** ✔（唯一被点名为 offset 的字段 ✔），`0:8:250` 降为第二例 ✗，并在回读时**同时观察 `+56`（applied）** ✔。运行手册头部已按此更新 ✔（`sh -n` 通过 ✔）。
- 依旧受门控 ✗：**尚未运行** ✔，只改候选顺序与依据 ✔。

### 27.4 交叉验证：副本注记的 id 在导出表中**全部存在** ✔
逐一核对 `0x20809006`(8,`0x81074a`) ✔、`0x2080901a`(2052,`0x48`) ✔、`0x20809021`(33288) ✔、`0x20809022`(623112) ✔、`0x20809023`(**66060** ⇒ 与 §24.3"`0x2080d024` 是该 id 的孪生"吻合 ✔)、`0x20809033`(916) ✔、`0x20809034`(840) ✔、`0x20809035`(1164,`0x10248`) ✔、`0x20809061`(776) ✔、`0x20809062`(648,`0x82004a`) ✔、`0x20809079`(400) ✔、`0x2080907b`(392) ✔、`0x20809083`(3096) ✔ ⇒ **副本与导出表不矛盾** ✔ ⇒ 这反过来加强 §25.1 的结论：副本里**真的没有**风扇/散热器 id ✗（不是表缺行所致 ✔）。
`0x20809006`（测量频率）**登记为补充只读候选** ✔：8 字节声明 ✔，副本语义为"周期计数器＋时间戳，需采样两次算 Δ" ✔（**注意**：按原目标，实时频率变化**不能**替代偏移设置成功 ✔，此读数只作旁证 ✔）。

### 27.5 功耗族读的实测状态更正（日志证据）
`outputs/telemetry-ctl-map.jsonl` 与 `stage-gsp-clkctl-20261017T092536Z/*.txt` 中，`rated-tdp-get-control` 多次 **`NV_OK`** ✔（wedge 期间那次为 `0x62` ✔），detail 为 `raw[0..2]=0 0 0 (client/input/vpstate echo)` ✔ ⇒ 分类更正为 **"已实现且实测（命令可通）；返回值的可解释性待确认"** ✗（不是"待实测" ✔，也不是"已解释" ✗）。
**验证范围**：静态（副本源码 + 导出表 + 测试机日志 ✔）＋运行手册仅改注释与首例顺序（`sh -n` 通过 ✔）。**未跑 GPU**，设备访问 1 次 ✔。

## 28. 各族**实测证据**核对：可读面比先前结论好得多（2026-10-18）

**方法** ✔：不改任何代码、不发任何新命令，只穷举测试机 `outputs/` 下**已有的**日志（`telemetry-ctl-map.jsonl`、各 `stage-*` 目录 ✔）与探测程序自身可发的检查名清单 ✔，逐族核对"到底有没有实测成功过" ✔。**注意**：这些实测记录均来自 **2026-10-17 之前**（wedge 之前 ✔）。

### 28.1 实测证据表（每条都可回溯到日志 ✔）
| 族 | 实测命令 / 路径 | 结果 | 可读 | 可解释 | 范围 | 原状态 | 恢复 |
|---|---|---|---|---|---|---|---|
| 核心/显存频率 | `rusd-readout`（RUSD 页）| **NV_OK**，`gpc 1530000 kHz`、`mem 6000984 kHz`、负载中 `graphics_clock_khz 1935000` ✔ | ✔ | ✔ | ✔（实时值）| ✔ 有基线 | 只读 |
| 温度 | `thermal-sensor`（走 1432 的 `0x20800513`）| **NV_OK**，`reading 47 \| range [0,127] \| target 0 provider 0` ✔ | ✔ | ✔ | **✔ 有范围 [0,127]** | ✔ | 只读 |
| 温度（另一路）| `rusd-readout` | `temp[gpu=44.46C…51.9C]`、负载中 `temperature_c 59.34` ✔ | ✔ | ✔ | ✔ | ✔ | 只读 |
| 温度（分类）| `thermal-classify` | **NV_OK**，`target 0 type 1 (=GPU) provider 0 type 1` ✔ | ✔ | ✔ | — | ✔ | 只读 |
| 风扇 | `rusd-readout` / `get-status` | `fan_rpm` **1613**（记录中的"前"采样，非 0）→**负载中 `fan_rpm 1833`/`fan_rpms [1833]`** ✔ | ✔ | ✔ | ✔（记录中 1610–1833；更早一次空转读数为 0，**未在本批记录中复现**）| ✔ | 只读 |
| 功耗 | `rusd-readout` / `get-status` | `limit 100000 mW`、`power_policy_tgp_mw 12857`、负载中 `power_mw 12798`、`utilization 99 %` ✔ | ✔ | ✔ | ✔ | ✔ | 只读 |
| 功耗（命令面）| `rated-tdp-get-control` | **NV_OK**（detail 仅 `raw[0..2]=0 0 0`）✔ | ✔ | **待确认** ✗ | 待确认 | 待确认 | 只读 |
| 热批处理 | `thermal-targets-sensors` / `thermal-counts` | **NV_OK**，`EXECUTE_V2 (0x20800513) paramSize 1432: batch accepted`、`1 target(s), 1 sensor(s)` ✔ | ✔ | ✔ | — | ✔ | 只读 |
### 28.2 明确被拒/失败（同样是证据 ✔）
| 命令 | 结果 | 判定 |
|---|---|---|
| `0x20809006` measure-freq | **`0x1f`**，穷举 44/55/60/61 尺寸与 11 个 (version\|size) 魔数**全不通过** ✔ | **接受的形式尚未定位** ✔（是**形式**问题 ✗，不是"无此命令" ✔——副本自己的 65568/32 字节形式同样被拒 ✔）|
| `internal-vpstate-info`（`0x20800ad4`）| `0x56` NOT_SUPPORTED（两形式皆然 ✔）| 驱动明确拒绝 ✔ |
| `edpp-limit-info` / `pmumon-clk-samples` / `perf-level-info-v2` | `0x56` / `0x56` / `0x1f` ✔ | 驱动明确拒绝 ✔ |
| `0x20800532` fan 公开 id | `0x56` ✔ | 驱动明确拒绝 ✔（两表皆无该行 ✔）|
### 28.3 由此对本会话先前结论的**更正** ✗
- 第 49 轮写的"热族＝调查未知（有具名层次）" ✗ **过度悲观**：实测里 **`0x20800513` 1432 形式被接受 ✔**，并且**真的带出传感器读数、范围 [0,127] 与 target** ✔ ⇒ 温度族是**可读/可解释/有范围/有原状态** ✔；仍然缺的是**热限/策略的写面** ✔（§25.3 的"仅一条热命令"仍成立 ✔）。
- 第 49 轮"风扇族只剩探测猜出的 `0x20808532`" ✗ **也只是命令面**：风扇 RPM 通过 **RUSD** 已经在负载中实测到 **1833 rpm** ✔ ⇒ 风扇族是**可读/可解释/有范围/有原状态** ✔；缺的是**控制（仲裁）面** ✔（§25.1 的"没有 RM id 注记"仍成立 ✔）。
- ⇒ **对本轮最高优先的直接意义** ✔：偏移通道所需的"**当前值 / 限制 / 原状态**"**已经在手** ✔（RUSD 给实时核心/显存频率 ✔，range 块给 ±18.75 MHz 跨度 ✔）⇒ 仍未解决的只剩两件：**①十二个 dword 里哪一个承载偏移**（§27.3 已给出首选与可反驳点 ✗，待写实验 ✗）、**②写权限前置条件**（§24、§26 ✔ 已定位为进程提权＋客户端 SET 入口缺失 ✗）。**不再是"读不到"** ✔。
### 28.4 附带确认：压力侧能力已在本机实测过 ✔
`stage6-fan-read-20261017T060900Z` 的 README 记录：**20 s Vulkan 负载中**采样 `nvoc-cli --macos-rm get-status -O json`，rc=0 ✔，读到 `graphics_clock_khz 1935000`、`engine_utilization gr 99.1 %`、`fan_rpm 1833` ✔ ⇒ **Vulkan 压力与实时遥测在本机跑通过** ✔（按原目标，这属于**压力负载**证据 ✔；**渲染成功/未崩溃 ≠ 超频稳定** ✗，故不能替代偏移设置成功 ✔）。
**验证范围**：静态（穷举测试机既有日志 ✔）＋ 无新命令、无写入 ✔。**未跑 GPU**（本轮未新增设备访问）✔。

## 29. **偏移本身已经是可读的**：`0x20809061@776` 接受并返回逐域"频率＋带符号 kHz 偏移"（2026-10-18，重大结论）

**来源** ✔：穷举测试机既有日志（`stage-clkdoc-*`、`stage-clkvf-read-*`、`stage-clkvf-stamped-*`、`stage-clkctl-*`，均为 2026-10-17 及更早 ✔）。**本轮未发任何新命令、无写入** ✔。

### 29.1 各 id 的**接受形式**已被实测穷举（"尺寸扫描为何徒劳"的答案）✔
| id | 试过的其它形式 | 结果 | 被接受的形式 |
|---|---|---|---|
| `0x20809061` vfp-get-info | 493060 / 83996 / 369796 / 157692 / 249844 ✔ | 全部 `0x1f` ✗ | **776 ✔ NV_OK（56 个非零 dword，首个 @0、末个 @264）** |
| `0x20809019` domains-get-info | 2520 / 296620 / 329388 ✔ | 全部 `0x1f` ✗ | 12336 ✔（既有实测 ✔）|
| `0x2080901b` domains-get-control | 2404（v1，noseed/`ff`/全 `ff`）与 24996（v2）✔ | **全部 `0x1f`** ✗ | **2108 ✔**（既有实测 ✔）|
| `0x20809021` vfp-get-info / `0x20809022` vfp-get-status | 61 种尺寸 × 11 个 stamped 魔数 ✔ | 全不通过 ✗ | **尚未定位** ✔ |
| `0x20809006` measure-freq | 44/55/60/61 尺寸与 11 个魔数 ✔ | 全不通过 ✗ | **尚未定位** ✔ |
⇒ **规律（有证据）** ✔：本驱动对同一 id 只接受**一个**形式，而它就是**导出表为该 id 声明的那一个尺寸**（776 ✔、12336 ✔、2108 ✔ 皆与表一致 ✔）；**副本那一层的尺寸（2404/24996/2520/493060…）在这里全部被拒** ✗ ⇒ 这也解释了此前"扩大尺寸扫描"为何无效 ✔（计 61 尺寸 × 11 魔数仍零收获 ✔）。

### 29.2 776 载荷的结构（**推断**，但有两处硬佐证 ✔）
载荷呈重复记录状：`00000303 <选择子> <频率 kHz> <字段4> <标志> 00000000` ✔，实测值如下（全部为日志原文 ✔）：
| 选择子 | 频率（kHz）| 字段4 | 说明 |
|---|---|---|---|
| 1 | **1 530 000** | `ffffffff` | **与 RUSD 实时 `gpc 1530000` 完全一致** ✔ |
| 2 | 1 455 000 | `ffffffff` | |
| 4 | 1 545 000 | `ffffffff` | |
| 8 | 404 488 | `ffffff0c` = **−244** | |
| **0x10（bit4：副本注为 MCLK ✔）** | **6 000 984** | `ffffff82` = **−126** | **与 RUSD 实时 `mem 6000984` 完全一致** ✔ |
| 0x20 | 1 318 976 | `ffffffff` | |
| 0x40 | 810 000 | `fffff0c0` = **−3904** | |
| 0x40000 | 107 488 | `fffffffb` = **−5** | |
| 0x80000 | 540 000 | `ffffff0c` = −244 | |
| 0x100000 | 1 408 976 | `ffffffff` | |
**推断** ✔：字段4是**带符号 kHz 偏移** —— 与驱动自身声明的"±18.75 MHz 偏移跨度"（§22 ✔）同量级、且取值都落在真实微调范围内（−5…−3904 kHz ✔）。
**硬佐证** ✔：**两个频率与 RUSD 实时读数逐位相同**（1 530 000 ✔、6 000 984 ✔）⇒ 该载荷是**真实的实时时钟文档** ✔，不是残余或伪值 ✔。
**可反驳点** ✔（写实验才有资格裁决 ✗）：受控 SET 之后，对应记录的字段4应随动 ✔；未写入时两次读取应保持一致 ✔（后者已成立 ✔，见 29.3）。
`ffffffff` ＝ −1 的语义**尚未定位** ✗（形似"未设置" ✔，与本会话早先记录的副本 `0x7FFFFFFF`"未设置"假说同类，但**不作为结论** ✗）。

### 29.3 `0x20809025@7188` 的 ∓18750 就地复核 ✔ 与"读取可重复"证据 ✔
- 原始 dump 复核 ✔：`gc 0x20809025 @7188` 出现 `+40:a5a5a5a5>ffffb6c2`（**−18750**）✔、`+44:…>0000493e`（**18750**）✔、`+252:…>00190000`（**1638400**）✔、`+264:…>ffffb6c2` ✔ ⇒ 记录 0 与记录 1 各自带该对值 ✔，与 §22/§23 一致 ✔。
- **可重复性** ✔：`stage-clkvf-read-20261017T072432Z` 与 `…072413Z` 的两次运行（`r1.jsonl`/`r2.jsonl`）**逐字节比较差异仅 66 字节、全部落在 `time` 字段**（`…873` vs `…875`）✔ ⇒ **载荷逐字节相同** ✔ ⇒ 可作为"原状态"基线证据 ✔。

### 29.4 由此对原目标最高优先的直接后果 ✔
- **第 3 条"先取得当前值/限制/原状态"已经完成** ✔：当前值＝RUSD 实时频率＋776 文档逐域频率 ✔；限制＝`0x20809025` 的 ±18.75 MHz 跨度与 1638400 kHz 上限 ✔；原状态＝776 文档逐域**偏移本身**（如 MCLK −126 kHz）＋两次读取逐字节相同 ✔。
- **第 4 条不受损** ✔：这里的偏移是从**文档读回**的，不是从"实时频率变化"推的 ✔ ⇒ 不以频率变化替代偏移设置成功 ✔。
- **只剩两件** ✔：**①写能力**（进程提权＋客户端 SET 入口缺失，§24/§26 ✔）；**②受控写入后该字段是否随动**（写实验 ✗，已排定 `0:44:250` ✔，见 §27.3 ✔）⇒ **都不是"读不到"** ✔。
### 29.5 更正
本会话早先"VF 读从未发过" ✗ **被证伪** ✔：`stage-clkvf-read-*`、`stage-clkvf-stamped-*` 存在且有重复运行 ✔；`0x20809061@776` 已实测 **NV_OK** ✔。
**验证范围**：静态（穷举测试机既有日志 ✔）；**未发新命令、无写入、未跑 GPU** ✔。

## 30. 把"可读的偏移"落到仓库封送层：`parse_clock_document` 与字节级 fixture（2026-10-18）

**选择依据** ✔：第 29 节已在既有日志中确证 `0x20809061@776` 是**被接受**的读取面且**发布逐域偏移** ✔，但仓库里**没有任何代码解析它** ✗（`core/native/macos_rm.c` 只发 `0x20809019/25/1b` ✗）。按原目标第 5 条"成功后接入现有公共接口"✔ 与第 2 条"从实际封送逻辑还原"✔，本轮先把**读的一半**做成可测试的仓库代码 ✔。

### 30.1 已落地
- `core/src/macos_offset.rs`：新增 `CLK_DOCUMENT_CMD (0x20809061)`、`CLK_DOCUMENT_BYTES (776)`、`CLK_DOCUMENT_RECORD_MARKER (0x00000303)`、`CLK_DOCUMENT_HEAD_BYTES (8)`、`CLK_DOCUMENT_RECORD_BYTES (24)`、`CLK_DOCUMENT_RECORDS (32)`，`ClkDocument{head, entries, unparsed_slots}`、`ClkDocumentEntry{index, domain_bit, frequency_khz, offset_khz, flags}` 与 `parse_clock_document()` ✔。
- `core/tests/data/macos-clkdoc-776.bin`（**776 字节，逐字节取自测试机 `outputs/stage-clkdoc-20261017T074448Z/clkdoc-read.txt` 的十六进制 dump** ✔）＋ `core/tests/macos_offset_fixture.rs` 的 fixture 测试 ✔。
- 几何自洽 ✔：8 + 32×24 = 776 ✔（与既有三条 block 的 tiling 测试并列断言 ✔）。
### 30.2 fixture 的实际内容（= 证据，不是构造值 ✔）
11 条记录：域位 1/2/4/8/0x10/0x20/0x40/0x40000/0x80000/0x100000/0x200000，频率 1 530 000 / 1 455 000 / 1 545 000 / 405 000 / **6 000 984** / 1 320 000 / 810 000 / 108 000 / 540 000 / 1 410 000 / 255，带符号字段 −1 / −1 / −1 / **−244** / **−126** / −1 / −3904 / −5 / −244 / −1 / 0 ✔。
**两处硬佐证** ✔：1 530 000 与 6 000 984 与该会话 RUSD 实时读数**逐位相同** ✔；0x10 正是副本自己分量掩码里的 MCLK 位 ✔。
**两个 stray 槽** ✗：槽 21 与槽 31 各只有 1 个 `0xff` 字节、其余为零 ✔ ⇒ 解析器**按索引报告**（`unparsed_slots`）而不是当作条目或丢弃 ✔ ⇒ "最后一条之后的槽是填充"仍是**推断**，尚未定论 ✔（不排除同容器内还有第二种数组 ✔）。
### 30.3 明确保留的推断边界 ✔
字段名（`frequency_khz`/`offset_khz`）是**读取语义的描述**，不是驱动声明；文档注释同时写出**佐证**与**可反驳点**（受控写入应使该字段随动 ✔，且尚未发生过任何写入 ✔）。`flags` 与 `ffffffff`(=−1) 的语义**尚未定位** ✔。
### 30.4 验证与范围
`cargo fmt --all -- --check` 干净 ✔；`cargo clippy --workspace --exclude cli-stressor-cuda-rs --all-targets -- -D warnings` 干净 ✔；`cargo test -p nvoc-core --all-targets` 全绿（含新增 3 个单元测试与 fixture 测试 ✔；93 passed ✔）。
**范围**：客户端本机编译/测试 ✔；**未发新命令、无写入、未用 GPU** ✔。**尚未接线** ✗：`core/native/macos_rm.c` 仍只读三条 block ⇒ 下一步是从该处发出 `0x20809061@776` 并把解析结果并入 `OffsetChannelSnapshot`/`SettingsCapabilities` ✔。

## 31. 接线：客户端封送层现在也读 `0x20809061@776`（2026-10-18）

**选择依据** ✔：§30 落地了解析器但**没有调用者** ✗（`core/native/macos_rm.c` 只发三条 block ✗）⇒ 原目标第 5 条"接入现有接口"未完成 ✔。本轮把**客户端路径**接到与探针相同的读面上 ✔（仍**只读**，写入门槛不变 ✔）。

### 31.1 改动（四处一致）
- `core/native/macos_rm_abi.h`：`NVOC_RM_OFFSET_KINDS 3→4`，新增 `NVOC_RM_OFFSET_CLOCK_DOCUMENT 3`（776 字节）✔。
- `core/native/macos_rm.c`：`offset_commands[]` 追加 `{0x20809061u, 776u}` ✔；注释由"three"改为"four"并保留"只能由 GSP 应答、wedge 时返回 `0x62`"✔。**只读**：无 SET id、无写入 ✔。
- `core/src/macos_rm.rs`：`OffsetBlock::ClockDocument`（kind 3 / bytes `CLK_DOCUMENT_BYTES`）✔；`read_offset_channel()` 增加第四次读取并调用 `parse_clock_document` ✔；`apply_offset_channel()` 把该 block 的结局写进同一条 note ✔，并**逐位列出**频率与当前偏移（例：`bit 0x10 at 6000984 kHz with offset -126 kHz`）✔。
- `core/src/macos_offset.rs`：`OffsetChannelSnapshot` 增加 `clock_document: OffsetBlockRead<ClkDocument>` ✔（沿用既有 `OffsetBlockRead` 三态：`Refused`/`Malformed`/`Parsed` ✔ ⇒ 拒绝**不会**被渲染成"偏移为零"✔）。
### 31.2 为什么不把偏移升成"具名域字段" ✔
`clock_offset_domains`/`clock_offset_limits_khz` 保持原样 ✔：776 文档里的域位是**驱动自己的**位，其中只有 `0x10` 能凭仓库内 NVAPI 副本命名为 MCLK（且其频率与实时显存时钟一致 ✔），其余十个位**没有名字** ✗ ⇒ 升格为具名域字段会**猜名字**，而 note 里逐位列值只报**实测值** ✔。
### 31.3 实际验证范围（重要）
- 客户端：`cargo fmt --all -- --check` 干净 ✔；`clippy --workspace --exclude cli-stressor-cuda-rs --all-targets -- -D warnings` 干净 ✔；`cargo test -p nvoc-core --all-targets` 全绿（93 passed ✔；含两个 `OffsetChannelSnapshot` fixture 构造点已随新字段更新 ✔）。
- **本机（Linux）不编译 C 也不编译 macOS 门控代码** ✗（`core/build.rs` 在非 macOS 直接返回 ✔）⇒ 回到**测试机**验证 ✔：`cargo build -p nvoc-core`（`OGKM=` 带上 ✔）build script 实际执行 ✔、crate 编译通过 ✔（3.48 s，无 error ✔）；另用 build.rs **同一组 include** 直接 `cc -std=c11 -fsyntax-only core/native/macos_rm.c` 通过 ✔。
- **覆盖前先核对**：测试机上四个文件改前 sha256 与 `b413f42^`（即第 54 轮提交态）**逐一相同** ✔（`macos_rm.rs b0492d91…`、`macos_rm.c 56217cae…`、`macos_rm_abi.h 169278c6…`、`macos_offset.rs 48fd7af3…`）⇒ 未覆盖任何新工作 ✔。
- **设备侧：未读** ✗ —— GSP 仍 `0x62`（uptime `1 day, 3:44`，同一 boot ✔）⇒ 新客户端路径状态 = **已实现待实测** ✔（`0x20809061@776` 这一**形态**被驱动接受这件事本身已在第 53 轮由探针实测 ✔，两者不混同 ✔）。
- 未执行任何写入 ✔，`SETTINGS_WRITE_SUPPORTED`/`SETTINGS_RESTORE_SUPPORTED` 仍为 `false` ✔。

## 32. 偏移进入公共接口：`clock_offsets` 与一处**过期断言**的更正（2026-10-18）

**选择依据** ✔：原目标第 5 条要求"接入现有公共 core/CLI/Python 接口"✗；第 31 轮只把偏移写进 note ✗ ⇒ 用户仍拿不到数值 ✔。同时发现一处**已经过期**的断言 ✗：`CLOCK_OFFSET_UNREADABLE_REASON` 仍写着"the offset itself is not reported as a number" ✗，而第 53 轮起该结论已被 `0x20809061` 的实测推翻 ✗ ⇒ 属必须更正的诚实性问题 ✔。

### 32.1 落地
- `core/src/macos_rm.rs`：新增公开类型 `ClockOffsetReading{domain_bit, frequency_khz, offset_khz}` ✔（含 `Serialize` ✔）与 `DeviceSettings.clock_offsets: Option<Vec<ClockOffsetReading>>` ✔，由 `apply_offset_channel` 从 `parse_clock_document` 的**实测载荷**填充 ✔；**拒绝/畸形时保持 `None`** ✗（不是空数组 ✗）⇒ "未读到" 永不渲染成"偏移为零" ✔。
- `core/src/lib.rs`：把 `ClockOffsetReading` 并入公开 re-export ✔。
- `nvoc-python/src/lib.rs`：`settings_macos_rm_to_map` 增加 `clock_offsets` 数组（`domain_bit`/`frequency_khz`/`offset_khz` ✔）。
- `cli`：`get-settings` 的 macOS 路径本来就是 `serde_json::to_value(QueryMacosRmSettings.output)` ✔（`cli/src/macos_rm.rs:47`）⇒ **无需新管线**，JSON 自动带上该字段 ✔；人读格式器是否显示该字段**尚未确认** ✗（见 32.3）。
### 32.2 域位保留而不换成名字 ✔
`clock_offsets` 同时给出 `domain_bit` 与 `frequency_khz` ✔：域位是**驱动自己的**位 ✔，仓库内只有 `0x10` 能凭 NVAPI 副本命名为 MCLK ✔，且其频率正是该会话实测的显存时钟 ✔ ⇒ 用名字替换位会把"名字"变成猜测，而频率是**实测** ✔。
### 32.3 明确保留的决定与未知 ✔
- `unavailable` 里的 `core_clock_offset`/`memory_clock_offset` **保留** ✗（**故意** ✔）：读虽已实现但**尚未在设备上实测** ✗（GSP 仍 wedge ✗），且这两族仍开放的问题是**旧的 `0x2080901B` 槽位身份**与"没有任何 SET"✗ ⇒ 原因字符串已改写为**只陈述这两点** ✔，不再说"偏移没被报出"✗。等设备实测答复后再决定是否**移除**这两个键 ✔（那时才可证伪/确认 ✔）。
- CLI **人读**输出是否显示 `clock_offsets` **尚未确认** ✗；JSON 路径已确认 ✔。
### 32.4 实际验证范围 ✔
- 客户端：`cargo fmt --all -- --check` 干净 ✔；`cargo clippy --workspace --exclude cli-stressor-cuda-rs --all-targets -- -D warnings` 干净 ✔；`cargo test -p nvoc-core --all-targets` **94 passed** ✔（新增一例：用**捕获的 776 载荷**填字段并断言 bit 1 = 1 530 000/−1、bit 0x10 = 6 000 984/−126 ✔，并断言 `Refused(0x62)` ⇒ `None` ✔ 且 note 同时含 `0x20809061` 与 `0x00000062` ✔）；`cargo check -p pynvoc` 通过 ✔（本机 pyo3 只做 check，不链接 ✔）。
- 测试机（macOS，`OGKM=` 带上）：`cargo build -p nvoc-core -p nvoc-cli` 通过 ✔ ⇒ macOS 门控代码与 **CLI 自身 macOS 路径**均编译 ✔；`nvapi-sys` 的 `unused variable: id` 警告为**既有** ✗、与本轮无关 ✔。
- 覆盖前核对 ✔：三个改动文件在测试机上的 sha256 与 `c3f24e1` 提交态**逐一相同** ✔ ⇒ 未覆盖新工作 ✔。
- **设备侧：本轮未读** ✗ —— 无重启，uptime `1 day, 3:48`（同一 boot ✔）；因此 `clock_offsets` 状态 = **已实现待实测** ✔。未执行任何写入 ✔，写入门槛保持关闭 ✔。

## 33. 人读输出验证：`clock_offsets` 能显示，但暴露出一处**真实的错渲染**（2026-10-18）

**选择依据** ✔：第 32 轮把"CLI 人读格式器是否显示 `clock_offsets`"记为**尚未确认** ✗。本轮先读代码确认 `format_get_settings_output` 是**按键泛化**的 ✔（对 `Value::Array` 走"标题＋子块"✔）⇒ 该问题可用**测试**而非设备答复 ✔。

### 33.1 用测试提问，结果发现缺陷 ✔
新增用例渲染一份 get-settings 形状的载荷（含 `clock_offsets` 两条记录 ✔）并断言字段与数值出现在人读输出里 ✔。**首次运行即失败** ✗，实际渲染：
```
  Clock Offsets
    Domain Bit: 0.001 MHz
    Frequency kHz: 1530 MHz
```
⇒ 根因定位到 `cli/src/output.rs` 的 `format_contextual_scalar` ✔：它对**上下文键**含 `clock`/`frequency` 的**所有**数值叶子做 kHz→MHz 缩放 ✗ ⇒ 对旁边的频率是对的 ✔，对**位索引**是错的 ✗（`domain_bit: 1` → `0.001 MHz` ✗）。
### 33.2 修复（最小、带语义理由）✔
新增一条**目标化豁免**：叶子名以 `_bit` 结尾者按原样数字渲染 ✔（"位永远不是频率" ✔）。该规则与文件既有的定向豁免风格一致（`value_key == "lanes"`、per-rail 瓦数、非 domain 的电压 ✔）。
**回归证据** ✔：`cargo test -p nvoc-cli` **82 passed / 0 failed** ✔，其余 81 例未变 ✔ ⇒ 既有已验收输出所依赖的缩放未被扰动 ✔；修复后渲染为 `Domain Bit: 1` ✔。
### 33.3 保留的已知渲染怪癖（记录，不改）✔
同上下文里 `offset_khz: -1` 渲染为 `-0.001 MHz` ✗：**数值正确**（偏移本就是频率差 ✔）但丢掉了字段名承诺的 kHz ✗ ⇒ 为保持本轮改动最小**未改** ✔，记为待决 ✔。
### 33.4 实际验证范围 ✔
客户端：`cargo fmt --all -- --check` 干净 ✔；`cargo test -p nvoc-cli` 82 passed ✔；`cargo test -p nvoc-core --all-targets` 94 passed ✔；`clippy --workspace --exclude cli-stressor-cuda-rs --all-targets -- -D warnings` 干净 ✔。测试机（`OGKM=`）：同步该文件后 `cargo build -p nvoc-cli` 通过 ✔（sha256 两侧一致 ✔）。
**设备访问**：本轮仅 wedge 轮询一次 ✔ —— verdict `0x00000062 NV_ERR_RESET_REQUIRED` ✔（探针原文：forward ALIVE 但 GSP 处于 reset-required；`0x2080b201` 仍 NV_OK，而所有拒绝都被折叠成 `0x62` ✔），uptime `1 day, 3:53`，同一 boot ⇒ **未重启、未写入、未做任何性能操作** ✔。
**状态**：第 32 轮遗留的"人读输出未确认"⇒ **已确认（有测试）** ✔；`clock_offsets` 在设备侧仍为 **已实现待实测** ✔。

## 34. 逐族只读审计（按原目标要求的"可读 / 可解释 / 范围 / 原状态 / 恢复"五段式，2026-10-18）

**选择依据** ✔：原目标把风扇/功耗/温度列为**必须分别调查**的次优先项 ✗，且要求每族独立给出五段证据与**剩余问题** ✔；此前 §28 是一张汇总表 ✗，缺"缺失层归属"与"证据边界" ✔。本轮**基于测试机已记录的文件**逐一核对 ✔（读文件 ✗ 非设备访问 ✔），并**更正一处与记录不符的旧断言** ✗（§28 风扇行 ✔）。

### 34.1 频率域偏移（最高优先通道）
- **可读** ✔：RUSD 实时频率（`graphics_clock_khz 1935000` / `memory_clock_khz 6000984`，`stage6-fan-read-20261017T060900Z/status-under-load.json` ✔）；偏移本身经 `0x20809061@776` 的 **776 字节时钟文档**实测 ✔（11 个 24 字节槽、域位＋频率＋**有符号**偏移 ✔，捕获见 `stage-clkdoc-20261017T074448Z/clkdoc-read.txt` ✔）。
- **可解释** ✔：槽位语义=域位/频率 kHz/偏移 kHz ✔；仓库内仅 `0x10` 可命名 MCLK ✔。
- **范围** ✔：`0x20809025` range 块给 **±18 750 kHz 偏移窗口** ✔（§29 ✔）。
- **原状态** ✔：重复读**逐字节一致** ✔。
- **恢复** ✗ **N/A——因为还没有 SET** ✔。
- **缺失层** ✗：**前置条件（写权限）＋客户端 SET 入口** ✔，四路独立支持：驱动源码的 legacy 权限判定 ✔、set 身份 `(cmd & 0xC000) == 0xC000` ✔、非特权调用实测 `0x1b` ✔、host 表 **0 条**特权行 ✔、`core/native/macos_rm.c` 内**没有任何 SET id** ✔、导出表把 `0x2080d01c` 命名为 `SetControl` ✔。⇒ 属**待授权** ✔，**不是**"只能固件逆向" ✗。

### 34.2 风扇
- **可读** ✔：`fan_rpm`/`fan_rpms`（RUSD 逐槽 ✔）——本批记录采样 **1610 / 1613 / 1616 / 1619 / 1833** ✔（`stage9-stress-20261017T041107Z/telemetry-before.jsonl` 与 `stage6-fan-read-20261017T060900Z/status-under-load.json` ✔）。
- **可解释** ✔：RPM 单值＝最高实时读数，逐槽列表跳过 0；单风扇卡二者一致 ✔。
- **范围** ✗：**没有声明式范围** ✗，只有**观察区间 1610–1833** ✔ ⇒ §28 那句"`fan 0 rpm`（空转）→…（0–1833 实测）"**已更正** ✔：`stage9-*` 的"前"采样是 **1613** 而非 0 ✗，0 rpm 只在更早一次空转探测里出现过、**未在本批记录中复现** ✗。
- **原状态** ✔：如上（1613，环境温度 44.4 °C、功耗 11.5 W、`graphics_clock_khz 1530000` ⇒ 当时**并非真空转** ✔）。
- **恢复** ✗ **N/A**（无 SET）。
- **缺失层** ✗：**公开 RM 读/写命令本身** —— 风扇百分比/模式/曲线/FanStop 都不在公开 opcode 内 ✔（`stage6-fan-read-20261017T060900Z/README.md` 的"缺口"节 ✔）⇒ 分类**未实现 / 待授权** ✔。
- **预订只读动作** ✔：wedge 解除后**在真正空转时重采一次** ✔，把"0 rpm"这一观察补成有据 ✔（若仍非 0，则"0"这一点作为**更早观察**保留、不与本批混写 ✗）。

### 34.3 温度
- **可读** ✔：公开 `NV2080_CTRL_CMD_THERMAL_SYSTEM_EXECUTE_V2 = 0x20800513` **NV_OK** ✔（`stage-thermal-read-20261017T061320Z/` 断言 6/6 ✔）。
- **可解释** ✔：1 target（type 1 = GPU）、1 sensor（target 0/provider 0）✔，读数与 RUSD `temperature_c` **四舍五入一致**（47 vs 47.4 ✔，采样间隔 <1 s ✔）。
- **范围** ✔ **声明式**：sensor range **[0,127]** ✔ ——**本批唯一的声明式范围** ✔（来自 OGKM 头文件、编译期权威 ✔）。
- **原状态** ✔：空转 44–47 °C ✔（跨记录一致 ✔）。
- **恢复** ✗ **N/A**：该命令的公开 opcode **全是 GET** ✔。
- **缺失层** ✗：**温度墙/目标的写面不在该命令的公开 opcode 内** ✔ ⇒ 写侧仍门控 ✔。

### 34.4 功耗
- **可读** ✔：RUSD 的 `power_mw 12798`、enforced `power_limit_mw 100000`、`power_limit_requested_mw 100000`、`power_policy_tgp_mw 12857` ✔（同 `status-under-load.json` ✔）。
- **可解释** ✔：enforced limit ≠ 可写范围 ≠ rated TDP ✔（`stage2-read-battery-20261017T062835Z/README.md` ✔）。
- **范围** ✗ **驱动明确拒绝**：`GET_EDPP_LIMIT_INFO (0x20800AFD)` → **`0x56 NV_ERR_NOT_SUPPORTED`** ✔ ⇒ 功耗墙 min/rated/max 窗口**读面不可用** ✔（`stage2-read-battery-*` 断言 ✔）。**这不是"尚未定位"** ✔，是 id 已知、结构在树、**驱动不实现** ✗。
- **原状态** ✔：同 34.2/34.3 的同批采样 ✔。
- **恢复** ✗ **N/A**：本轮及此前均**未跑任何 SET** ✔（`review-stage-reg-read-20261017.md:96` 明记 `--rated-tdp-set` 未在 GPU 上跑 ✔）。
- **剩余未知** ✗：write-only 的 `0x2080d024@66060` 用途 ✔（§26 ✔）。

### 34.5 `RATED_TDP`（独立候选）
- **可读** ⚠️：`rated-tdp-get-control (V2.4)` **返回 NV_OK** ✔，但**记录载荷只有输入回显** `raw[0..2]=0x00000000 0x00000000 0x00000000 ("client/input/vpstate echo")` ✗（`telemetry-ctl-map.jsonl:19` ✔）⇒ **尚未真正取得 rated TDP 数值** ✗ ⇒ 不能拿它充当偏移/VF 控制 ✔（原目标原话 ✔）。
- **可解释/范围/原状态/恢复** ✗：均**未取得**（因值本身没拿到 ✔）。
- **分类** ✔：**已实现待实测 / 调查未知** ✔；**可证伪的下一步** ✔：按导出声明尺寸重发、并检查请求侧结构是否缺字段（当前只回显 client/input/vpstate ⇒ 更像**调用帧不完整**而非驱动拒绝 ✗）✔。楔形期间该命令同样被折叠成 `0x62` ✔（`stage-clkctl-guard-20261017T094527Z/clkctl-read-guard.txt` ✔）。

### 34.6 顺带确认（压力侧前提）
`stage9-vulkan-20261017T041959Z` 与 `stage9-stress-20261017T041107Z` 记录了 **Vulkan-only 负载** ✔，`load.txt` 中 `nvrm: b81 PERF_BOOST flags 0x12 INFINITE`、`[VKGFX-H] …fps` 等横幅由 **NVK/nvrm 一侧**打印 ✔ ⇒ 原目标要求的"底层是 NVK、不要默认必须经 MoltenVK"这一前提**已有记录支持** ✔（非假设 ✔）。**边界** ✗：这是**压力负载**证据 ✔，**渲染成功 ≠ 超频稳定** ✗，故不能替代偏移设置成功 ✔。

## 35. 封送依据核对：公开树没有这些 id、两层结构不同、以及一处旧结论的更正（2026-10-18）

**选择依据** ✔：原目标第 2 条要求"从**实际封送逻辑**还原偏移 GET/SET"，且明确禁止用"头文件搜不到"当未知的理由 ✗。本轮把**仓库与树内已有材料**逐条核对 ✔，并**更正一处被反复引用的旧结论** ✗。

### 35.1 公开 OGKM 树里没有这些 id（是事实，但不是未知的理由）✔
`reference/open-gpu-kernel-modules-610.57.04` 全树 **无** `0x20809019 / 0x2080901b / 0x2080d01c` 任何字面量 ✗；`ctrl2080/` 下与本族相关的头文件是残片（`ctrl2080clk.h` **128 行**、`ctrl2080clkavfs.h` **36 行** ✔），`grep -rn "0x20809[0-9a-f][0-9a-f]"` 在整个 SDK ctrl 头里 **零命中** ✗ ⇒ 这些命令**不在开源头里** ✔。
但这**不构成"未知"** ✔，因为两处树内权威给出了封送依据 ✔：
- **GSP 导出表**（`tools/extract-gsp-method-table.py`，ELF64/RISC-V `gsp_tu10x.bin` ✔）给出**权威 paramSize**：`0x20809019=12336`、`0x2080901b=2108`、`0x2080d01c=2108`、`0x20809025=7188`、`0x20809061=776` ✔；
- **树内 NVAPI 副本**给出上层调用者结构与**逐字段**语义（§27/§29 ✔）。
### 35.2 两层结构不同——这正是"字段映射只能是带证伪的推断"的原因 ✔
树内注释（`probe-macos-rm/src/nvx_suite.c:1786-1790` ✔）与实测一致：NVAPI **调用者**层是 `0x964 = 2404`（`mask@+8`、records `@+100` stride `72` ✔），而 GSP 校验的 **RM 块**是 **`2108 = 60 + 32×64`**（`mask@+4` ✔）。
⇒ `probe` 早期把**调用者的 2404** 发给该 id 得到 `0x1f` ✔，**不是**"命令不存在"✗，而是**层错了** ✔。故把 NVAPI 的 `offsetKhz@+44` 映射到 RM 块的十二个值槽，**必然是推断** ✗ ⇒ 有名字的证伪条件就是**同字段回读** ✔（这正是 §31 起的 `clock_offsets` 走 776 文档、而不走该块的原因 ✔）。
### 35.3 写图（树内已记录，格式与实测一致）✔
`mask@+4=0xFF` + `0xA5` 背景 ⇒ **106/527 dwords 被改写** ✔；头 60 B **14 次写**（`+0` u8、`+8=0x01010000`、`+12..+56` 为 0 ✔）；每记录 **13 槽**：`rec+0` 类型戳 ＋ `rec+8,12,16,20,24,28,32,36,40,44,52,56` ✔；`rec+4/rec+48/rec+60` **从不写** ✔；掩码位→记录索引 **1:1** ✔；算术闭合 `106 = 14 + 7×13 + 1` ✔。
### 35.4 更正：INDEX 第 54 行"值 dword 全 0 ⇒ 本机是 V1 形态、不给 offset"
**该结论来自掩码放在 `+8` 的早期运行** ✗：`clkctl-read-first.txt:16,32,48` 与 `clkctl-read-mask-sweep-lite.txt:31,47,63` 三个掩码值（`0xFF`/`all`/`0`）**都**只回"**3 non-zero bytes, first @10 last @11**" ✔ ⇒ 掩码位置错时驱动几乎什么都不填 ✔。
而掩码放对（`+4`）的 `0xA5` 运行显示 **106/527 dwords 被改写** ✔（`clkctl-offsweep-run1-A5.txt:32` ✔）。
⇒ **"值槽从未被写"不成立** ✗；**"写入的值是零"也未被逐槽捕获证实** ✗（"changed" 只说明与 `0xA5` 不同 ✔，两者兼容 ✔）⇒ 现状应记为：**值槽确被写入，逐槽内容尚未捕获** ✔ ⇒ 需要下面这次**已有但从未跑过**的捕获 ✔。
### 35.5 预订动作（零风险、**不需要改代码**）✔
`probe-macos-rm/src/nvx_suite.c:2230` **已经有** `{ "domains-get-control 0x2080901B @2108 in@4=0xFF bg=A5", 0x2080901b, 2108, 4, 0xff, 0xa5a5a5a5 }` ✔（另有 `in@4=0x3FF` L2232、`in@4=0x1` L2234、`in@4=0` L2236 ✔），但**现有两份 hex 捕获里没有 2108 那一行** ✗（`clkctl-hex-0925.txt` / `clkctl-hex-recheck.txt` 只含 9019 与 9025 ✔）⇒ **该 case 从未被执行** ✗。
⇒ **下一步**：健康会话里跑一次 `--clkctl-hex`，即可逐槽读出十二个值 ✔，**仍不发任何 SET** ✔ ⇒ 这是把"哪个 dword 承载偏移"钉死的最短路径 ✔。
**顺带✔**：该表里的 `in@4=0x3FF` 已有**实测预测**可对照 ✔——`0x20809019` 目录解出 **10 行**（`rows-as-bits 0x3ff` ✔），行序 id 为 **0,1,4,2,3,20,19,6,21,5** ✔，而 `0xFF` 只到 **行 0..7** ✔ ⇒ 行 8（id 21 MSD）/行 9（id 5 HOST）此前**未被任何掩码读到** ✔。

## 36. 阶段 10 的第一步：macOS/非 CUDA 构建适配（含一处自造测试缺陷的修复）2026-10-18

**选择依据** ✔：原目标阶段 10 的首行是"**macOS optimizer/非 CUDA 构建适配**"✗，并写明"缺少条件继续所有独立代码/测试，不以 dry-run 宣称实测完成"✔；本轮据此做**不依赖 GPU** 的构建层工作 ✔。

### 36.1 缺口（实测）✗
`auto-optimizer/Cargo.toml` 原先的三个 bundling feature **都含 CUDA**：`stressor-bundled = ["dep:cli-stressor-cuda-rs","cli-stressor-cuda-rs/cuda","cli-stressor-cuda-rs/vulkan"]`、`stressor-bundled-cuda11/12`，而 `default = ["stressor-bundled-cuda12"]` ✔。macOS 既无 NVAPI/NVML 也无 CUDA 运行时 ⇒ **在 macOS 上只能 `--no-default-features`，等于完全不捆绑 stressor** ✗ ⇒ 阶段 10 要求的"非 CUDA 构建适配"缺一条可用路径 ✔。
### 36.2 改法（保留原默认 feature，不改任何 cfg）✔
把"是否在进程内捆绑 stressor"与"用哪个后端"分开：
`stressor-bundled = ["dep:cli-stressor-cuda-rs"]`（源码里所有 `#[cfg(feature = "stressor-bundled")]` 门都不变 ✔）、`stressor-bundled-cuda = ["stressor-bundled","cli-stressor-cuda-rs/cuda","cli-stressor-cuda-rs/vulkan"]`、`stressor-bundled-cuda11/12` 基于它、新增 **`stressor-bundled-vulkan = ["stressor-bundled","cli-stressor-cuda-rs/vulkan"]`** ✔ ⇒ `default` 解析出的 feature 集合与改动前**完全相同**（cuda＋vulkan ✔），无任何既有构建行为改变 ✔。
### 36.3 验证（本机分析侧 ＋ 真机 macOS）✔
| 检查 | 结果 |
|---|---|
| `cargo metadata`（feature 引用合法） | rc 0 ✔ |
| `cargo check -p nvoc-auto-optimizer --no-default-features --features stressor-bundled-vulkan`（Linux） | rc 0 ✔，`cargo tree` 中 **`cudarc` 命中数 = 0** ✔ |
| `cargo check -p nvoc-auto-optimizer --no-default-features --features stressor-external` | rc 0 ✔ |
| `cargo check -p nvoc-auto-optimizer`（默认，cuda＋vulkan） | rc 0 ✔（未回归 ✔） |
| `cargo fmt --all -- --check` / `cargo clippy -p nvoc-auto-optimizer --all-targets -- -D warnings` | rc 0 ✔ |
| `cargo test -p nvoc-auto-optimizer --bin nvoc-auto-optimizer`（默认 / vulkan-only 两路） | **66 passed, 0 failed** ✔✔ |
| **真机 macOS**：`OGKM=… cargo test -p nvoc-auto-optimizer --no-default-features --features stressor-bundled-vulkan --bin nvoc-auto-optimizer` | **65 passed, 0 failed** ✔✔（含 `macos_write_gate_follows_the_backend_capability ... ok` ✔） |
⇒ macOS 上**非 CUDA 捆绑 stressor 的构建链已实测可用** ✔；**该 stressor 的 Vulkan 实际运行仍为"已实现待实测"**（wedge 未解除，本轮未跑 ✗）。
### 36.4 顺带修复：我自造的写入门控测试缺陷 ✗
`auto-optimizer/src/main.rs` 的 `macos_write_gate_follows_the_backend_capability`（由**本分支 `9ff687f`（我自己的提交）**引入 ✗）断言 `refused == mutates`，而 `refused = cfg!(target_os="macos") && mutates && !write_supported` ✗ ⇒ **非 macOS 主机上 `refused` 恒为 false**，断言必然失败（本机 Linux 上三种 feature 路都复现 ✗），尽管注释自称"在其他 target 上是空转的" ✗；而在 macOS 上该断言又退化为**同义反复** ✗（由公式本身构造出相等 ✗）⇒ 两头都没证明什么 ✔。
修法 ✔：门控无法生效时**提前返回** ✔；先钉住"被固定的命令集合中至少有一条会改设置"（防止命令表过期导致空转通过 ✔）；在 macOS 上改为断言**非平凡事实**——四条命令都要求提权 ✔。真机 macOS 上该测试**真实执行并通过** ✔（见 36.3 表 ✔）。
**设备访问**：本轮仅 wedge 轮询一次（`0x00000062`，uptime `1 day, 4 hrs`）；真机那次 `cargo test` 是**纯单元测试、不访问设备** ✔ ⇒ 无写入、无性能操作 ✔。

## 37. `RATED_TDP` 请求结构核对：不是"缺字段"，是**种子与预期答案同值**（2026-10-18）

**选择依据** ✔：功能表把 `RATED_TDP` 记为"命令 NV_OK 但载荷仅输入回显 ⇒ 尚未取得数值"，并写明下一步＝"重发＋**查请求结构是否缺字段**"✔；本轮就把这句话查清 ✔（纯静态＋真机编译 ✔，不新增设备访问 ✗）。

### 37.1 这条命令**在公开树里有声明**（与 CLK 族相反，所以必须逐条查而不是一概而论）✔
- `src/common/sdk/nvidia/inc/ctrl/ctrl2080/ctrl2080perf.h:399`：`#define NV2080_CTRL_CMD_PERF_RATED_TDP_GET_CONTROL (0x2080206e)` ✔；
- 同文件 `:405-418` 给出权威结构 ✔：`typedef struct NV2080_CTRL_PERF_RATED_TDP_CONTROL_PARAMS { NV2080_CTRL_PERF_RATED_TDP_CLIENT client; /*[in]*/ NV2080_CTRL_PERF_RATED_TDP_ACTION input; /*[in/out]*/ NV2080_CTRL_PERF_RATED_TDP_VPSTATE_TYPE vPstateType; /*[in]*/ }` ✔，命令文档为"This command **retrieves the current requested RATED_TDP action** corresponding to the specified client" ✔ ⇒ **唯一输出字段是 `input`** ✔；
- 该 id 也出现在驱动自带的分发表 `src/nvidia/generated/g_subdevice_nvoc.c:11139` ✔（`methodId=0x2080206e` ✔）；
- 动作枚举 ✔：`DEFAULT=0 / FORCE_EXCEED=1 / FORCE_LIMIT=2 / FORCE_LOCK=3 / FORCE_FLOOR=4` ✔；`STATUS_PARAMS_MESSAGE_ID = 0x6D` ⇒ status 命令 `0x2080206d` ✔（探针**早已发送** ✔）。
⇒ 结论 ✔：请求**没有缺字段**（三个声明字段都在 ✔、12 B 即整结构 ✔）；缺的是**能区分"真输出"与"回显"的种子** ✗。
### 37.2 真正的混淆：`memset` 种子 = `0` = `ACTION_DEFAULT`
探针两处都用 `memset(&p, 0, sizeof p)` 再只填 `client` ✔（`nvx_suite.c:512-514` ✔、`:731-734` ✔）⇒ 种子 `input = 0 = ACTION_DEFAULT` ✔ —— 而**未被强制限速的 client 的正确答案本来就是 0** ✗ ⇒ 输出串里那句 `(client/input/vpstate echo)`（`:517` ✔）以及功能表"仅输入回显"都是**假设，不是测量** ✗。这正是本轮要纠的错 ✔。
### 37.3 已加入的**可证伪**只读用例（预注册预测）✔
在 `item_rated_tdp_reads` 内、零种子那一遍之后，**保留原遍**并**新增**一遍 ✔：对每个 client 以 `input = FORCE_LIMIT(2)` 播种 ✔，并把结果分类打印 ✔。
**预注册预测**（写进代码注释，先于运行登记 ✔）：若驱动真的填当前动作，则**上一遍答 `DEFAULT(0)` 的 client 在这里必须仍答 0** ✔；若这些 client 回来**恰好是 2**，则该字段确为回显 ✔；因为两遍在同一会话内，这同时排除了"所有 client 恰好都是 FORCE_LIMIT"的解释 ✔。零种子一遍因此**不是冗余**，而是这次判断的对照 ✔。
**只读性** ✔：`GET_CONTROL` 既不做仲裁也不 latch；SET 孪生 `0x2080206f` **仍未发送** ✗ ✔。
### 37.4 验证范围与状态 ✔
- 真机 macOS 编译 ✔：`OGKM=… sh build.sh` ⇒ **`built probe-macos-rm-idle (profile idle)`** ✔（按交接文档 §7.0 只认这一行 ✔）。
- **未运行** ✗：GSP 仍 `0x62` ✔（本轮仅一次 wedge 轮询：`0x00000062`，uptime `1 day, 4:13`，同一 boot ✔）⇒ 新用例属**已实现待实测** ✔。
- 分类 ✔：结构/字段/枚举＝**已定位（公开头文件直接给出 ✔，非推断）**；各 client 当前动作与 RATED_TDP 数值＝**已实现待实测**；写＝**待授权** ✔。

## 38. 压力侧就绪度核对：Vulkan 非 CUDA 路径的构建与入口契约（2026-10-18）

**选择依据** ✔：次优先级里明确要求"压力侧先复用原 NVOC 的 **Vulkan 压力实现**（`cli-stressor-cuda-rs --vulkan-only`，底层就是 NVK），**wedge 解除后**才跑"✔ ⇒ 本轮把**跑之前必须先成立的条件**逐条核实 ✔（纯构建/参数面 ✔，不加载 GPU ✗）。

### 38.1 构建可行性 ✔
- 本机（分析侧）`CARGO_NET_OFFLINE=true cargo check -p cli-stressor-cuda-rs --no-default-features --features vulkan` ⇒ **rc 0（20.79 s）** ✔（`ash 0.38.0+1.3.281`、`naga 30.0.1` 均已在 `Cargo.lock` 内 ✔）。
- 测试机 macOS ✔：`cargo build -p cli-stressor-cuda-rs --no-default-features --features vulkan` ⇒ **`Finished` 且未触发重编** ⇒ 现存 `target/debug/cli-stressor-cuda-rs`（10 885 680 B，**10 月 17 日 10:10**，即第 9 阶段那批 ✔）**已经就是这套 feature 的产物** ✔。
- **独立佐证**（cargo 会按 feature 集合区分指纹 ✔，故"未重编"是有意义的 ✔）：`target/debug/.fingerprint/` 下同时存在 `cli-stressor-cuda-rs-0633834fc92c7fa1`＝`features: ["ash","vulkan"]` ✔、`074330bd1b3bdef2`/`78a309eeaac19e51`＝`["ash","cuda","cuda12","default","vulkan"]` ✔、`1a1cd0f24d19e1b4`＝`["ash","cuda","cuda12","vulkan"]` ✔ ⇒ **无 CUDA 的那套确实在本机构建过且与默认那套分开记账** ✔。
### 38.2 入口契约（怎么被优化器驱动）✔
- `auto-optimizer/src/macos_eval.rs:63-66`：`--vulkan-stressor <path>` 是**硬性要求**（缺失即报错 ✔）；`:180-187`：`Command::new(&opts.stressor).arg("--vulkan-only")` 再按需追加 `--vulkan-width/height/shells/iters` ✔。
- `cli-stressor-cuda-rs` 参数面实测（`--help`，仅打印参数、**不加载设备** ✗）✔：`--vulkan-only`＝"Run Vulkan-only stress (skip CUDA workload)" ✔、`--vulkan`＝FurMark 式图形负载（默认离屏 target，除非 `--vulkan-window` ✔）✔；未以 `vulkan` feature 构建时 `--vulkan-only` 会给出明确提示 ✔（`main.rs:1315` ✔）。
- 因此"bundled"的真义 ✗：不是进程内线程 ✔，而是 `stressor_process.rs:10,67-93` 的 `is_bundled`／`bundled_command` ⇒ **把路径解析为 `current_exe()` 并以 worker 哨兵 argv 重新调起自身** ✔ ⇒ 压力仍以**子进程**运行 ✔（同一可执行文件、无需另找一个二进制 ✔）。
### 38.3 更正我自己的措辞 ✗
第 60 轮我在 `auto-optimizer/Cargo.toml` 的 feature 注释里写了"bundle the stressor crate **in-process**"× ⇒ 已按上述真实机制改写为"link the stressor crate into this binary … re-invokes it with a worker sentinel in argv, so the stress still runs as a child process" ✔。
### 38.4 状态与设备访问 ✔
- 分类 ✔：压力侧**构建/入口就绪＝已实现且实测（构建层）**；**实际负载运行＝已实现待实测**（wedge ✗）；Metal 校验器仍**未跑** ✗。
- 本节**未跑任何负载** ✗（`cargo build` 与 `--help` 均不访问设备 ✔）。
- 设备访问 ✔：本轮仅一次 wedge 轮询（verdict `0x00000062`，uptime `1 day, 4:16`，同一 boot ✔）⇒ 未重启、未写入、无性能操作 ✔。

## 39. 服务入口能力核对：`srv` 把写能力硬编码成了与后端相反的结论（阶段 11 第 5 条）2026-10-18

**选择依据** ✔：阶段 11 第 5 条要求"**各入口能力、范围和错误与后端一致**，既不能后端可用而入口一律拒绝，也不能可点但无实现"✔ ⇒ 本轮逐个入口核对，发现服务入口与后端能力**不一致** ✗。

### 39.1 缺口（实测）✗
- `srv/src/macos.rs` 的 `supports_writes()` **硬编码 `false`** ✗（原 L134 ✔），而同一能力在 `core` 侧有唯一常量 `nvoc_core::macos_rm::SETTINGS_WRITE_SUPPORTED` ✔ —— 优化器写入门控在该常量上（`9ff687f` ✔），**服务入口没有** ✗ ⇒ 两个入口可以各自漂移 ✔。
- 该方法的消费方实际决定用户可见行为 ✔：`srv/src/http.rs:488`（能力上报 ✔）与 `srv/src/runtime.rs:145,171`（是否启用控制闭环 ✔）都直接读它 ⇒ 漂移会变成对外可见的不一致 ✔。
- 模块文档两处已被实测推翻 ✗：①"the public 610.57.04 RM control surface has **no** clock/voltage offset, lock, fan or thermal-policy **write** command"× —— 写命令**存在** ✔（`0x2080d01c` 是 `0x2080901b` 的孪生、且是 set 身份 `(cmd & 0xC000) == 0xC000` ✔）；②"the private GETs … return `0x1f` or `0x56`"× —— 那是**早期尺寸猜测**的应答 ✔，驱动只接受其导出表声明的尺寸 ✔，`0x20809061@776` 就是被接受并发布了逐域偏移的那一个 ✔。
- 这正是原目标明令禁止的表述错误 ✗："root GET 权限不足**不得伪装成命令不存在**" ✔。

### 39.2 修法 ✔
- `supports_writes()` 改为返回 `nvoc_core::macos_rm::SETTINGS_WRITE_SUPPORTED` ✔（今天该常量为 false ✔ ⇒ **行为不变** ✔，但两入口从此同源 ✔）。
- 共享拒绝理由 `READ_ONLY` 改为点名真正缺失的两层 ✔：写路径 `0x2080d01c` **存在** ✔，但 (a) 需要本客户端没有的权限 ✔（非特权调用方实测 `NV_ERR_INSUFFICIENT_PERMISSIONS 0x1b` ✔）；(b) 原生层不发任何 SET id ✔。
- `read_offset_mhz` 的理由不再宣称"没有可读的偏移命令"✗：偏移**可读** ✔（`nvoc_core::macos_rm::DeviceSettings::clock_offsets`，来自 `0x20809061` 时钟文档 ✔），只是文档的域位尚未全部映射到 `OffsetDomain` 名字 ✔ ⇒ 按域取数会编码未经验证的假设，故如实说明 ✔。
- `read_vf_curve` 改为"**not located yet**"✗（原来写"driver 上没有可读 V/F 控制"× —— 准确的表述是**尚未定位** ✔）。
### 39.3 测试与验证范围 ✔
- 新增 `the_shared_reason_names_the_missing_layers_not_a_missing_command` ✔：钉住理由中含 `0x2080d01c` 与 `privilege`、且**不含** `"no writable RM control"` ✔ ⇒ 防止"命令不存在"的说法回流 ✔。
- 既有 `every_actuator_fails_closed_with_the_shared_reason` 中 `assert!(!supports_writes())` 改为**与常量相等** ✔ ⇒ 本地副本无法再漂移 ✔。
- 实测 ✔：Linux `cargo fmt --all -- --check` rc 0 ✔；`cargo clippy -p nvoc-srv --all-targets -- -D warnings` **rc 0** ✔；`cargo test -p nvoc-srv` ⇒ **53 passed, 0 failed** ✔；**测试机 macOS** `OGKM=… cargo test -p nvoc-srv` ⇒ **53 passed, 0 failed**（与 Linux 同数 ✔）✔。
- 已知且**未动** ✗：`nvapi-rs/sys/src/nvapi.rs:87` 在 macOS 目标上有 `unused variable: id` 警告 ✔ —— 属**vendored 依赖**、非本次改动 ✔ ⇒ 按"不覆盖他人工作"保留原样 ✔，仅记录 ✔。
### 39.4 设备访问 ✔
本轮仅一次 wedge 轮询（`0x00000062`，uptime `1 day, 4:19`，同一 boot ✔）；该 crate 的单测不访问设备 ✔（`macos.rs` 的测试以空 GPU 列表构造后端 ✔）⇒ 未重启、未写入、无性能操作 ✔。

## 40. 两张 `flags` 表是两个命名空间；`0x2080a06e` 的定位推进与可证伪用例（2026-10-18）

**选择依据** ✔：纠错表上"`0x2080a06e`@12 flags `0x48` 与 GET 同尺寸但 flag 类别不同 ⇒ **是否同命令尚未定位**"与"flag 位 `0x40000` 含义尚未定位"两条一直挂着 ✔；本轮用**驱动自带的生成分发表**（此前没用过的证据源 ✔）推进它们 ✔。

### 40.1 纠错：两张表的 `flags` **不是同一个命名空间** ✗
| 命令 | 驱动自带分发表 `src/nvidia/generated/g_subdevice_nvoc.c` | GSP 导出表（`gsp-families-610.57.04.txt`） |
|---|---|---|
| `0x2080206e` | `flags=0x4a`、`accessRight=0x0`、`subdeviceCtrlCmdPerfRatedTdpGetControl`、`paramSize=sizeof(NV2080_CTRL_PERF_RATED_TDP_CONTROL_PARAMS)` | `0x00010008` ✔ |
| `0x2080206f` | `flags=0x10008`、`accessRight=0x0`、`subdeviceCtrlCmdPerfRatedTdpSetControl`、同尺寸 | `0x00040048` ✔ |
⇒ 两者**每个 id 都对不上**，而且**取值交叉**（分发表里 SET 的 `0x10008` 正是导出表里 GET 的 `0x10008`）✗ ⇒ **两张表的 `flags` 分属不同命名空间** ✔ ⇒ 任何"按 flag 推断特权/接口"的结论都必须写明用的是哪张表 ✔（这是新增的一条纠错表条目 ✔）。
### 40.2 驱动自带分发表作为证据源：可用，但**只覆盖公开半区** ✔
- 规模实测 ✔：`grep -c "methodId=*/"` ⇒ **641** 行 ✔；`accessRight` 取值分布 ＝ **640×`0x0` ＋ 1×`0x2`** ⇒ 该字段在此表里**基本未用** ⇒ **不是特权编码** ✔（特权在别处，见 40.1 的命名空间警告 ✔）。
- 它能给的东西很值钱 ✔：`methodId`、**处理函数名**、`paramSize = sizeof(...)`、`accessRight`、`flags` ✔ ⇒ 对公开半区的 id，可以直接读出"谁实现、要多大参数" ✔。
- **但**：`0x2080901b`、`0x2080d01c`、`0x2080a06e` 在这张表里**完全不存在** ✗（大小写不敏感检索 ✔，`0x2080a0xx` 一个都没有 ✔）⇒ 该表只覆盖公开半区 ⇒ **它里面的缺席不构成"命令不存在"的证据** ✔ ✔（与"头文件搜不到 ≠ 不存在"同一条纪律 ✔）。
### 40.3 `0x2080a06e`：定位推进 ✔＋一处尺寸不对称 ✗
- 实测记录 ✔：PERF 族、**12 字节**、`flags 0x48`、孪生 `0x2080e06f` 是 **8 字节的 SET** ✔；其 message id **`0x6e` 与公开 GET `0x2080206e` 完全相同** ✔。
- 孪生图里这条本就标着 **"NO / SIZE DIFFERS - resolve SET by its own row"** ✔ ⇒ **GET 12 ／ SET 8 的尺寸不对称**意味着 40.1 意义上"同一条控制消息的另一份声明"，而**不是**字节同构的孪生 ✔。
- **状态：尚未定位**（身份）✗，但**已定位到具体比较对象** ✔ ⇒ 不再是无依据扫描 ✔。
- **已加入可证伪只读用例**（`--rated-tdp-read`，先于运行登记预测 ✔）：以**其自身声明的 12 字节**发送 `0x2080a06e`、`client=RM`、`input=FORCE_LIMIT(2)` ✔。预测 ✔：若是同一条 get-control 的非特权声明 ⇒ 返回 **NV_OK** 且三元组与 `0x2080206e` 相同 ✔；`0x1f` ⇒ "驱动只接受其导出表声明的尺寸"这条规则在此不成立 ✔；`0x56` ⇒ 该驱动把这一行编译掉了 ✔。**SET 孪生 `0x2080e06f` 仍未发送** ✗。
### 40.4 验证范围 ✔
- 真机 macOS 编译 ✔：`OGKM=… sh build.sh` ⇒ **`built probe-macos-rm-idle (profile idle)`** ✔。
- **未运行** ✗（GSP 仍 `0x62` ✔）⇒ 新用例＝**已实现待实测** ✔。
- 设备访问 ✔：本轮仅一次 wedge 轮询（`0x00000062`，uptime `1 day, 4:24`，同一 boot ✔）；真机编译与源码检索均不访问设备 ✔。

## 41. wedge 是**间歇性**的：首次观测到恢复窗口与正确的健康判据（2026-10-18）

**选择依据** ✔：原目标要求"轮询要非阻塞""不要因外部条件标 blocked"✔；本轮轮询出现**新观测** ⇒ 足以修正此前"楔形是硬锁存、只能等重启"的操作假设 ✗。

### 41.1 实测：恢复窗口真实存在，但很短 ✗
- 本机 macOS 本地时间 **11:46** ✔：`./probe-macos-rm-idle --anchor-read` 的 **`anchor verdict`** 行返回 **`status 0x00000000`（NV_OK）** ✔ —— 楔形开始以来首次健康观测 ✔。
- 数秒后同一命令（runbook 内部首例）⇒ `0x2080901b@2108 in@4=0xFF bg=A5` 返回 `0x00000062` ✗，且同一 bundle 内 `fb-get-info-v2`／`fb-stats-get`／`perf-current-pstate`／`rated-tdp-get-control`／`gpumon`／`pmumon`／`rusd-page`／`perf-level-info-v2` 全 `0x62` ✗（无关状态的 `bios-sku-info`／`bus-get-pci-info` 仍 `NV_OK` ✔）。
- 再三次**门控重试**（间隔 15 s）⇒ verdict 均 `0x62` ✗。
⇒ 结论 ✔：**楔形不是硬锁存** ✔；GSP 会自行回到 NV_OK 然后又很快回到 reset-required ✗ ⇒ 策略从"等重启"改为"**用门控重试抓窗口**"✔（重启仍由用户执行，不自动重启 ✔）。
### 41.2 判据行纠正 ✗（我自己踩过一次）
- 健康判据只有 **`anchor verdict`** 行的状态 ✔（`0x00000000` ＝ 健康 ✔ ／ `0x00000062` ＝ 楔形 ✗）。
- `anchor host size gate` 行**恒为 `0x1f`** ✔ —— 它自述 "the host NVOC paramSize gate is alive and **GPU-state independent**" ✔ ⇒ 用 `grep -o "status 0x…" | head -1` 抓第一处状态会读到**这一行** ✗ ⇒ 必须按**行名**检索 ✔ ✔。
### 41.3 可用配方（避开窗口错过）✔
- 同一次 ssh 调用内：先读 `anchor verdict` ✔ → 仅当 `0x00000000` 才执行 `tools/clock-offset-read-run.sh` ✔ ⇒ 两次独立 ssh 之间隔数秒，正是本轮错过窗口的原因 ✗。
- runbook **自身也有守门** ✔：首例见 `0x62`/`0x65` 即停，并声明 "Reboot, then rerun; **nothing was written**" ✔ ⇒ 只读语义成立 ✔。
### 41.4 顺带复核导出表 capture（降级一条口径）✗
- 本次复核的 dump 实得 **1081 行** ✗，与交接文档记载的 **1118 行**不一致（差 37 行 ⇒ 待核 ✔）⇒ 引用行数时必须写明是"工具声称"还是"实得 dump 行数"✔。
- `flags & 0x4000`（bit14）命中的行 ＝ **19 行** ✔，而该 capture 的 `GET/SET` 列把这 19 行**全标成 GET** ✗；但 `0x2080206f` 的 flags `0x00040048` 明明带 bit14 ✗ ⇒ **该列不是方向的可信来源** ✗ ⇒ 纠错：方向只认逐条证据（生成分发表的处理函数名如 `...SetControl` ✔，或 set 身份 `(cmd & 0xC000) == 0xC000` ✔）；该 capture 里 94 行 `SET` 同样**降级为线索** ✔。
### 41.5 设备访问 ✔
本轮设备调用 ✔：轮询 1 次（`0x62` ✔）＋诊断 1 次 ✔＋门控重试 3 次（各 1 次 anchor ✔）＋后台门控重试（每 60 s 一次 anchor，命中即跑只读 battery ✔）⇒ **全部只读** ✔，未重启、未写入、无性能 SET ✔。

## 42. 私有 id 空间的结构由**仓库内材料**直接给出：接口字节 + 消息 id，写半边是「+0x40 接口字节、+1 消息 id」（304/304 实测）2026-10-18

**选择依据** ✔：原目标要求"从**实际封送逻辑**还原偏移 GET/SET""不得用'头文件搜不到'代替对仓库已有材料的核对"✔；第 41 轮把"某 capture 的 `GET/SET` 列"降级后又发现代码里流传着一条**算术上不成立**的孪生等式 ✗ ⇒ 本轮据仓库内材料与实测把它彻底定清 ✔。
### 42.1 编码约定是 SDK 自己写出来的 ✔
- `reference/open-gpu-kernel-modules-610.57.04/src/common/sdk/nvidia/inc/ctrl/ctrl2080/ctrl2080thermal.h:136-137` 相邻两行 ✔：
  `NV2080_CTRL_CMD_THERMAL_SYSTEM_EXECUTE_V2 (0x20800513U)` ← `(…_THERMAL_INTERFACE_ID << 8) | …_MESSAGE_ID` ／ `NV2080_CTRL_CMD_THERMAL_SYSTEM_EXECUTE_V2_PHYSICAL (0x20808513U)` ← `(…_THERMAL_LEGACY_NON_PRIVILEGED_INTERFACE_ID << 8) | 同 一 MESSAGE_ID` ✔ ✔
  ⇒ 同一条消息 id、两个接口字节（`0x05` ／ `0x85`）✔ ⇒ **接口字节的 bit7 ＝ legacy non-privileged 别名** ✔（且该结构与 `probe-macos-rm/tools/summarise-method-table.py` 自带文档"a 0x80-0xBF category byte is the legacy **non-privileged** read half"完全一致 ✔）。
- 同一目录下 `ctrl2080clk.h`／`ctrl2080clkavfs.h`／`ctrl2080fan.h`／`ctrl2080thermal.h` 都在 ✔（我早前几次检索用错了目录层级 ✗ ⇒ 已纠正 ✔）；但 `0x2080901b`／`0x2080d01c` 以及全部实测 THERMAL id **在这些头文件里都没有声明** ✗ ⇒ 公开树只给出**约定与公开半区** ✔，私有一族的名字仍属尚未定位 ✔ ✔。
### 42.2 写半边规则：不是 `id | 0x4000`，而是**接口字节 +0x40 且消息 id +1** ✔
- 实测（`gsp-families-610.57.04.txt`，本机复核 ✔）：read 半区（字节 `0x80-0xBF`）共 **304** 行 ✔，其中 **304 行（100%）** 的孪生列指向写半区（字节 `0xC0-0xFF`）且低位 **+1** ✔；低位**相同**的 `|0x4000` 形式命中 **0 行** ✗ ✔；write 半区 75 行，**75/75** 都指回 read 半区 ✔。
- 四族一致 ✔：THERMAL `0x2080852c → 0x2080c52d` ✔、THERMAL `0x20808540 → 0x2080c541` ✔、CLK 偏移 `0x2080901b → 0x2080d01c` ✔、PERF `0x2080a06e → 0x2080e06f` ✔ ⇒ 规则 ✔：**写 id ＝ 读 id 的接口字节 +0x40（bit6）、消息 id +1** ✔。
### 42.3 纠错：三处**面向用户的理由串**里那条等式是**算术错的** ✗
- 原文：`0x2080D01C = 0x2080901B | 0x4000` ✗ —— 实际 `0x2080901B | 0x4000 = 0x2080D01B` ✗（消息 id 没进一）⇒ 与孪生列记的 `0x2080D01C` 差 1 ✔。
- 已修 ✔：`core/src/macos_rm.rs` 的 `WRITE_NEEDS_PRIVILEGE_REASON`（L122）与写能力理由（L364）、`cli/src/macos_rm.rs` 的拒绝文案（L20）三处均改为按 42.2 的口径叙述 ✔（结论不变：0xC0-0xFF 的写半边在 root 以下被 host forwarder 以 `0x1b` 拒 ✔ —— 该结论的依据是**实测 `0x1b`** 与 host 表无特权行 ✔，不依赖原等式 ✔）。
- 工具同修 ✔：`probe-macos-rm/tools/summarise-method-table.py` 的孪生枚举原来只试 `mid | 0x4000` ✗ ⇒ 现改为**先试相邻 id、再试裸形式** ✔ 并在文档串与输出行里写明 ✔（在本机对 `gsp-table-full.tsv` 运行 ⇒ 71 对、其中同尺寸 43 ✔）。
### 42.4 行数卫生（三个 artifact、三个数）✗
- `nvoc-analysis/gsp-table.tsv`／`gsp-table-full.tsv` ⇒ **890 行**（工具报 889 行有效 ✔）；`gsp-families-610.57.04.txt` ⇒ **1081 行**；交接文档记 **1118 行** ✗ ⇒ 三者是**不同 artifact** ✔。
- 且 families capture **可证不完整** ✗：304 个 read 行的孪生指向写 id，但该文件里只有 **75** 个写行存在 ✔ ⇒ 引用"表的行数"必须同时写明**哪个文件** ✔ ✔。
### 42.5 验证范围 ✔
- 本机（客户端分析，不构建 ✔）：python3 跑修好的工具 ✔；awk 复核 304/304 与 0/304 ✔。
- Rust 侧 ✔：`cargo fmt --all -- --check` rc 0 ✔；`cargo test -p nvoc-core -p nvoc-cli -p nvoc-srv` ⇒ **82／94／53 passed，0 failed**（GPU 相关 36＋8 ignored ✔）✔。
- 设备访问 ✔：仅后台门控重试（每 60 s 一次 anchor 只读读 ✔，本轮 attempt 1-4 皆 `0x62` ✗）⇒ 未重启、未写入、无性能 SET ✔。

## 43. 尺寸列＝`sizeof(PARAMS)`：用开放头文件的编译尺寸**校准**导出表，并按此命名可命名的行（2026-10-18）

**选择依据** ✔：原目标要求"从**实际封送逻辑**还原""不得用'头文件搜不到'代替核对"✔。第 42 轮定清了 id 结构，本轮把**尺寸列**也校准到 SDK 类型上，使"按尺寸命名命令"成为有依据的动作而不是猜 ✔。
### 43.1 校准（真机编译测得，非推断）✔
- 真机命令（`nvoc-head`，只读、不碰设备 ✔）：写一个只 `#include <ctrl/ctrl2080/ctrl2080*.h>` 的小 C 程序打印 `sizeof`，用 `cc -I$OGKM/src/common/sdk/nvidia/inc` 编译运行 ✔。
- 结果 ✔：`sizeof(NV2080_CTRL_THERMAL_SYSTEM_EXECUTE_V2_PARAMS) = 1432` ✔ 而导出表实测 `0x20800513` ＝ **1432**、其 legacy 别名 `0x20808513` 也 ＝ **1432** ✔ ⇒ **导出表的 size 列就是 `sizeof(PARAMS)`** ✔ ✔（两条独立路径一致 ✔）；也印证头文件"同一 MESSAGE_ID、两个接口 id"⇒ 别名与公开同胞**同结构同尺寸** ✔。
- 由此确立互校法 ✔：SDK／生成表给出的 `paramSize` 与实测 size 可互相校验；**同尺寸不唯一**（12／24／56／8／4 在多个族里大量重复 ✗）⇒ 尺寸是**佐证**，命名仍需 id 结构或公开命名支持 ✔。
### 43.2 `0x2080a06e` 的身份：不再"尚未定位" ✔
- 接口字节 `0xa0 = 0x20 | 0x80`（PERF 族的 legacy non-privileged 别名，见 §42）✔，消息 id `0x6e` 与公开 `0x2080206e` **相同**，而公开 id 在 SDK 中被命名为 `NV2080_CTRL_CMD_PERF_RATED_TDP_GET_CONTROL`（`ctrl2080perf.h:399`）⇒ **`0x2080a06e` 就是同一 RATED_TDP get-control 的 legacy 别名** ✔ ✔。
- 佐证 ✔：实测 12 B ＝ `sizeof(NV2080_CTRL_PERF_RATED_TDP_GET_CONTROL_PARAMS) = 12` ✔（但 `NV2080_CTRL_PERF_GET_VID_ENG_PERFMON_SAMPLE_PARAMS` 亦为 12 ✗ ⇒ 单凭尺寸不能命名 ✔）。
- ⚠️ 其写半边 `0x2080e06f` 实测 **8 B**，与 SDK 的 `NV2080_CTRL_PERF_RATED_TDP_SET_CONTROL_PARAMS = 12` **不一致** ✗ ⇒ **写半边必须按它自己的行解析**，不得假设与读半边同构 ✔（与 §40 该孪生对的注记一致 ✔）。
### 43.3 按尺寸可命名的候选行 ✔
- `0x20801037`（CLK，6024）与 `sizeof(NV2080_CTRL_CLK_PMUMON_CLK_DOMAINS_GET_SAMPLES_PARAMS) = 6024` 相同 ⇒ **候选命名**：PMU 监控的时钟域采样 ✔（记为候选，因同尺寸非唯一 ✔）。
- `0x2080852e`（THERMAL，1432）与 execute-v2 同尺寸，且其孪生 `0x2080c52f` **不在**那 6 个可写行里 ⇒ 读侧同类 ✔（候选 ✔）。
- `0x20808513`（已实测 NV_OK）＝ `…_THERMAL_SYSTEM_EXECUTE_V2_PHYSICAL` ✔（§42 头文件直接命名 ✔）。
### 43.4 开放树与生成表的**精确边界**（负面结论也要有依据）✗
- `ctrl2080thermal.h` 里 `…_PARAMS` 类型只有 **1** 个、`ctrl2080clk.h` 只有 **1** 个 ⇒ 38 行 THERMAL 中只有 1 行能由开放头文件命名 ✔；偏移通道的 2108 结构在开放头文件里**没有**对应类型 ✗（其形状已知：`60 + 32×64` ✔）。
- `ctrl2080fan.h` ＝ **33 行空壳** ✗（仅 `#pragma once` ＋ `#include <nvtypes.h>`；0 个 `#define`、0 个 `typedef struct`）⇒ **风扇族在开放 SDK 里没有任何声明** ✔ ⇒ 风扇族的"仓库内材料"是**驱动自身的 fan 代码与生成派发表**，不是 SDK 头文件 ✔（精确结论，非"搜不到就算了" ✔）。
- 生成派发表 `src/nvidia/generated/g_subdevice_nvoc.c`（**641** 行）中，5 个公开 THERMAL id 只有 `0x20800513` 出现 1 次且带 `paramSize=sizeof(NV2080_CTRL_THERMAL_SYSTEM_EXECUTE_V2_PARAMS)` ✔，另 4 个缺席 ✗ ⇒ 该表**只覆盖公开半区的子集**，缺席不能证明命令不存在 ✔（与 §40 一致 ✔）。
### 43.5 风扇／热族审计小结（本轮新增部分；各族独立 ✔）
- **温度传感读取**：`0x20808513`（`…_SYSTEM_EXECUTE_V2_PHYSICAL`）已在本会话实测 **NV_OK** ✔（可读 ✔）；参数结构由开放头文件命名且尺寸校准一致 ✔（可解释 ✔）。范围／原状态／恢复：**待实测**（需一次干净的只读跑 ✔）。
- **热族其余 37 行**：38 行中 6 行有写孪生 ✔（`…c52d@1440`／`…c531@980`／`…c535@1556`／`…c53d@3100`／`…c541@24`／`…c54d@20448` ✔），其余为只读 ✔；其中 1432／6024 两行获候选命名 ✔，其余 36 行**尚未定位**（开放树无对应类型 ✗）✔。
- **风扇族**：开放 SDK 无声明（空壳 ✗）；候选为 `0x20808540@24`（legacy 别名，可写孪生 `0x2080c541@24` ✔）；含义与单位**尚未定位** ✔。
- **功耗族**：`RATED_TDP` get-control 的 legacy 别名身份已定（§43.2 ✔），SET 结构按自身行解析 ✔；`SETTINGS_WRITE_SUPPORTED` 仍为 false ⇒ **未写入任何功耗值** ✔。

## 44. `0x2080901b` 写集合由散文改为**可执行断言**：真 0xA5 dump 从此可被自动裁决（2026-10-18）

### 44.1 问题（两个都还没有证据）
- 写面仍缺两件事：handler 在 0xA5 背景下**实际写了哪些 record-relative 槽位**（这决定 SET 该打哪个槽 ✔），以及 `106/527 changed, last @564` 是否真的由目录十行与各自 type 推出 ✔。
- 此前只有 106 是实测 ✗，27／132 与 last @116／@564／@692 只是探针注释里的预测 ✗，本文件也只是复述 ⇒ **没有任何代码在检查它**，规则被改动或抄错都不会被发现 ✗。
- 已录的 `clkctl-hex.txt`（2026-10-17 17:08）**不含** `0x2080901b` 块 ✗：`grep 901b` 零命中 ✔。它是更早的一次 dump；现源码 `nvx_stage2_clkctl_hex()` 的 case 表**已经包含**该 id 的四个 case（首例正是 `in@4=0xFF bg=A5` ✔，另有 `0x3FF`／`0x1`／`0`）⇒ 该块的真字节只能等一次健康窗口 ✗（wedge 自 10-17 起未清，见 §41 ✔）。**我去查这一点时先误判为"工具没 dump 901b"，随即被源码 case 表证伪** ✔ —— 教训：先读源码再下"缺"的结论 ✔。

### 44.2 本轮改动（仓库内工具，纯静态 ✔）
`probe-macos-rm/tools/decode-clk-containers.py` 新增：
- `RECORDED_ROWS`＝实机十行（含各自 type word ✔）、`WRITE_SET_HEAD=14`、`WRITE_SET_PER_TYPE={0x08:13, 0x09:13, 0x03:1}`、`RECORDED_WRITE_SET={0x1:(27,116), 0xFF:(106,564), 0x3FF:(132,692)}`（前者的 106 实测，另两个是探针的预注册预测 ✔）；
- `write_set_model(mask)`：按目录行与 type 规则算出改变 dword 数与最后一个改变偏移；遇到**无规则的 type 返回 `(None, None)`**，不把未知类型当 0 ✔；
- `a5_block(mask)`：把模型写成字节（head 14；type 08/09 写 12 个值 dword ＋ stamp；type 03 只写 stamp；rec+4／+48／+60 不写；stamp **只写低两字节**，故 rec+0 写成 `a5a50808` ✔）；
- `write_set_check(buf)`：逐行裁决——每行实际写了的相对槽位 vs 该 type 的规则 ✔、字节隐含的 head 数 vs HEAD=14 ✔、最后一个改变偏移 vs 最后一个 value 行的 rec+56 ✔；**零背景块直接拒绝**（"no dword still holds 0xa5a5a5a5"），因为零背景下"写了 0"与"未触碰"不可区分 ✗；
- `decode()` 对 `0x2080901b` 自动追加该行；`--selftest` 断言三个 mask 的 (计数, 末偏移) 全部 MATCH，并断言零背景被拒绝、未知 type 被 withhold ✔。

### 44.3 实测（本机，**零 GPU 请求** ✔）
- `python3 -m py_compile` 通过 ✔；`--selftest` 输出 `selftest ok: geometry + round-trip + field decode + write-set model` ✔。
- 对已录 `clkctl-hex.txt` 复跑：目录十字矩阵三处仍全 `MATCH`（`rows-as-bits 0x000003ff`／`union 0x0038007f`）、`rows: 0:id0 1:id1 …` 与 `seed 0xff reaches rows 0..7, beyond it: row 8 id 21; row 9 id 5` 全部不变 ⇒ **无回归** ✔。
- 新代码无 >88 列行 ✔；本机无 ruff（`ruff: command not found` ✔，CI 侧检查 ✔）。
- 模型对三个 mask 自洽：`0x1→27/@116`、`0xFF→106/@564`、`0x3FF→132/@692`，其中 type 03 行（第 6 行）只贡献 1 ✔ —— 这正是"8 选中里有 1 行不写值"的算术，也解释了为何 106≠8×13 ✔。

### 44.4 可证伪点与仍开放的问题（都不许用"跑不出来"顶替 ✔）
- `HEAD=14` 是**唯一拟合常数**（由那次 106 反推 ✔）；可独立核对的是一行一行的 type 规则（8 行各自 13／13／…／1 ✔）。真 0xA5 dump 若某行槽位数不符，工具会直接打 `MISMATCH` ✔。
- **槽位身份仍尚未定位** ✗：12 个槽位里哪个是 signed offset（+8／+12／+16 三选一 ✔）只能靠一次最小步 SET 后的**同字段回读**判定，实时频率**不参与** ✔ ⇒ 因此**仍未发任何 SET** ✔。
- 下一次健康窗口的收益：`tools/clock-offset-read-run.sh` 的 `--clkctl-hex` 电池含该 id 的 A5 case ✔，runbook 已把该 dump 交给本工具 ✔（其解码调用处 ✔）⇒ 槽位与写集合将变成"跑完即可读结论"的机械判定，而不再需要第二次实机请求 ✔（GET 不可重复：wedge 时根本发不出去 ✔）。

### 44.5 工具自身与 CI 门禁对齐（诚实记录，含"我改了什么、没改什么"）
- **发现** ✔：仓库 CI 的 lint job 是 `ruff check .`（`.github/workflows/ci.yml`，`pip install ruff` 取最新 ✔），而本机在**全仓库**范围内跑出**只有 2 个错误**，且都在本工具里：`f-string-missing-placeholders`（F541），位置是 `--selftest` 里那段合成文本 fixture 的第 546／547 行——**是旧行，不是本轮新增** ✔（同一处 0.16.10 与 0.17.1 都报 ⇒ 与该 job 的 ruff 版本无关 ✔）。去掉那两个多余的 `f` 前缀后，**全仓库 `ruff check .` ＝ All checks passed** ✔。
- **格式** ✔：CI 的 fmt job 是 `ruff format . --check`（`preview = true` ⇒ **版本敏感** ✔）。本机用 `ruff@0.17.1` 只对**本工具这一个文件**执行 `ruff format`：该文件从 would-be-reformatted 变为 **already formatted** ✔；全仓库"would be reformatted"文件数 **10→9** ✔。**其余 9 个漂移文件一律未动** ✔ —— 它们是别人/别族的代码，本轮不去制造无关 diff ✔。
- **行为不变（这是关键证据）** ✔：格式化后 `--selftest` 仍输出 `selftest ok: geometry + round-trip + field decode + write-set model` ✔；对已录 `clkctl-hex.txt` 的解码输出 **md5 前后完全一致**（`7bf52cb8eb151a575ad3d1d70c824750`）✔ ⇒ **纯格式改动，零语义变化** ✔。
- **规模与边界** ✔：`174 insertions / 104 deletions`，**只限本工具文件** ✔（我自撰的工具，未触碰他人文件 ✔，无 `reset --hard`／无 `clean` ✔）。

## 45. 把数字钉到捕获上：runbook 规则表述纠错 + 位标记类的实测（2026-10-18）

### 45.1 触发
两个**活文档**（不是历史记录）里各有一个可疑数字/表述：`probe-macos-rm/tools/clock-offset-write-run.sh` 把 SET 孪生规则写成 `(id | 0x4000) + 1` 并称 "the corrected table carries 47 instances"，又写 "on the corrected 1118-row table that test selects exactly the 92 write-half ids"；`core/src/macos_rm.rs` 的文档注释把同一句 "exactly the 92 write-half ids" 放在**未指明捕获**的语境里。§42 已把规则纠正为"category byte +0x40 且 message id +1"并记录 304/304 实测 ⇒ 活文档与已证结论不一致，必须改。

### 45.2 实测（本机可复现，889 行捕获 `gsp-table-full.tsv`）
- 按 **bit15/bit14 组合**分类（不再留"缺省桶"）：`0x0` **481** 行、`0x4000` **19** 行、`0x8000` **300** 行、`0xc000` **89** 行，合计 889 ✔。**自我纠错**：我第一次的分类脚本只写了 `0x8000..0xBFFF`／`0xC000`／`0` 三桶，**漏掉 `0x4000` 单独出现这一桶**，19 行被静默并进 "public/other"（481 里含它们）；本节按四桶重算并记录该失误 ✔。
- `0x4000` 单独出现的 19 行**全部落在公开类别 0x40-0x44**：`0x20804006@776`、`0x20804001@6616`、`0x20804101@8`、`0x20804102@8`、`0x20804201@104`、`0x20804402@140` 等（flags：`0xc0` 十六行、`0x158`、`0x48`）⇒ **bit14 单独不构成"特权"**；转发器的判定式是 `(cmd & 0xC000) == 0xC000`，它在这份捕获上**只选出那 89 行写半边、一个读行都不选** ✔。所以"写半边＝特权半边"对**该判定式**成立，而"bit14 = privileged"的说法不精确。
- 孪生规则按 §42 的纠正形式实测：存在 **(category byte + 0x40, message id + 1)** 孪生的读行 **56/300**，其中同 paramSize **41**；写成 `(id | 0x4000) + 1` 在这份捕获上给出同一批 56 个（因为读半边是 0x80-0xBF 字节，加 0x40 与 `| 0x4000` 等价；这个等价对已含 bit6 的字节不成立）✔ ⇒ runbook 里 "47 instances" 与 "92 write-half ids" 属于**另一份捕获（1118 行 recount）**的数，与我这 889 行的 56／89 **不能互推** ✔。
- 逐 id 核对 runbook 的两个例子：`0x2080901b` size **2108** flags `0x48` → `0x2080d01c` size **2108** flags `0x44` ✔；`0x2080901f@19416` → `0x2080d020@19416` ✔；`0x2080d01b` **不存在** ✔。

### 45.3 改动（只改活文档/注释与其依据，不改任何行为）
1. runbook 的规则段改写为实测形式（category byte +0x40 与 message id +1；注明 `| 0x4000` 只在读半边等价），计数**按捕获指名**（889 行：56/300，同尺寸 41）。
2. runbook 的特权段与 `core/src/macos_rm.rs` 的注释改为"该判定式选出恰好写半边——1118 行 recount 里 92 行、889 行 dump 里 89 行，两次都没选到读行"。
3. `dump-gsp-families.py` 的模块 docstring 补上"bit14 单独不是特权"的实测反例（19 行公开 0x40-0x44）；其**机器可读输出行未改**（避免破坏下游 grep）。

### 45.4 仍然未知 / 新候选（均为只读，未做设备请求）
- **候选**：`0x20804006@776` 与私有时钟文档 `0x20809061@776` 的 paramSize 相同 ⇒ 公开族里是否存在同一份文档结构？尺寸巧合的可能同样存在，**未测**，只记为候选。
- 1118 行 recount 与 889 行 disassembly dump 的**差集内容**仍未定位（是否另一固件映像／另一台机器的导出）。

### 45.5 验证范围与提交
本机：`sh -n` 通过 ✔；`cargo fmt --all -- --check` 干净 ✔；`cargo test --package nvoc-core --all-targets` **115 passed / 0 failed** ✔（注释改动，行为不变）；`ruff check` 对被触动的 py 文件 All checks passed ✔；`dump-gsp-families.py` 属**既存** 9 个格式漂移文件之一（已用 `git show HEAD:` 证明**改动前就漂移** ✔），故不整文件重排、保持 diff 最小 ✔。**测试机零 GPU 请求** ✔。

## 46. 229 行丢弃的归类与影响：偏移通道 ids 不受影响，但孪生统计必须按表说明口径（2026-10-18）

### 46.1 触发
goal 原文与交接 §4.1 都提到"旧工具静默丢了 229 行"；我此前只把它记为"三份捕获（890／1081／1118）未对齐"的**未知**。本轮把机器上 2026-10-17 的 recount 工件取回本机做**只读**分析 ✔（`outputs/` 不进仓库 ✔）。

### 46.2 丢弃原因其实已在仓库工具里写清（我的"未知"是已修 bug）
`probe-macos-rm/tools/extract-gsp-method-table.py:75-85` 记录：旧代码有两个多余守卫 `fl < 0x10000 and ar < 0x10000`，**静默丢掉 229 行真实行**（889 而非 1118），因为活跃行的 `flags` 经常带 16 位以上的位（例：`0x20809006` flags `0x81074a`、`0x20809062` flags `0x82004a` ⇒ 这两个 id 曾被报"表里没有"，其实是 paramSize 8 与 648 的真实行）；去掉守卫后 4 字节对齐与 32 字节步进两种扫描得到**同一批 1118 行**（⇒ 放宽谓词未引入假阳性）；`tools/gsp-handler-map.py:358` 另有 `ck(len(rows) == 1118)` 自检 ✔。**教训**：先查仓库工具，再把它记成未知 ✔。

### 46.3 实测归类（本机只读，两表逐 id 比对）
- 机器工件 `outputs/stage-gsp-table-recount-20261017T130832Z/gsp-table.tsv` = **1119 行（1118 行数据）**；同目录 `gsp-table.log` 记 `# /Users/Shared/nvfw/nvidia/610.57.04/gsp_tu10x.bin: 1118 rows, e_machine=243, categories={…}` ✔；产出工具是仓库内的 `extract-gsp-method-table.py` ✔。
- 与旧导出 `gsp-table.tsv`（**889 行**，md5 `642c3f38…`）比对：**旧导出是固件表的严格子集**（只在旧表出现的 id = **0**）⇒ 丢的正是 **229** 行 ✔。
- 229 行按 bit15/bit14 分类：公开 `0x0` **190**、bit14-only **1**、读半边 **35**、写半边 **3**；按类别字节，前几位是 0x0a(53)、0x01(42)、0xa0(15)、0x12(14)、0x13(12)、0x18(12)。
- 被丢的 **35 个读 id**：`0x20808159 0x20808161 0x20808164 0x20808513 0x20808544 0x2080880f 0x20809001 0x20809006 0x20809035 0x20809038 0x20809062 0x20809063 0x20809513 0x2080a024 0x2080a027 0x2080a02d 0x2080a05e 0x2080a066 0x2080a06b 0x2080a06c 0x2080a095 0x2080a09d 0x2080a09f 0x2080a0a0 0x2080a0a4 0x2080a0a6 0x2080a0ba 0x2080a0bb 0x2080a61f 0x2080a620 0x2080a622 0x2080a624 0x2080a62b 0x2080a708 0x2080b402`；被丢的 **3 个写 id**：`0x2080d055`（读伙伴 `0x20809054` 在表内）、`0x2080e0ae`（伙伴 `0x2080a0ad` 在表内）、`0x2080f310`（**没有**读伙伴）。

### 46.4 对偏移通道的影响：**没有影响**；但孪生统计必须按表说明口径
- 本计划依赖的 id 在**两份表里都在**且尺寸/标志一致：`0x2080901b@2108/0x48`、`0x2080d01c@2108/0x44`、`0x20809019@12336/0x48`、`0x20809025@7188/0x48`、`0x20809061@776/0x48`、`0x2080a06e@12/0x48` ✔ ⇒"写孪生在表内"这条**不受丢弃影响** ✔。
- **`0x20808513@1432` 只在固件表里**（旧导出缺失），flags **0x10248**（与 0x48/0x44 不同类）⇒ 这是"某 id 查不到"过去确实可能等于**捕获不全**的又一实例 ✔。
- 孪生**存在率**（规则＝category byte +0x40 且 message id **+1**）：旧 889 行 → 读行 300、孪生存在 **56**、同 paramSize **41**；固件 1118 行 → 读行 335、孪生存在 **62**、同 paramSize **47**；固件表 **92** 个写行中有 **62** 个是某读行的孪生（⇒ 30 个不是）✔ ⇒"孪生规则不普遍"的结论**被加强** ✔。
- **必须与"可推导率"分开的口径**：`gsp-families-610.57.04.txt`（1081 行捕获）里 **304 个读行的 twin 列全部等于 `(mid|0x4000)+1`**（304/304）——那是**规则被套用**，不是"该 write 行存在"；这 304 个推得 id 里只有 **55** 个是固件表的行 ✔。我此前把 "304/304" 与"孪生存在率"写在同一句里，此处明确分开：**可推导率 100%（算术）≠ 存在率（约 60 例／三百余读行）** ✔。
- `| 0x4000` 与 "category byte +0x40" 对**读半边**（0x80-0xBF）等价，这正是那份捕获的 twin 列与 §42 纠正规则给出同一批 id 的原因 ✔。

### 46.5 方法上的坑（记录）
- 我中途用 `| ((msg) + 1)`（把 message id **按位或**而非**加一**；`0x1b|0x1c = 0x1f`）实现孪生，得到 52/300、23 的**错误**数字；改回整字 `+1`（进位形式）后与 §45/上一轮记录一致（56/300、41）✔ ⇒ 孪生必须按 **id 算术（加一）** 实现，不能按位或。
- 本机 `gsp-table.tsv` 与机器 recount 的 `gsp-table.tsv` **内容不同**（md5 `642c3f38…` vs `60b75472…`），此前把两者当同一份是错的 ✔。

### 46.6 验证范围与提交
本机只读分析：**无 GPU 请求、未发 SET、未提权** ✔；取回工件放 `/home/lyy/work/nvoc-analysis/machine-recount/`（**不在仓库内** ✔）。仓库改动仅 `probe-macos-rm/tools/dump-gsp-families.py` 的 docstring（把 "lists 889 methods" 改为 1118 并写明丢弃原因，**机器可读输出未改**）。验证：`py_compile`＋工具自跑＋`ruff check`＋`git diff --check`。测试机本轮零 GPU 请求 ✔。

## 47. 偏移记录的**身份**怎么定：同快照"发布频率 = 实时时钟"的比对，及其与"成功判据"的分界（2026-10-18）

### 47.1 问题
`0x20809019` 目录只标**位**（本设备用到 0..6 与 19..21），公开 OGKM 头文件没有任何 clock-domain 枚举，唯一交叉参考是 Windows/NVAPI 的 `NV_GPU_CLOCK_DOMAIN_ID`（0..6＝GPC/XBAR/SYS/HUB/M/HOST/DISP）⇒ 它只是**假设**。于是 `get-clock-offset --domain graphics` 这类"某一名字对应某一数值"的入口只能拒绝 ✗（§上轮记录）。

### 47.2 可用的测量（本条的依据，非推断）
时钟文档（`0x20809061`）每条记录同时给出**它持有的频率**与**它应用的偏移**；`get-status` 在同一快照里报出公开域的实时时钟。已录捕获中，至少两条记录的发布频率与同一会话 RUSD 报出的实时图形／显存时钟**逐位相同**（矩阵前文已录；显存那条＝bit `0x10`、`6000984 kHz`、offset `-126 kHz`）。

### 47.3 规则（可实现、可证伪）
名字只在**精确相等且唯一**时附加：`name` ← 唯一一条"发布频率 == 本快照该名字对应的实时时钟"的记录。无匹配、并列、或缺实时时钟 ⇒ **不命名**（并列时在 notes 写明"命名其中一条是选择而非测量"）。**失败方向恒为未命名，不是起错名** ✔。两个公开名可合法指向同一条记录（空载时图形与 SM 时钟常相同），此时两个名字都报出并附说明，不静默取其一 ✔。

### 47.4 与"实时频率不得代替写成功"的分界（必须写清）
本规则里实时频率**只用于决定名字指向哪条记录**，不参与"写是否生效"：报告的 `offset_khz` 仍是文档自己的字段，且比对的两个数字来自**同一快照**。也就是说，这是一次**身份用途**，不是成功判据 ✔ —— 成功判据仍只能是**写入后同字段回读**（`+8`／`+12`／`+16` 三选一仍尚未定位 ✗）。

### 47.5 残余不确定性（不隐藏）
- 4 个公开名字里，`processor` 对应 `get-status` 的 `sm_clock_khz`；`video` 对应 `video_clock_khz`。名字是**前端词汇**，不是设备域名的证明 ⇒ 证据串随名字返回，读者可自行否证 ✔。
- 图形域在 boost 变化时可能**匹配不上**任何记录 ⇒ 该快照它保持未命名。这不代表"图形域没有记录"，只是"本快照没有可比对的相等值" ✔。
- 该规则从未在设备上运行过 ⇒ 实现层面已离线验证（§47.6），设备层面归"已实现待实测" ✔。

### 47.6 验证范围与提交
改动：`core/src/macos_rm.rs`（`NamedClockOffset`、`corroborate_named_offsets`、`DeviceSettings::clock_offsets_named`、`DeviceSettings::named_clock_offset`、`unnamed_offset_reason`）、`core/src/lib.rs`（再导出）、`cli/src/macos_rm.rs`（`get-clock-offset` 入允许清单并按证据回答）、`nvoc-python/src/lib.rs`（投影 `clock_offsets_named`）。验证：`cargo test -p nvoc-core --all-targets` 98 passed／0 failed、`cargo test -p nvoc-cli` 83 passed／0 failed、workspace clippy `-D warnings` 干净、`cargo fmt --all -- --check` 干净；`pynvoc` 测试二进制在本机缺少 libpython 无法链接（`PyObject_Str` 等未定义）⇒ 其运行时验证归测试机。**本机与测试机两侧 sha256 逐文件相同** ✔；测试机本轮**零 GPU 请求**（仅只读门控重试第 11 次读数 `0x00000062`，未重启），**未发任何 SET** ✔。


## 48. 汇总：仍然有效的更正、已撤回的断言、以及"引用哪个捕获"的卫生规则（2026-10-18）

本节不引入新证据，只把散在 §1–§47 与测试机 `outputs/` 记录里的更正**归拢成一张可核对的表**，并补上 2026-10-18 本轮新测到的一条（第 14 项）。目的是：下一个读者不必读 1500 行才能知道"哪些旧话已经不能再引用"。**每条都指向它的证据来源；没有证据的一律留在"尚未定位"，不在这里被写成结论。**

### 48.1 已撤回的断言（引用它们会导致错误动作）

| # | 已撤回的说法 | 为什么撤回 | 证据 |
|---|---|---|---|
| 1 | 把 `0x964 = 2404` 当私有 CLK id 的块尺寸 | 那是 NVAPI **调用者**层；GSP 校验的 RM 块是 `2108 = 60 + 32×64`。发 2404 得到 `0x1f` 是**层错了**，不是"命令不存在" | 树内注释 `probe-macos-rm/src/nvx_suite.c:1786-1790`；实测 `0x1f` |
| 2 | `0x2080D01C = 0x2080901B \| 0x4000` | 算术错：上式 = `0x2080D01B`（消息 id 没进位）。真规则＝**接口字节 +0x40 且消息 id +1** | §42.2（304/304）；因此 `0x2080d01b` 不存在 |
| 3 | "十二个 value dword 全 0 ⇒ 本机是 V1 形态、没有偏移" | 那些运行把掩码放在 `+8`，驱动几乎什么都不填。掩码放 `+4` 时 **106/527 dwords 被改写** | §35.4；`clkctl-offsweep-run1-A5.txt:32` |
| 4 | "bit14 = 特权" | bit14 单独出现有 **19** 行**公开** 0x40-0x44 行；特权/写半边是转发器判定式 `(cmd & 0xC000) == 0xC000` 选出来的 | §45.2、§46.4 |
| 5 | 跨表比较 `flags` | 生成派发表（`g_subdevice_nvoc.c`）与 GSP 导出表的 `flags` **不是同一命名空间**，两者逐 id 都对不上且取值交叉 | §40.1 |
| 6 | 用捕获的 `GET/SET` 列定方向 | 该列把 19 个带 bit14 的行全标 GET，而 `0x2080206f`（SET）的 flags 明带 bit14 ⇒ 降级为**线索** | §41.4 |
| 7 | "公开 610.57.04 控制面**没有**任何 clock/voltage offset、lock、fan、thermal-policy **写**命令" | 写命令**存在**（0xC0-0xFF 半边）；缺的是**权限**（非特权实测 `0x1b`）与原生层不发任何 SET id | §39.1；`SETTINGS_WRITE_SUPPORTED=false` |
| 8 | "私有 GET 都返回 `0x1f` 或 `0x56`" | 那是**早期尺寸猜测**的应答；驱动只接受其导出表声明的尺寸。`0x20809061@776` 被接受并**发布了逐域偏移** | §39.1；`stage-clkdoc-20261017T074448Z/clkdoc-read.txt` |
| 9 | "RATED_TDP 只回显输入 ⇒ 拿不到值" | 种子是 `0 = ACTION_DEFAULT`，**未限速的正确答案本来就是 0** ⇒ 那是假设不是测量；已加两遍可证伪用例（未跑） | §37.1–37.3 |
| 10 | "旧工具静默丢了 229 行 = 固件层的谜" | 是仓库工具里两个多余守卫（`fl < 0x10000 and ar < 0x10000`）丢的，**已修**；放宽谓词未引入假阳性 | §46.2；`extract-gsp-method-table.py:75-85` |
| 11 | "wedge 期间 `get-status`/`get-fan-info` 绿 ⇒ 值可信" | 绿只证明**入口可达**。且本轮实测：wedge 期间 `MacosRmSession` **在建立会话（RUSD 映射）阶段就被拒** ⇒ 任何基于会话的读（偏移通道、公共热读）**根本发不出去** | 第 14 项；`get-legacy-temp-sensor` 理由串 |
| 12 | "wedge 是硬锁存，只能等重启" | 11:46 实测到一次健康窗口、数秒后即回 `0x62` ⇒ 它是**间歇性**的，正确做法是门控重试抓窗口（重启仍由用户执行） | §41.1–41.3 |
| 13 | "风扇本轮批次空转 0 rpm" | 该批次的前置采样是 **1613**，不是 0；0 rpm 只在更早一次探测里出现过，**未在本批复现** | §34.2 |
| 14 | "写面/温度族只是'可读但客户端未接线'" | 温度族的**量程**已在第 78 轮真接线（`0x20800513`，按驱动 target type 位掩码配对）；接线后的实测答复是**会话建立被拒**（见 11），不是"未接线" | 测试机 `outputs/review-stage-gsp-clkctl-20261017.md` §112；core 单测 `the_thermal_read_fills_the_range_of_its_own_target_and_nothing_else` |

### 48.2 仍然成立的更正（应继续沿用）

- **id 结构**：`id = 0x20800000 | (接口字节 << 8) | 消息 id`；接口字节 bit7（`0x80`）= legacy non-privileged 别名；写半边 = 接口字节 `+0x40` 且消息 id `+1`（§42）。
- **尺寸列 = `sizeof(PARAMS)`**：已用开放头文件编译尺寸校准（`NV2080_CTRL_THERMAL_SYSTEM_EXECUTE_V2_PARAMS = 1432` ↔ 导出表 1432，两条独立路径一致）；但**同尺寸不唯一** ⇒ 尺寸是佐证，命名仍需 id 结构或公开命名（§43）。
- **掩码位置**：RM 块 `mask@+4`（**不是** NVAPI 调用者层的 `+8`）；`HEAD=14` 是**唯一拟合常数**，行级 type 规则（13/13/…/1）可独立核对（§44）。
- **写集合可被机器裁决**：`decode-clk-containers.py` 的 `write_set_check()` 对真 0xA5 dump 逐行 MATCH/MISMATCH，且**拒绝零背景块**（零背景下"写了 0"与"未触碰"不可区分）（§44.2）。
- **偏移记录身份**：只在**同快照内**"发布频率 = 该名字对应的实时时钟"且**精确唯一**时命名；实时频率只用于**身份**，**不参与"写是否生效"**——成功判据只能是写入后**同字段回读**（§47.4）。
- **`0x2080a06e` 身份已定**：PERF 族 legacy 别名，即同一 RATED_TDP get-control；其 **SET 孪生 `0x2080e06f` 实测 8 B ≠ SDK 的 12 B** ⇒ **写半边必须按它自己的行解析**，不得假设与读半边同构（§43.2–43.3）。
- **缺席不等于不存在**：`0x2080901b`/`0x2080d01c`/`0x2080a06e` 在生成派发表里**完全不存在**，而该表只覆盖公开半区的子集（§40.2）；`ctrl2080fan.h` 是 **33 行空壳**（0 个 `#define`、0 个 `typedef`）⇒ 风扇族的材料是驱动自身代码与生成表，不是 SDK 头文件（§43.4）。

### 48.3 仍未定位（不许被"跑不出来"顶替，也不许被写死为"只能固件逆向"）

1. **十二个 value 槽里哪一个承载有符号偏移**（`+8`/`+12`/`+16` 三选一）—— 需一次最小步 SET 后的**同字段回读**；实时频率不参与（§44.4）。
2. **写权限前置条件** —— 需一次交互式 `sudo`（属**待授权**，不是能力缺失）。
3. **范围记录里 `+8 = 0x190000`、range 标签 `0x010x0101`、目录 `+40`/`+44` 的 i16 对与 `+76` 的单位**。
4. **三条 range 记录是否 HUB(3)/M(4)/HOST(5)**。
5. **flag 位 `0x40000` 的含义**；19 个公开 `0x4000` 行是否构成一个独立的"公开特权半边"。
6. **`0x20809026` 在 2508 下是否给实时值**；host 是否转发 `0x20808540`。
7. **776 文档头部各值的含义**；`processor ↔ sm_clock_khz`、`video ↔ video_clock_khz` 这两个名字等价性是**前端词汇**，不是设备域名证明（§47.5）。
8. **公共热读的 provider index 与本入口 `controller` 字段的关系**（本轮已把 provider index 写进理由串，但映射仍未建立 ⇒ 该字段留 `null`，不做猜测映射）。

### 48.4 捕获卫生（引用数字时必须写清 artifact）

| artifact | 行数/规模 | 备注 |
|---|---|---|
| `gsp-table-full.tsv`（本机导出） | 889 行 | md5 `642c3f3861e7a6d2c8d677c23197c76d`；工具报 889 有效 |
| 机器 recount `gsp-table.tsv` | 1118 行数据（文件 1119 行） | md5 `60b75472d8c01ba5c62e5387a49c8387`；由修好的 `extract-gsp-method-table.py` 产出 |
| `gsp-families-610.57.04.txt` | 1081 行 | 其 twin 列是**规则套用**（304/304 算术），不是"该行存在" |

- **孪生两条口径必须分开**：可推导率（算术，100%）≠ 存在率（固件 1118 行里 **62** 例；旧 889 行里 **56** 例）。
- 孪生必须按 **id 算术（消息 id +1）** 实现；用按位或 `x | (mid+1)` 会得到偏小且错误的数字。

## 49. 同一块容器的**第二种方法**复核、两条口径更正、以及 `0x2080901d` 的实测周期（2026-10-18）

本轮全部是**对已录字节的静态重读**（零新 GPU 请求）。三条结果里两条是更正，一条是把"算术推导"降级为"候选并预登记判决条件"。

### 49.1 `0x20809025` 的布局由独立方法复核（结论不变，可信度提高）

- 既有证据是**已录 payload 解码**（`stage-gsp-clkctl-20261017T092536Z/clkctl-hex.txt`，零种子读）＋与 `nvapi-rs` 的 Windows 记录对齐（§5.1）。
- 本轮用**另一种方法**复核：`clkctl-offsweep-run1-A5.txt` 的 `0xA5` 背景差分（`mask@+4=0xFF`，工具自报 `106/527 dwords changed`）在同一 id 上给出**同一组**记录位置与字段值：`+20/+244/+468`（stride 224）、`rec+0` 类型戳 `0x01030101/0x01040101/0x01050101`、`rec+8 = 0x190000`、`rec+20 = 0xffffb6c2 (-18750)`、`rec+24 = 0x493e (18750)`、`rec+28 = 0x00ff0100`。
- 两条独立方法（已录 payload vs 背景差分）**逐字段一致** ⇒ 该容器的 `(head 20, stride 224)` 与字段值可以当作已定 ✔。**单位问题不存在**：`rec+8 = 1638400` 的名字与单位来自仓库内 NVAPI 侧字段（`clk_freq_ctrl_info_entry`，记为 `max kHz`，= 1638.4 MHz），不是从数字形状猜的；"1638.4 = 1600×1.024" 只是巧合，不构成新问题。

### 49.2 更正一：`0x20809025` 头部 `+4 = 0x7` 不能写成"支持的域掩码 bits 0,1,2"

`gsp-method-table-and-legacy-forwarding.md` §5.1 目前的表述是 `supported-domain mask — bits 0,1,2`，而同一块的三条记录的**类型字节是 3/4/5**（不是 0/1/2）。两者是**两个坐标系**：

- 若 `+4` 的每一位是"域位"，则记录应当带域位 0/1/2，而不是类型 3/4/5；
- 同一族里已有先例：`0x20809019` 的头部**同时**给出两种命名（`+4 = 0x3ff` 行位、`+12/+20 = 0x38007f` 域位并集），并且 §14 据实机字节把 `0x2080901b` 的 `+4` 定为**条目选择器**；
- 因此本块更可能也是**条目选择器**（掩码第 i 位 ↔ 第 i 条已填记录），而不是域位掩码。

**处理**：按 §48 的纪律，这是**候选更正**而非已定结论——仅凭 `0x20809025` 自己的字节无法区分。**可证伪条件**（写给下一个健康窗口）：在 `nvx_known_wedge_ids` 之外找一个掩码与记录数不一致的样本（例如掩码置位多于已填记录数，或记录带某型而掩码不置位），或找到同时打印两种坐标系的容器。**在此之前，引用该字段必须写成"掩码（here an entry selector is the reading consistent with §14）"，不能写成"支持的域"。**

### 49.3 更正二：`0x2080901d` 的 `(40, 1020)` 是**算术**，不是几何证据；且该算术不唯一

- 工具 `GEOMETRY[0x2080901D] = (32680, 40, 1020)` 满足 `40 + 32×1020 = 32680` ✔，但同一等式有**四个解**：`(40,1020) (168,1016) (296,1012) (424,1008)`（头部每 +128、步长每 −4）；这正是不该把分解当布局的原因。
- 本轮从 `0xA5` 运行拿到该 id 的**第一个实测结构**：改动 dword 分布以 **128 字节**为周期重复（`+36..+56` 组与 `+164..+184` 组三个 dword 逐字节相同，两个各自差一个字节）——像逐域记录表。
- **关键点**：128 周期**不能**平铺上面四个解（1012–1020 字节 = 7×128 加 116–124）⇒ 若该周期延续，则"算术解"根本不是证据 ✗；而且 `(36,128)` 下两组正好是相邻两条记录，`(40,1020)` 下第一组落在**头部**、第二组落在 rec0+124。
- **预登记判决条件**（已写进工具 `MEASURED_REPETITION` 与 `--selftest` 断言）：下次取得 `0x2080901d` 的**完整 payload（0xA5 背景）**后，若第三组出现在 `+292` ⇒ stride 128 成立、`(40,1020)` 作废；若下一组出现在 `+1060` ⇒ head 1020 成立。**在那之前，`0x2080901d` 记作"读成功（NV_OK，352/8170 dwords）但布局尚未定位"。**

### 49.4 更正三（本轮发现的自有缺陷）：本机 `core-fixtures/*.bin` 曾是**全零**

`nvoc-analysis/core-fixtures/macos-offset-directory.bin` 与 `macos-offset-range.bin` 实为**早期错误掩码运行**的产物，**3084/1797 个 dword 全为 0** ⇒ 任何基于它们的离线分析都是空的。已从**真实捕获**重建（`md5 182d7851c6fff2f28520846bae90b1df` / `9cba1f2228a50b82ecc175cc9e257c3a`；非零 dword 94 / 22，含 `0x190000`×3 与 `0xffffb6c2`×3）。教训与 §48.4 同源：**引用载荷必须带 artifact 与 md5**。

### 49.5 同一次运行里其余 id 的实测判读（补齐记录）

| id / 尺寸 | 结果 | 备注 |
|---|---|---|
| `0x2080901a @2052` | `0x1f` | 尺寸/戳不符，非"命令不存在"（阴性对照已定 `0x1b`/`0x56`） |
| `0x2080901c @808` | `0x65` TIMEOUT | 与既有记录一致 |
| `0x20809026 @5332` | `0x65` TIMEOUT | **随后把整个会话锁进 `0x62`** ⇒ 已登记进 `nvx_known_wedge_ids`；下次改发仓库内副本声明的 **2508**（`MASK-SEEDED at +4`） |
| `0x2080901d @32680` | **`NV_OK`**（352/8170 dwords changed） | 布局尚未定位（见 49.3）；是少数"被接受且真的填了内容"的大块 |
| `V2.15 perf-level-info-v2` | `0x1f` | 容量 0 → 0x0；容量 1 → `0x1f` ⇒ **驱动不暴露时钟域列表** |
| RUSD 读该帧 | `temp gpu=50.75C pstate 1 gpc 1530000 mem 6000984 fan 0 rpm` | 空转风扇 0 rpm 的**又一实例**（§34.2 的更正仍成立：本批某前置采样是 1613，两者并存） |

## 50. `0x2080901b` 的**有符号偏移落在哪一槽**：定位到 `rec+8`，`VALUES[2]` 那条反证不适用于本块（2026-10-18）

这是本目标最高优先里长期挂着的"尚未定位"项之一（写入门槛后的**回读必须知道看哪个 dword**）。本轮把它从"三选一"降到"一个有依据的读数 + 一个对非目标的挂账"。

### 50.1 依据之一：仓库内**调用者侧**字段表把第一槽命名为偏移

`nvapi-rs/sys/src/gpu/clock.rs` 的 `clk_ctrl_entry`（`NV_GPU_CLOCK_CLIENT_CLK_DOMAINS_CONTROL_V1`，magic `0x964`）逐字段给出：

| 常量 | 值 | 注释（原文） |
|---|---|---|
| `MASK` | 8 | controllable domain mask（输入＋回显） |
| `BASE` / `STRIDE` | 100 / 72 | 每域记录基址与步长 |
| `TYPE` | 0 | u8 类型判别 |
| `OFFSET_KHZ` | 44 | **signed kHz offset (i32)** |
| `RANGE_MIN` / `RANGE_MAX` | 48 / 52 | i32 kHz |
| `APPLIED` | 56 | i32 kHz |

⇒ 五槽里的**第一槽（调用者 +44）就是有符号偏移**。并且本驱动报出的记录类型是 **3/8/9**（目录实测），全部落在 `ClkDomainControlEntry::v1_marshalable`（`1|3|4|5|6|7|8|9`，`nvapi-rs/src/clock.rs`）里 ⇒ **V1 的命名适用于它们**；`v1_marshalable` 为真时 `values_kHz[0] = off`（`gpu.rs:1863` 起的 V1 回退分支逐槽赋值）也印证"槽 0 是偏移"。

### 50.2 依据之二：跨层字段差是**实测**的，且由值本身佐证

§27/§49 已记为：`0x20809025` 容器里调用者命名的 `REC_MAX_KHZ/REC_MIN_OFFSET/REC_MAX_OFFSET`（调用者 +0x2C/+0x38/+0x3C）与实际在 **RM `rec+8/+20/+24`** 读出的 `1638400 / -18750 / +18750` 成对——**三个字段对、同一个差 0x24**，其中两个（MIN/MAX_OFFSET）是**偏移语义**的字段，值是同一批实测值 ⇒ 该差不是推断而是测量，且线性规则由三个 k 值确定（k=1、2 由拟合得出，槽位为 `rec+8+4k`）。把同一差用于 `0x2080901b` 的调用者字段表 ⇒ **有符号偏移 = RM `rec+8`**。
另有一条独立一致性：§14 实测的写集合是每个 `0x08/0x09` 记录**连续 13 个 dword**（`rec+8 … rec+56`，`rec+8+4×12 = rec+56`）⇒ "调用者字段 k"与"RM 槽 k"是同一个下标，槽 k 位于 `rec+8+4k` ✔。

### 50.3 反证 `VALUES[2]` 的适用范围：它是**另一代**结构，且本驱动拒收那一代

- `clk_ctrl_entry_v2`（`NV_GPU_CLOCK_CLIENT_CLK_DOMAINS_CONTROL_V2`，magic `0x261A4` / 24996 B）用的是 `BASE 292 / STRIDE 772 / VALUES@268`，Blackwell 子表把 `FREQ_KHZ = 0x114` 标注为 `== VALUES[2]` ⇒ **V2 世代**里频率增量在第 3 个值槽，**不是**第 1 个。
- 但本驱动的 `0x2080901b` **只在 RM 尺寸 2108** 上给出过 `NV_OK`；把 V2 的**调用者尺寸 24996** 直接发到 RM 入口是 `0x1f`（`stage-clkvf-stamped` 记录的"满堂 0x1f"）。两层结构里 RM 层只认 RM 块，调用者走 V1 还是 V2 由上层 NVAPI 决定；R610 这一路（本机 `610.57.04`）对应 V1（2404）⇒ **V2 的值映射不是关于本块的证据**。
- 因此工具里"slot 0 还是 VALUES[2] 两说"的表述**收敛为**：本块取 slot 0 ⇒ `rec+8`；`VALUES[2]` 作为**另一代**记在注释里，不再与它并列悬置。

### 50.4 做成了什么（可执行）

- `probe-macos-rm/tools/decode-clk-containers.py`：CANDIDATES 注释改写为上述三条依据；新增 `cross_check_rust_field_map()` 并接进 `--selftest`——它**直接读** `nvapi-rs/sys/src/gpu/clock.rs` 与 `nvapi-rs/src/clock.rs`，断言 `OFFSET_KHZ/RANGE_MIN/RANGE_MAX/APPLIED = 44/48/52/56`、断言工具打印的五个槽恰等于"这四个加下一个，减去 0x24"、断言 `v1_marshalable` 仍覆盖 3/8/9、断言 V2 的 `BASE/STRIDE = 292/772` 属另一代。**任何一侧漂移都会让 selftest 报错**（已验证：把打印槽从 16 改成 28 会触发 `([8,12,20,24,28],[8,12,16,20,24])`）。
- `probe-macos-rm/tools/clock-offset-write-run.sh`：门控写实验的**候选顺序改为 `+8` 先、`+44` 后**（后者作为"若本容器差与 9025 不同"的兜底），并写清依据；`+56`（applied）仍作回读观察点。**这一步只改脚本与注释，未发任何写。**

### 50.5 仍然保留的不确定性（写进门槛前的最后一句）

差 0x24 是在 `0x20809025` 这一个容器上实测的；把它用到 `0x2080901b` 仍跨容器。**最终裁决只能是**：在写入门槛满足后，对单域做最小步长写入，并**在同一字段上回读**（写入 dword 读回 `orig+delta`、其余值 dword 逐字节不变、恢复后整块逐字节相同），且**不以实时频率变化为判据**。在此之前，`rec+8` 记为"有依据的读数"，`rec+44` 记为兜底。

---

## 51. `0x2080901d` 的"实测周期"现在由**产物**复核：能证明的是重复，不是步长；第三次出现才是判据（2026-10-18）

§49.3 记过一条实测观察（`0x2080901d` 的 A5 差分里有一处 128 字节的重复）并预登记了"第三次出现"作为判别。本轮把那条观察**变成可复核的检查**，同时把它的**射程**收窄到产物真正显示的范围——本轮**没有新发任何 GPU 请求**。

### 51.1 产物说什么（逐字可复核）

产物＝`outputs/stage-gsp-clkctl-20261017T092536Z/clkctl-offsweep-run1-A5.txt`（2026-10-17 捕获，md5 `bdb3db742ae9bd44990efbffa6eda8e0`，两端一致）。其中 `0x2080901d @32680` 那台的摘要行是 `352/8170 dwords changed (first @0 last @3376)`，而**逐条打印只给了前 40 个改动 dword**，末尾停在 `+184`。在这个窗口内：

- 改动集合包含 `+12 +16 … +56`（12 个 dword，连续）与 `+140 +144 … +184`（同样 12 个，连续），两组**位置一一对应、相距 128**；
- 值里仍带 `0xa5` 字节（例如 `+56=0x00850085`、`+184=0x00860086`、`+52=0xff1f00a5`、`+180=0xff2100a5`）⇒ 驱动写的是**子 dword 字段**，不是整 dword；两组的差别只在那一两个字节（`0x1f/0x21`、`0x85/0x86`）。
- 窗口之外（`+184` 到 `+3376`）**产物里没有数据**。

### 51.2 这不是"惯例之争"，而是两种结构

128 字节周期**不能**铺满四个算术解中的任何一个：`1012..1020 B = 7*128 + 116..124`。反过来，若记录真的是 128 字节间隔，`32680` 只有**唯一**一个自洽切分：`40 + 255*128 = 32680`（即 255 条记录），而不是 `0x2080901b` 那套 **32 条**。所以"40/1020（32 条记录）"与"40/128（255 条记录）"是**两种不同的东西**，不能靠算术二选一——只能靠第三次出现的位置。**本轮不把 `GEOMETRY[0x2080901d]` 从"仅算术"升级为"已验证"**，只把观察升级为"产物复核"。

### 51.3 与 §49.3 记的 `(36, 164)` 的关系（不是更正，是口径）

§49.3 记的组起点 `(36, 164)` 是同一处重复再往里 24 字节的读法（`+36` 与 `+164` 在产物里同样都改动过）。两种读法都对，但**判别语句必须与口径无关**，因此 §51 的预测写成"从第二次出现 `+140` 起算"：周期 128 ⇒ 第三次在 **+268**；步长 1020 ⇒ 第三次在 **+1160**。旧的 `+292`/`+1060` 与这两个数只差那 24 字节的口径。

### 51.4 可证伪的下一步（唯一能定这一步的观察）

**一个完整的 `0x2080901d` payload（非零背景 0xA5）**，把 `+268` 与 `+1160` 两处都落在打印窗口内：出现 `+268` ⇒ 周期 128，`(40, 1020)` 那套被否；出现 `+1160` ⇒ 32×1020 成立，128 只是记录内部的子结构；两处都没有 ⇒ 两种读法都不对，回到产物重看。**这正是健康窗口解除后最省的一次只读**（一次 GET，32680 B）。

### 51.5 做成可执行的复核（含反例）

- 新增 `probe-macos-rm/fixtures/tu116-1d-32680-A5.txt`（产物逐字节副本，配 `fixtures/README.md` 记来源与两条读法注意）与 `check_measured_repetition()`：**先校验 md5**（产物漂移即失败），再从字节里重新求出 §51.1 那两组位置、断言 `max(changed) == +184 < +3376`（即"窗口是被截断的前 40 个，不是整个差分"）、断言摘要行确实在产物里、断言 `(32680-40) % 128 == 0 且 (32680-40)//128 == 255`。
- **反例**已接进 `--selftest`：改动产物一个字节 ⇒ 必须撞 md5 闸门；把 md5 重新指向变异体 ⇒ 必须撞"重复不成立"断言（两条都在本机与测试机上跑过）。
- 顺带修掉一个 CI 相关的小缺陷：该文件此前**不满足** `ruff format`（仓库 CI 第 31 行对根目录跑 `ruff format . --check`，`probe-macos-rm/tools/` 在范围内），现已格式化并通过 `ruff check`。
