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
background so a skipped field stays visible: seeding `0xFF` at +4 answers `NV_OK`
and writes 106 dwords (`first @0 last @564`), i.e. the handler reads the mask and
never writes it back -- it is the only head dword that is not rewritten. Seeding
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
