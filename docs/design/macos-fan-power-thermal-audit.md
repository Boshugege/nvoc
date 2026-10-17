# Fan, power and thermal on the macOS backend: three independent read-only audits

Date: 2026-10-17. Card: GTX 1650 SUPER / TU116 / `10de:2187`, macOS 15.8.1, NullMoth 2.0.4,
RM/OGKM 610.57.04, no sudo.

The goal asks for these three families to be investigated **separately** and for each to give
its own evidence for *readable / interpretable / range / original state / restore*, so a
blocked clock-offset channel cannot be used to declare the sensor families unavailable. This
document does that. It is deliberately not a summary of the feature matrix: it states, per
family, what has actually been observed, where the observation came from, and what is still
open.

## Source-side cross-check (this round)

The rows above cite three evidence classes. `[header]` and `[api]` were re-derived
from the 610.57.04 tree itself rather than from grep impressions, and the result
is worth stating plainly:

* The whole public SDK contains **775** `NV2080_CTRL_CMD_*` ids. The only clock
  command among them is `NV2080_CTRL_CMD_CLK_PMUMON_CLK_DOMAINS_GET_SAMPLES`
  (`0x20801037`, the public clock table the probe already measured); named
  `VOLT` ids: none. `ctrl2080volt.h`, `ctrl2080clkavfs.h`, `ctrl2080pmumon.h`
  and `ctrl2080fan.h` are all generated stubs with **zero** command defines.
  So the clock/voltage control plane is private by construction, which is the
  stated premise of the offset work, now with a citation.
* `ctrl2080thermal.h` is real (417 lines, 2 commands) and the accepted batch
  matches it byte for byte: 1432 = 24 + 32 x 44. Corrected census: the firmware
  table's thermal family is 32 GET / 6 SET rows, so 31 non-privileged thermal
  reads are still untried even though the documented instruction set is not.
* Fan: the stub is empty (33 lines) and `NV2080_CTRL_FAN` appears nowhere in the
  tree; the interface lives in NVAPI's cooler API, recorded in
  `nvapi-rs/sys/src/gpu/cooler.rs`.

## Method and evidence classes

Every claim below carries its class, because the three classes have different strength:

* **[probe]** measured by `probe-macos-rm` (read-only, one purpose-built command battery per
  family). Raw bundles live on the test machine under `outputs/stage-*`; the tool's own record
  is [`probe-macos-rm/VALIDATION-MATRIX.md`](../../probe-macos-rm/VALIDATION-MATRIX.md). The
  numbers quoted here were re-read from the bundles on 2026-10-17, not from that summary.
* **[backend]** measured through the product path (`nvoc-cli` / `pynvoc` -> `nvoc-core` macOS RM
  backend -> RUSD page).
* **[header]** a fact about the public OGKM 610.57.04 tree, reproducible with grep.

Re-reading the bundles instead of the summary changed two statements in this document: the fan
tachometer has been observed **spinning** (not only at zero), and `average_power_mw` is
reported as a signed `-1` rather than as a missing field. Both are recorded below as measured.

## 1. Thermal

| | |
| --- | --- |
| Readable? | **Yes, on two independent paths.** `[probe]` public in-tree `NV2080_CTRL_CMD_THERMAL_SYSTEM_EXECUTE_V2` `0x20800513` (non-privileged twin `0x20808513`), paramSize 1432, accepted with `NV_OK`; `[backend]` RUSD `temperature_c` is a live field in `get-status`. |
| Interpretable? | **Yes.** `[probe]` 1 target, type 1 (=GPU per the driver's own enum 1/2/4/8 = GPU/MEMORY/POWER_SUPPLY/BOARD), 1 sensor, provider 0 type 1, all four sub-opcodes `rc 0x0`. `[backend]` the RUSD temperature is in °C to 1/256 resolution. |
| Range | `[probe]` the driver's own `SENSOR_READING_RANGE` says `[0,127]`. `[backend]` observed 44.46-51.90 °C across recorded bundles, and the two paths agree (47 in the batch, 47 in the concurrent RUSD read). |
| Original state | `[probe]` sensor 0 reading 47, target 0, provider 0, exec 1/1/1/1. `[backend]` nothing has ever been written, because no thermal write has been sent. |
| Restore | Nothing to restore yet: the read path writes nothing. The batch interface is *capable* of setting a sensor target (same opcode family), but no SET has been issued under the write gate, so "restore" is untested rather than impossible. |
| Interface | `[header]` the accepted batch is the documented struct **to the byte**: `NV2080_CTRL_THERMAL_SYSTEM_EXECUTE_V2_PARAMS` is 6 u32 (24 B) plus `NV2080_CTRL_THERMAL_SYSTEM_INSTRUCTION[0x20]`, and the measured paramSize is 1432 = 24 + 32 x 44, so `INSTRUCTION` is exactly 44 B (result +0, executed +4, opcode +8, 32 B operands +12). That is why the probe's per-instruction `exec`/`rc` fields line up, and it fixes the offsets for any future instruction without guessing. `[header]` The target enum defines MEMORY(2), POWER_SUPPLY(4) and BOARD(8), yet the driver counts 1 target / 1 sensor, so the absence of those temperatures is the driver reporting no such target object, not the tool failing to ask. |
| What is still unused | `[table]` the *documented* instruction set is fully exercised, but the *family* is not used up: the corrected GSP census (`outputs/stage-gsp-table-recount-20261017T130832Z/families.txt` and its `gsp-table.tsv`, 1118 rows) gives cat `0x05 THERMAL` **32 GET / 6 SET** rows, of which exactly **one** is the public `0x20800513@1432` and **31** are the non-privileged read half `0x208085xx` (`0x20808501@1036`, `0x20808513@1432`, `0x20808524@4`, ...). The host table has only **1** thermal row, so those 31 live firmware-side and are reachable by the same legacy forwarding this project already uses. No id names are available (the SDK names two), so a family-scoped, size-checked, read-only pass over the 31 is the one justified next step here -- not a blind scan: the family is named by the dispatch table and every paramSize is known before the call. `0x20808513` being *present* also means the twin is a real firmware row, so a `_PHYSICAL` refusal may not be attributed to "host-side only" (an earlier draft of this row said exactly that, from the stale 889-row table; withdrawn). |
| Open | Only 1 target/1 sensor exists, which matches `[backend]` `UNPUBLISHED_ON_THIS_GPU` (`temperature_memory_c`, `temperature_board_c`, `temperature_power_supply_c` are measured absent with a reason, not fabricated); whether the batch can also *set* the GPU target, and whether that is a different resource from the enforced limit, is unmeasured. |

## 2. Power

| | |
| --- | --- |
| Readable? | **Partly, and the boundary is measured.** `[backend]` the enforced limit reads as `limit 100000` mW and instantaneous draw as `tgp` 11272-11492 mW plus `inst` 11344-11531 mW at idle. `[probe]` the RATED_TDP arbitration control `0x2080206E` answers `NV_OK` for all five documented clients, and rejects client 5 and 255 with `0x1f` -- so the lookup is validated, not a blind `NV_OK`. **Not readable:** `[probe]` `GET_EDPP_LIMIT_INFO` `0x20800AFD` returns `0x56 NV_ERR_NOT_SUPPORTED`, so the min/rated/max window is unavailable; `[backend]` `average_power_mw` stays unpublished (the RUSD field reads `-1`), and the four internal PM GETs (`GET_PM1_STATE` `0x20800ace`, `GET_VPSTATE_INFO` `0x20800ad4`, `PERF_GPU_BOOST_SYNC_GET_INFO` `0x20800a80`, `PERF_GET_AUX_POWER_STATE` `0x20800a81`) all return `0x56`, which takes the whole internal write candidate set (UPDATE_TGPU_LIMIT, SET_VPSTATE, CONFIGURE_TGP_MODE, CONFIGURE_TURBO_V2, SET_PM1_STATE, UNSET_DYNAMIC_BOOST_LIMIT) out of reach as a *readable* pair. |
| Interpretable? | `[backend]` mW, and 100000 mW = 100 W is exactly the board's rated TDP, so the enforced limit is a meaningful absolute wall rather than an opaque number. `[probe]` RATED_TDP client 2 (GLOBAL) reports `action DEFAULT(0)` with `vPstate 1` (TURBO_BOOST), clients 0/1 (RM, WAR) with `vPstate 0` (RATED_TDP). |
| Range | Enforced limit 100000 mW; idle draw ~11.3 W; RATED_TDP `action` domain is the documented enum (DEFAULT / FORCE_EXCEED in the public header). |
| Original state | `[probe]` all five clients `DEFAULT(0)`, i.e. no client has requested anything; the derived `PERF_RATED_TDP_STATUS` id `0x2080206d` answers `0x56`, so that half of the arbitration is *not* confirmed and the id remains derived. |
| Restore | The one write pair implemented for this family is `PERF_RATED_TDP_SET_CONTROL` `0x2080206F` (`[probe]` V3.4): it records the original per-client action first and refuses to write if the pre-read fails, then read-back compares, and `atexit`/SIGINT/SIGTERM restore the original. It is implemented but **still gated and unsent**. |
| Open | Whether `action FORCE_EXCEED` on GLOBAL actually lifts a real ceiling on this card is unmeasured (needs one authorised write). The min/rated/max window may only become readable through the offset/VF family's own blocks. |

**RATED_TDP is an independent candidate, not a substitute for offset or VF control.** It changes
which arbitration ceiling applies; it does not change a clock offset, does not change a voltage
point, and says nothing about stability at a given clock. It must never be reported as evidence
that the offset channel works.

## 3. Fan

| | |
| --- | --- |
| Readable? | **Yes for the tachometer** `[backend]`: RUSD publishes `fan_rpm` (and a `fan_rpms` array), and it has been observed both at 0 and at a real speed. |
| Interpretable? | `[backend]` rpm. `[backend]` recorded values: a bundle records `fan 1654 rpm` alongside the coolest reading in the set (44.46 °C), while every other recorded readout is `fan 0 rpm` at 48.65-51.90 °C. So the field is live and a zero-RPM idle state is real on this board; the two states cannot be ordered into a curve from snapshots alone. |
| Range | Observed {0, 1654} rpm at idle-ish load. No upper bound has been measured, because no load has been applied. |
| Original state | Stock idle: 0 rpm at 48-52 °C (semi-passive), with a recorded excursion to 1654 rpm. Nothing has been written. |
| Restore | Nothing to restore: fan control is read-only on every path used so far. |
| Open | The fan *control* has no command in the OS-facing control space: `[header]` `ctrl2080fan.h` is a **33-line generated stub with no content at all** -- license header, `#pragma once`, and the comment `Source file: ctrl/ctrl2080/ctrl2080fan.finn` are the whole file, so there is not one define or struct in it; `[header]` the token `NV2080_CTRL_FAN` occurs **nowhere in the whole 610.57.04 tree**, which is what distinguishes "the file was generated empty" from "my grep pattern missed it". `[api]` The interface does exist one layer up: nvapi-rs records the whole cooler surface -- `NvAPI_GPU_GetCoolerSettings` V1/V3/V4 (152/1288/1368 B), `NvAPI_GPU_SetCoolerLevels`, `NvAPI_GPU_RestoreCoolerSettings`, `NvAPI_GPU_GetTachReading`, with cooler types FAN/WATER/LIQUID_NO2, per-cooler `currentLevel`/`currentMinLevel`/`currentMaxLevel` in percent, a policy enum, and a tachometer `{speedRPM, bSupported, maxSpeedRPM, minSpeedRPM}`. So the fields the table above asks for (range, original state, restore) *do* exist in a fan interface -- what is unproven is the mapping from that interface onto a control the macOS RM path accepts. `[table]` Suggested, not proven: no row of the GSP forwarding table has paramSize 1288 or 1368, and 152 appears only for four unrelated ids, so the cooler interface is not *identifiable* in the firmware table either (the table carries no names, so this is a shape argument, not a proof). The VRAM/board sensor absence also means any curve would be driven by the single GPU sensor. |

## Summary

| Family | Read | Interpret | Range | Original state | Restore | Classification |
| --- | --- | --- | --- | --- | --- | --- |
| Thermal (GPU) | measured, two paths | measured | measured `[0,127]` | measured | n/a (no write sent) | implemented-and-measured (read); write awaiting authorisation |
| Thermal (mem/board/PSU) | measured absent, reason two-sided (`[header]` enum defines them, driver counts one target) | n/a | n/a | n/a | n/a | not applicable to this GPU, reported with a reason |
| Power: enforced limit + draw | measured | measured | 100 W rated / ~11.3 W idle | measured | n/a | implemented-and-measured (read) |
| Power: min/rated/max window | refused `0x56` | n/a | n/a | n/a | n/a | refused by the driver (EDPP not supported on desktop TU116) |
| Power: RATED_TDP arbitration | measured (5 clients, validated lookup) | measured | read: no client requested anything | measured | implemented, not sent | implemented-awaiting-measurement |
| Fan tachometer | measured (0 and 1654 rpm) | measured | {0, 1654} rpm observed | measured | n/a | implemented-and-measured (read) |
| Fan control (target) | no command in the OS-facing control space (`[header]` empty generated stub, `NV2080_CTRL_FAN` nowhere in the tree); exists as NVAPI coolers (`[api]`) | n/a | interface defines percent + tachometer max/min | unknown on this driver | interface provides `RestoreCoolerSettings` | investigation-open, layer named: NVAPI cooler API -> control mapping |

Nothing in this document required a write. The only writes that could happen next are the
already-implemented RATED_TDP SET (gated) and, later, a single-domain offset step, and both are
required to record the original value before writing and to restore it.
