# NVRM 610 method table and the control dispatch chain (macOS, TU116)

> Status: reverse-engineering record for the macOS RM control path. Read-only
> investigation; no writes, no driver changes. Host evidence: 2026-10-17,
> macOS 15.8.1 / x86_64 / GTX 1650 SUPER (TU116, 10de:2187) / NullMoth 2.0.4.

This note answers "where does an RM control actually go on this box, and which
commands does the driver implement?" — the question that decides whether a
private NVAPI-only command id can be exercised at all on macOS.

## 1. Which driver answers

`kmutil showloaded` reports the graphics stack as:

```
com.nullmoth.NVRM    (0.1)  UUID 8D086296-B3D4-3855-843F-2A2FA27E3462
com.nullmoth.NVAccel (0.1)  UUID 9EF6296F-50BD-38B2-ADF1-9E7D650AC69A
com.nullmoth.NVRMFB  (0.1)
com.nullmoth.NVRMAGDC(0.1)
```

`/Library/Extensions/NVRM.kext/Contents/MacOS/NVRM` has the same UUID
(`8d086296b3d43855843f2a2fa27e3462`, sha256 `81c6ba3a…7fde8`), so the file on
disk *is* the loaded image. Its version string is

```
nvidia id: NVIDIA UNIX Open Kernel Module for x86_64 610.57.04 Release Build (NullMoth Systems)
```

i.e. this kext is the **open-gpu-kernel-modules 610.57.04 RM**, rebuilt for
macOS — not a clean-room shim. The strings `rmapiutilGetControlInfo(cmd, &flags,
NULL, &ctrlParamsSize)`, `src/libraries/nvoc/src/runtime.c` and the
`generated/g_*_nvoc.c` names all match the public source tree one-for-one, and
the extracted method table below matches the public generated tables exactly.

Consequence: every id in this table is answered by the host RM, and "which
*public* ids exist" can be answered by reading the array out of the binary
instead of guessing.

**But this table is not the whole control surface** (corrected 2026-10-17). Any
id carrying the legacy bit `0x8000` that is *not* in this table is forwarded to
the **GSP firmware**, which has its own 889-row method table — 169 CLK and 151
VOLT rows where this table has 1 and 0. Reading this array therefore answers
"is this a host-implemented command", not "does this command exist on this
machine". See `gsp-method-table-and-legacy-forwarding.md`.

## 2. The dispatch chain

`NV_ESC_RM_CONTROL` carries an `NVOS54_PARAMETERS`
(`hClient, hObject, cmd, flags, params, paramsSize`, `nvos.h`). The kernel side is
`src/nvidia/src/kernel/rmapi/control.c`:

```
_rmapiRmControl()                                        control.c:346
  if (cmd == NVXXXX_CTRL_CMD_NULL || (CATEGORY==0 && INDEX==0))  return NV_OK;
  getCtrlInfoStatus = rmapiutilGetControlInfo(cmd, &ctrlFlags,
                          &ctrlAccessRight, &ctrlParamsSize);     control.c:445
  if (((paramsSize != 0) && (pUserParams == 0)) ||
      ((paramsSize == 0) && (pUserParams != 0)) ||
      ((getCtrlInfoStatus == NV_OK) && (paramsSize != ctrlParamsSize)))
  {
      rmStatus = NV_ERR_INVALID_ARGUMENT;                         control.c:448-458
      goto done;
  }
  ...
  rmStatus = serverControl(&g_resServ, &rmCtrlParams);            control.c:511
```

`rmapiutilGetControlInfo()` (`rmapi_utils.c:161-196`) resolves the class from
the command id and binary-searches the class export array:

```
RS_RESOURCE_DESC *pResourceDesc =
    RsResInfoByExternalClassId(DRF_VAL(XXXX, _CTRL_CMD, _CLASS, cmd));
pClassDef = pResourceDesc->pClassInfo;
pMethodDef = nvocGetExportedMethodDefFromMethodInfo_IMPL(pClassDef->pExportInfo, cmd);
*pParamsSize = pMethodDef->paramSize;
```

`nvocGetExportedMethodDefFromMethodInfo_IMPL()` (`libraries/nvoc/src/runtime.c:299`)
documents the lookup: *"The export array is sorted by methodId, so we can binary
search it"*. `serverControl()` (`libraries/resserv/src/rs_server.c:1460`) then
locks the client, resolves `hObject` to a resource and calls `resControl()`,
which dispatches on the *object* — hence the carrier-dependence measured
earlier (`hSubdevice` works where `hDevice`/`hClient` answer `0x56`).

Two properties follow, and both matter for the private-id work:

1. **Exact paramSize match.** For a command that *is* in the table, any other
   `paramsSize` is rejected with `NV_ERR_INVALID_ARGUMENT (0x1f)` before the
   handler runs. The converse does not hold: a registered id sent at the
   registered size can still answer `0x1f`, because handlers return the same
   code for invalid *input* (measured — see §5).
2. **No size check for commands that are not in the table.** The comparison is
   guarded by `getCtrlInfoStatus == NV_OK`, so an unregistered command skips it
   and is dispatched regardless of size. A size-dependent `0x1f` for an
   unregistered id therefore *cannot* come from this gate.

Command-id encoding (`src/common/sdk/nvidia/inc/ctrl/ctrlxxxx.h`):

```
cmd_class    [31:16]      NVXXXX_CTRL_CMD_CLASS
cmd_reserved [15:14]      legacy flags: NVxxxx_CTRL_LEGACY_PRIVILEGED  (0xC0)
                                        NVxxxx_CTRL_LEGACY_NON_PRIVILEGED (0x80)
cmd_category [13:8]       NVXXXX_CTRL_CMD_CATEGORY (6 bits)
cmd_index     [7:0]       NVXXXX_CTRL_CMD_INDEX
```

so `0x20809061` is class `0x2080`, category `0x10` (CLK) *with the legacy
non-privileged bit set*, index `0x61`; `0x2080B201` is VOLT (`0x32`) legacy
non-privileged; `0x2080D0xx`/`0x2080F2xx` are their privileged twins.

## 3. Extracting the live method table

`probe-macos-rm/tools/extract-nvrm-method-table.py` walks the binary for the
40-byte `NVOC_EXPORTED_METHOD_DEF` records (`pFunc, flags, accessRight,
methodId, paramSize, pClassInfo, funcName`) and joins them with the generated
`g_*_nvoc.c` function names. It is read-only and its output for this box is
archived outside the repo as
`outputs/method-table-nvrm-610.57.04.tsv` (604 rows).

Category histogram of the registered subdevice controls:

```
cat 01 GPU 95   cat 03 EVENT 8   cat 04 TIMER 5   cat 05 THERMAL 1
cat 06 I2C 4    cat 08 BIOS 5    cat 0a INTERNAL 175 (perf/pmgr/nvlink/...)
cat 10 CLK 1    cat 11 FIFO 23   cat 12 GR 43     cat 13 FB 37
cat 17 MC 7     cat 18 BUS 31    cat 20 PERF 13   cat 22 RC 8
cat 23 GPIO 4   cat 24 NVD 5     cat 25 DMA 2     cat 26 PMGR 1
cat 27 POWER 2  cat 28 LPWR 2    cat 2a CE 18     cat 30 NVLINK 57
cat 31 FLCN 6   cat 34 ECC 7     cat 35 FLA 4     cat 36 GSP 6
cat 38 GRMGR 1  cat 3b BIF 1     cat 3d UCODE_FUZZER 5
cat 40 DISPLAY 15  cat 41 (41) 2  cat 81 GPU_LEGACY_NON_PRIVILEGED 3
cat a7 POWER_LEGACY_NON_PRIVILEGED 1
```

The control-relevant subset (exact sizes as registered — no size guessing):

| Category | methodId | paramSize | name |
| --- | --- | --- | --- |
| THERMAL (05) | 0x20800513 | 1432 | `subdeviceCtrlCmdThermalSystemExecuteV2` |
| CLK (10) | 0x20801037 | 6024 | `subdeviceCtrlCmdClkPmumonClkDomainsGetSamples` |
| PERF (20) | 0x2080200a | 8 | `KPerfBoost` |

> `KPerfBoost` is the one host-registered PERF write the local NVK stack actually exercises:
> `libvulkan_nouveau.dylib` sends it twice (device create: `BOOST_TO_MAX|CUDA_YES` + `INFINITE`;
> device destroy: `CMD_CLEAR`), both gated on `nvrm610_on("BOOST")` and both `NV_ESC_RM_CONTROL`.
> See `nvk-perf-boost-internal.md`. This is a driver-internal request, **not** an agent SET.
| PERF (20) | 0x2080200b | 780 | `PerfGetLevelInfo_V2` |
| PERF (20) | 0x2080205a / 0x2080205b | 4 | `PerfGetPowerstate` / `PerfSetPowerstate` |
| PERF (20) | 0x20802068 | 4 | `PerfGetCurrentPstate` |
| PERF (20) | 0x20802069 | 8 | `PerfGetTegraPerfmonSample` |
| PERF (20) | 0x2080206e / 0x2080206f | 12 | `PerfRatedTdpGetControl` / `…SetControl` |
| PERF (20) | 0x20802087 | 12 | `PerfGetVideoEnginePerfmonSample` |
| PERF (20) | 0x20802093 | 1 | `PerfReservePerfmonHw` |
| PERF (20) | 0x20802096 | 55888 | `PerfGetGpumonPerfmonUtilSamplesV2` |
| PMGR (26) | 0x20802609 | 8 | `PmgrGetModuleInfo` |
| POWER (27) | 0x2080270d / 0x2080270e | 12 / 8 | `Gc6Entry` / `Gc6Exit` |
| INTERNAL (0a) | 0x20800a80 | 16 | `InternalPerfGpuBoostSyncGetInfo` |
| INTERNAL (0a) | 0x20800a81 | 4 | `InternalPerfGetAuxPowerState` |
| INTERNAL (0a) | 0x20800a93 | 3880 | `InternalNvlinkGetLinkAndClockInfo` |
| INTERNAL (0a) | 0x20800ace | 2 | `InternalPerfPfmReqHndlrGetPm1State` |
| INTERNAL (0a) | 0x20800ad4 / 0x20800ad5 | 8 | `…PfmReqHndlrGetVpstateInfo` / `…GetVpstateMapping` |
| INTERNAL (0a) | 0x20800afd | 24 | `InternalPmgrPfmReqHndlrGetEdppLimitInfo` |

**What is *not* there** (all counts are rows in the live table):

* no fan/cooler control of any kind (0 rows in every category);
* no voltage command — category `0x32` (VOLT) is absent, as are the
  legacy-non-privileged `0xb2` and legacy-privileged `0xf2` rows;
* no clock-offset / V-F-point / ClockClient command — category `0x10` has
  exactly one row (the PMUMON sample reader above), and categories `0x90`
  (CLK legacy non-privileged) and `0xd0` (CLK legacy privileged) have none;
* no thermal-target/policy command — one THERMAL row total.

This replaces the older, weaker statement ("the public SDK headers carry no
`NV2080_CTRL_CMD_*` for these families") with the stronger one: **the loaded
driver's own compiled method table has no such entry**, so
`rmapiutilGetControlInfo()` cannot resolve them on this host.

Registered ≠ working: `0x56 (NV_ERR_NOT_SUPPORTED)` is *not* "not in the
table". The INTERNAL/PM reads that *are* registered all answer `0x56` here with
their exact sizes (`0x20800ace`, `0x20800ad4`, `0x20800a80`, `0x20800a81`,
bundle `stage-regcmp-20261017T081500Z`), i.e. the handler exists and declines
in this context. Failure codes must be attributed per command, not per code.

## 4. Open anomaly: two accepted ids that the table does not contain

Measured on the same host (all GET, read-only, `nvx_known_set_ids` filtered):

| cmd | paramsSize | stamp | seed | status | payload |
| --- | --- | --- | --- | --- | --- |
| 0x20809061 | **776** | none | none | `NV_OK` | 8 B head (`0xFF`, `0x7FF`) + 32×24 B records (`tag 0x0303`, domain bit, kHz value, 3 tail dwords) |
| 0x20809061 | 83996 / 157692 / 249844 / 369796 / 493060 | VFP whitelist stamps | — | `0x1f` | — |
| 0x2080B201 | **2444 … 500008** (2440 and below rejected) | none | none | `NV_OK` | 12 B head (`0xFF`) + one record containing 800000 µV / 756250 µV |
| 0x2080B213 | **1036** | none | none | `NV_OK` | 12 B head (`0xFF`) + 32×32 B zero records |
| 0x20809019 / 0x2080901b / 0x20809006 / 0x20809062 / 0x20809023 / 0x20809025 / 0x20809026 / 0x2080B202 | documented sizes | documented stamps | documented seeds | `0x1f` | — |

> **Superseded for the clock ids (2026-10-17, later, read-only).** Those `0x1f`
> results used the *documented NVAPI* sizes. This host's private controls are
> served by the **GSP**, which validates the parameter block against an exact
> `paramSize` of its own; retried at the GSP's size, `0x20809019@12336`,
> `0x2080901b@2108` and `0x20809025@7188` all answer `NV_OK`. `0x2080901b`'s
> input mask sits at **`+4`** (not the NVAPI caller's `+8`), and
> `0x20809025@7188` decodes to a per-domain offset **range** of ±18750 kHz.
> See `gsp-method-table-and-legacy-forwarding.md` §4, §5.1, §5.2.

The `0x20809061` acceptance is *size-dependent* (776 only, out of a 44-step
ladder from 4 B to 4.3 MB), and the 776 table is **live**: tracked at 1 Hz
across an original-NVOC Vulkan load, its `0x1` record matched the RUSD
`RUSD_CLK_PUBLIC_DOMAIN_GRAPHICS.actualClkKHz` sample-for-sample
(1530000 → 1950000/1935000 → 1530000) and its `0x10` record matched the RUSD
memory-domain clock 15/15 (`stage-clkload-20261017T075559Z`).

Yet those ids are **not in the method table**, and they do not appear in the
loaded binaries *in any form*:

```
0x20809061 u32le=0  u16le(low16)=141  ascii=0
0x2080b201 u32le=0  u16le(low16)=141  ascii=0
0x2080901b u32le=0  u16le(low16)=129  ascii=0
   (contrast: 0x20801037 u32le=1, 0x20800102 u32le=19)
```

NVRM, NVAccel, NVRMFB, NVRMAGDC, `libvulkan_nouveau.dylib`, `libvulkan.dylib`
and `air-opt` were all scanned; no id of these families appears as a u32, as
the LE half-word, or as ASCII.

**This section is superseded (2026-10-17).** The responder *is* the GSP
firmware, reached by the host's legacy-non-privileged forwarding path. The
reason none of it was visible before is that the firmware file lives at
`/Users/Shared/nvfw/nvidia/610.57.04/gsp_tu10x.bin` — outside every path that
had been searched — and the host's only test of these ids is the single legacy
bit `0x8000` (`rmapi_deprecated.h:41`), not a table lookup, so the size
comparison at `control.c:448-458` genuinely never runs for them. Full write-up,
chain, table and corrected claims:
`docs/reverse-engineering/nvapi/gsp-method-table-and-legacy-forwarding.md`.

Two of this section's negative anchors are also now *explained* rather than
open: `0x2080FFFF -> 0x1b` is the privileged pair `0xC000` plus a non-root
caller (a privilege answer, not a missing command), and `0x2080A0FF -> 0x56` is
a successful forward to a GSP that has no such method. And `0x2080b201` turned
out to keep answering while every genuinely GSP-forwarded id was failing with
`0x62 NV_ERR_RESET_REQUIRED`, so that one id is **not** served by the GSP
legacy path either.

For the record, the earlier "disambiguation steps":

1. ~~run the same id through a second user-client type~~ — **done, negative**
   (see §5): `--service nvrm` and `--service nvaccel` answer byte-identically,
   so the responder is the shared RM behind both user clients, not a separate
   NVAccel-side shim;
2. the *object* axis was checked too (`--reg-read`); the GSP forward is
   per-device, not per-object;
3. the firmware-standing question is settled by the GSP's own export table
   (889 rows; 169 CLK + 151 VOLT), so no author request is needed for it.

`0x20809061@776` is therefore re-attributed to the GSP, not left
"unattributed"; the private clock/VF/voltage controls remain write-gated.

## 5. Live-table cross-check (`--reg-read`) and the user-client A/B

`--reg-read` sends every id below at exactly the paramSize the extracted table
registers and records the status plus a payload summary (`--service` pins the
user client). Bundle `stage-reg-20261017T082800Z` / `stage-svc-20261017T085500Z`:

| result | ids | reading |
| --- | --- | --- |
| `NV_OK`, payload written | `0x20802609` (PMGR), `0x20801239`, `0x2080122a` (KGR), `0x2080200b`, `0x2080205a`, `0x20802068`, `0x20801354` | registered *and* answered with their live size; the KGR/GPC-mask rows are the first GPC-topology reads this host yields |
| `0x1f` at the **registered** size | `0x20800102` (580), `0x20801201` (32), `0x20801303` (1028), `0x20802087` (12), `0x20802096` (55888) | the pre-dispatch size gate passed, so this is *handler-side input rejection* (empty info list / zero sample count / unset engine index), not a bad size. `0x20802087` answers `NV_OK` once `engineType` is set (bundle `stage-regcmp-20261017T081500Z`), which confirms the reading |
| `0x56` at the registered size | `0x20801037`, `0x20800ace`, `0x20800ad4`, `0x20800a80`, `0x20800afd`, `0x20800a1c`, `0x20800a36`, `0x20800a40`, `0x20800a7a`, `0x20800406` | handler exists, declines in this context |

So a `0x1f` is only evidence of a *size* problem when contrasted with the
table; at the registered size the interesting question becomes *which input
field* was wrong. This is the discrimination the earlier private-id scan could
not make, and it is now part of the pass rather than a footnote.

### 5.1 The user-client A/B is negative

`nvx_xnu.c` prefers `IOServiceOpen(NVAccel, type 0x4E560000|n)` and falls back
to plain `NVRM`, mirroring `nvrm_xnu_open()`. `--service nvrm|nvaccel` pins one.
Across the whole `--reg-read` + `--clklevel-read` battery the two produce
**byte-identical** status and payload lines, including the unexplained
`0x20809061` values above. The responder therefore lives behind both user
clients (shared RM), and "a separate NVAccel shim answers the odd ids" is ruled
out for this host.

## 6. `PERF_GET_LEVEL_INFO_V2`: registered, `NV_OK`, and empty

`NV2080_CTRL_CMD_PERF_GET_LEVEL_INFO_V2 (0x2080200b)` is the only clock-shaped
command in the live table, and 780 is exactly the size of the published
parameter block (`ctrl2080perf.h:679-684`):

```
struct { NvU32 level; NvU32 flags;
         NV2080_CTRL_PERF_GET_CLK_INFO perfGetClkInfoList[32];   // 24 B each:
         NvU32 perfGetClkInfoListSize; }                         // flags, domain,
                                                                 // current, default,
                                                                 // min, max
4 + 4 + 32*24 + 4 = 780
```

`--clklevel-read` exercises it at levels 0/1/2 plus the `TYPE_OVERCLOCK` bit
(bundle `stage-clklevel-20261017T083400Z`). Result:

| input | status | output |
| --- | --- | --- |
| level 0, flags 0 | `NV_OK` | flags `0x2` (MODE_DESKTOP), `0 of 32` records filled, listSize 0 |
| level 1 | `NV_OK` | flags `0x0` (MODE_NONE), `0 of 32` |
| level 2 | `NV_OK` | flags `0x4` (MODE_MAXPERF), `0 of 32` |
| level 0, flags `0x1` (OVERCLOCK) | `NV_OK` | flags `0x2`, `0 of 32` |
| level 2, flags `0x5` | `NV_OK` | flags `0x4`, `0 of 32` |
| level 0, **listSize preset 32** | `0x1f` | field is input-shaped; a non-zero value is refused |

This is the "returns success but only a placeholder" failure layer in the
flesh: the level→mode echo is real and deterministic, the per-domain
current/default/min/max table is never populated. Per the **public 610.57.04
source** that is the expected outcome on this configuration, which is the
second data point pointing at the HAL layer:

* the non-VF HAL for this id is `subdeviceCtrlCmdPerfGetLevelInfo_V2_92bfc3`
  (`g_subdevice_nvoc.h:8137`): `NV_ASSERT_PRECOMP(0); return NV_ERR_NOT_SUPPORTED;`
* the vGPU HAL (`subdeviceCtrlCmdPerfGetLevelInfo_V2_VF`,
  `kern_perf_ctrl.c:194`) just forwards to `rpcPerfGetLevelInfo_HAL`, and TU116
  is on the `rpcPerfGetLevelInfo_STUB` list (`g_rpc_private.h:431`), which
  returns `NV_VGPU_MSG_RESULT_RPC_UNKNOWN_FUNCTION`.

Neither arm populates the clock list, yet the box answers `NV_OK` with a
level-dependent mode flag. Compare `0x20801037`, whose non-VF HAL is the same
kind of stub and which *does* answer `0x56` — so the two ids are not answering
through the same code path. Attribution stays open: the outcome is recorded as
**"registered, succeeds, returns no clock data"**, not as "unsupported" and not
as "range read".

Consequence for stages 4-7: `PERF_GET_LEVEL_INFO_V2` cannot back a clock-range
UI on this host as-is. The clock-domain **control read** remains the
unattributed `0x20809061 @776` (live kHz table, no decoded offset/range fields),
and the offset/range decoder written for `0x2080901b` stays unused because that
id answers `0x1f`.

## 7. Reproducing

```
# on the test machine (read-only)
# the *host* NVOC export table:
python3 probe-macos-rm/tools/extract-nvrm-method-table.py \
    /Library/Extensions/NVRM.kext/Contents/MacOS/NVRM \
    "$OGKM" > outputs/method-table-nvrm-<ver>.tsv

# the *GSP* export table (the one that actually answers the private ids):
python3 probe-macos-rm/tools/extract-gsp-method-table.py \
    /Users/Shared/nvfw/nvidia/610.57.04/gsp_tu10x.bin \
    > outputs/gsp-table-<ver>.tsv

python3 - <<'EOF'
import struct
b = open("/Library/Extensions/NVRM.kext/Contents/MacOS/NVRM","rb").read()
for i in (0x20809061, 0x2080B201, 0x2080901b):
    print(hex(i), b.count(struct.pack("<I", i)))
EOF
```

> **Caution (2026-10-17):** these private GETs are forwarded to the GSP and a
> malformed one can wedge it — `0x2080901c@808` and `0x20809026@5332` with a
> `0xA5` background both returned `0x65 NV_ERR_TIMEOUT`, after which every
> legacy-forwarded id answered `0x62 NV_ERR_RESET_REQUIRED` for the rest of the
> session. Recovery needs a driver reload or reboot. See
> `gsp-method-table-and-legacy-forwarding.md` §6.

Bundles cited here: `stage-clkdoc-20261017T074256Z`,
`stage-clkdoc-20261017T074448Z`, `stage-clkobj-20261017T074744Z`,
`stage-clkobj-20261017T074755Z`, `stage-clkload-20261017T075559Z`,
`stage-regcmp-20261017T081500Z`, `stage-reg-20261017T082800Z`,
`stage-clklevel-20261017T083400Z`, `stage-svc-20261017T085500Z`.
