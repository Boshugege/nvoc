# The GSP answers the private clock/voltage ids (macOS, TU116, 610.57.04)

> Status: reverse-engineering record, read-only. Host evidence 2026-10-17,
> macOS 15.8.1 / x86_64 / GTX 1650 SUPER (TU116, 10de:2187) / NullMoth 2.0.4 /
> NVRM 610.57.04. This note **supersedes** §4 of
> `nvrm-610-method-table-and-dispatch.md`, which concluded the responder was
> "not located" and asserted that no GSP firmware exists on the host.

## 1. Correction summary

Five earlier claims were wrong and are corrected here:

| earlier claim | correction |
| --- | --- |
| "the responder is not located; TU116 runs host-side RM without GSP" | the responder **is the GSP firmware** reached by the host's legacy-non-privileged forwarding path |
| "no GSP firmware file exists under `/Library/Extensions`, `/Library/NullMoth` or `/usr/local`" | the firmware lives at `/Users/Shared/nvfw/nvidia/610.57.04/{gsp_tu10x.bin,gsp_ga10x.bin,ucodes_*.bin}` — outside every path that was searched |
| "the private ids have no implementation body on this machine" | they have none in the host NVOC table, but **169 CLK + 151 VOLT rows** exist in the GSP's own export table |
| "`0x2080b201` is answered outside the GSP path" (first version of §7) | it is **not** host-side — the host has no CLK/VOLT row and no id literal at all (§7); it is answered by the GSP, which survives a wedge that kills its own category sibling `0x2080b202` |
| "a `0x62` sweep tells host-implemented rows from GSP-dispatched ones" (first version of §6.1) | refuted by that same pair: the gate is **per command**, so `0x62` classifies a *gated handler/subsystem*, not a dispatch path (§7) |

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

### 2.1 Privilege is encoded in the *id*, and every clock/voltage SET carries it

`rmapi_deprecated.h:39-42` defines the legacy mask as a pair, not a single bit:

```c
#define RM_GSS_LEGACY_MASK                   0x00008000
#define RM_GSS_LEGACY_MASK_NON_PRIVILEGED    0x00008000
#define RM_GSS_LEGACY_MASK_PRIVILEGED        0x0000C000
```

and `RmGssLegacyRpcCmd` (`rmapi_gss_legacy_control.c:44-52`) gates on the
privileged half **before** it looks at a single byte of the parameters:

```c
NV_ASSERT_OR_RETURN((pArgs->cmd & RM_GSS_LEGACY_MASK), NV_ERR_INVALID_STATE);
if (((pArgs->cmd & RM_GSS_LEGACY_MASK_PRIVILEGED) == RM_GSS_LEGACY_MASK_PRIVILEGED) &&
    (pSecInfo->privLevel < RS_PRIV_LEVEL_USER_ROOT))
{
    return NV_ERR_INSUFFICIENT_PERMISSIONS;
}
```

So bit 15 alone means "forwarded, non-privileged", and bits 15+14 mean
"forwarded, but only for a client at `RS_PRIV_LEVEL_USER_ROOT` or above",
refused with `0x1b` before the GSP is consulted. That single line is the whole
explanation of the long-standing `0x2080FFFF -> 0x1b` (§2) and of why no
private **write** has ever been reachable from this un-privileged client: the
write half of a private pair is, by construction, the read id with the
privileged bit OR'd in. `0x2080901b` (read) vs `0x2080d01c` (write) is exactly
`0x90 -> 0xd0`, i.e. `|0x4000`, *and* `+1` in the low byte.

Counted over the GSP's own 889 rows (each of which is only ever reached *after*
the host has decided to forward):

| GSP table rows carrying | count | of which carry `0xC000` (privileged) |
| --- | --- | --- |
| the write flag (`flags & 0x04`) | 159 | 80 |
| the read flag (`flags & 0x08`) | 513 | 9 |
| the write flag **and** the legacy bit (15 set) | 82 | **80** |

Of the 82 write ids the host will forward at all, 80 are privileged. The two
exceptions are `0x2080854a` (2 B) and `0x2080b703` (75008 B); neither is a
clock-domain or clock-offset control. Every clock/voltage SET identified by
this effort — `0x2080d01c`, `0x2080d0a3`, `0x2080d024`, `0x2080f214` — is in
the privileged 80. What that means for the write plan is in §9.

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
VAs. `flags` is the access class, and the low nibble picks the direction: `0x48`
(read) or `0x44` (write), both carrying `0x40`. 368 rows are `0x48`, 121 are
`0x44`, and 159 rows carry the write bit somewhere in their flags. That field
is what decides whether a row is a *writer*, independently of any `+1` naming
convention (§5.2). Category histogram puts **cat 0x10 (CLK) = 169 rows** and
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
| +0, +8, +12 … +56 | `0x25 NV_ERR_INVALID_DATA` | the block is refused; only the `+4` seed makes the same `0xA5` block acceptable |
| **+4** | `NV_OK` | the driver fills records: bytes `+60 = 08 08`, `+124 = 09 09`, and zeroes the value dwords |

The sweep's information is *"which single dword makes this block acceptable"*,
not *"which dword is ignored"*: with an `0xA5` background every other dword is
still non-zero, so exactly one field is interpreted — the mask at `+4`.

The two type bytes `0x08` and `0x09` are exactly what `nvapi-rs` documents for
live Turing: *"Turing live (RTX 2070) mixes {0x08, 0x09}, Ada live (RTX 4060
Laptop) reports 0x0A."* So the records **are** the clock-domain records and the
record base is `+60`, stride 64.

The **value** dwords however all come back zero -- and §4.2 shows the driver
*wrote* them (they change from `0xA5` to `0`), so they are marshalled fields
whose stock value is 0, not fields the handler skips. That distinction matters,
because the usual way this was read conflates two layers. `nvapi-rs` records
*"V1 silently drops those records (the type dword is still written on GET, the
value dwords never are)"* and **"V2 is the REAL read/write path"** -- but that
describes the **NVAPI-side** handler that copies an RM block into the NVAPI
*caller* struct (2404 B, `mask@+8`, 32x72 B records at `+100`). Here the RM
block itself is the 2108 B one, and its records *are* filled:

| layer | what it is | status on this box |
| --- | --- | --- |
| L1 NVAPI caller struct | 2404 B, `mask@+8`, records `+100`/72 B | what `nvapi-rs` describes (Windows) |
| L2 Windows escape container | `0x07000109`-family | not used on macOS |
| L3 RM control param | 2108 B, `mask@+4`, records `+60`/64 B | what the GSP validates |
| L4 per-domain record inside L3 | 64 B, 13 dwords written | written by the GSP handler |

An L1 drop does **not** prove an L4 drop. What is still undecided is narrower
than before: whether one of those 13 dwords *is* the current offset, which would
become non-zero once an offset is applied. At stock every one of them is `0`, so
a stock read cannot pin the field map -- it needs either an independent source
for the same field, or a single gated minimum-step write with a same-field
readback (write gate G1 in `WRITE-VERIFICATION-PLAN.md`).

Earlier attempts put the mask at `+0` or `+8` (from the NVAPI caller layout).
Under a zeroed background those answer `NV_OK` too, but only because the real
mask field at `+4` is then still `0` — a zero mask is accepted and selects no
records, which is exactly the "populated header, empty records" shape of those
runs. A zero mask is therefore kept as the deliberate *no records* baseline
case in `--clkctl-hex` (§8), next to the `+4 = 0xFF` case.

### 4.1 Why no `0x2080901b` handler body can be found by scanning

All 889 table rows have their *own* `pFunc` (889 rows, 889 distinct values), so
`pFunc` is not a shared dispatcher and the id is only a table row. Each `pFunc`
is a ~30-instruction thunk of the same shape; the shape — not a literal —
carries the method identity:

```asm
ld   a5, 0x158(a0)      ; RM client
ld   s1, 0xe0(a5)       ; device
addi a5, s1, 0x2000     ;
ld   a5, -0x78(a5)      ; X = device->[0x1f88]      (0 -> error path)
addi a5, a5, 0x1000     ;
ld   a0, 0x148(a5)      ; iface = X->[0x1148]        (0 -> error path)
ld   a5, 0x20(a0)       ; iface->[0x20]              (0 -> error path)
ld   a5, 0(a5)          ;
jalr a5                 ; iface->vtbl[0](iface, 0)  (0 -> error path)
ld   a5, 0x38(a0)       ;
jr   a5                 ; ret->vtbl[0x38](device, ret, params)
```

so the marshalling for the `0x2080901b` GET lives in the **seventh slot of a
runtime object's vtable**, and for the `0x2080d01c` SET (`pFunc 0x4d5c3c0`) in
the **fifth slot of a different object**:

| | `0x2080901b` (GET) | `0x2080d01c` (SET) |
| --- | --- | --- |
| dispatch base | `X->[0x1148]` | `X->[0x1150]` |
| extra preconditions | none | `*(s1+0x2050) != 0`, then its `+0x4f4` byte `!= 0`, and the `+0x179` byte of `X->[0x1148]` `== 0` |
| slot invoked with the param block | `+0x38` | `+0x28` |

Every failing branch loads a packed constant (`lui a1, 0x5f2d` / `addi a1, a1,
…`) that resolves outside the `.fwimage` mapping, so it is a log/service tag
rather than a printable message.

Two consequences. (a) A literal or `nm` scan over the firmware cannot find a
`0x2080901b` body: the id exists only as a table row and the body is a vtable
slot whose object is built at runtime. (b) The SET path is *not* the GET path
with the other direction — it additionally refuses to run unless two flag bytes
on two different objects are in the required state. Those two flags are the
natural first suspects for a "clock control enabled" precondition, and they are
why a first write attempt has to re-verify the gates instead of assuming that
the GET's success implies the SET's.

### 4.2 What the mask selects, and the record shape (measured, not fitted)

Three logs pin the geometry down. The third one corrects an arithmetic error in
the first two-read model (see "the earlier model was wrong" below).

**(a) `0xA5` background, `mask@+4 = 0xFF`** (`clkctl-offsweep-run1-A5.txt`):
header says `106/527 dwords changed (first @0 last @564)`. With an all-`0xA5`
pre-seed *every* write changes a dword (no field legitimately holds
`0xA5A5A5A5`), so changed == written. The run's per-dword list was capped at 40
lines by the then-current build; it shows, in address order, the writes
`0, 8, 12, 16, 20, 24, 28, 32, 36, 40, 44, 48, 52, 56` (14 dwords below the
first record) and then two 13-dword groups at `60+64k+{0,8,12,16,20,24,28,32,
36,40,44,52,56}` for `k = 0, 1`.

The same log is also the only record of the write *widths*, and they are not
uniform — the delta lines show exactly which bytes moved under an all-`0xA5`
pre-seed:

| offset | value written | width |
| --- | --- | --- |
| `+0` | `0xff` | **u8** (`a5a5a5a5 -> a5a5a5ff`: only byte 0 moved) |
| `+8` | `0x01010000` | u32 |
| `+12` | `0` | **u16** (`a5a5a5a5 -> a5a50000`) |
| `+16, +20, +24, +28, +32, +36, +40, +44, +48, +52` | `0` | u32 each |
| `+56` | `0` | **u8** (`a5a5a5a5 -> a5a5a500`) |
| `+60`, `+124` (record stamps) | `0x0808`, `0x0909` | **u16** each |

so the head's "14 writes" are 1 byte + 1 dword + 1 u16 + 10 dwords + 1 byte,
and a record's 13 slots are 1 u16 stamp + 12 dwords. That mix is evidence in
itself: `+12` and `+56` are dwords *inside a record* but u16/u8 at the same
block offsets in the head, so the head is a different struct, not "record 0".

**(b) small-counter background (`dword[i] = i+1`), `mask@+4 = 0x2`**
(`clkctl-read-mask-sweep-lite.txt`, case `counter`): `U=500 W=3 Z=24`, with
`W +0:s00000001>w000000ff`, `W +8:s000000ff>w01010000`,
`W +124:s00000020>w00000909`. The `Z` runs are the writes of a zero value that
a zeroed background cannot show, and the verbatim dump confirms they are
`+12..+56` (12 dwords) and the record at `+124`'s twelve value slots. This is
the measurement that fixes the head: **14 writes**, not 2.

**(c) same run, `mask@+0 = 0xFF` (`in@0=FF cnt`)**: `W +8 +124` only, `Z=24`,
i.e. the identical body. The `+0 = 0xFF` write is invisible here only because
`+0` was seeded `0xFF` by the case itself.

The model that explains all three:

* the head is **60 bytes**; it is written unconditionally:
  `+0` = `0xFF` (a **u8** -- the `0xA5` log shows only byte 0 changing),
  `+8` = `0x01010000` (constant), `+12 … +56` = twelve zero dwords.
  `+4` is the caller's **input mask** (echoed, so a same-value write is
  invisible in a delta log -- which is why (a) shows 14 head writes and not 15).
* the records are **64 bytes** at `+60 + 64*k`, `k = 0 … 31` (2108 = 60 + 32·64),
  indexed by the mask **bit**: in (b) the mask the driver actually read is the
  counter value at `+4`, i.e. `0x2` = bit 1, and the one record written is the
  one at `+60 + 64*1 = +124`. So bit -> record index, one to one.
* each selected record writes the same 13 slots: `rec+0` (**u16** type stamp,
  both bytes equal: `08 08`, `09 09`, `03 03`), `rec+8, 12, 16, 20, 24, 28, 32,
  36, 40, 44` and `rec+52`, `rec+56`. `rec+4`, `rec+48` and `rec+60` are the only
  per-record dwords a GET never writes (measured in (a) and re-measured as `U`
  in (b): `+128`, `+172`, `+184` stayed at their counter values).

**The earlier model was wrong.** It read the `106` as `2 + 8*13` and concluded
the two extra head dwords were `+0`/`+8` alone. But (b) measures 14 head writes,
and 14 + 8*13 = 118 != 106. The count *is* 106, so the records cannot all carry
13 fields: `106 = 14 + 92`, and the two records the capped list reaches (types
`08` and `09`) do carry 13 each -> the remaining six records behind mask bits
`2…7` must average `(92-26)/6 = 11`. That is a **per-type field set**, exactly
the shape the NVAPI V1 handler shows (`internal type -> which dwords get
marshalled`), and it is the open item this note now carries.

Predictions this makes (registered in the probe as `--clkctl-read`'s leading
counter pass: one request per mask bit `0x1…0x80`, plus `0xFF`/`0x3FF`):

| `mask@+4` (counter bg) | expected |
| --- | --- |
| any single bit `b` | writes only at `+60+64b`; the head's 14 writes; the record's field count says the type |
| `0xFF` | 8 records -> 14 + (2x13 + 6x11) = 106 written dwords, last @564 |
| `0x3FF` | the 10 directory entries of `0x20809019` -> 14 + 2x13 + 8x11 = 128 (if every other type carries 11) |

A `0x25` answer to a single-bit mask would mean that bit names a domain with no
record on this part, and the per-bit answer then fixes the bit -> index map
directly instead of by inference.

Consequence for the write path: `rec+4`, `rec+48` and `rec+60` are the only
per-record dwords this GET never overwrites, so they are the first place a SET
input could live -- to be confirmed against `0x2080d01c`, not assumed. The
*current offset* is still not identified: every value slot reads back `0` at
stock, so the offset field can be pinned only by (i) the per-type field map
above plus the NVAPI-field correspondence of §5.1, or (ii) a gated single-step
SET with a same-field read-back.

### 4.3 Candidate mapping of the 64 B record fields (not yet verified)

The record's 13 written slots are `rec+0` (u16 type) plus twelve dwords. Two
independent coordinate facts suggest which of them is the *offset*:

1. **Measured field shift.** `0x20809025` (same ClockClient controller tree, same
   driver) carries `max_kHz`/`offset_floor`/`offset_ceiling` at `rec+8`,
   `rec+0x14`, `rec+0x18`; the NVAPI struct for the same control
   (`nvapi-rs` `clk_freq_ctrl_info_entry`) carries the same three at `+0x2C`,
   `+0x38`, `+0x3C`. Same spacing, constant `0x24` shift -> the RM record's
   field *order* is the NVAPI record's field order shifted by `0x24`.
2. **NVAPI V1 field list** for this very control
   (`nvapi-rs/sys/src/gpu/clock.rs:1393` `clk_ctrl_entry`): `OFFSET_KHZ = 44`
   (`0x2C`), `RANGE_MIN = 48`, `RANGE_MAX = 52`, `APPLIED = 56`, `EXTRA = 60`.

Applying the `0x24` shift to fact 2 gives the candidate mapping

| RM offset | candidate | TU116 stock |
| --- | --- | --- |
| `rec+8` | `offset_kHz` (i32), the **current offset** | `0` |
| `rec+12` | `range_min` | `0` |
| `rec+16` | `range_max` | `0` |
| `rec+20` | `applied` | `0` |
| `rec+24` | `extra` | `0` |
| `rec+28 … rec+44`, `rec+52`, `rec+56` | unr attributed / per-type tail | `0` |

Caveats, stated because they matter: the shift is *measured* only on
`0x20809025`'s three fields, the V1 caller struct is layer **L1** (its `+44`
is absolute inside a 2404 B caller block, so the "shift" is a coordinate
correspondence and not a pointer arithmetic claim), and the 901b record does
**not** return `-18750/+18750` in `rec+12`/`rec+16` at stock, so those two are
*not* the supported range -- the range lives in `0x20809025`. Treat the table as
a ranked candidate list: the per-bit counter map (§4.2) and a gated
same-field read-back after a single-step SET are the two tests that can promote
or kill each row. Nothing here is evidence that a SET is safe.

### 4.4 The SET twin's own gates, read out of the GSP image

§4.1 says the two thunks do not share a body; disassembling them shows why, and
turns the SET into a list of named preconditions rather than a black box.
`pFunc` for the GET is `0x4d3b464`, for the SET `0x4d5c3c0` (§3 extraction;
both are reached through the vtable slots, so no literal for either is
findable by scanning for the id). In both, the context is the same:

```
a5 = *(pCtl + 0x158) ; s1 = *(a5 + 0xe0)      ; the GSP client/device context
x  = *(*(s1 + 0x1f88) + 0x1148)               ; the clock-domain controller
```

The **GET** goes straight there:

```
v = (*(*(x + 0x20)))(0)          ; the controller's helper object
return (*(v + 0x38))(s1, v, pParams)          ; vtbl slot 0x38 = the body
```

The **SET** inserts five checks, each returning a *named* status, and only then
calls a *different* object's slot 0x28:

```
a4 = *(s1 + 0x2050)                 ; G1
  if (!a4)                            -> 0x56 NV_ERR_NOT_SUPPORTED
  if (!*(u8 *)(a4 + 0x4f4))           -> 0x56 NV_ERR_NOT_SUPPORTED   ; G2
  if (*(u8 *)(x + 0x179))             -> 0x6c NV_ERR_GPU_IN_DEBUG_MODE ; G3
a0 = *(*(s1 + 0x1f88) + 0x1150)     ; G4  the SET controller object
  if (!a0)                            -> 0x56 NV_ERR_NOT_SUPPORTED
  if (!*(a0 + 0x20))                  -> 0x56 NV_ERR_NOT_SUPPORTED   ; G5
v  = (*(*(a0 + 0x20)))(0)
  if (!v)                             -> 0x56 NV_ERR_NOT_SUPPORTED
return (*(v + 0x28))(s1, v, pParams)          ; vtbl slot 0x28 = the body
```

Read off the actual instruction stream (`beqz`/`bnez` targets), the two enable
gates G1/G2 and the missing-object paths resolve to `addi s1, zero, 0x56` +
`ret`, and G3 to `addi s1, zero, 0x6c` + `ret`. The three facts that matter:

* **the SET body exists and is a sibling of the GET's**, one vtable slot over
  (0x28 vs 0x38) on a *second* controller object at `x+0x1150` — so "the GSP
  cannot do this" is not supported by any evidence here;
* **G3 is invisible from the read side.** `x+0x179` is a byte in the GET's own
  controller object, and `NV_ERR_GPU_IN_DEBUG_MODE` on the first write attempt
  would be a *legitimate* answer, not a crash, not a refusal-by-absence. Any
  first-write harness must treat `0x6c` as an expected, aborting outcome;
* **G1/G2 live on a different object** (`*(s1+0x2050)`, flag at `+0x4f4`) —
  the natural home of a "clock control enabled" switch, and the reason the
  GET's success cannot be read as the SET's permission (§4.1).

None of G1-G5 is observable by reading; only an attempt answers them. That is
exactly why the plan keeps the write behind `WRITE-VERIFICATION-PLAN.md` §7's
G1-G6 gates and why "the GET works" was never accepted as a write licence.

### 4.5 The writer is still not identified statically -- and the obvious trap

Two store-set searches over `.text` (`0x4bfa000 .. 0x5bef000`, RISC-V, all
`sw`/`sd`/`sh`/`sb` with a target-listed immediate, grouped by base register,
window <= 0x300) give a *lead* and a *trap*:

* **the trap.** The head's written dword set is
  `{0, 8, 0xc, 0x10, 0x14, 0x18, 0x1c, 0x20, 0x24, 0x28, 0x2c, 0x30, 0x34, 0x38}`
  (§4.2a) and a record's is the same set **minus `0x30`**. The record set is
  therefore a *subset* of the head set, so a window matching the record's
  thirteen offsets proves nothing about which of the two structs it writes --
  every 13-slot hit is also a 14-slot hit. Any earlier "record writer
  candidate" list built from the thirteen offsets alone is non-discriminating.
* **the lead.** Requiring all fourteen *including* `0x30`, plus the mask echo
  at `+4`, leaves four windows, all of which also carry the `u16` stamp of the
  first record:

  | window start | base | stores |
  | --- | --- | --- |
  | `0x4e0e8c0`, `0x4e0eaf8` | `s1` | the 14 head slots + `+4` |
  | `0x4e99d5c` | `s2` | the 14 head slots + `+4` |
  | `0x4e99e54` | `s1` | the 14 head slots + `+4` + `+0x3c` |
  | `0x4e99d48` | `s2` | the 14 head slots + `+4` + `+0x3c` |

  `+0x3c` is exactly where record 0's `u16` stamp goes, so these four are
  *candidate* head writers and not disproof of the model. None can be promoted
  to proof: the handler is entered through a vtable slot (§4.1/§4.4), so there
  is no call edge from a literal id to these addresses, and a store-offset
  match does not identify a struct field. The `0x01010000` constant the head
  writes at `+8` is not a usable anchor either: `lui x, 0x01010` (= 0x01010000)
  occurs **257 times** across `.text`, several of them as the `lui a5, 0x1010;
  addiw a5, a5, 0x101` idiom that builds 0x01010101 for a byte-average, and the
  rest as compare bounds.


So the field identity in §4.3 stays where it was: *live blocks* plus the
NVAPI-record correspondence, with the per-bit counter map (§4.2) as the
measurement that can promote or kill a row -- not disassembly.

## 5. Neighbouring containers that *do* carry values

All GET, all at the GSP's own size, all with a zeroed input block:

| id @size | status | what came back |
| --- | --- | --- |
| `0x20809019 @12336` | `NV_OK` | 48 B head + 32×384 B record table, **10** records populated; per record `+4` is the domain bit (`0,1,4,2,3,20,19,6,21,5`) and `+0` the type bytes `{08,09,08,09,09,09,03,08,09,09}` — the Turing mix of §4.2 |
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

One measured shape is worth recording, because it is the *second* place in this
family where a per-domain limit pair lives: `rec+44` reads `0x03e8fc18` on
domain bit `0x1` and `0x0bb8fc18` on bit `0x10`, which as one `i32` is the
nonsensical `65600536` / `196672536` but as two **`i16` halves** is
`(-1000, +1000)` and `(-1000, +3000)`. Two of ten records carry it, both with
the same floor, so "a pair" is the better reading -- but the unit, and whether
it is an offset range, a step range or something else, are **not** closed by
this. `tools/decode-clk-containers.py` prints that slot both ways (raw dword and
`i16 pair`) so nobody has to choose before the evidence does.

### 5.2 GET -> SET pairing inside the legacy CLK block (a table fact)

The GSP export table lists both directions. Inside the clock/voltage block the
pairs are separated by **`+1` in the low byte** with the category byte moving
`0x90` (GET) -> `0xd0` (SET) — which, per §2.1, is the privileged bit — and each
pair carries an **identical `paramSize`**. Since the GSP validates the size
exactly (§3), that equality is strong corroboration rather than a naming guess.
The `flags` column is the GSP's own access class (§3) and it, not the `+1`,
decides whether the twin writes:

| GET | size | flags | SET | size | flags |
| --- | --- | --- | --- | --- | --- |
| `0x2080901b` | 2108 | 0x48 read | `0x2080d01c` | 2108 | **0x44 write** |
| `0x2080901f` | 19416 | 0x48 read | `0x2080d020` | 19416 | **0x44 write** |
| `0x20809023` | 66060 | 0x48 read | `0x2080d024` | 66060 | **0x44 write** |
| `0x20809027` | 6288 | 0x448 (0x48\|0x400) | `0x2080d028` | 6288 | **0x44 write** |
| `0x2080902c` | 4116 | 0x48 read | `0x2080d02d` | 4116 | **0x44 write** |
| `0x20809030` | 1544 | 0x48 read | `0x2080d031` | 1544 | **0x44 write** |
| `0x20809046` | 8264 | 0x48 read | `0x2080d047` | 8264 | **0x44 write** |
| `0x208090a2` | 524 | 0x48 read | `0x2080d0a3` | 524 | **0x44 write** |
| `0x208090b2` | 812 | 0x48 read | `0x2080d0b3` | 812 | 0x48 *read* |
| `0x2080b207` | 392 | 0x48 read | `0x2080f208` | 392 | **0x44 write** |
| `0x2080b211` | 520 | 0x48 read | `0x2080f212` | 520 | **0x44 write** |
| `0x2080b213` | 1036 | 0x48 read | `0x2080f214` | 1036 | **0x44 write** |
| `0x2080b218` | 9220 | 0x48 read | `0x2080f219` | 9220 | **0x44 write** |

Two corrections to the first version of this section, both from counting the
whole table rather than the block:

* `0x208090b2 -> 0x2080d0b3` is a same-size `+1` twin whose **both** rows are
  `0x48` (read) — so the `+1` convention alone does not imply a write face, and
  "GET has a twin" must not be read as "the twin can be written".
* scoped over the whole legacy read set (the 300 rows with bit 15 and not bit
  14), only **41** have a same-size `+1/+0x40` twin, 15 have one at a
  *different* size, and 244 have none. So `+1` is a *local* regularity of the
  clock/voltage block, not a table law; the earlier wording ("a table fact")
  holds for the rows above, not for the table as a whole.

Two things follow that change the plan:

* `0x20809019` (12336, domains info), `0x20809025` (7188, freq-controller info)
  and `0x20809026` (5332, freq-controller status) have **no SET twin in this
  table at all** — they are read-only views of the domain set. So the
  ClkFreqController `+/-18.75 MHz` value in §5.1 is a *limit* source only.
* the write-flagged rows in the block are **`0x2080d01c @2108`** (the SET twin
  of the §4 GET), `0x2080d0a3 @524` (VF points) and `0x2080f214 @1036` (rails).
  The first is the single candidate for a domain-offset *write* on this box,
  and it stays unsent until §4's GET returns a current value and its range
  (`WRITE-VERIFICATION-PLAN.md`). The two directions do not share a handler:
  §4.1 and §4.4 show the SET thunk carrying five preconditions the GET thunk
  does not have, so the SET's own gate state has to be verified before it is
  used — and per §2.1 all three of these ids are privileged, i.e. not even
  *reachable* from this un-privileged client.

## 6. Safety: a bad private GET can wedge the GSP for the session

Observed 2026-10-17, in order, all read-only, same user client. The run used
the **pre-hardening** `--clkctl-offsweep`, whose part (b) filled every block
with `0xA5A5A5A5` (the `seed@+-1` in the log means "no seed offset", not "zero
background"; the `a5a5a5a5 > ...` dumps on the `0x2080901d`/`0x20809025` lines
are that background):

```
0x2080901a @2052   (0xA5) -> 0x1f NV_ERR_INVALID_ARGUMENT
0x2080901c @808    (0xA5) -> 0x65 NV_ERR_TIMEOUT          <- survives
0x2080901d @32680  (0xA5) -> NV_OK                        <- still fine
0x20809025 @7188   (0xA5) -> NV_OK                        <- still fine
0x20809026 @5332   (0xA5) -> 0x65 NV_ERR_TIMEOUT          <- latches the wedge
then, for the rest of the session, every legacy-forwarded id:
0x20809019 @12336 -> 0x62 NV_ERR_RESET_REQUIRED
0x20809061 @776   -> 0x62 NV_ERR_RESET_REQUIRED
NV00DE RUSD page  -> 0x62 NV_ERR_RESET_REQUIRED
```

Two lessons, both of which change the runbook:

* the `0x65 TIMEOUT` of `0x2080901c @808` was **survivable** — the next two
  private GETs answered `NV_OK` after it. A timeout is therefore not always
  fatal, which is exactly why the rule has to be "stop at the first one"
  rather than "one more try is fine".
* the fatal request was `0x20809026 @5332` **with an `0xA5`-filled block**, and
  there is **no experiment that separates "this id" from "this content"**:
  `0x20809026` has never been read with a zeroed block, and `0x20809025` (its
  info sibling) is perfectly happy with `0xA5` *and* with zeros. Safety-wise
  the ambiguity is resolved the conservative way — the id is treated as a
  hazard **regardless of payload** and refused outright, rather than being
  retried with a "safer" background.

The state did not clear on its own over 40+ minutes and with new user clients.
Only a driver reload / reboot resets it; both are **out of bounds** for this
effort, so no attempt was made. The host stayed healthy throughout
(`WindowServer` uptime unbroken, no panic report, `uptime` continuous).

Because of this, `--clkctl-offsweep` no longer fills any id except `0x2080901b`
with a non-zero background (the one id that provably rejects bad content with
`0x25` instead of timing out), and the value containers are read with a zeroed
block only. Ids whose size is accepted with *either* background so far:
`0x20809019`, `0x20809025`, `0x20809061` (zeroed), and `0x2080901b`, `0x2080901d`,
`0x20809025` (`0xA5`).

**This is a hard safety boundary for the whole clock-offset campaign**: two
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
several registered perf/pmgr/clk rows are GSP-implemented too.

**Correction (2026-10-17T11:11Z, §7).** The last sentence of the previous
version — "a `0x62` sweep is therefore a cheap way to tell host-implemented
rows from GSP-dispatched ones" — does not hold. `0x2080b201` answers `NV_OK`
*while* its own category sibling `0x2080b202` answers `0x62`, in the same
session, on the same forwarding path, and **neither** has a host handler (§7
proves both facts from source and from the loaded binaries). A post-latch
`0x62` therefore identifies a **gated handler/subsystem**, not a dispatch path;
do not use it to classify host-vs-GSP.

## 7. `0x2080b201` survives the wedge: the gate is per command

While every GSP-forwarded id above answered `0x62 NV_ERR_RESET_REQUIRED`,
`0x2080b201 @2444` kept answering `NV_OK` with its usual
`mask 0xFF + 800000 µV / 756250 µV` payload — while *registered* GSP-backed
commands like `0x2080200b` and `0x20802609` were `0x62` (see §6.1).

**Correction (2026-10-17T11:11Z).** The first version of this section read that
as "`0x2080b201` is answered *outside* the GSP path". Two measurements exclude
it:

1. **The host has no handler for any CLK/VOLT private id.** The host's only
   deprecated-control rows are the eight in
   `src/nvidia/interface/deprecated/rmapi_deprecated_control.c`
   (`rmDeprecatedControlTable`):
   `NV0000_CTRL_CMD_SYSTEM_GET_BUILD_VERSION`, `NV2080_CTRL_CMD_BIOS_GET_INFO`,
   `NV2080_CTRL_CMD_BUS_GET_INFO`, `NV0080_CTRL_CMD_BSP_GET_CAPS`,
   `NV2080_CTRL_CMD_GPU_GET_INFO`, `NV0073_CTRL_CMD_DP_SET_MSA_PROPERTIES`,
   `NV0080_CTRL_CMD_MSENC_GET_CAPS`, `NV2080_CTRL_CMD_FB_GET_INFO` — no CLK row,
   no VOLT row. Everything else with bit `0x8000` goes to `RmGssLegacyRpcCmd`,
   which does no table lookup, no size check, no name check: it copies the
   block and calls `pRmApi->Control(PHYSICAL)`.
2. **The id literal is absent from every loaded host binary.** A byte scan for
   the little-endian u32 of `0x2080b201`, `0x2080b202`, `0x2080b213`,
   `0x20809019`, `0x2080901b`, `0x20809026` and `0x20809061` returns **0 hits**
   in `NVRM`, `NVAccel`, `NVRMFB`, `NVRMAGDC` and the nvmtl dylibs, while the
   same scan finds the *public, host-implemented* `0x20800102` **19** times and
   `0x2080122a` twice. A host-side handler would have to compare against a
   literal; there is none.

So `0x2080b201` is served by the GSP, and the wedge is **command-selective**.
`probe-macos-rm --tierb-read` (read-only, one zeroed block per id at the size
the GSP's own table gives) now walks the whole VOLT (`0x32`) read half and
records exactly that, in one session
(`outputs/volt-read-20261017T111107Z/`):

| command | size | wedged result |
| --- | --- | --- |
| `0x2080b201` rails get-info | 2444 | **`NV_OK`** — 16 non-zero dwords, stable on re-read |
| `0x2080b202` rails get-status | 3232 | `0x62` |
| `0x2080b205` `b207` `b209` `b210` `b211` `b212` `b213` `b214` `b216` `b217` `b218` | GSP table sizes | `0x62` (all eleven) |
| `0x208090a2` vfpoints-get | 524 | `0x62` |

Read that as: the private VOLT read half is not uniformly alive *or* uniformly
dead; exactly one of the thirteen read ids still answers. Whether the gate sits
in the host's physical-RM shim (a command-selective "GSP client reset pending"
check) or in the GSP itself is **not** separated yet — both earlier readings
("all legacy-forwarded ids die", "b201 is host-side") are excluded, this one is
not. The pass therefore treats the two codes differently: `0x65
NV_ERR_TIMEOUT` stops the pass (it is the latch of §6), `0x62` is recorded and
the pass continues, because "which read faces still answer" *is* the
measurement.

## 8. Reproducing

```sh
# on the test machine, read-only; needs the GSP alive (see §6)
OGKM=$HOME/Desktop/macOS-nvoc-develop/reference/open-gpu-kernel-modules-610.57.04
cd probe-macos-rm && OGKM=$OGKM sh build.sh
python3 tools/extract-gsp-method-table.py \
        /Users/Shared/nvfw/nvidia/610.57.04/gsp_tu10x.bin > /tmp/gsp-table.tsv
./probe-macos-rm-idle --clkctl-read     # GSP-sized GETs, incl. mask@+4
./probe-macos-rm-idle --clkctl-hex      # THE lossless record: full hex of
                                        # 0x2080901b @2108 with mask@+4 = 0xFF
                                        # and = 0, 0x20809025, 0x20809019, and
                                        # 0x2080901d @32680 last
./probe-macos-rm-idle --clkctl-offsweep # 0x2080901b A5 sweep ONLY (see §6)
```

Use `tools/clock-offset-read-run.sh` to run the battery into one bundle with a
SHA-256 manifest and the offline decoder. Prefer `--clkctl-hex` over the
`--clkctl-offsweep` delta printout for anything that has to survive the session:
the 2026-10-17 `0xA5` sweep reported `106/527` changed dwords for `0x2080901b`
but its log kept 40 of them (`0x2080901d`: 352 reported, 60 kept), so a delta
printout alone is not a reproducible record of a block.

`tools/decode-clk-containers.py` (run by the runbook into `decode.txt`) now
prints a **mask-driven candidate table** for `0x2080901b`: one row per bit the
block's own echoed `+4` mask names, showing `rec+8/12/16/20/24` *including the
all-zero case*. That case is the stock reading and the generic record printer
skips it, which is exactly the row §11.3 needs to be read after the reboot. The
decoder still refuses to name any of the five slots (`offset_khz?`,
`freq_offset_or_range_max?`, …) because §11.3's order question is open.

## 9. What is still missing

* a container that returns the **current** per-domain offset. Candidates, in
  the order they should be tried once the GSP is alive again — steps 1 and 2
  are now the first two cases of one `--clkctl-hex` run:
  1. `0x2080901b @2108` re-read with the §4 `+4` mask and a **zeroed**
     background, to check whether the `08`/`09` records are really
     "types only" or whether a single-sourced mask leaves fields that the
     `0xA5` sweep could not distinguish from a driver-zeroed word. The run now
     uses `0x1` / `0xFF` / `0x3FF` so it also tests §4.2's entry-selector model
     against the predicted type bytes at `+60 … +636`.
  2. `0x2080901d @32680` with a zeroed block (it answered `NV_OK` with an
     `0xA5` background; the header is `+0` low byte `0xff`, `+4 = 0x07ffffff`
     and the records carry small fields such as `0x00850085` /
     `0x00ff0093`), to see the actual values rather than the A5 overlay.
  3. *Not* a candidate: `0x20809026 @5332`. It latched the wedge of §6, and
     because "the id" cannot be separated from "the `0xA5` content" (see §6)
     it must not be sent as part of a read battery at all. If it is ever tried
     again it has to be a single, deliberate, last request of a session, with
     the user told that the private path may die for the rest of the session.
     `0x2080901a @2052` is also out of the battery: an `0xA5` block returned
     `0x1f`, and the zeroed form has never been tried, so its size alone is
     evidently not what the GSP wants.
  4. `0x2080901c @808` is likewise *not* a battery case: it timed out once
     (survivably), and removing it costs nothing.
* the **write** half: `0x2080d01c @2108` (§5.2). Not sent, and not sendable
  until the GET half yields current/limit/state. The step granularity is still
  unknown (the `+/-18750 kHz` pair is the *range*, not a step), so the
  "minimum step" for a first write is not yet defined.
* **which layer is missing, now answered for this client.** §2.1: the id-space
  itself puts every clock/voltage SET in the privileged half (`0xC000`), and
  `RmGssLegacyRpcCmd` returns `0x1b` for a client below
  `RS_PRIV_LEVEL_USER_ROOT` before the GSP sees anything. So for the
  un-privileged client this session has, the missing thing is **privilege, not
  protocol and not driver support** — the GSP body exists (§4.4) and the host
  gate is five lines of source. Two consequences that must not be blurred:
  (a) `0x1b` from a SET id is *not* evidence that the id is unsupported, and a
  `0x56`/`0x6c` would have to come from the GSP, i.e. from a privileged caller;
  (b) whether a *root* caller actually lands at `RS_PRIV_LEVEL_USER_ROOT` in
  NullMoth's port is **not verified** (the only measured fact is that this user
  is below it), and verifying it needs a root client, which the standing
  permission boundary does not include.
* the VFP/"get-control" family at its **own** GSP sizes, which has never been
  read: `0x2080901f @19416`, `0x20809021 @33288`, `0x20809022 @623112`,
  `0x20809023 @66060` (SET twin `0x2080d024 @66060`), `0x20809027 @6288`,
  `0x2080902c @4116`. A *get-control* return is the natural place for a
  per-domain offset/applied pair, i.e. a better G1 candidate than re-reading
  901b -- but these ids are untried, and the one unanswered safety fact is that
  `0x20809026` latched the §6 wedge, so an untried id at its true size is
  exactly the risk class that wedge belongs to. They stay out of the shipped
  read battery until the wedge has a mechanism (or until an operator accepts a
  single deliberate, last-request attempt).
* the V2 form of the clock-domain control — `nvapi-rs` says the value dwords
  live there, and the GSP table does not offer a second size for
  `0x2080901b`, so V2 is either versioned inside the same 2108 B block or not
  available on this TU116 GSP;
* the body behind §4.1's vtable slots. `pFunc` is a thunk, so the field-level
  marshalling of the GET and the SET is not reachable by scanning `.fwimage`
  for the id or for a size; it needs the runtime object (or a data-section
  cross-reference from the slot number), which is why the field map here is
  derived from live blocks rather than from disassembly. §4.5 records what a
  bounded static attempt did produce (four candidate head writers at
  `0x4e0e8c0`/`0x4e0eaf8`/`0x4e99d5c`/`0x4e99e54`/`0x4e99d48`) and why the
  obvious thirteen-offset search cannot promote any of them;
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

**State recheck 2026-10-17T10:03Z**: `--clkctl-hex` now starts with
`0x2080901b @2108 in@4=0xFF`, seeds `+4`, reads back `0x62
NV_ERR_RESET_REQUIRED` and aborts the remaining dumps
(`outputs/stage-clkctl-hex-v2-20261017T100350Z/`). That is the intended
behaviour on a wedged GSP — the battery stops instead of walking the id list —
but it means every block above still dates from the pre-wedge session.
`uptime` has been polled all session (19h46m at 10:04Z without a reset), so the
private path is still dead and **nothing has been written to the GPU**.

## 10. Candidate-mapping correction table (2026-10-17)

The user's brief for this phase asked for a table that separates, for every
candidate id: the original record, the actual source, the version it belongs
to, the **full marshalling**, and a **reproducible read-only request**. This is
that table. "Source" means where the number's authority comes from now —
`GSP table` = a row of the firmware's own export table (§3, 889 rows, each row
carrying an authoritative `paramSize`), `not in GSP table` = searched and
absent, which is a *different* statement from "the driver does not know it".

| # | id | original record / claim | actual source | version it belongs to | full marshalling (measured) | reproducible read-only request | verdict |
| --- | --- | --- | --- | --- | --- | --- | --- |
| 1 | `0x20809019` | "ClockClient domain info" (§4 of the superseded note) | GSP table @**12336** | R610.57.04 TU116 GSP | in: zeroed block. out: `48` B head (`+0` low byte `0xff`, `+4=0x3ff`, `+8=0x335`) + `32 x 384` B records; `rec+0` u16 type stamp, `rec+4` single domain bit, `rec+44` two `i16` (`0x03e8fc18` = `-1000,+1000`) | `probe-macos-rm-idle --clkctl-hex` (lossless hex) | `NV_OK`, 10 filled records, types `{08,09,03}`. No current/offset field observed |
| 2 | `0x2080901b` | "ClockClient GET_CONTROL / offset+range read" | GSP table @**2108** | same | in: `+4` = u32 domain mask (validated: an invalid mask gives `0x25 NV_ERR_INVALID_DATA`); out: `60` B head (14 writes: `+0` u8 `0xff`, `+8` u32 `0x01010000`, `+12` u16 `0`, `+16..+56`) + bit-sparse `64` B records at `+60+64*bit` (`u16` type stamp @`+0`, value dwords `+8..+44` and `+52`,`+56`; `+4`,`+48`,`+60` **never** written) | `--clkctl-read` (per-bit counter map) / `--clkctl-hex` (lossless) | `NV_OK`; types `08`/`09` records write 13 slots, other types 11; **every value dword reads 0** (stock). Current offset **not** obtained |
| 3 | `0x20809006` | "frequency measurement" | **not in GSP table** | — | 32 B / stamp `0x10020` form only | `--clkdoc-read` | every documented size and stamp -> `0x1f`; not usable here |
| 4 | `0x20809061` / `0x20809062` | "V/F directory and status" | `9061` **is** a GSP row @**776**; `9062` is **not** in the table | the 582.41-numbered VFP pair does *not* map onto these sizes | `776 = 8 + 32*24`: 2 head masks (`0xFF`/`0x7FF`) + 11 filled per-domain records carrying a kHz-magnitude value | `--clkvf-read`, `--clkctl-hex` | `9061` readable and **live** (matches RUSD GPC/MCLK sample-for-sample); `9062` at every documented size -> `0x1f` |
| 5 | `0x2080b213` | "voltage-rail control read" | GSP table @**1036** | `voltrails-family-full-layout-r610.md` | out: mask `0xFF` + `32 x 32` B records (all zero at stock) | `--tierb-read` (now walks the whole VOLT read half) | healthy: `NV_OK`; **wedged: `0x62`** |
| 6 | `0x208090a2` | "all V/F paths fail" | GSP table @**524**, `flags 0x48` = **read** | — | read half of the `0xd0a3` pair | `--tierb-read` | it is a GET and *was* sendable; wedged: `0x62`. The old "all V/F paths fail" wording was wrong |
| 7 | `0x20809025` | "offset range source" | GSP table @**7188** | same | `20 + 32*224`: `rec+8` = max VF kHz (`1638400`), `rec+20`/`rec+24` = `-18750`/`+18750` | `--clkctl-hex` | `NV_OK`; this is the **range**, not the current offset |
| 8 | "`0x62` after the wedge tells host-implemented from GSP-dispatched" | — | refuted by §7 (b201 `NV_OK` vs b202 `0x62`, neither has a host row or literal) | — | — | `--tierb-read` | **correction**: the gate is per command |
| 9 | "the write face is missing protocol / driver support" | — | host gate: `rmapi_gss_legacy_control.c` returns `0x1b` for `(cmd & 0xC000) == 0xC000` when `privLevel < RS_PRIV_LEVEL_USER_ROOT` | §2.1 | — | — | **correction**: what is missing is **privilege**; the GSP body exists (§4.4) |

Two rules this table is meant to enforce, because both were violated by earlier
notes in this repo:

* an id is "absent" only when it is absent from the **GSP's own table** (the
  host table is not the dispatch authority), and even then `0x1f`
  (`INVALID_ARGUMENT`) means "your block did not match", not "no such
  command" — the negative anchors `0x2080a0ff -> 0x56` (forwarded, GSP has no
  method) and `0x2080ffff -> 0x1b` (privileged) are what "absent" looks like;
* a value that reads `0` is a **stock reading**, not a missing field, only if
  the same slot is shown to be *written* by the responder. The `0xA5`
  background sweep is what separates the two (`0xA5 -> 0` is a write of zero;
  `0xA5 -> 0xA5` is a field the responder never touches). The `0x2080901b`
  record writes 13 slots in a `0xA5` block, so its zeros are the driver's own
  zeros — which is *not yet* the same statement as "the current offset is 0
  for every domain", because no non-zero reference exists on a stock board.

**State recheck 2026-10-17T11:11Z**: still wedged (`uptime` 20h49m without a
reset, no reboot, no kext reload, nothing written to the GPU). New in this
window: the whole-VOLT read census of §7 (`outputs/volt-read-20261017T111107Z/`)
and the host-binaries literal scan that goes with it. The clock-domain read
battery still aborts on its first case by design, so §4's `+4` mask
confirmation with a **zeroed** background, and the current-offset question
itself, stay blocked on the reboot — they are the first two cases of
`tools/clock-offset-read-run.sh`.

## 11. The clock-domain offset channel: ids named, slot order reopened, privilege resolved (2026-10-17)

§4 reconstructed `0x2080901b` from live blocks and §9 listed what was still
missing. This section closes two of those items from sources already in the
repo — the `nvapi-rs` NVAPI layer and the macOS port's own OS glue — and it
**demotes one row of §4.3** that had been read as settled. No new GPU request
was sent; every claim below is either a source reading or a re-reading of a
block captured before the §6 wedge.

### 11.1 The ids are NVAPI entry points, and the write recipe is written down

`nvapi-rs/sys/src/gpu/clock.rs:1365-1375` maps this family onto private NVAPI
calls and `:1930-1976` declares them; the middle layer drives them in
`nvapi-rs/src/gpu.rs:2218` (`set_clk_domain_offset`):

| RM id | NVAPI entry point | NVAPI id | caller-side block | recorder's verdict |
| --- | --- | --- | --- | --- |
| `0x20809019` | `ClockClkDomainsGetInfo` | `0x57B5A5DF` | 2520 B, magic `0x109D8` | "returns UNRESOLVED on R575.74; GetControl supersedes it for discovery" |
| `0x2080901b` | `ClockClkDomainsGetControl` | `0xF58938F5` | 2404 B, magic `0x10964` | "WORKS live on Ada 4060 Laptop" |
| `0x2080d01c` | `ClockClkDomainsSetControl` | `0xD14B69CF` | same 2404 B block, by reference | "DANGEROUS GPU clock write" |
| `0x20809006` | `ClockCounterMeasureAvgFreq` | `0xFB8F61EC` (nvapi-rs) vs `0x527FC458` (`oc-surface-audit.md:45`) | 32 B, magic `0x10020` | the two records disagree — and §10 row 3 says the id is not in the GSP table at all |

Two consequences that matter *on this host*:

* our 2108 B block is **not** the NVAPI caller block (2404 B, `0x964`): it is
  the **RM** block one layer down, and its head `+8` is the responder's own
  constant `0x01010000` (§4.2), *not* the caller magic `0x10964`. The
  "version-gate on `0x10964`" step of the recipe therefore applies to the layer
  we bypass, and the only size/version gate that can refuse us is the GSP's own
  `paramSize` (2108 on both halves, §5.2). **Do not seed a magic into the RM
  block**: nothing measured writes `0x10964` there, and the head `+8` is written
  unconditionally by the responder.
* the recipe's remaining steps port one-to-one and are exactly this goal's
  write-verification plan: snapshot the whole block -> patch a **copy** -> SET ->
  GET read-back -> verify -> restore the snapshot on mismatch. Its extra rule is
  worth adopting verbatim: **never submit a broad mask to `SET_CONTROL`.** GET
  is mask-*seeded*, so a broad seed is how the record set is discovered; a broad
  seed to SET asks the driver to apply 32 records, most of which it never filled.

### 11.2 Which records a V1 SET can write, from the marshaller's own switch

The same layer records the per-type write rule
(`nvapi-rs/src/clock.rs:384-410`): `v1_marshalable(t) = t in {1,3,4,5,6,7,8,9}`,
the V2 write path accepts `{1,3..=10,15}`, and protocol type `2` is marshalled
by **neither** — a SET on such a record is silently dropped (recorded there as
live-confirmed). This host's `0x2080901b` returns types `03` / `08` / `09`
(§10 row 2), all three V1-marshalable, so a first single-domain write is
well-posed *provided the target record's type is read first*. The corollary is a
trap: readability and writability are independent per record — "the GET
succeeded" says **nothing** about the SET.

### 11.3 §4.3's `rec+8 = offset_kHz` is a candidate, not a result

`nvapi-rs` disagrees with itself about the value-slot order, and both halves are
in the repo:

* the medium layer's own comment on the write path
  (`sys/src/gpu/clock.rs:1930-1976`, `src/gpu.rs:2218`) says *"per the article
  slot 0 is the signed frequency offset (kHz)"*;
* the Blackwell field map (`sys/src/gpu/clock.rs:1766-1773`) says the frequency
  delta is **`VALUES[2]`** (`rec+0x114`) and the voltage term is `VALUES[4]`
  (`rec+0x11C`).

Under §4.3's measured `0x24` shift, NVAPI slot 2 of the V1 five-dword value
block (NVAPI `rec+52`) lands on RM `rec+16` — the slot §4.3 currently labels
`range_max`. So the type-8/9 records carry **two** live candidates for the
current frequency offset: `rec+8` (the article's slot 0) and `rec+16`
(Blackwell's slot 2). Every one of them reads `0` at stock, which cannot
separate them; only the per-bit counter map (which field set each type writes)
plus a gated single-step SET with a same-field read-back can. §4.3's table is
therefore down-graded from "ranked list with a leading row" to "candidate list,
slot order unresolved".

### 11.4 The privilege precondition is resolved *on macOS*, at source level

§9 left open "whether a root caller actually lands at `RS_PRIV_LEVEL_USER_ROOT`"
because that needed a root client. The macOS port's own sources answer it
without running anything:

| fact | source |
| --- | --- |
| the level is a function of the caller's uid | `kexts/NVRM/os-xnu.cpp:174`: `os_is_administrator(void) { ... return (kauth_getuid() == 0) ? NV_TRUE : NV_FALSE; }` — this overrides the Linux body (`kernel-open/nvidia/os-interface.c:389`, `NV_IS_SUSER()`), and the port ships a hardening test that forbids the stub-to-true form (`tools/test_security_hardening.py:80`) |
| the level is stamped on every control that arrives | `src/nvidia/src/kernel/rmapi/entry_points.c:185` and `arch/nvalloc/unix/src/escape.c:381`: `pSecInfo->privLevel = osIsAdministrator() ? RS_PRIV_LEVEL_USER_ROOT : RS_PRIV_LEVEL_USER;` (`arch/nvalloc/unix/src/os.c:648` forwards to `os_is_administrator()`) |
| `0x1b` *is* that gate's return code | `src/common/sdk/nvidia/inc/nvstatuscodes.h:56`: `NV_ERR_INSUFFICIENT_PERMISSIONS, 0x0000001B` — and it is what §2 measured from `0x2080FFFF` |
| the unprivileged ctl client is open *on purpose*, behind exactly this check | `kexts/NVRM/NVRM.cpp:1487-1496`: "the escalated operations behind it are gated by the real-privilege check in `_os_is_administrator` and the root requirement on first init" |

So: **uid 0 ⇒ `RS_PRIV_LEVEL_USER_ROOT` ⇒ the §2.1 gate passes.** What remains
unknown is whether the *GSP* then also refuses — a different check on a
different layer, and `0x1b` exists there too: `ctrl2080gpu.h:1222/2427/2556`
document `NV_ERR_INSUFFICIENT_PERMISSIONS` as a *control* result. That gives a
first write an unambiguous two-outcome reading:

* `0x1b` from a **root** client ⇒ there is a second, GSP-side permission layer;
* anything else (`0x56` / `0x6c` / `NV_OK`) ⇒ the host gate is behind us and the
  answer is the SET thunk's own preconditions (§4.4).

Runtime confirmation needs a root client, which the standing permission boundary
does not include (this box has no `sudo`), so nothing was run: the claim above is
source-level and is labelled as such.

### 11.5 Status of the write gate, 2026-10-17

* **G0** (privilege reachable) — the source chain to it is now complete (§11.4);
  the empirical half still needs a root client.
* **G1** (current value) — **not obtained**; §11.3 leaves two candidate fields,
  both `0` at stock.
* **G2** (range) — obtained from `0x20809025` (`rec+20`/`rec+24` =
  −18750/+18750, §5.1), but the unit is still unclosed (kHz ⇒ ±18.75 MHz, or a
  voltage term).
* **G3** (original state) — the snapshot is the all-zero GET block, which is the
  expected stock state *only if* the target field is the offset; it is not yet a
  validated "before" for a chosen field.
* **G4** (step) — unknown; the ±18750 pair is a range, not a step.
* **G5** (GSP usable) — **no**: `0x62 NV_ERR_RESET_REQUIRED` since the §6 wedge,
  polled unbroken through 2026-10-17T11:12Z.
* **G6** (the SET's own preconditions) — enumerated in §4.4 (`0x56` ×4, `0x6c`
  once); all runtime values unknown.

Net: what is missing for the clock-domain SET is **the caller's privilege** — a
precondition that is one line of macOS glue (§11.4) and is satisfiable by uid 0
— and behind it the *step/field* question of §11.3. It is not a protocol gap,
not a missing firmware body (§4.4), and not a marshalling gap (the 2108 B
geometry is measured on both halves).
