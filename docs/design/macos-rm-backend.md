# Native macOS RM backend

The Rust core and `nvoc-cli` expose a read-only NullMoth RM backend. Verified
hardware: GTX 1650 SUPER (`10de:2187`), macOS 15.8.1 / 24H32, NullMoth 2.0.4,
RM 610.57.04, ordinary user without sudo. This does not establish support for
every GPU claimed by NullMoth or other macOS releases.

## Build and run

Build on the x86_64 test Mac with Command Line Tools, the repository-pinned
Rust 1.95.0 toolchain, the pinned `nvapi-rs` submodule, and OGKM 610.57.04 headers:

```sh
export OGKM=/path/to/open-gpu-kernel-modules-610.57.04
cargo build -p nvoc-cli --locked
./target/debug/nvoc-cli get-gpu-list
./target/debug/nvoc-cli get-info --gpu 0 -O json
./target/debug/nvoc-cli get-status --macos-rm -O json
./target/debug/nvoc-cli get-settings --macos-rm -O json
```

The read-only probe carries an explicit opt-in for the campaign-sourced Tier-B
GETs (voltage rails + V/F points) — they stay out of the default battery so an
ordinary run never sends IDs that have no SDK header:

```sh
(cd probe-macos-rm && sh build.sh && ./probe-macos-rm-idle --stages 01 --tierb-read)
```

macOS selects RM automatically. `--macos-rm` selects it explicitly and conflicts
with NVAPI/NVML flags and library-path overrides. An explicit request on a
different architecture or operating system returns an unsupported-platform
error. Windows/Linux auto-selection remains unchanged. The NVAPI Rust crate is
still a compile-time dependency of the existing core, but the RM path never
initializes its runtime, nor NVML. No external probe executable or OGKM headers
are needed at runtime.

## Current command coverage

| Command | Behavior |
| --- | --- |
| `get-gpu-list` | GPU name, PCI address, NVOC ID, RM GPU ID, backend flags |
| `get-info` | RM version, PCI/VBIOS metadata, VRAM/BAR1/L2 sizes, class count |
| `get-status` | P-state and the full RUSD telemetry vocabulary below (clock domains, temperatures, power/TGP/limits, throttle reasons, fan RPM, GPU/memory/engine utilization, PCIe link generation) plus video-memory usage; missing fields are `null` / `N/A` with reasons |
| `list` and offline `get-vbios --input` | Existing operations without GPU discovery |
| Other live queries and all settings writes | Refused before opening the driver |

Status exit code 0 means the query completed; per-field availability is
reported independently. Missing temperature, clocks, power, fan and utilization
values are never represented by zero, and a value whose RUSD section has not
published anything is withheld instead of being shown as its default. A valid
zero RPM or zero utilization reading is preserved. JSON clocks are kHz, target
clocks are MHz, power/limits are mW, temperatures are Celsius, utilization is
percent, memory usage is bytes, and fan RPM is the maximum of the cooler slots
the driver reports as live. Human output uses MHz, W, Celsius, rpm, MiB and
percent. Optional information-query failures preserve their RM status in
`unavailable`; connection, version or object-chain failures return an error.
Capabilities describe implemented backend surfaces, not a guarantee that every
query succeeds on every board.

The full command-by-command status (101 CLI commands, the non-CLI consumers and
the stage 0-12 roadmap) lives in
[the macOS backend feature matrix](macos-backend-feature-matrix.md).

## Telemetry vocabulary (ABI v3)

`nvoc_rm_sample` (see `core/native/macos_rm_abi.h`) carries one status +
timestamp per polled RUSD section plus the value blocks below. The native
layer transports the driver's own units and leaves naming, plausibility and
decoding to Rust, so the one-shot query (`interpret_oneshot`) and the
continuous monitor (`interpret_sample`) share exactly one interpretation rule
set (`publish_all`).

| Section (`cl00de.h`) | Fields | Notes |
| --- | --- | --- |
| `clkPublicDomainInfos` | `{graphics,memory,video,sm}_{clock_khz,target_clock_mhz}` | SM is 0/0 on Turing: the driver does not implement that domain, so it is reported `unavailable` ("driver reports no value for this field") rather than as a 0 MHz clock |
| `temperatures[]` | `temperature_c`, `temperature_memory_c`, `temperature_board_c`, `temperature_power_supply_c` | one RUSD section per sensor and therefore per-field freshness; boards without a memory/board/MB sensor report `RUSD_TIMESTAMP_INVALID` and stay unavailable |
| `instPowerUsage` | `power_mw` | instantaneous die power |
| `powerLimitGpu` | `power_limit_mw`, `power_limit_requested_mw` | enforced vs requested wall |
| `avgPowerUsage` | `average_power_mw` | driver average window; unpublished on this board |
| `powerPolicyStatus` | `power_policy_tgp_mw` | total GPU power from the power-policy block (nvidia-smi's "power policy") |
| `clkThrottleInfo` | `throttle_reason_mask`, `throttle_reasons[]` | bits 0-8 decoded (`gpu_idle`, `application_clock_setting`, `sw_power_cap`, `hw_slowdown`, `sync_boost`, `sw_thermal_slowdown`, `hw_thermal_slowdown`, `hw_power_brakes_slowdown`, `display_clock_setting`); the raw mask stays in JSON |
| `fanCoolerStatus` | `fan_rpm`, `fan_rpms[]` | the driver does not report how many coolers exist, so 0 slots are skipped and implausible slots (>20 000 rpm, seen as unpopulated garbage) are dropped; if every non-zero slot is implausible the section is reported as an implausible value |
| `perfDevUtil` | `utilization_percent`, `memory_utilization_percent` | GPU / frame-buffer busy percent |
| `procUtil` | `engine_utilization_percent` | one entry per engine (`gr`, `fb`, `nvenc`, `nvdec`, `nvjpg`, `nvofa`), converted from the driver's percent x100 |
| `pciBusData` | `pcie_link_gen`, `pcie_max_link_gen` | decoded from `RUSD_BUS_DATA_PCIE_GEN_INFO`; a zero word means the driver does not report a link |
| live RM control | `pstate` | `NV2080_CTRL_CMD_PERF_GET_CURRENT_PSTATE`, re-read every sample |
| live RM control | `memory_used_bytes` | `NV2080_CTRL_CMD_FB_GET_INFO_V2` heap size − heap free (the used/free pair nvml reports) |

`memEcc` and `memRowRemap` are deliberately **not** carried: even with
`NV00DE_RUSD_POLL_MEMORY` requested the 610.57.04 driver leaves both at
`RUSD_TIMESTAMP_INVALID` on this board, so they would be permanent `null`
fields. `NV00DE_RUSD_POLL_MEMORY` is therefore not requested either.

Plausibility bounds are part of the contract, not a cosmetic filter: a fan slot
that reads 294 118 rpm, a temperature outside −50…150 C, a clock above 20 GHz
or a utilization above 100 % is reported as `unavailable` with an
"implausible value" reason instead of being published as a measurement.

The current transport selects the first matching NVAccel/NVRM service. It does
not enumerate all physical GPUs in a multi-NVIDIA system. `--gpu` still applies
the existing NVOC index / bus-derived ID selection to the returned inventory.
RM GPU IDs are recorded separately and are not treated as PCI bus numbers.

## Architecture

```text
nvoc-cli → TargetInventory::discover(BackendSet::MacosRm)
         → GpuTarget::macos_rm()
         → run(QueryMacosRmInfo / QueryMacosRmStatus)
         → core::macos_rm → native/macos_rm.c
         → NVOC nvx_xnu / nvx_rm → IOKit → NullMoth RM
```

The C bridge compiles against the official OGKM headers and exports a small
versioned snapshot ABI. Its layout is size-checked in C and Rust. It reuses
NVOC's own transport, not upstream driver source. Each snapshot opens a session,
checks RM 610.57.04, creates the RM object chain, issues GET controls, and frees
objects and connections on success or failure. A Rust mutex serializes the
transport's process-global fd table. Status refresh checks device identity
against the discovery snapshot. Discovery/information queries do not allocate
a shared page. Status queries allocate RUSD with CLOCK/PERF/POWER/THERMAL/FAN
polling, map its system memory read-only through the **control fd**, and use
acquire fences around sequence-checked reads. Polled fields must advance from
the first timestamp observed in the current session, within a 1.5 s wait budget;
non-polled fields require a valid even sequence. Unstable, unavailable or
non-advancing fields return null with a reason. RUSD timestamps/sequences are
available in JSON as `rusd_timestamps`; they are diagnostic, not a single
cross-field snapshot time. The mapping is explicitly removed before freeing
the shared object and RM chain. An exceptional unmap failure drops telemetry
and defers resource reclamation to connection teardown. No polling-interval
override or performance SET controls are sent.

Rust callers can use the common operation reporting API:

```rust,no_run
use nvoc_core::{BackendSet, QueryMacosRmInfo, QueryMacosRmStatus, discover_targets, run};

let inventory = discover_targets(BackendSet::MacosRm)?;
for target in inventory.targets() {
    let info = run(&target, QueryMacosRmInfo)?.output;
    let status = run(&target, QueryMacosRmStatus)?.output;
    println!("{:?}: {:?}", info.name, status.pstate);
}
# Ok::<(), nvoc_core::Error>(())
```

The existing NVAPI-shaped `QueryGpuInfo` / `QueryGpuStatus` output types are
unchanged. RM callers use the explicitly typed operations above. Python, GUI,
TUI, the optimizer and service are not yet integrated with this backend.

## Continuous monitoring

`get-status` can stream samples instead of a single reading:

```sh
nvoc-cli get-status --macos-rm --interval 1 --count 10 -O json  # 10 JSONL lines
nvoc-cli get-status --macos-rm --interval 0.5                   # until Ctrl-C
```

- `--interval SECONDS` (fractional) switches `get-status` into continuous mode;
  `--count N` stops after N samples (0 or omitted = run until Ctrl-C). Both
  require the macOS RM backend and are refused for NVAPI/NVML.
- Accepted intervals are 1 ms to 24 h. Below that the run would busy-loop the
  driver instead of sampling (and `Duration::from_secs_f64` rounds to zero);
  above it the value is almost certainly a typo. Out-of-range input is refused
  before any driver access, and never by panicking - a release build uses
  `panic = "abort"`, so a panic would abort the process without unwinding the
  session.
- Cadence: one sample, then `interval` of rest, so the observed period is the
  interval plus the sampling cost (measured: `--interval 1` gives 1.000-1.011 s
  gaps, `--interval 2` gives 2.001-2.011 s, with 0.1-1.0 ms of sampling).
- JSON output becomes one JSONL object per sample (`schema
  nvoc/macos-rm-sample/3`): `sequence`, `host_time` (RFC 3339) and
  `host_time_unix_ns`, `sample_duration_us`, the telemetry fields in their raw
  units, `rusd_timestamps`, `updated`, `freshness`, `epoch_reset` and
  `unavailable`. Human output keeps the engineering-unit block per sample,
  lists the fields that did not refresh and reports any stamp reset.
- Ctrl-C stops after the current sample (including during a reconnect backoff),
  prints a summary line on stderr and exits 0, unwinding the session (mapping,
  RM objects, file descriptors).
- A closed consumer (`nvoc-cli get-status --interval 1 | head -1`) also ends the
  run cleanly with exit 0: the stream is written with `writeln!`/`flush` errors
  handled instead of the `println!` panic, which would have aborted on
  `BrokenPipe`. Non-pipe write failures are reported and exit non-zero.
- Raw logs stay outside the repository (`<workspace>/outputs/`).

### Session semantics

Continuous mode calls the `nvoc_macos_rm_session_*` ABI: the connection chain
and the read-only RUSD mapping are created once and reused for every sample
instead of being rebuilt per query. The one-shot snapshot ABI is unchanged and
still opens/closes per call.

The struct layouts and the entry-point signatures live in one shared header,
`core/native/macos_rm_abi.h`. `macos_rm.c` includes it, so a definition that
drifts from the declared prototype stops compiling, and the header's
`_Static_assert`s pin the same sizes/offsets the Rust side asserts with
`size_of!`/`offset_of!` (`core/tests/macos_rm_abi.c` re-checks the signatures
and the status codes; the ignored hardware test
`native_and_rust_abi_layouts_agree` compares both languages' runtime
fingerprints through `nvoc_macos_rm_abi_layout`). Every entry point treats
`error`/`cap` as optional, requires an exact `size` argument, and only touches
outputs on success.

`nvoc_macos_rm_session_open` also hands back each polled section's
**mapping-time watermark**. The monitor must see a later value before it may
treat a reading as new; without that baseline a session opened on an already
old page would report the stale value as its "first fresh sample".

Freshness rules (per field, `freshness[field]`):

- `advancing` - the section was copied coherently and its stamp/sequence
  changed on this sample: the value is new and `updated[field]` is `true`.
- `unchanged` - not refreshed on this sample but still inside the staleness
  window. The last coherent value is repeated with `updated[field] = false`; it
  is never replaced by a zero, and valid `0 RPM` / `0 %` readings are kept.
- `never_refreshed` - the section has not advanced once since the session
  opened, so its value is withheld (`null`) with a "no RUSD update since this
  session opened (N ms)" reason. The open path waits up to 1.5 s for every
  section and reports the ones still waiting on stderr, so a slow driver is
  visible rather than silently presented as fresh.
- `stale` - it did advance earlier in this session but has been silent past
  `MONITOR_STALE_AFTER` (5 s): the value is withheld with a "no RUSD update for
  N ms" reason. The window is host monotonic time between our samples, not the
  absolute RUSD stamp (the driver stamp is in the host real-time domain but
  lags wall clock by ~1.9 s and drifts).
- `unsupported` - the section carries no valid timestamp
  (`RUSD_TIMESTAMP_INVALID`): the driver does not publish it right now, so
  retrying will not help until it does.
- `unstable` - the section changed while it was copied (sequence/timestamp
  raced): no coherent value this round, but retrying is reasonable.
- A stamp/sequence that moves **backwards** is new data from a restarted
  timeline (driver clock reset, page re-initialisation); the fields are listed
  in `epoch_reset` and no absolute age is derived from the stamp.
- P-State is re-read from RM on every sample because it is a live control, not
  a RUSD section, so it is `advancing` whenever the query succeeds.
- A single quiet field is normal and never triggers a reconnect. When **no**
  section advances for six consecutive samples the session is considered dead
  and `sample()` returns an error so the caller reconnects.

Reconnect rules: the old session is explicitly closed (and any release failure
is reported) *before* a new one is opened, because the transport and the
driver's poller are process-global. Reopening uses a bounded 0/0.5/1/2 s
backoff, at most 8 consecutive reconnects, an interruptible wait, and a strict
identity check (PCI address, device id, RM GPU id). `MacosRmSession::close()`
returns any native release error and `Drop` is only the fallback for callers
that forget it.

P-State, clocks, temperature, power and utilization are not one atomic
snapshot: sections update on the driver's own ~500 ms schedule and
`rusd_timestamps` carries each field's own stamp.

## Settings surface (read-only)

`get-settings` on macOS reports the configuration the RM session can actually
see: P-state, the enforced and requested power limit, the PCIe generation, and
the board/SKU/memory identity, all taken from the same snapshot as `get-status`
so the readback and the telemetry cannot disagree. Alongside them the payload
carries `capabilities` (`settings_read: true`, `settings_write: false`,
`settings_restore: false`), an `unavailable` map naming every configuration
family the backend cannot read (`core_clock_offset`, `memory_clock_offset`,
`voltage_boost`, `thermal_limit`, `fan_control`, `pstate_lock`, `vf_curve`,
`power_limit_write`, `settings_restore`) and `notes` with the evidence.

Nothing is fabricated: a family that cannot be read is absent from the payload
and named in `unavailable`, never emitted as a default-looking zero. Writes are
refused before the driver is opened, and `settings_restore` stays false until a
write path exists to restore from — the snapshot/readback/restore sequence has
no meaning without one.

Why the configuration families are unreadable on 610.57.04 through this
transport (probed read-only, `probe-macos-rm --tierb-read`; sizes and payload
shape corrected in `a87876a`, evidence `stage3-paramsizes-20261017T044129Z`):

An earlier pass recorded these GETs as "the driver refuses this command". That
was **our** bug, not the driver's: `0x1f` is `NV_ERR_INVALID_ARGUMENT`, the
"your parameters are the wrong shape" code, not a capability answer. The
control confirms it — a real control (`BUS_GET_PCI_INFO` 0x20801801) also
answers `0x1f` when handed the wrong size, while a method identifier that does
not exist answers `NV_ERR_OBJECT_NOT_FOUND` 0x1b.

| Control | Result (corrected parameter size) |
| --- | --- |
| `NV2080_CTRL_CMD_PERF_GET_LEVEL_INFO_V2` 0x2080200b | capacity 0 → `NV_OK` with **0 records**; capacity ≥1 → `0x1f`. The driver publishes **no clock-domain table** either way |
| `rails-get-info` 0x2080B201 | accepted only at **2444 B** → `NV_OK`; payload has just **16** non-zero words |
| `rails-get-status` 0x2080B202 | accepted only at **3232 B** → `NV_OK`; payload **0** non-zero words and differs between two calls (not stable) |
| `vfpoints-get` 0x208090A2 | accepted only at **524 B** → `NV_OK`; payload has **1** non-zero word — not a curve |
| smaller buffers for any of the three | `NV_ERR_INVALID_ARGUMENT` 0x1f (a single record's stride is below the accepted size) |
| `BUS_GET_PCI_INFO` 0x20801801 with the wrong size | `NV_ERR_INVALID_ARGUMENT` 0x1f — proves `0x1f` = wrong parameters |
| a method identifier that does not exist | `NV_ERR_OBJECT_NOT_FOUND` 0x1b |
| `edpp-limit-info`, `pmumon-clk-samples` | `NV_ERR_NOT_SUPPORTED` 0x56 |

**Acceptance is not data.** Handing the three GETs the size the driver accepts
does make them return `NV_OK`, but the payloads do not look like the readings
the campaign layouts predict (16/0/1 non-zero words) and `rails-get-status` is
not even stable across two calls, so the layouts remain undecoded and the
values must not be used. Separately, the public SDK carries no write path for
these families at all: the full `NV2080_CTRL_CMD_PERF_*` set in 610.57.04's
`ctrl2080perf.h` contains no clock/voltage offset or VF-edit command. Until the
Tier-B layouts are decoded (or another read channel is confirmed), and a
non-public offset/VF interface is found, clock offsets, locks, the V/F curve,
fan policy and thermal policy stay "cannot read", and therefore cannot be
ranged, snapshotted or restored.

## Consumers (pynvoc)

The native backend is reachable from the shared Rust core *and* from the Python
bindings the GUI/TUI drive, so macOS telemetry is not CLI-only:

```python
import pynvoc

gpus = pynvoc.discover_gpus("macos-rm")     # backend_macos_rm: true
gpu = gpus[0]["gpu_id_hex"]
info = pynvoc.query_info(gpu, "macos-rm")   # identity, VRAM, driver version
status = pynvoc.query_status(gpu, "macos-rm")
```

`query_info`/`query_status` dispatch to the RM session whenever the selected
target has a macOS RM backend; NVAPI/NVML are never initialized on that path.
Both normalizers emit the same field names and units as the NVAPI/NVML paths
(`gpu_clock_mhz`, `mem_clock_mhz`, `temperature_c`, `temp_core`, `power_w`,
`power_limit_w`, `eff_gpu_clock_mhz`, `fan_rpm`, `pstate` as `"P<n>"`), so an
existing dashboard renders macOS without a backend branch; the RM-specific
extras (per-fan `fan_rpms`, `engine_utilization_percent`, `pcie_link_gen`,
`memory_used_mib`, `throttle_reasons`, typed temperatures, the requested/policy
power values and `unavailable`) ride along under their own keys. Unit
conversion (kHz→MHz, mW→W, bytes→MiB) happens once, in the normalizer. Fields
the driver does not publish stay absent — never fabricated as zero — and
`unavailable` carries the reason.

## Consumers (srv, read-only)

The control service reaches the same channel through a read-only
`ControlBackend` (`srv/src/macos.rs::MacosRmBackend`), selected at discovery by
`cfg(target_os = "macos")`. It maps the RM status onto the same
`SensorBundle`/`MonitorSample` types the NVAPI backend fills in, so
`/status`, `/api/status`, `/api/gpus`, `/api/info`, `/api/history` and
`/config` behave like any other platform, and it reports
`supports_writes() == false` — which pins the loop to `Auto` (observe only),
skips every hand-back call, and makes `/mode?value=pid|manual`, `/fan`,
`/api/oc/*`, `/api/reset` and `/oc_global` fail before the driver is touched
(`409`, or `ERR: read-only macOS backend: no writable RM control is reachable
on this driver`). There is no macOS fan PID loop, manual duty, or offset/limit
write until a writable interface exists. Evidence: `stage11-srv-macos-*` +
`stage11-srv-smoke-*` in the delivery `outputs/`, and
`review-stage11-srv-macos-backend-20261017.md`.

## Validation

```sh
cargo test -p nvoc-core -p nvoc-cli --all-targets
cargo clippy -p nvoc-core -p nvoc-cli --all-targets --no-deps -- -D warnings
cargo test -p nvoc-core --test macos_rm_readonly -- --ignored --nocapture

# Python bindings: portable unit tests, then the real extension module.
cargo test -p pynvoc --no-default-features
RUSTFLAGS="-C link-arg=-undefined -C link-arg=dynamic_lookup" \
  cargo build -p pynvoc --release   # maturin does this automatically
```

The read-only hardware tests are serialized (a process-wide mutex): the NVOC
transport and the driver's RUSD poller are process-global, and two concurrent
read-only clients can starve each other's 1.5 s freshness budget. They cover
identity, VRAM, refreshed P-state, missing-data semantics, selection, repeated
timestamp advancement, the C/Rust ABI fingerprint, explicit
close/reopen cycles and fd accumulation.

The last command requires the initialized GPU and only performs GET queries
and RM resource allocation/cleanup. It verifies identity, VRAM, refreshed
P-state, missing-data semantics, selection, repeated timestamp advancement, no fd
accumulation and absence of NVAPI initialization. CPU protocol checks can also
be run on the build host:

```sh
cc -std=c11 -Wall -Wextra -Werror -Icore/native \
  -I"$OGKM/src/common/sdk/nvidia/inc" core/tests/rusd_read.c -o /tmp/nvoc-rusd-read
/tmp/nvoc-rusd-read
cc -std=c11 -Wall -Wextra -Werror -Icore/native -fsyntax-only core/tests/macos_rm_abi.c
# The backend itself must type-check against the driver's own headers.
cc -std=c11 -Wall -Wextra -Werror -Wno-unused-parameter -fsyntax-only \
  -I"$OGKM/src/common/sdk/nvidia/inc" \
  -I"$OGKM/src/nvidia/arch/nvalloc/unix/include" \
  -Iprobe-macos-rm/src -Icore/native core/native/macos_rm.c
```

`rusd_read.c` covers the RUSD validity/freshness/stable-copy helpers;
`macos_rm_abi.c` is a compile-time contract test for the shared header (it is
checked with `-fsyntax-only`, since it only needs to type-check); the last
command compiles `macos_rm.c` against the extracted 610.57.04 SDK headers, so a
`cl00de.h` field rename or a struct-shape change fails the check list even on a
host without a GPU.
Performance-write tests remain ignored. Raw test output belongs in workspace
`outputs/`, outside the repository.
