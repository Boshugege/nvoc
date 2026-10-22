# TU116 / GSP 610.57.04 fan percentage and firmware auto

The shared macOS backend supports the single fitted cooler on the measured
TU116 / 610.57.04 device. It uses fixed RM controls without loading NVML.
This is percentage/manual control and firmware auto reset; RPM targets,
hardware curves remain a separate, unimplemented control. The control record's
mode word also accepts 2, which is what `set-fanstop-status` writes for stop.

## Protocol source

IDA 9.5.261001 analysis of the same-version official Linux
`libnvidia-ml.so.610.57.04` supplied the request construction. Library provenance
and SHA256 are recorded in [the board power note](macos-board-power-r610.md).
Libraries, IDA databases and raw captures stay outside Git.

| Operation | NVML helper address | RM command | Bytes / request seed |
| --- | --- | --- | --- |
| Cooler metadata | `0xb6600` (mask consumer `0xb9280`) | `0x2080852e` | 1432, zero |
| Status | `0xb6180` (percentage consumer `0xbaab0`) | `0x2080852f` | 844, u32 at +4 = 1 |
| Current control | `0xb6300` | `0x20808530` | 980, u32 at +12 = 1 |
| Default control | `0xb6300`, reset caller `0xbb2f0` | `0x20808530` | same, additionally byte +16 = 1 |
| Set control | `0xb6480`, setter caller `0xbb910` | `0x2080c531` | 980, complete saved/modified control |

All fixed command sizes are present in the measured 1118-row GSP method table.
Metadata u32 +12 returns mask 1: cooler slot 0 is present even at zero RPM.
NVML maps API fan indices through set bits; this implementation admits only the
measured single-cooler mask, rather than assuming a general dense slot mapping.

## Fields used

Offsets below are from the start of the entire block, for cooler 0 only.

| Block / offset | Meaning in this path |
| --- | --- |
| Status +4 | selection mask 1 |
| Status +12 | measured type 0x0303 |
| Status +20 | current RPM |
| Status +24 / +28 | minimum / maximum percentage fraction, Q16: 0 / 65536 |
| Status +32 | current driver status percentage fraction, Q16 |
| Control +12 | selection mask 1 |
| Control +16 | ordinary GET selector 0; default GET echoes selector 1 |
| Control +20 | measured control type 3 |
| Control +36 | u32 mode: 0 firmware auto, 1 manual, 2 stop (accepted, echoed) |
| Control +40 | requested percentage fraction, Q16 |

The NVML writer uses `round(percent * 65536 / 100)`, implemented for integer
requests as `(percent * 65536 + 50) / 100`. A 35% request becomes 22938,
so exact decoded readback is 35.0006103515625%. Do not promise exact decimal
round trips before considering Q16 quantization.

**Status percentage and target percentage are independent.** The first fixed
35% experiment observed roughly 1800 RPM and status Q16 near 42224 (64.43%),
while target Q16 remained 22938. The shared NVML-shaped query rounds the status
percentage to an integer; extended settings retain fractional current and target
percentages. No PWM or RPM-target meaning is inferred from these values.

The control record stride in the NVML implementation is 60 bytes, beginning at
byte 20. Calibration and all other records are preserved; fields such as +32,
+48, +60 and +64 are not reinterpreted as writable RPM or policy parameters.

## Transaction and public consumers

The native bridge has five fixed kinds, exact sizes and request seeds. SET
requires root and validates mask, ordinary selector, type, mode and Q16 range.
The Rust device session rechecks PCI/device/RM identity and driver version.
A process-wide transport mutex spans metadata/status, snapshot, SET, complete
readback, and any rollback. It does not serialize unrelated external processes.

Fan stop is the same manual-shaped write with mode 2 and a zero target; nothing
else in the record changes. It exists because the driver accepts the value:
NVML's own writer hard-codes mode 1, so stop is not reachable through NVML even
though the firmware takes it. On this board the stop mode and manual 0 percent
behave the same (both hold 0 rpm), so the difference recorded here is the mode
word, not an observation - with one measured distinction: the stop write parks the
fan only after a few seconds, the same lagged response the percentage writes show
rpm 1922 immediately after `on` and 0 four seconds later.

Manual modifies only bytes +36..+44 of the saved 980-byte control. Auto obtains
the default block, validates mode 0, selector 1 and equality of all remaining
bytes against the snapshot, then copies only mode/target into the original
ordinary-selector block. It does not send the echoed GET-default flag as a SET
selector. Any unvalidated default/calibration difference fails before SET.

The existing shared `QueryFanInfo`, `SetFanSpeed` (manual only) and
`ResetFanSpeed` operations dispatch to the device. `SetMacosRmFanControl` adds
cooperative cancellation through the same device implementation. Errors after
SET and cooperative cancellation restore and verify the exact saved block using
the existing bounded rollback transaction. Successful setters persist. Abort,
SIGKILL and power loss cannot execute process cleanup.

- CLI: `set-fan-speed 35 --percent --fan 0 --macos-rm`,
  `reset-fan-speed --fan 0 --macos-rm`; omitted/all fan selects the one cooler.
  RPM/legacy cooler selectors, slot >0 and software continuous policy are refused
  before discovery. Firmware auto uses the reset command.
- Python: `set_fan(gpu, 'macos-rm', '0', 'manual', 35)`,
  `set_fan(gpu, 'auto', 'all', 'auto', None)`, `reset_fan_speed(gpu, 0)` on macOS.
  Explicit NVML/NVAPI selections retain their existing paths.
- Device/Python settings include `fan_control` and narrow `fan_percent_write`.
  Bulk settings/restore, UI writes, service loops and optimizer stay separately gated.
- Device fan queries use metadata and retain a present zero-RPM cooler.
  The telemetry-only `DeviceStatus::fan_report` remains a limited RPM projection;
  it cannot infer populated slots from zero readings alone.

## Device evidence

- `stage-fan-control-roundtrip-20261021T172842Z`: fixed 35% external experiment,
  complete target readback, RPM 0 to about 1800, original auto control restored
  byte-for-byte, healthy before/after.
- Final `stage-product-fan-20261021T175023Z` (earlier run `stage-product-fan-20261021T174112Z` also passed): actual CLI 35% then Python 36%; CLI reset,
  Python auto and Python reset each restore the original auto block. The GET-default
  block was captured while manual. All 980 original fan bytes restored; power 12
  bytes and four clock-control masks (8432 bytes) unchanged. Healthy anchors.

These are short, low-temperature control checks, not a thermal stability test,
proof of every percentage, or validation of RPM or curve control. Fan stop has its
own round trip in `outputs/stage-fanstop-product-20261022T033318Z`: on/off with
mode-2 readback, unchanged neighbouring domains and a byte-identical restore.
