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
