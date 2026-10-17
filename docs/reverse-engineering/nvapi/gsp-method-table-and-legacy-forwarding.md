# The GSP answers the private clock/voltage ids (macOS, TU116, 610.57.04)

> Status: reverse-engineering record, read-only. Host evidence 2026-10-17,
> macOS 15.8.1 / x86_64 / GTX 1650 SUPER (TU116, 10de:2187) / NullMoth 2.0.4 /
> NVRM 610.57.04. This note **supersedes** §4 of
> `nvrm-610-method-table-and-dispatch.md`, which concluded the responder was
> "not located" and asserted that no GSP firmware exists on the host.

## 1. Correction summary

Three earlier claims were wrong and are corrected here:

| earlier claim | correction |
| --- | --- |
| "the responder is not located; TU116 runs host-side RM without GSP" | the responder **is the GSP firmware** reached by the host's legacy-non-privileged forwarding path |
| "no GSP firmware file exists under `/Library/Extensions`, `/Library/NullMoth` or `/usr/local`" | the firmware lives at `/Users/Shared/nvfw/nvidia/610.57.04/{gsp_tu10x.bin,gsp_ga10x.bin,ucodes_*.bin}` — outside every path that was searched |
| "the private ids have no implementation body on this machine" | they have none in the host NVOC table, but **169 CLK + 151 VOLT rows** exist in the GSP's own export table |

The host table result is unchanged and still true: the loaded NVRM's NVOC
export table has **1** CLK row (`0x20801037/6024`), **0** VOLT rows and **0**
legacy `0x90/0xb2/0xf2` rows. That table is simply not the only dispatch path.

## 2. The dispatch chain (`NV_ESC_RM_CONTROL`)

Verified line-by-line against OGKM 610.57.04 (the exact tree NullMoth rebuilt)
and the NullMoth kext source:

```
NVRMUserClient::esc(NV_ESC_RM_CONTROL)        kexts/NVRM/NVRM.cpp:1812
  -> rm_ioctl(...)
  -> escape.c:793  case NV_ESC_RM_CONTROL
  -> Nv04ControlWithSecInfo -> _nv04ControlWithSecInfo
  -> RmDeprecatedGetControlHandler(pArgs)     entry_points.c:498
       rmapi_deprecated_control.c:100
       - rmDeprecatedControlTable rows = the 8 host NVOC ids only
       - otherwise IsGssLegacyCall(cmd)       rmapi_deprecated_control.c:95
           == !!(cmd & RM_GSS_LEGACY_MASK)    rmapi_deprecated.h:41
           RM_GSS_LEGACY_MASK = 0x00008000
         guarded by IS_GSP_CLIENT(pGpu)       rmapi_deprecated_control.c:133
  -> RmGssLegacyRpcCmd(...)                   rmapi_gss_legacy_control.c:38
       comment in source: "We no longer support these in RM, but until all the
       numerous tools are updated ... just forward all of them to GSP and let
       it deal with what is or isn't valid."
       local work is only:  (a) ptr/size null consistency,
                            (b) privileged bit 0xC000 vs RS_PRIV_LEVEL_USER_ROOT
                                -> 0x1b,
                            (c) translate + pRmApi->Control(PHYSICAL, ...)
```

Consequences, each of which matches a previously unexplained observation:

* **no host table row, no id literal in any host binary** — the host only tests
  bit `0x8000`; the id is opaque to it. `0x2080901b & 0x8000` is `0x8000`, so
  it takes this path.
* **`0x2080FFFF` -> `0x1b INSUFFICIENT_PERMISSIONS`** — `0xFFFF` carries both
  legacy bits, `0xC000` is the privileged pair, and the probe process is not
  root. This is a *privilege* answer, not "command does not exist".
* **`0x2080A0FF` -> `0x56 NOT_SUPPORTED`** — forwarded successfully; the GSP
  has no such method.
* **size-dependent `0x1f`** — the GSP validates its *own* `paramSize`, so the
  NVAPI-layer struct size is the wrong number to send (see §4).
* **`--service nvrm` and `--service nvaccel` answer byte-identically** — both
  user clients reach the same GSP.

## 3. The GSP's own export table

`gsp_tu10x.bin` is an ELF64 RISC-V image (`e_machine=243`) with the payload in
`.fwimage` (file offset `0x40`, size `0x1c01dc8`, load VA base `0x4000000`).
Inside it is the method table the GSP dispatches on. Extracted with
`probe-macos-rm/tools/extract-gsp-method-table.py`:

```sh
python3 probe-macos-rm/tools/extract-gsp-method-table.py \
        /Users/Shared/nvfw/nvidia/610.57.04/gsp_tu10x.bin > gsp-table.tsv
# 889 rows
```

Row layout (32 B): `+0 methodId(u32) +4 paramSize(u32) +8 pClassInfo(u64)
+0x10 pFunc(u64) +0x18 flags(u32) +0x1c accessRight(u32)`; pointers are image
VAs. Category histogram puts **cat 0x10 (CLK) = 169 rows** and
**cat 0x32 (VOLT) = 151 rows**, vs **1** and **0** in the host table.

Authoritative `paramSize` for the ids this effort uses (GET unless marked):

| id | GSP paramSize | notes |
| --- | --- | --- |
| 0x20809019 | 12336 | 48 B head + 32×384 B records |
| 0x2080901a | 2052 | |
| 0x2080901b | 2108 | 60 B head + 32×64 B records |
| 0x2080901c | 808 | |
| 0x2080901d | 32680 | |
| 0x20809021 | 33288 | VFP get-info |
| 0x20809022 | 623112 | VFP get-status |
| 0x20809023 | 66060 | VFP get-control |
| 0x20809025 | 7188 | 20 B head + 32×224 B records |
| 0x20809026 | 5332 | |
| 0x20809061 | 776 | the live kHz table |
| 0x2080b201 | 2444 | rails get-info |
| 0x2080b213 | 1036 | rails get-control |
| 0x2080d01c | 2108 | **SET** twin of 0x2080901b |
| 0x2080d024 | 66060 | **SET** twin of 0x20809023 |
| 0x2080f214 | 1036 | **SET** twin of 0x2080b213 |

Not present in the GSP table at all: `0x20809006` (CLK_MEASURE_FREQ) and
`0x20809062` — so the `8` and `648` sizes used earlier were never derivable.

Note the two families do **not** share a record size: 0x20809019 uses 384 B
records, 0x2080901b 64 B, 0x20809025 224 B.

The GET->SET pairs in this table are separated by `+1` in the low byte with the
category byte moving `0x90`->`0xd0` (`0xb2`->`0xf2` for the VOLT block), and
each pair carries an **identical `paramSize`**; the full list is in §5.2.

## 4. `0x2080901b` GET: the input mask is at **+4**, and V1 returns types only

`nvapi-rs` (`clock.rs` `NV_GPU_CLOCK_CLIENT_CLK_DOMAINS_CONTROL_V1`) records the
NVAPI *caller* struct as 2404 B with `mask@+8` and 32×72 B records at `+100`.
That is the Windows/NVAPI view. The RM block the GSP wants is **2108 B**, and
its mask is at **+4** — established by feeding a `0xA5A5A5A5` background (so a
driver write of *zero* is no longer invisible) and sweeping one header dword at
a time with a `0xFF` seed:

| seed position | status | effect |
| --- | --- | --- |
| +0, +8, +12 … +56 | `0x25 NV_ERR_INVALID_DATA` | non-zero anywhere but the mask field is rejected |
| **+4** | `NV_OK` | the driver fills records: bytes `+60 = 08 08`, `+124 = 09 09`, and zeroes the value dwords |

The two type bytes `0x08` and `0x09` are exactly what `nvapi-rs` documents for
live Turing: *"Turing live (RTX 2070) mixes {0x08, 0x09}, Ada live (RTX 4060
Laptop) reports 0x0A."* So the records **are** the clock-domain records and the
record base is `+60`, stride 64.

The **value** dwords however stay zero. That matches `nvapi-rs`'s description
of the V1 handler verbatim: *"V1 silently drops those records (the type dword
is still written on GET, the value dwords never are)"*, and the same paragraph
says **"V2 is the REAL read/write path"**. So on this box
`0x2080901b@2108` is the V1 form: it confirms which domains exist and their
record type, and it does **not** hand back an offset or a range.

Earlier attempts put the mask at `+0` or `+8` (from the NVAPI caller layout);
both are valid-but-ignored, which is why every earlier run came back with a
populated header and empty records.

## 5. Neighbouring containers that *do* carry values

All GET, all at the GSP's own size, all with a zeroed input block:

| id @size | status | what came back |
| --- | --- | --- |
| `0x20809019 @12336` | `NV_OK` | 32×384 B record table, 9 records populated; record type bytes `{08,09,08,09,09,09,03,08,09,09}` — i.e. the Turing mix again |
| `0x2080901d @32680` | `NV_OK` | 352/8170 dwords written; header `+0` low byte `0xff`, `+4 = 0x07ffffff`; per-record fields at `+56 = 0x00850085`, `+144 = 0x00ff0093`, `+184 = 0x00860086` |
| `0x20809025 @7188` | `NV_OK` | 20 B head + 32×224 B records, 3 records populated; each record ends `… ffffb6c2 0000493e` = **−18750 / +18750 kHz** around a `0` offset field, i.e. a symmetric ±18.75 MHz *offset* range |
| `0x20809061 @776` | `NV_OK` | the live per-domain kHz table (already verified against RUSD) |

`0x20809019` and `0x2080901d` carry a record **type** table; `0x20809025` is the
only container so far that carries an explicit offset **range**. None of them
has yet been shown to hand back a *current offset* value, and none has been
written to.

### 5.1 `0x20809025 @7188` decoded: the per-domain offset **range**

Geometry comes from the two independent sources agreeing: the GSP table says
`7188` and the payload is exactly `20 + 32 x 224`, with the mask at `+4`. The
zeroed read returns mask `0x7` and **three** populated records (controller bits
0/1/2). Field values below are read verbatim out of
`stage-gsp-clkctl-20261017T092536Z/clkctl-hex.txt`:

| record field | live value | reading |
| --- | --- | --- |
| head `+0` | `0xff` | u8 info/count byte the driver writes over the zero seed |
| head `+4` | `0x7` | supported-domain mask — bits 0,1,2 |
| head `+12` / `+16` | `100` / `0x100` | un-attributed header words |
| rec `+0` (u32) | `0x01030101` `0x01040101` `0x01050101` | `{u8 1, u8 type=3/4/5, u8 1, u8 1}` |
| rec `+8` (u32) | `1638400` | **max VF frequency, kHz** (= 1638.4 MHz) |
| rec `+20` (i32) | `-18750` | **offset floor, kHz** |
| rec `+24` (i32) | `+18750` | **offset ceiling, kHz** |
| rec `+28` (u32) | `0x00ff0100` | flags |

The record types 3/4/5 are the NAFLL-style domain ids, so the three offsets
that this TU116 lets you address are *not* the {0x08,0x09} record types of
§4 — they are a different (smaller) set, which is why §4's container cannot be
the range source.

**Cross-check.** `nvapi-rs`'s `clk_freq_ctrl_info_entry` describes the same
controller tree as reached from Windows (magic `0x10C4C`, `3148 = 76 + 32x76`,
records `0x60` B) and records, for a TU116 with mask `0x7`, exactly
*"records carry {1638400 max_kHz, -18750/+18750 offset pair}"*. The three
values match this box to the digit, and their relative gaps agree in both
variants (`+8/+20/+24` here vs `+0x2C/+0x38/+0x3C` there is a constant `0x24`
shift). So the GSP serves a *wider* record of the **same** controller tree: the
macOS block is not a different control, it is a newer/other revision of the
same struct.

What this buys the goal: a **believe-your-eyes per-domain offset range**
(+/-18.75 MHz on three domains) at the exact size the GSP accepts. What it does
*not* give: the currently applied offset, the write granularity, or any proof
that `0x2080d01c` accepts the same record. **No SET was sent.**

### 5.3 `0x20809019 @12336` decoded: the domain catalogue

Geometry `48 + 32 x 384`, header `+0` low byte `0xff`, `+4 = 0x3ff` (ten
domains supported), `+8 = 0x335`. Nine records are populated and they carry
more than a type byte:

| record | type bytes `rec+0` | `rec+4` domain mask | `rec+28` version-ish pair | other live fields |
| --- | --- | --- | --- | --- |
| 0 | `08 08 01 00` | `0x1` | `0x0b0b` | `+40=0x7530`, `+44`, `+48=0x01000103`, `+52=0xffff`, `+76=0x22a` |
| 1 | `09 09 01 00` | `0x2` | `0x1717` | `+12=0xffffffff`, `+16=0xff00`, `+48=0x01010204` |
| 2 | `08 08 01 00` | `0x10` | `0x0a06` | `+12=4`, `+16=0x100`, `+44`, `+76=0x100` |
| 3 | `09 09 01 00` | `0x4` | `0x1919` | `+12=0xffffffff`, `+16=0xff00` |
| 4 | `09 09 01 00` | `0x8` | `0x110c` | `+12=0xffffffff`, `+72=0x700` |
| 5 | `09 09 01 00` | `0x100000` | `0x1818` | `+12=0xffffffff` |
| 6 | `03 03 01 00` | `0x80000` | — | `+16=0x21cff00` |
| 7 | `08 08 01 00` | `0x40` | `0x0500` | `+76=0x10` |
| 8 | `09 09 01 00` | `0x200000` | `0x1612` | `+72=0x200` |

The `rec+4` values are single bits of a 22-bit domain mask (bits 0,1,4,2,3,20,
19,6,21,5), which is the cleanest available mapping from "controller bit N" to
"domain type" on this part. Whether the `rec+40/+44` kHz-scale words are limits
or current values is **not** established here and must not be assumed.

### 5.2 GET -> SET pairing inside the legacy CLK block (a table fact)

The GSP export table lists both directions. Across the block the pairs are
separated by **`+1` in the low byte**, with the category byte moving
`0x90` (GET) -> `0xd0` (SET); every pair is then corroborated by an
**identical `paramSize`**, and since the GSP validates the size exactly (§3)
that equality is strong corroboration rather than a naming guess:

| GET | size | SET | size |
| --- | --- | --- | --- |
| `0x2080901b` | 2108 | `0x2080d01c` | 2108 |
| `0x2080901f` | 19416 | `0x2080d020` | 19416 |
| `0x20809023` | 66060 | `0x2080d024` | 66060 |
| `0x20809027` | 6288 | `0x2080d028` | 6288 |
| `0x2080902c` | 4116 | `0x2080d02d` | 4116 |
| `0x20809030` | 1544 | `0x2080d031` | 1544 |
| `0x20809046` | 8264 | `0x2080d047` | 8264 |
| `0x208090a2` | 524 | `0x2080d0a3` | 524 |
| `0x208090b2` | 812 | `0x2080d0b3` | 812 |
| `0x2080b207` | 392 | `0x2080f208` | 392 |
| `0x2080b211` | 520 | `0x2080f212` | 520 |
| `0x2080b213` | 1036 | `0x2080f214` | 1036 |
| `0x2080b218` | 9220 | `0x2080f219` | 9220 |

Two things follow that change the plan:

* `0x20809019` (12336, domains info), `0x20809025` (7188, freq-controller info)
  and `0x20809026` (5332, freq-controller status) have **no SET twin in this
  table at all** — they are read-only views of the domain set. So the
  ClkFreqController `+/-18.75 MHz` value in §5.1 is a *limit* source only.
* the only write face in the block that pairs with a clock-domain control GET
  is **`0x2080d01c @2108`**, the SET twin of the §4 GET. That is therefore the
  single candidate for a domain-offset *write* on this box, and it stays
  unsent until §4's GET returns a current value and its range (`WRITE-VERIFICATION-PLAN.md`).

## 6. Safety: a bad private GET can wedge the GSP for the session

Observed 2026-10-17, in order, all read-only, same user client:

```
0x2080901c @808   (0xA5 background) -> 0x65 NV_ERR_TIMEOUT
0x20809026 @5332  (0xA5 background) -> 0x65 NV_ERR_TIMEOUT
then, for the rest of the session:
0x20809019 @12336 -> 0x62 NV_ERR_RESET_REQUIRED
0x20809061 @776   -> 0x62 NV_ERR_RESET_REQUIRED
NV00DE RUSD page  -> 0x62 NV_ERR_RESET_REQUIRED
```

The state did not clear on its own over 40+ minutes and with new user clients.
Only a driver reload / reboot resets it; both are **out of bounds** for this
effort, so no attempt was made. The host stayed healthy throughout
(`WindowServer` uptime unbroken, no panic report, `uptime` continuous).

Because of this, `--clkctl-offsweep` no longer fills any id except `0x2080901b`
with a non-zero background (the one id that provably rejects bad content with
`0x25` instead of timing out), and the value containers are read with a zeroed
block only.

**This is a hard safety boundary for the whole clock-offset campaign**: three
read-only GETs were enough to take the GSP's legacy path down for the session,
without any SET being sent. Any further probe of this family has to treat a
`0x65 TIMEOUT` as "stop immediately", not "retry with a different size".

### 6.1 The wedge is a live "is this GSP-dispatched?" oracle

`--reg-read` re-sends every *host-table-registered* id at its registered size.
Run while the wedge was active, it split them cleanly:

| outcome | ids |
| --- | --- |
| `0x62 NV_ERR_RESET_REQUIRED` | `0x20802609` pmgr-get-module-info, `0x20801037` clk-pmumon-domains, `0x2080200b` perf-level-info-v2, `0x2080205a`, `0x20802068`, perf-get-vid-eng-perfmon, perf-get-gpumon-util-samples |
| `NV_OK` | `0x2080122a` kgr-get-gfx-gpc-tpc-info, kgr-get-gpc-mask, fb-get-static-bar1-info |
| `0x56 NOT_SUPPORTED` | the internal-pm / memsys / timer family |
| `0x1f INVALID_ARGUMENT` | `0x20800102`, `0x20801201`, `0x20801303` |

The `0x62` set is exactly the set that was `NV_OK` before the wedge and that
also goes to the GSP — *including* ids that **are** in the host table. So being
registered in the host NVOC table does **not** mean the host answers a command;
several registered perf/pmgr/clk rows are GSP-implemented too. A `0x62` sweep
is therefore a cheap way to tell host-implemented rows from GSP-dispatched
ones, without touching the firmware.

## 7. `0x2080b201` is answered *outside* the GSP path

While every GSP-forwarded id above answered `0x62 NV_ERR_RESET_REQUIRED`,
`0x2080b201 @2444` kept answering `NV_OK` with its usual
`mask 0xFF + 800000 µV / 756250 µV` payload — while *registered* GSP-backed
commands like `0x2080200b` and `0x20802609` were `0x62` (see §6.1). So the
wedge covers the GSP service generally, not just the legacy forward, and
`0x2080b201` is not served by it at all, despite carrying the `0x8000` legacy
bit. It is either a host-side handler or a different transport. This finally gives
the long-standing "unattributed responder" of §4 a definite counter-example:
at least one accepted private id survives a GSP wedge, and at least one does
not.

## 8. Reproducing

```sh
# on the test machine, read-only; needs the GSP alive (see §6)
OGKM=$HOME/Desktop/macOS-nvoc-develop/reference/open-gpu-kernel-modules-610.57.04
cd probe-macos-rm && OGKM=$OGKM sh build.sh
python3 tools/extract-gsp-method-table.py \
        /Users/Shared/nvfw/nvidia/610.57.04/gsp_tu10x.bin > /tmp/gsp-table.tsv
./probe-macos-rm-idle --clkctl-read     # GSP-sized GETs, incl. mask@+4
./probe-macos-rm-idle --clkctl-hex      # full hex of 12336 / 7188 blocks
./probe-macos-rm-idle --clkctl-offsweep # 0x2080901b A5 sweep ONLY (see §6)
```

## 9. What is still missing

* a container that returns the **current** per-domain offset. Candidates, in
  the order they should be tried once the GSP is alive again:
  1. `0x20809026 @5332` (freq-controller **status**, the natural status half of
     §5.1's info container) — with a **zeroed** block only. Every previous
     attempt used a `0xA5A5A5A5` background and is therefore void (see §6);
     `5332 = 84 + 32x164` is the only header/stride split that puts the records
     on 4-byte boundaries.
  2. `0x2080901b @2108` re-read with the §4 `+4` mask and a **zeroed**
     background, to check whether the `08`/`09` records are really
     "types only" or whether a single-sourced mask leaves fields the `0xA5`
     sweep could not distinguish from a driver-zeroed word.
  3. `0x2080901a @2052` (`4 + 32x64`) — same record shape as §4 with a 4-byte
     head; never read with a zeroed block.
* the **write** half: `0x2080d01c @2108` (§5.2). Not sent, and not sendable
  until the GET half yields current/limit/state. The step granularity is still
  unknown (the `+/-18750 kHz` pair is the *range*, not a step), so the
  "minimum step" for a first write is not yet defined.
* the V2 form of the clock-domain control — `nvapi-rs` says the value dwords
  live there, and the GSP table does not offer a second size for
  `0x2080901b`, so V2 is either versioned inside the same 2108 B block or not
  available on this TU116 GSP;
* confirmation of §4's `+4` mask field with a zeroed background (blocked: the
  GSP is wedged — see §6).

**State recheck 2026-10-17T09:40Z**: `--clkctl-hex` (the two safe blocks only)
still returns `0x62 NV_ERR_RESET_REQUIRED` for both, so the wedge of §6 has
held for the whole session. Recovery needs a reboot (or a kext reload, which
needs root and is outside the standing permission boundary); nothing was
reloaded, no driver/EFI state was touched. All of `--clkctl-read`,
`--clkctl-hex`, `--clkctl-offsweep`, `--clkdom-read`, `--clkvf-read`,
`--reg-read` on this box are **read-only** and refuse the SET ids before
sending.
