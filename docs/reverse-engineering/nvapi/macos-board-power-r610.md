# TU116 / GSP610.57.04 board power limit

This records the native macOS route validated after the clock-offset work. It
uses the existing RM transport and firmware controls; no NVML runtime is loaded
on macOS and no driver, firmware or boot configuration is changed.

## Sources and method

The official Linux610.57.04 package supplied two files for **static analysis**:
`libnvidia-ml.so.610.57.04` (SHA256
`50feda0f0d2712bd3b82d1c2f8c5083b9169607d5db481ebc278ec76582068a0`)
and `libnvidia-api.so.1` (SHA256
`fea6a0090bbdca9b294682b34a8808e1a4e3f6613f9b6684b412893d5047cef8`).
The archive was streamed as zstd/tar; the installer was never executed. Libraries,
IDA databases and raw captures remain outside Git. IDA9.5.261001 opened and
decompiled the x86_64 NVML library. Addresses below are ELF virtual addresses.

The NVML exports first validate a device and dispatch through method tables.
Following those tables, rather than stopping at the export wrapper, gives:

| Operation | NVML implementation | RM command / bytes |
| --- | --- | --- |
| Board-policy metadata | `cDevicePowerPolicyGetInfoUncached` at `0xf75a0` | GET `0x2080a618` /20000 |
| Client configured limit | `cDeviceGetClientPowerLimit` at `0xf9b60` | GET `0x2080a61d` /12 |
| Client limit setter | `cDeviceSetClientPowerLimit` at `0xf9970` | SET `0x2080e61e` /12 |
| Policy view interpretation | `cDeviceGetPowerManagementLimitInfo` at `0x1003f0` | selects board policy and mW fields |

All three RM commands and sizes are present in the **1118-row GSP export table**.
Their absence from the host export table does not imply absence from the driver:
legacy forwarding reaches the firmware, as the actual calls below demonstrate.

## Policy metadata

GET-info starts with a zeroed20000-byte block. The NVML code chooses the first
non-`0xff` selector at byte152,154,183. The selected index must be below32 and set
in the u32 mask at byte4. Its record starts at `208 +252*index`.

| Record offset | Meaning in this path |
| --- | --- |
| +0,+1 | measured type/class0,9 for the selected TU116 board policy |
| +2 | units tag:0 means milliwatts in this consumer |
| +4 | minimum mW |
| +8 | default mW |
| +12 | maximum mW |

The device returns selector2, mask including bit2 and the bounds
**70000 /100000 /100000mW**. Other policies use other types and units; their
numbers must not be treated as board watts. The default is a policy default,
not proof of a nameplate/rated TDP.

## Client control

The12-byte GET request is zeroed, then seeded as follows. The NVML setter uses
the same layout. Its control byte1 selects the client, independently of the
metadata's policy record index.

| Offset | Width | Selected value / meaning |
| --- | --- | --- |
| 0 | u8 | board selector1 |
| 1 | u8 | client`0xfe` for the ordinary NVML power-management limit |
| 2 | u16 | zero in the measured request/response |
| 4 | u32 | configured limit in mW |
| 8 | u32 | zero in the measured request/response |

GET returned `01 fe 00 00 a0 86 01 00 00 00 00 00`, i.e.100000mW.
The implementation preserves the snapshot and changes **only bytes4..8**. Root,
TU116`10de:2187`, exact driver version, live metadata/type/units/window and the
original value are checked before SET. It currently admits only the measured
70..100W window; it does not write arbitrary policy indices or infer percentages.

## Device evidence

The initial fixed experiment at source base`6e53e8f`:

1. Snapshot100000mW, all metadata checks satisfied.
2. SET99000mW returned NV_OK; GET returned all12 expected bytes exactly.
3. RUSD requested/enforced wall advanced to99000mW on the next polled update.
4. Restored the original12-byte block; SET/GET returned NV_OK and exact identity.
5. RUSD requested/enforced returned to100000mW within the bounded3-second wait.
6. Explicit close succeeded; anchors before/after reported healthy.

Artifact:`stage-power-limit-roundtrip-20261021T170040Z/` in the external outputs.
This proves configuration, observed enforcement and restoration, rather than
merely an accepted SET. It does not establish performance under a limiting load.
Product-level CLI/Python and fault tests are indexed in feature matrix§139.

The native product uses the same exact-block transaction as clocks: a successful
setter persists; errors or cooperative cancellation restore and compare the
original snapshot with one bounded retry. Unverified recovery reports
`RESTORE NOT VERIFIED`. SIGKILL/abort/power-loss recovery is not promised. The
mutex is process-local; concurrent external writers are not an atomic transaction.
RUSD wall and configuration reads are separate and can briefly disagree.

## Clock-lock and fan leads from the same analysis

These are **investigation results, not enabled controls**:

- The ordinary NVML frequency-range helper at`0x1103a0` sends
  `0x2080e078` with a16-byte count/pointer carrier referencing two316-byte
  entries. Reset helper`0x10b8a0` uses the same command. This command is absent
  from both the measured GSP and host export tables. No guessed replacement
  SET was sent. A second memory-specific route is selected by
  `0x2080a06e` capability state and uses `0x2080e06f`/8; its prerequisites and
  restoration semantics remain unvalidated. This is not proof that every clock
  lock mechanism is unavailable.
- NVML cooler helper`0xb6180` uses GET`0x2080852f`/844, and control helper
  `0xb6300` uses GET`0x20808530`/980. Setter`0xb6480` names
  SET`0x2080c531`/980. All are in the GSP table; the two GETs returned NV_OK
  with selected cooler0 on this device. No fan SET was performed in this batch.
  Per-cooler field decoding, limits, original mode and verified restoration must
  be established before enabling fan writes.
