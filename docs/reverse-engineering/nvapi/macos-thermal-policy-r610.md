# TU116 / GSP 610.57.04 thermal policy target

This note records the native macOS thermal policy route validated on the measured GTX 1650 SUPER (TU116, RM 610.57.04). It is separate from the public sensor read and from the fixed GPU Max policy.

## Static protocol source

IDA 9.5.261001 followed the same-version Linux `libnvidia-ml.so.610.57.04` thermal helpers. The extracted library, IDA database, and raw captures remain outside Git under `outputs/control-protocol-20261021/`. The GSP method table contains these fixed commands:

| Operation | RM command | Size | Seed |
| --- | --- | ---: | --- |
| policy metadata | `0x2080852a` | 1784 | zero |
| policy control GET | `0x2080852c` | 1440 | u32 +8 = 5 |
| policy control SET | `0x2080c52d` | 1440 | complete saved block |

The metadata mask is 5. Policy index 0 is GPU Max; policy index 2 is the acoustic target. The current device returns selectors `0xff, 2, 0xff, 0`, with the last selector not enabled for this consumer.

## Measured policy values

- GPU Max policy 0: min/default/max **88/88/88°C**. It is fixed on this device and is not treated as the acoustic target.
- Acoustic policy 2: min/default/max **65/83/87°C**; current value starts at 83°C.
- Driver-derived curve fields are normalized on SET. The transaction sends only the acoustic target at control offset `+212` (FX8), then expects the complete known normalized reply: target at +212/+236, derived points at +248/+276/+280/+284/+288, while preserving all other bytes and policy records.
- Shutdown selector 3 returns `NV_ERR_NOT_SUPPORTED (0x56)`; Slowdown cache decoding is not yet located; MemoryMax and GpsCurr selectors are not published for this card. These remain null with a shared reason.

## Transaction and consumers

The native bridge has fixed operation kinds and exact sizes. Rust rechecks TU116/610.57.04 identity, metadata mask/selectors, type 4, bounds, FX8 whole-degree encoding, current/default values, calibration fields, and derived-field model. A process-local transport mutex spans metadata, snapshot, SET, full normalized readback, and rollback. Root is required. Errors, readback mismatches, and cooperative cancellation restore the exact original 1440-byte block with bounded retries. SIGKILL, abort, and power loss cannot execute cleanup.

The shared `QueryTemperatureThresholds` operation now reports GPU Max and acoustic min/current/max independently. `SetNvmlAcousticTemp` dispatches to the native transaction on macOS; GPU Max writes explicitly refuse because it is a fixed policy.

- CLI: `get-temp-thresholds --macos-rm`; `set-temp-limit 82 --domain acoustic --macos-rm`; `reset-temp-limit --domain acoustic --macos-rm`. Missing/incorrect domain and 64/88°C requests fail before GPU selection.
- Python: `set_thermal_limit('0x100', 82, 'acoustic')` and `reset_thermal_limit('0x100', 'acoustic')`. The existing non-macOS routes remain unchanged.
- Settings publish `thermal_policy` and narrow `acoustic_target_write`; bulk settings/restore, GPU Max writes, FanStop, service loops, and optimizer remain separately gated.

## Device evidence

- `stage-thermal-policy-read-20261021T175735Z/`: all four fixed GETs accepted; metadata/status/control decoded; unsupported shutdown selector recorded without writing.
- `stage-thermal-target-roundtrip-20261021T180259Z/`, `180301Z`, `180304Z`, `180307Z`: target 82, 81, 65, 87°C respectively, each restored the original 1440-byte block.
- Final product `stage-product-thermal-20261021T233009Z/`: CLI 82°C, Python 81°C, CLI reset, Python 82°C, Python reset; thresholds and normalized derived fields verified at each step; final 83°C restored. Fan 980 bytes, power 12 bytes, and clock 8432 bytes remained unchanged.
- `stage-product-thermal-rejections-20261021T233041Z/`: non-root `0x1b`, invalid bounds, missing/wrong domain, and reset without explicit acoustic domain rejected before selecting an absent GPU; original 1440-byte block unchanged.

This validates a short target-policy control loop, not thermal stability under load, arbitrary GPU Max policy changes, full severity-table support, or fan curves/FanStop.
