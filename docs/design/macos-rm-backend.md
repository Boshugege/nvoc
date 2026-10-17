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
| `get-status` | P-state and RUSD temperature, clocks, power, enforced limit, max fan RPM and GPU utilization; missing fields are `null` / `N/A` with reasons |
| `list` and offline `get-vbios --input` | Existing operations without GPU discovery |
| Other live queries and all settings writes | Refused before opening the driver |

Status exit code 0 means the query completed; per-field availability is
reported independently. Missing temperature, clocks, power, fan and utilization
values are never represented by zero. A valid zero RPM or zero utilization
reading is preserved. JSON clocks are kHz, power/limits are mW, temperatures
are Celsius, utilization is percent, and fan RPM is the maximum of the shared
page's cooler slots. Human output uses MHz, W, Celsius and RPM. Optional
information-query failures preserve their RM status in `unavailable`;
connection, version or object-chain failures return
an error. Capabilities describe implemented backend surfaces, not a guarantee
that every query succeeds on every board.

The full command-by-command status (101 CLI commands, the non-CLI consumers and
the stage 0-12 roadmap) lives in
[the macOS backend feature matrix](macos-backend-feature-matrix.md).

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
  nvoc/macos-rm-sample/2`): `sequence`, `host_time` (RFC 3339) and
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

## Validation

```sh
cargo test -p nvoc-core -p nvoc-cli --all-targets
cargo clippy -p nvoc-core -p nvoc-cli --all-targets --no-deps -- -D warnings
cargo test -p nvoc-core --test macos_rm_readonly -- --ignored --nocapture
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
```

`rusd_read.c` covers the RUSD validity/freshness/stable-copy helpers;
`macos_rm_abi.c` is a compile-time contract test for the shared header (it is
checked with `-fsyntax-only`, since it only needs to type-check).
Performance-write tests remain ignored. Raw test output belongs in workspace
`outputs/`, outside the repository.
