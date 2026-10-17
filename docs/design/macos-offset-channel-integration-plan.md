# macOS core/memory frequency-offset channel: integration plan for the public core

Status: **plan + read-path design, 2026-10-17. No GPU request, no write.**
Scope: stage 4 ("core/memory frequency control: offset GET/SET") and its stage 5
hand-off to the public CLI/Python surface, for the macOS backend.

This document exists because the project's first acceptance criterion is
*user-visible behaviour parity*, and that parity is delivered through the public
`get-settings` / `set-*` vocabulary — not through the validation probe. The
probe has the channel; the public core does not. This is the inventory of what
"not wired yet" concretely means, so the hand-off is mechanical once the field
identity (G1) is closed.

## 1. What already exists (evidence, not assumption)

| layer | state | evidence |
| --- | --- | --- |
| private-control transport in the public core | **exists and is linked** | `core/build.rs:19-23` compiles `core/native/macos_rm.c` together with `probe-macos-rm/src/nvx_xnu.c` and `probe-macos-rm/src/nvx_rm.c`, i.e. the backend uses *the same* `nvx_rm_control()` the probe does |
| a live RM control call in that backend | **exists** | `core/native/macos_rm.c:133-152` (`read_live_controls`) issues `NV2080_CTRL_CMD_PERF_GET_CURRENT_PSTATE` and `NV2080_CTRL_CMD_FB_GET_INFO_V2` on every sample |
| arbitrary-buffer control | **supported** | `nvx_rm_control(ctl_fd, client, object, cmd, buf, size)` (`probe-macos-rm/src/nvx_rm.h:13`) takes a caller buffer and size; the probe sends the 2108 B `0x2080901b` block through it |
| clock/offset read in the public core | **absent** | `core/src/macos_rm.rs` contains **no** RM control call at all; the only occurrences of `0x20809019` / `0x20809025` / `0x2080901B` are inside prose in `notes` (`:183`) and the `CLOCK_OFFSET_UNREADABLE_REASON` string (`:110`) |
| telemetry the public core does carry | RUSD page + the two controls above | `core/native/macos_rm_abi.h` (`nvoc_rm_sample`, ABI v3, 14 RUSD sections) |

Consequence: wiring the offset channel is a **bounded extension of an existing
call path** — one more `nvx_rm_control` invocation plus ABI and Rust plumbing.
It is not new transport work, and it needs no kernel or driver change.

## 2. The three layers to extend, in dependency order

1. **native** `core/native/macos_rm.c` / `macos_rm_abi.h` (ABI v3 → v4): a
   `read_offset_channel()` next to `read_live_controls()`, plus the carried
   block in `nvoc_rm_sample` and the matching `_Static_assert`s.
2. **Rust backend** `core/src/macos_rm.rs`: `DeviceSettings.offset_*` fields,
   the capability/`unavailable` entry, the `offset_of!`/`size_of` layout
   assertions that mirror the C header.
3. **public surface** the `SettingsCapabilities` / `unavailable` vocabulary
   already used by `get-settings` and `pynvoc`; the CLI flag(s) follow the
   existing per-family pattern rather than a new one.

## 3. Read path (can be built and shipped before G1 is closed)

Three GETs, all `NV_OK` pre-wedge, all refused by the wedge as `0x62`:

| id | size | what it carries | how it is used |
| --- | --- | --- | --- |
| `0x20809019` | 12336 | 10-entry domain directory: `entry:domain_bit:type` = `0:1:0808 1:2:0909 2:16:0808 3:4:0909 4:8:0909 5:1048576:0909 6:524288:0303 7:64:0808 8:2097152:0909 9:32:0909` | maps the public domain names (graphics/memory/…) to the *entry index* the control block selects |
| `0x20809025` | 7188 | per-record offset range, floor −18750 / ceiling +18750 kHz (3 identical records, header mask `+4 = 7`) | the "limits" half of the user requirement; also the bound a SET must respect |
| `0x2080901b` | 2108 | header 14 written dwords (mask at `+4`) + 32 × 64 B entry records | the "current value" half: the 12 value dwords of each selected type-`08`/`09` record |

Read-path rules that must survive the hand-off:

* **status per id, not per family.** Every one of the three has its own RM
  status, and the public field must be `None` + a reason when its id is not
  `NV_OK` (during the current wedge that is `0x62` for all three). A missing
  reading must never be rendered as `0`.
* **entry index ≠ domain bit.** The control block's `+4` selects *entries*;
  the domain bit is a field of the record. Any code that treats the mask as a
  domain-bit mask is wrong (doc §14.1/§14.3).
* **the range block is not the offset.** `0x20809025` bounds; `0x2080901b`
  carries. Conflating them silently reports a limit as a setting.

## 4. The write path stays where it is

The write gate is unchanged and is not affected by this plan: the SET twin
`0x2080d01c@2108` is a privileged legacy id, unreachable from an unprivileged
client (`0x1b` before the GSP sees it, §4.4); `settings_write=false` and
`settings_restore=false` remain until a same-field read-back and a byte-exact
restore have been demonstrated **on the GPU**, which is exactly what
`probe-macos-rm --write --clk-offset-set BIT:OFF:DELTA` is the execution
vehicle for. The integration plan therefore adds **reads only**; it cannot
weaken the write gate, and it deliberately does not need to.

## 5. Acceptance criteria for the hand-off

The family may be reported as *implemented and measured* only when all of:

1. the three reads above return the same values through the **public** backend
   as through the probe, on the same boot;
2. the graphics and memory offsets are reported as `0` **only** because the
   value dwords were measured zero on a background that cannot miss a write
   (§15.3), not because the block was never inspected;
3. the reported range is the device's, not a constant;
4. `set` remains refused while `settings_write == false`, with the reason
   naming the missing piece (currently: the offset's slot identity, G1).

## 6. Open items this plan does not resolve

* **G1 (field identity)** — which of the twelve value dwords is the signed kHz
  offset. Unchanged; needs a non-zero value dword or the gated single-step SET
  with same-field read-back. Everything in §3 works without it.
* **Domain attribution of the range records** — `0x20809025`'s three records
  carry `+0 = 0x01030101 / 0x01040101 / 0x01050101`, i.e. a counter-like middle
  byte `03/04/05`, while `0x20809019`'s directory enumerates *domain bits*
  (`1,2,16,4,8,…`). Whether `03/04/05` are domain *indices* — and how they map
  onto the directory's bits — is **not yet established**; until it is, the
  range must be reported per-record without a domain name, or with the mapping
  marked unverified.
* **Whether the wedge's `0x62` should surface as `unavailable` or as a
  transient error** in the public vocabulary. Today the family is statically
  unavailable; the reason text distinguishes "not yet wired" from "driver
  refused".
* **Which domain each directory entry is (open, with a falsification path).**
  The stamped field is a bit position, not an index, and the ten the device uses
  are bits 0..6 and 19..21. The public OGKM headers contain no clock-domain
  enum at all, so no name can be cited from the driver tree; the only
  cross-reference is the NVAPI/Windows `NV_GPU_CLOCK_DOMAIN_ID` namespace (32
  wide, `nvapi-rs/sys/src/gpu/clock.rs`), which names 0..6
  GPC/XBAR/SYS/HUB/M/HOST/DISP and 19..21 LEG/PWR/MSD -- the same two clusters.
  Working hypothesis, deliberately *not* surfaced as a name: bit j is domain j,
  so directory index 0 (bit 0) is the core engine and index 2 (bit 4) the memory
  domain. Falsified by a single-domain offset read (or, once writes are
  permitted, a single-domain offset write) on a domain whose entry index is
  known. Until then the offsets stay in `unavailable` and the window stays
  domain-independent -- which costs nothing, because all range records agree.
* **Wedge-time live-control payloads may be zeroed (found 2026-10-17, while
  verifying §3 on the wedged machine).** `get-status --macos-rm` on the wedged
  card reports `pstate = 0` and `memory_used_bytes = 0` with those two fields
  *absent* from `unavailable`: the live controls `PERF_GET_CURRENT_PSTATE` and
  `FB_GET_INFO_V2` answer `NV_OK` while every RUSD field is correctly null with
  "RUSD allocation/mapping failed: 0x00000062". The pre-wedge recording of the
  same boot has the same control answering `P1` and a non-zero heap. A wedged
  GSP answering `NV_OK` with a zeroed structure would make `publish_live`'s
  "advancing" claim wrong -- i.e. exactly the fake-zero this project forbids.
  Not resolved here, because a healthy *idle* baseline has never been recorded:
  an idle card may legitimately read 0.
  *Sharpened 2026-10-17 with the header evidence (backend doc, "What the live
  `pstate` value means"): the declared value set makes `0` = `UNDEFINED` /
  `CLEAR_FORCED` and `P0` = `0x1`, the driver's own code never reads those
  constants, and the word is filled by the GSP over the RPC path -- so the
  healthy value can differ in base from the NVML index this project publishes
  elsewhere, while the `P{n}` rendering asserts the index base. If a healthy
  *idle* session reproduces the pre-wedge `1` for the same state, the zero is
  explained by the wedge and the base question is answered together with it; if
  it reproduces `0`, then either the base differs or the card really is
  undefined while idle, and the rendering must be revisited rather than the
  number. **Planned check:** immediately after the
  next restart, before any load, read `get-status --macos-rm` on the idle card;
  a non-zero heap or P-state there means the wedge was zeroing them. No code
  change is justified until that reading exists -- guessing here would be its own
  kind of fake.

## 7. Verification scope of this document

Static: `core/build.rs`, `core/native/*`, `core/src/macos_rm.rs`,
`nvoc-python/src/lib.rs`, and the recorded probe runs cited above. **No
compilation was performed on the client**, and no GPU request was sent. The
claims about the three GETs' payloads come from the recorded pre-wedge runs
(§9-§15 of `docs/reverse-engineering/nvapi/gsp-method-table-and-legacy-forwarding.md`),
not from a new measurement.
