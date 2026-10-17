# Which RM control ids this driver really implements — per family

> Status: read-only audit, 2026-10-17. Host: macOS 15.8.1 / x86_64 /
> GTX 1650 SUPER (TU116, 10de:2187) / NullMoth 2.0.4 / NVRM 610.57.04.
> No writes, no driver/EFI/boot-args changes.

The earlier family verdicts were phrased as "the public SDK headers carry no
`NV2080_CTRL_CMD_*` for this". That is a statement about *headers*. This audit
replaces it with a statement about **the loaded driver's own compiled method
table**, and it is mechanical, so it can be re-run on any host:

1. extract the NVOC export array from the loaded image
   (`probe-macos-rm/tools/extract-nvrm-method-table.py`, 604 rows **at the
   time of writing; 608 after the 2026-10-17 relaxation of its `flags == 0`
   guard — see §16 of `gsp-method-table-and-legacy-forwarding.md`**);
2. collect every `0x2080xxxx` control id *referenced anywhere in this repo*
   (the reverse-engineering notes, `nvapi-rs`, the probe, the backends);
3. intersect.

## Result

```
distinct 0x2080xxxx ids referenced in the repo : 114
  of which present in the loaded method table  :  49
  of which absent                              :  65
```

| category | referenced | **registered** | examples of the absent ones |
| --- | --- | --- | --- |
| 0x01 GPU | 3 | 3 | — |
| 0x04 TIMER | 1 | 1 | — |
| 0x05 THERMAL | 7 | **1** (`0x20800513`) | `0x20800532` fan control, `0x20808513`, `0x2080852f/3b`, `0x20800540` |
| 0x06 I2C | 2 | 1 | `0x208086xx` |
| 0x08 BIOS | 1 | 1 | — |
| 0x0a INTERNAL | 18 | 17 | one legacy-flagged twin |
| 0x10 CLK | 32 | **1** (`0x20801037`) | `0x20809019/1a/1b`, `0x20809006`, `0x20809061/62`, `0x20809021/22/23`, `0x20809025/26`, `0x208090a0/a1`, `0x208090b0/b1`, `0x2080d01c/d024/d036/d084` |
| 0x12 GR | 3 | 3 | — |
| 0x13 FB | 4 | 4 | — |
| 0x18 BUS | 3 | 3 | — |
| 0x20 PERF | 23 | 11 | `0x2080a0b1/b3/b5/b6`, `0x2080e0b0/e0b7` (VFE), `0x2080902a` |
| 0x26 PMGR | 5 | **1** (`0x20802609`) | `0x2080a601`, `0x2080a613` |
| 0x27 POWER | 3 | 2 | one legacy twin |
| 0x30 NVLINK | 1 | 0 | — |
| **0x32 VOLT** (incl. the `0xb2`/`0xf2` legacy encodings) | 4 | **0** | `0x2080b201`, `0x2080b202`, `0x2080b213`, `0x2080f214` |
| other legacy-flagged twins | 4 | 0 | — |

**The pattern is the finding**: every id that *is* registered comes from the
public SDK; **not one** id derived from the Windows/NVAPI reverse-engineering
work is registered. So "held back by my marshalling" is no longer a plausible
reading of the clock/volt/fan families — the kernel RM on this host has no
implementation body for them.

## Per family

**Fan / cooler.** The only RM command any recorded path uses is
`0x20800532` (category `0x05`, the driver-runs-`0x07000038`-escape path). It is
**not** in the table, and no fan/cooler/acoustic category exists at all — the
whole `0x05` category is one row (`THERMAL_SYSTEM_EXECUTE_V2`). The Windows
side reaches fan control through escapes `0x07000198` / `0x07000038`, which are
not RM controls and have no macOS counterpart. Per-fan RPM stays a RUSD read.
*Feasible candidate:* none in the RM. *Verdict:* **no implementation body**.

**Thermal.** One registered row: `0x20800513 ThermalSystemExecuteV2`
(@1432 B), already wrapped read-only (`--thermal-read`) and measured:
1 target, 1 sensor, 45-47 °C, **sensor reading range [0,127]**. That range is a
*reading* scale, not a settable temperature wall — it does not license a
temperature-limit slider. The policy/wall ids (`0x20800540`, `0x20808540`, …)
are absent. *Feasible candidate:* the existing read. *Verdict:* **read yes,
wall no**.

**Clock / V-F.** One registered row (`0x20801037`), and it answers `0x56`.
The single clock-shaped command that answers `NV_OK` is
`PERF_GET_LEVEL_INFO_V2` (`0x2080200b`), which returns a level-dependent mode
flag and **zero** of its 32 clk-info records (see the method-table note §6), and
the clock-domain controls answer `0x1f` even after the NVAPI-layer magic was
removed and the size space was narrowed to four derived candidates. *Feasible
candidate:* the unattributed `0x20809061 @776` live kHz table (no offset/range
fields) plus RUSD. *Verdict:* **no control read**.

**Voltage.** Category `0x32` has **zero** registered rows in all three
encodings. The two surfaces the host *does* answer (`0x2080B201`, `0x2080B213`)
are not in the table and are not size-gated the way the table's rows are —
`0x2080B201` accepts every `paramSize ≥ 2444` — so they must be answered outside
`rmapiutilGetControlInfo()`. They yield one rail's µV pair and nothing writable.
*Feasible candidate:* the read-only `0x2080B201 @2444`. *Verdict:* **read-only,
unattributed, no set path**.

**Power / PM.** The INTERNAL PM family *is* registered — including its writes:

| id | name | size | measured |
| --- | --- | --- | --- |
| `0x20800a80` | `InternalPerfGpuBoostSyncGetInfo` | 16 | `0x56` |
| `0x20800a81` | `InternalPerfGetAuxPowerState` | 4 | `0x56` |
| `0x20800ace` | `...GetPm1State` | 2 | `0x56` |
| `0x20800acf` | `...SetPm1State` | 1 | not sent (write) |
| `0x20800ad0` | `...UpdateEdppLimit` | 8 | not sent (write) |
| `0x20800ad1` | `...UpdateTgpuLimit` | 4 | not sent (write) |
| `0x20800ad2` | `...ConfigureTgpMode` | 1 | not sent (write) |
| `0x20800ad3` | `...ConfigureTurboV2` | 4 | not sent (write) |
| `0x20800ad4` | `...GetVpstateInfo` | 8 | `0x56` |
| `0x20800ad5` | `...GetVpstateMapping` | 8 | not sent |
| `0x20800ad6` | `...SetVpstate` | 4 | not sent (write) |
| `0x20800afd` | `...GetEdppLimitInfo` | 24 | `0x56` |

This refines the earlier "the whole family is unreachable": the *writes are
compiled in*, but every read twin that could observe the pre-state answers
`0x56`, so there is no snapshot/restore evidence and the writes stay gated.
`GetEdppLimitInfo` answering `0x56` is also why the power-limit *window*
(min/rated/max W) has no source. *Feasible candidate:* RUSD enforced limit
(`= 100 W`) plus `PERF_RATED_TDP_GET_CONTROL` (`0x2080206e`, `NV_OK`, five
clients). *Verdict:* **reads partially yes, control writes gated**.

## The registered write set (for completeness, none exercised)

`0x20800acf`, `0x20800ad0`, `0x20800ad1`, `0x20800ad2`, `0x20800ad3`,
`0x20800ad6`, `0x2080205b` (`PerfSetPowerstate`), `0x2080206f`
(`PerfRatedTdpSetControl`). Only the last has a snapshot/readback path already
implemented (`probe-macos-rm/WRITE-VERIFICATION-PLAN.md`), and it still has not
been sent to the GPU.

## Reproducing

```
python3 probe-macos-rm/tools/extract-nvrm-method-table.py \
    /Library/Extensions/NVRM.kext/Contents/MacOS/NVRM "$OGKM" > table.tsv
# then intersect with every 0x2080xxxx literal in the repo (see the header)
```
