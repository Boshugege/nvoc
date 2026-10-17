//! Native, read-only NullMoth RM backend for macOS 15+ on x86_64.
//!
//! Uses the same IOKit transport as the validation probe, linked into this
//! library. No external probe executable, NVAPI, or NVML runtime is required.
//! The initial implementation discovers the first service exposed by NullMoth.
use crate::{Error, GpuId, PciAddress};
use serde::Serialize;
use std::collections::BTreeMap;
use std::time::{Duration, Instant};

#[derive(Debug, Clone, Serialize)]
pub struct DeviceInfo {
    pub name: Option<String>,
    pub driver_version: String,
    pub rm_gpu_id: u32,
    pub pci_address: String,
    pub vendor_id: u32,
    pub device_id: u32,
    pub subsystem_id: Option<u32>,
    pub revision: Option<u32>,
    pub board_id: Option<u32>,
    pub chip_sku: Option<String>,
    pub project: Option<String>,
    pub memory_total_bytes: Option<u64>,
    pub memory_usable_bytes: Option<u64>,
    pub bar1_bytes: Option<u64>,
    pub l2_cache_bytes: Option<u64>,
    pub supported_class_count: Option<u32>,
    pub unavailable: BTreeMap<String, String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct DeviceStatus {
    pub pstate: Option<u32>,
    pub graphics_clock_khz: Option<u32>,
    pub memory_clock_khz: Option<u32>,
    pub temperature_c: Option<f64>,
    pub power_mw: Option<u32>,
    pub power_limit_mw: Option<u32>,
    pub fan_rpm: Option<u32>,
    pub utilization_percent: Option<u32>,
    /// RUSD per-section update timestamp, or even sequence for non-polled data.
    pub rusd_timestamps: BTreeMap<String, u64>,
    pub unavailable: BTreeMap<String, String>,
}

#[derive(Debug, Clone)]
pub struct Device {
    id: GpuId,
    info: DeviceInfo,
}

impl Device {
    pub fn id(&self) -> GpuId {
        self.id
    }

    pub fn info(&self) -> &DeviceInfo {
        &self.info
    }

    /// Refreshes RM state and closes all native handles before returning.
    pub fn status(&self) -> Result<DeviceStatus, Error> {
        let snapshot = read_snapshot(true)?;
        let current = device_from_snapshot(&snapshot);
        if current.info.pci_address != self.info.pci_address
            || current.info.device_id != self.info.device_id
            || current.info.rm_gpu_id != self.info.rm_gpu_id
        {
            return Err(Error::Custom(
                "macOS RM device changed; rediscover GPUs".into(),
            ));
        }
        Ok(status_from_snapshot(&snapshot))
    }
}

pub fn discover() -> Result<Vec<Device>, Error> {
    read_snapshot(false).map(|snapshot| vec![device_from_snapshot(&snapshot)])
}

/// Typed macOS info query for the common `run` / `run_many` operation API.
#[derive(Debug, Clone, Copy)]
pub struct QueryMacosRmInfo;

impl crate::GpuOperation for QueryMacosRmInfo {
    type Output = DeviceInfo;

    fn kind(&self) -> crate::OperationKind {
        crate::OperationKind::QueryGpuInfo
    }

    fn run(&self, target: &crate::GpuTarget<'_>) -> Result<Self::Output, Error> {
        Ok(target.macos_rm()?.info().clone())
    }
}

/// Refreshes the macOS status without falling back to another GPU backend.
#[derive(Debug, Clone, Copy)]
pub struct QueryMacosRmStatus;

impl crate::GpuOperation for QueryMacosRmStatus {
    type Output = DeviceStatus;

    fn kind(&self) -> crate::OperationKind {
        crate::OperationKind::QueryGpuStatus
    }

    fn run(&self, target: &crate::GpuTarget<'_>) -> Result<Self::Output, Error> {
        target.macos_rm()?.status()
    }
}

/// Version of the private C ABI in core/native/macos_rm_abi.h. Bump both
/// sides together: the C header pins the same number and the hardware test
/// compares the two fingerprints.
const ABI_VERSION: u32 = 2;

/// Slots returned by [`abi_layout`] and by the native
/// nvoc_macos_rm_abi_layout(): sizes, field offsets, the ABI version and the
/// per-field status codes Rust matches on. Keeping them identical is what the
/// C header's `_Static_assert`s and the hardware test enforce.
pub const ABI_LAYOUT_SLOTS: usize = 21;

/// Private stable C ABI, not an NVIDIA RM structure. NVIDIA layouts live only
// in native/macos_rm.c, compiled against the driver's matching OGKM headers.
#[repr(C)]
struct Snapshot {
    abi_version: u32,
    rm_gpu_id: u32,
    domain: u32,
    bus: u32,
    slot: u32,
    function: u32,
    vendor_id: u32,
    device_id: u32,
    subsystem_id: u32,
    revision: u32,
    board_id: u32,
    class_count: u32,
    total_bytes: u64,
    usable_bytes: u64,
    bar1_bytes: u64,
    l2_bytes: u64,
    pstate: u32,
    memory_status: u32,
    pstate_status: u32,
    pci_status: u32,
    bios_status: u32,
    name_status: u32,
    class_status: u32,
    driver: [u8; 64],
    name: [u8; 64],
    sku: [u8; 16],
    project: [u8; 16],
    telemetry: Telemetry,
}

#[repr(C)]
#[derive(Default)]
struct Telemetry {
    status: u32,
    valid: u32,
    graphics_khz: u32,
    memory_khz: u32,
    temperature_fx256: i32,
    power_mw: u32,
    limit_mw: u32,
    fan_rpm: u32,
    utilization_percent: u32,
    reserved: u32,
    timestamps: [u64; 6],
}

#[cfg(all(target_os = "macos", target_arch = "x86_64"))]
const _: () = assert!(std::mem::size_of::<Snapshot>() == 360);

fn text(bytes: &[u8]) -> String {
    let end = bytes
        .iter()
        .position(|&byte| byte == 0)
        .unwrap_or(bytes.len());
    String::from_utf8_lossy(&bytes[..end]).into_owned()
}

fn query_error(status: u32) -> String {
    format!("RM query failed: 0x{status:08x}")
}

fn device_from_snapshot(s: &Snapshot) -> Device {
    let pci = PciAddress {
        domain: s.domain,
        bus: s.bus,
        device: s.slot,
        function: s.function,
    };
    let unavailable = [
        ("name", s.name_status),
        ("memory", s.memory_status),
        ("pci_details", s.pci_status),
        ("bios", s.bios_status),
        ("classes", s.class_status),
    ]
    .into_iter()
    .filter(|(_, status)| *status != 0)
    .map(|(field, status)| (field.into(), query_error(status)))
    .collect();
    Device {
        id: GpuId::from_pci_address(pci),
        info: DeviceInfo {
            name: (s.name_status == 0)
                .then(|| text(&s.name))
                .filter(|name| !name.is_empty()),
            driver_version: text(&s.driver),
            rm_gpu_id: s.rm_gpu_id,
            pci_address: pci.to_string(),
            vendor_id: s.vendor_id,
            device_id: s.device_id,
            subsystem_id: (s.pci_status == 0).then_some(s.subsystem_id),
            revision: (s.pci_status == 0).then_some(s.revision),
            board_id: (s.bios_status == 0).then_some(s.board_id),
            chip_sku: (s.bios_status == 0).then(|| text(&s.sku)),
            project: (s.bios_status == 0).then(|| text(&s.project)),
            memory_total_bytes: (s.memory_status == 0).then_some(s.total_bytes),
            memory_usable_bytes: (s.memory_status == 0).then_some(s.usable_bytes),
            bar1_bytes: (s.memory_status == 0).then_some(s.bar1_bytes),
            l2_cache_bytes: (s.memory_status == 0).then_some(s.l2_bytes),
            supported_class_count: (s.class_status == 0).then_some(s.class_count),
            unavailable,
        },
    }
}

fn status_from_snapshot(s: &Snapshot) -> DeviceStatus {
    let t = &s.telemetry;
    let mut unavailable = BTreeMap::new();
    let mut rusd_timestamps = BTreeMap::new();
    let mut valid = |field: &str, section: usize, sane: bool| {
        let reason = if t.status != 0 {
            Some(format!(
                "RUSD allocation/mapping failed: 0x{:08x}",
                t.status
            ))
        } else if t.valid & (1 << section) == 0 {
            Some("RUSD field unavailable, unstable or did not update within 1.5 s".into())
        } else if !sane {
            Some("RUSD field reported an invalid value".into())
        } else {
            None
        };
        if let Some(reason) = reason {
            unavailable.insert(field.into(), reason);
            false
        } else {
            rusd_timestamps.insert(field.into(), t.timestamps[section]);
            true
        }
    };
    let graphics_clock_khz =
        valid("graphics_clock_khz", 0, t.graphics_khz != u32::MAX).then_some(t.graphics_khz);
    let memory_clock_khz =
        valid("memory_clock_khz", 0, t.memory_khz != u32::MAX).then_some(t.memory_khz);
    let temperature_c = valid(
        "temperature_c",
        1,
        (-50 * 256..=150 * 256).contains(&t.temperature_fx256),
    )
    .then_some(f64::from(t.temperature_fx256) / 256.0);
    let power_mw = valid("power_mw", 2, t.power_mw != u32::MAX).then_some(t.power_mw);
    let power_limit_mw = valid("power_limit_mw", 3, t.limit_mw != u32::MAX).then_some(t.limit_mw);
    let fan_rpm = valid("fan_rpm", 4, t.fan_rpm != u32::MAX).then_some(t.fan_rpm);
    let utilization_percent = valid("utilization_percent", 5, t.utilization_percent <= 100)
        .then_some(t.utilization_percent);
    if s.pstate_status != 0 {
        unavailable.insert("pstate".into(), query_error(s.pstate_status));
    }
    DeviceStatus {
        pstate: (s.pstate_status == 0).then_some(s.pstate),
        graphics_clock_khz,
        memory_clock_khz,
        temperature_c,
        power_mw,
        power_limit_mw,
        fan_rpm,
        utilization_percent,
        rusd_timestamps,
        unavailable,
    }
}

/// nvx_xnu owns a process-global fd table, so every native call (one-shot and
/// session) must be serialized through this single lock.
#[cfg(all(target_os = "macos", target_arch = "x86_64"))]
fn transport() -> &'static std::sync::Mutex<()> {
    static TRANSPORT: std::sync::Mutex<()> = std::sync::Mutex::new(());
    &TRANSPORT
}

#[cfg(all(target_os = "macos", target_arch = "x86_64"))]
fn lock_transport() -> Result<std::sync::MutexGuard<'static, ()>, Error> {
    transport()
        .lock()
        .map_err(|_| Error::Custom("macOS RM transport lock poisoned".into()))
}

#[cfg(all(target_os = "macos", target_arch = "x86_64"))]
unsafe extern "C" {
    fn nvoc_macos_rm_snapshot(
        out: *mut Snapshot,
        size: usize,
        telemetry: u32,
        error: *mut u8,
        cap: usize,
    ) -> u32;
    fn nvoc_macos_rm_session_open(
        out: *mut *mut SessionOpaque,
        info: *mut Snapshot,
        info_size: usize,
        baseline: *mut u64,
        baseline_count: usize,
        error: *mut u8,
        cap: usize,
    ) -> u32;
    fn nvoc_macos_rm_session_sample(
        handle: *mut SessionOpaque,
        out: *mut RawSample,
        size: usize,
        error: *mut u8,
        cap: usize,
    ) -> u32;
    fn nvoc_macos_rm_session_close(handle: *mut SessionOpaque, error: *mut u8, cap: usize) -> u32;
    fn nvoc_macos_rm_abi_layout(out: *mut u64, count: usize) -> usize;
}

#[cfg(all(target_os = "macos", target_arch = "x86_64"))]
fn read_snapshot(telemetry: bool) -> Result<Snapshot, Error> {
    let _guard = lock_transport()?;
    let mut snapshot = std::mem::MaybeUninit::<Snapshot>::uninit();
    let mut error = [0u8; 256];
    // SAFETY: C receives correctly-sized writable buffers, initializes the
    // entire POD snapshot on success, and does not retain either pointer.
    // The mutex serializes every access to nvx_xnu's global connection table.
    let status = unsafe {
        nvoc_macos_rm_snapshot(
            snapshot.as_mut_ptr(),
            std::mem::size_of::<Snapshot>(),
            u32::from(telemetry),
            error.as_mut_ptr(),
            error.len(),
        )
    };
    if status != 0 {
        return Err(Error::Custom(format!("macOS RM: {}", text(&error))));
    }
    // SAFETY: success guarantees all fields were initialized by C.
    let snapshot = unsafe { snapshot.assume_init() };
    if snapshot.abi_version != ABI_VERSION {
        return Err(Error::Custom(
            "macOS RM snapshot ABI version mismatch".into(),
        ));
    }
    Ok(snapshot)
}

/// The native half of the layout fingerprint, read through the shared ABI
/// header. Compared with [`abi_layout`] by the ignored hardware test.
#[cfg(all(target_os = "macos", target_arch = "x86_64"))]
pub fn native_abi_layout() -> Result<[u64; ABI_LAYOUT_SLOTS], Error> {
    let _guard = lock_transport()?;
    let mut out = [0u64; ABI_LAYOUT_SLOTS];
    // SAFETY: the C entry point only writes into the slice we pass and reports
    // how many slots this build actually has.
    let count = unsafe { nvoc_macos_rm_abi_layout(out.as_mut_ptr(), out.len()) };
    if count != ABI_LAYOUT_SLOTS {
        return Err(Error::Custom(format!(
            "macOS RM ABI layout: native build reports {count} slots, Rust expects {ABI_LAYOUT_SLOTS}"
        )));
    }
    Ok(out)
}

#[cfg(not(all(target_os = "macos", target_arch = "x86_64")))]
pub fn native_abi_layout() -> Result<[u64; ABI_LAYOUT_SLOTS], Error> {
    Err(Error::Custom(
        "macOS RM backend requires x86_64 macOS with the NullMoth driver".into(),
    ))
}

#[cfg(not(all(target_os = "macos", target_arch = "x86_64")))]
fn read_snapshot(_telemetry: bool) -> Result<Snapshot, Error> {
    Err(Error::Custom(
        "macOS RM backend requires x86_64 macOS with the NullMoth driver".into(),
    ))
}

// ---------------------------------------------------------------------------
// Continuous monitoring: a persistent read-only session plus the per-sample
// freshness/update interpretation that decides field validity.
// ---------------------------------------------------------------------------

/// RUSD section indices, matching the enum in core/native/macos_rm.c.
const RUSD_FIELDS: usize = 6;
const FIELD_CLOCK: usize = 0;
const FIELD_TEMP: usize = 1;
const FIELD_POWER: usize = 2;
const FIELD_LIMIT: usize = 3;
const FIELD_FAN: usize = 4;
const FIELD_UTIL: usize = 5;

/// A polled section that has not advanced for this long is reported as
/// unavailable instead of repeating its last value forever. The driver's
/// measured poll cadence is 500 ms (see
/// docs/reverse-engineering/macos-gtx1650s-access-2026-10-16.md), so this is a
/// wide margin that will not fire on a healthy idle GPU.
pub const MONITOR_STALE_AFTER: Duration = Duration::from_secs(5);

/// Default budget for the first sample of a session: every polled section
/// should advance past the mapping-time baseline before the first monitor line
/// is emitted, matching the one-shot query's freshness guarantee.
const INITIAL_FRESH_BUDGET: Duration = Duration::from_millis(1500);

/// Raw per-field telemetry exactly as produced by the native session. Every
/// field is a plain integer so the C/Rust layouts can be size-checked.
#[repr(C)]
#[derive(Default, Clone, Copy)]
struct RawSample {
    abi_version: u32,
    field_status: [u32; RUSD_FIELDS],
    graphics_khz: u32,
    memory_khz: u32,
    temperature_fx256: i32,
    power_mw: u32,
    limit_mw: u32,
    fan_rpm: u32,
    utilization_percent: u32,
    pstate: u32,
    pstate_status: u32,
    timestamps: [u64; RUSD_FIELDS],
}

/// The Rust half of the C/Rust layout fingerprint. Slot meanings match
/// NVOC_RM_LAYOUT_* in core/native/macos_rm_abi.h and the assertions in
/// core/tests/macos_rm_abi.c; `native_abi_layout()` returns the C half.
pub fn abi_layout() -> [u64; ABI_LAYOUT_SLOTS] {
    [
        std::mem::size_of::<Snapshot>() as u64,
        std::mem::size_of::<RawSample>() as u64,
        std::mem::size_of::<Telemetry>() as u64,
        std::mem::offset_of!(Snapshot, telemetry) as u64,
        std::mem::offset_of!(Snapshot, driver) as u64,
        std::mem::offset_of!(Telemetry, timestamps) as u64,
        std::mem::offset_of!(Telemetry, temperature_fx256) as u64,
        std::mem::offset_of!(RawSample, graphics_khz) as u64,
        std::mem::offset_of!(RawSample, pstate) as u64,
        std::mem::offset_of!(RawSample, timestamps) as u64,
        ABI_VERSION as u64,
        RUSD_FIELDS as u64,
        FIELD_OK as u64,
        FIELD_UNSTABLE as u64,
        FIELD_UNAVAILABLE as u64,
        FIELD_CLOCK as u64,
        FIELD_TEMP as u64,
        FIELD_POWER as u64,
        FIELD_LIMIT as u64,
        FIELD_FAN as u64,
        FIELD_UTIL as u64,
    ]
}

/// Compile-time mirror of the `_Static_assert`s in macos_rm_abi.h. A layout
/// change on either side fails the build instead of silently mis-reading
/// driver memory.
const _: () = {
    assert!(std::mem::size_of::<Snapshot>() == 360);
    assert!(std::mem::size_of::<RawSample>() == 112);
    assert!(std::mem::size_of::<Telemetry>() == 88);
    assert!(std::mem::offset_of!(Snapshot, telemetry) == 272);
    assert!(std::mem::offset_of!(Snapshot, driver) == 108);
    assert!(std::mem::offset_of!(Telemetry, timestamps) == 40);
    assert!(std::mem::offset_of!(Telemetry, temperature_fx256) == 16);
    assert!(std::mem::offset_of!(RawSample, graphics_khz) == 28);
    assert!(std::mem::offset_of!(RawSample, pstate) == 56);
    assert!(std::mem::offset_of!(RawSample, timestamps) == 64);
};

/// Opaque native session handle; only ever touched through the extern ABI.
#[cfg(all(target_os = "macos", target_arch = "x86_64"))]
#[repr(C)]
struct SessionOpaque {
    _private: [u8; 0],
}

/// One monitored status reading: the status plus, per field, whether the
/// underlying RUSD section advanced since the previous sample. A `false` entry
/// means the last coherent value is still shown but has not refreshed.
#[derive(Debug, Clone, Serialize)]
pub struct DeviceSample {
    #[serde(flatten)]
    pub status: DeviceStatus,
    /// Per field: did this field's RUSD section advance on this sample?
    pub updated: BTreeMap<String, bool>,
    /// Per field freshness classification; see [`FieldState`]. This is how a
    /// value that was never refreshed since the session opened is told apart
    /// from one that merely did not change on this sample.
    pub freshness: BTreeMap<String, FieldState>,
    /// Fields whose stamp/sequence went backwards on this sample: the driver
    /// reset its clock or re-initialised the shared page.
    pub epoch_reset: Vec<String>,
    /// How many RUSD sections advanced. Not serialized: it feeds the internal
    /// "the whole page went silent" reconnect decision.
    #[serde(skip)]
    advancing_sections: usize,
}

impl DeviceSample {
    /// RUSD sections that advanced on this sample (pstate is a live RM control
    /// re-read every sample and is not counted here).
    pub fn advancing_sections(&self) -> usize {
        self.advancing_sections
    }
}

/// Field status codes reported by the native side (macos_rm_abi.h).
const FIELD_OK: u32 = 0;
const FIELD_UNSTABLE: u32 = 1;
const FIELD_UNAVAILABLE: u32 = 2;

/// Consecutive samples with no section advancing at all before the session is
/// considered dead and the caller is asked to reconnect. One quiet field is
/// normal; every section silent this long is not.
const FROZEN_SAMPLES_BEFORE_RECONNECT: u32 = 6;

/// Freshness of one received field, per field and per sample.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum FieldState {
    /// The section advanced on this sample: the published value is new.
    Advancing,
    /// The section did not advance on this sample but is still inside the
    /// staleness window: the last value is repeated with `updated: false`.
    Unchanged,
    /// The section has never advanced since this session opened, so no value
    /// has been confirmed new yet.
    NeverRefreshed,
    /// The section advanced earlier in this session but has been silent past
    /// the staleness window: its last value is no longer trustworthy.
    Stale,
    /// The driver publishes no valid timestamp for this section right now
    /// (cl00de.h RUSD_TIMESTAMP_INVALID), so retrying will not help yet.
    Unsupported,
    /// The section changed while it was copied: no coherent value this round.
    Unstable,
}

impl FieldState {
    /// A value may only be published for these states.
    fn publishes_value(self) -> bool {
        matches!(self, FieldState::Advancing | FieldState::Unchanged)
    }

    fn is_advancing(self) -> bool {
        matches!(self, FieldState::Advancing)
    }
}

/// Per-section monitor state. `ts` is seeded with the session's mapping-time
/// baseline, so a value that was already old when the session opened is never
/// mistaken for a fresh reading.
#[derive(Debug, Clone, Copy, Default)]
struct FieldTracker {
    /// Last RUSD timestamp (polled section) or sequence (non-polled section).
    ts: u64,
    /// Session-relative monotonic nanoseconds when the value last changed.
    changed_ns: u64,
    /// True once the section advanced at least once since the session opened.
    seen_change: bool,
}

/// One section's verdict for this sample.
struct SectionCheck {
    state: FieldState,
    /// Milliseconds the section has been silent: since it last changed, or
    /// since the session opened when it never changed.
    silent_ms: u64,
    /// The stamp/sequence went backwards: the driver reset its clock or
    /// re-initialised the page. The value is still new, but the timeline
    /// restarted and no absolute age may be derived from it.
    epoch_reset: bool,
}

impl SectionCheck {
    fn reason(&self, stale_ms: u64) -> String {
        match self.state {
            FieldState::Unsupported => {
                "RUSD section has no valid timestamp (driver does not publish it)".into()
            }
            FieldState::Unstable => {
                "RUSD section changed while it was copied (unstable sequence)".into()
            }
            FieldState::NeverRefreshed => format!(
                "no RUSD update since this session opened ({} ms)",
                self.silent_ms
            ),
            FieldState::Stale => format!(
                "no RUSD update for {} ms (staleness window {} ms)",
                self.silent_ms, stale_ms
            ),
            FieldState::Advancing | FieldState::Unchanged => String::new(),
        }
    }
}

/// Classify one RUSD section. Freshness comes from advancement since the
/// baseline plus host monotonic time, never from the stamp's absolute value:
/// the driver's stamps lag wall clock and drift, and non-polled sections carry
/// sequence numbers rather than nanoseconds (class/cl00de.h).
fn track_section(
    tracker: &mut FieldTracker,
    raw_status: u32,
    ts: u64,
    now_ns: u64,
    stale_ns: u64,
) -> SectionCheck {
    let silent_ns = now_ns.saturating_sub(tracker.changed_ns);
    match raw_status {
        FIELD_OK => {}
        FIELD_UNAVAILABLE => {
            return SectionCheck {
                state: FieldState::Unsupported,
                silent_ms: silent_ns / 1_000_000,
                epoch_reset: false,
            };
        }
        _ => {
            return SectionCheck {
                state: FieldState::Unstable,
                silent_ms: silent_ns / 1_000_000,
                epoch_reset: false,
            };
        }
    }
    if ts == 0 {
        return SectionCheck {
            state: FieldState::Unsupported,
            silent_ms: silent_ns / 1_000_000,
            epoch_reset: false,
        };
    }
    if ts != tracker.ts {
        // Any change is new data, including a backwards jump: a clock or
        // sequence reset still published a freshly written section.
        let epoch_reset = ts < tracker.ts;
        tracker.ts = ts;
        tracker.changed_ns = now_ns;
        tracker.seen_change = true;
        return SectionCheck {
            state: FieldState::Advancing,
            silent_ms: 0,
            epoch_reset,
        };
    }
    let state = if !tracker.seen_change {
        FieldState::NeverRefreshed
    } else if silent_ns > stale_ns {
        FieldState::Stale
    } else {
        FieldState::Unchanged
    };
    SectionCheck {
        state,
        silent_ms: silent_ns / 1_000_000,
        epoch_reset: false,
    }
}

/// Everything one sample records about its fields.
#[derive(Default)]
struct SampleFields {
    unavailable: BTreeMap<String, String>,
    timestamps: BTreeMap<String, u64>,
    updated: BTreeMap<String, bool>,
    freshness: BTreeMap<String, FieldState>,
    epoch_resets: Vec<String>,
    advancing_sections: usize,
}

impl SampleFields {
    /// Record one field. Returns whether a value may be published for it.
    fn publish(
        &mut self,
        name: &str,
        check: &SectionCheck,
        ts: u64,
        sane: bool,
        stale_ms: u64,
    ) -> bool {
        self.freshness.insert(name.into(), check.state);
        if check.epoch_reset {
            self.epoch_resets.push(name.into());
        }
        if !check.state.publishes_value() {
            self.unavailable.insert(name.into(), check.reason(stale_ms));
            return false;
        }
        if !sane {
            self.unavailable
                .insert(name.into(), "RUSD field reported an invalid value".into());
            return false;
        }
        self.timestamps.insert(name.into(), ts);
        self.updated.insert(name.into(), check.state.is_advancing());
        true
    }

    /// Publish a whole section at once (the clock section feeds two fields).
    fn note_section(&mut self, check: &SectionCheck) {
        if check.state.is_advancing() {
            self.advancing_sections += 1;
        }
    }
}

/// Interpret one raw native sample. Pure and deterministic so the freshness,
/// staleness, zero-value, epoch-reset and partial-missing rules are CPU
/// testable.
fn interpret_sample(
    trackers: &mut [FieldTracker; RUSD_FIELDS],
    raw: &RawSample,
    now_ns: u64,
    stale_after: Duration,
) -> DeviceSample {
    let stale_ns = u64::try_from(stale_after.as_nanos()).unwrap_or(u64::MAX);
    let stale_ms = stale_ns / 1_000_000;
    let mut fields = SampleFields::default();

    let clock = track_section(
        &mut trackers[FIELD_CLOCK],
        raw.field_status[FIELD_CLOCK],
        raw.timestamps[FIELD_CLOCK],
        now_ns,
        stale_ns,
    );
    fields.note_section(&clock);
    let graphics_clock_khz = fields
        .publish(
            "graphics_clock_khz",
            &clock,
            raw.timestamps[FIELD_CLOCK],
            raw.graphics_khz != u32::MAX,
            stale_ms,
        )
        .then_some(raw.graphics_khz);
    let memory_clock_khz = fields
        .publish(
            "memory_clock_khz",
            &clock,
            raw.timestamps[FIELD_CLOCK],
            raw.memory_khz != u32::MAX,
            stale_ms,
        )
        .then_some(raw.memory_khz);

    let temp = track_section(
        &mut trackers[FIELD_TEMP],
        raw.field_status[FIELD_TEMP],
        raw.timestamps[FIELD_TEMP],
        now_ns,
        stale_ns,
    );
    fields.note_section(&temp);
    let temperature_c = fields
        .publish(
            "temperature_c",
            &temp,
            raw.timestamps[FIELD_TEMP],
            (-50 * 256..=150 * 256).contains(&raw.temperature_fx256),
            stale_ms,
        )
        .then_some(f64::from(raw.temperature_fx256) / 256.0);

    let power = track_section(
        &mut trackers[FIELD_POWER],
        raw.field_status[FIELD_POWER],
        raw.timestamps[FIELD_POWER],
        now_ns,
        stale_ns,
    );
    fields.note_section(&power);
    let power_mw = fields
        .publish(
            "power_mw",
            &power,
            raw.timestamps[FIELD_POWER],
            raw.power_mw != u32::MAX,
            stale_ms,
        )
        .then_some(raw.power_mw);

    let limit = track_section(
        &mut trackers[FIELD_LIMIT],
        raw.field_status[FIELD_LIMIT],
        raw.timestamps[FIELD_LIMIT],
        now_ns,
        stale_ns,
    );
    fields.note_section(&limit);
    let power_limit_mw = fields
        .publish(
            "power_limit_mw",
            &limit,
            raw.timestamps[FIELD_LIMIT],
            raw.limit_mw != u32::MAX,
            stale_ms,
        )
        .then_some(raw.limit_mw);

    let fan = track_section(
        &mut trackers[FIELD_FAN],
        raw.field_status[FIELD_FAN],
        raw.timestamps[FIELD_FAN],
        now_ns,
        stale_ns,
    );
    fields.note_section(&fan);
    let fan_rpm = fields
        .publish(
            "fan_rpm",
            &fan,
            raw.timestamps[FIELD_FAN],
            raw.fan_rpm != u32::MAX,
            stale_ms,
        )
        .then_some(raw.fan_rpm);

    let util = track_section(
        &mut trackers[FIELD_UTIL],
        raw.field_status[FIELD_UTIL],
        raw.timestamps[FIELD_UTIL],
        now_ns,
        stale_ns,
    );
    fields.note_section(&util);
    let utilization_percent = fields
        .publish(
            "utilization_percent",
            &util,
            raw.timestamps[FIELD_UTIL],
            raw.utilization_percent <= 100,
            stale_ms,
        )
        .then_some(raw.utilization_percent);

    // P-state is a live RM control re-read on every sample, not a RUSD
    // section, so it is new whenever the query succeeds.
    let pstate = if raw.pstate_status == 0 {
        fields
            .freshness
            .insert("pstate".into(), FieldState::Advancing);
        Some(raw.pstate)
    } else {
        fields
            .freshness
            .insert("pstate".into(), FieldState::Unsupported);
        fields
            .unavailable
            .insert("pstate".into(), query_error(raw.pstate_status));
        None
    };

    DeviceSample {
        status: DeviceStatus {
            pstate,
            graphics_clock_khz,
            memory_clock_khz,
            temperature_c,
            power_mw,
            power_limit_mw,
            fan_rpm,
            utilization_percent,
            rusd_timestamps: fields.timestamps,
            unavailable: fields.unavailable,
        },
        updated: fields.updated,
        freshness: fields.freshness,
        epoch_reset: fields.epoch_resets,
        advancing_sections: fields.advancing_sections,
    }
}

/// RUSD sections in the order reported by
/// [`MacosRmSession::initial_freshness`] and by the native field indices.
pub const MONITOR_SECTIONS: [&str; RUSD_FIELDS] = [
    "clock",
    "temperature",
    "power",
    "power_limit",
    "fan",
    "utilization",
];

/// A persistent read-only RM session: the connection chain and the mapped RUSD
/// page are created once and reused for every [`sample`](Self::sample).
///
/// [`close`](Self::close) releases everything and reports a release error;
/// `Drop` is the fallback for the paths that forget to call it (it cannot
/// report anything, and a panic-abort build never reaches it at all).
pub struct MacosRmSession {
    #[cfg(all(target_os = "macos", target_arch = "x86_64"))]
    handle: *mut SessionOpaque,
    info: DeviceInfo,
    trackers: [FieldTracker; RUSD_FIELDS],
    start: Instant,
    stale_after: Duration,
    /// Per section: did it publish a new value inside the open budget? A
    /// section that did not is reported as `never_refreshed` until it does,
    /// instead of being presented as a fresh reading.
    initial_fresh: [bool; RUSD_FIELDS],
    /// Consecutive samples in which no RUSD section advanced.
    silent_samples: u32,
}

impl MacosRmSession {
    /// Open with the default staleness window and no cancellation hook.
    pub fn open() -> Result<Self, Error> {
        Self::open_with(MONITOR_STALE_AFTER, INITIAL_FRESH_BUDGET, &|| false)
    }

    /// Open a session, waiting at most `initial_budget` for every polled
    /// section to publish its first new value, and bailing out early when
    /// `cancelled()` returns true so a stop request is never stuck behind the
    /// freshness wait.
    ///
    /// A timed-out wait is not an open failure: the affected sections are
    /// reported as [`FieldState::NeverRefreshed`] until they publish, and
    /// [`initial_freshness`](Self::initial_freshness) says which ones are
    /// still waiting.
    #[cfg(all(target_os = "macos", target_arch = "x86_64"))]
    pub fn open_with(
        stale_after: Duration,
        initial_budget: Duration,
        cancelled: &dyn Fn() -> bool,
    ) -> Result<Self, Error> {
        let mut info = std::mem::MaybeUninit::<Snapshot>::uninit();
        let mut handle: *mut SessionOpaque = std::ptr::null_mut();
        // The native side hands back each polled section's mapping-time
        // watermark: the value a reading must move past to count as new.
        let mut baseline = [0u64; RUSD_FIELDS];
        let mut error = [0u8; 256];
        let status = {
            let _guard = lock_transport()?;
            // SAFETY: C receives correctly sized writable buffers and either
            // fills `info`/`baseline` fully or returns an error with `handle`
            // left null.
            unsafe {
                nvoc_macos_rm_session_open(
                    &mut handle,
                    info.as_mut_ptr(),
                    std::mem::size_of::<Snapshot>(),
                    baseline.as_mut_ptr(),
                    RUSD_FIELDS,
                    error.as_mut_ptr(),
                    error.len(),
                )
            }
        };
        if status != 0 {
            return Err(Error::Custom(format!("macOS RM: {}", text(&error))));
        }
        // SAFETY: a zero status means C initialised every field.
        let info = unsafe { info.assume_init() };
        if info.abi_version != ABI_VERSION {
            let mut err = [0u8; 256];
            let _guard = lock_transport()?;
            // SAFETY: the handle came from a successful open and is closed once.
            let _ = unsafe { nvoc_macos_rm_session_close(handle, err.as_mut_ptr(), err.len()) };
            return Err(Error::Custom(format!(
                "macOS RM session ABI version {} is not {ABI_VERSION}",
                info.abi_version
            )));
        }
        let device = device_from_snapshot(&info);
        let mut session = MacosRmSession {
            handle,
            info: device.info,
            trackers: baseline.map(|ts| FieldTracker {
                ts,
                changed_ns: 0,
                seen_change: false,
            }),
            start: Instant::now(),
            stale_after,
            initial_fresh: [false; RUSD_FIELDS],
            silent_samples: 0,
        };
        session.initial_fresh = session.wait_initial_freshness(initial_budget, cancelled);
        Ok(session)
    }

    #[cfg(not(all(target_os = "macos", target_arch = "x86_64")))]
    pub fn open_with(
        _stale_after: Duration,
        _initial_budget: Duration,
        _cancelled: &dyn Fn() -> bool,
    ) -> Result<Self, Error> {
        Err(Error::Custom(
            "macOS RM backend requires x86_64 macOS with the NullMoth driver".into(),
        ))
    }

    pub fn info(&self) -> &DeviceInfo {
        &self.info
    }

    /// Identity triple used to detect that the backend now reports a different
    /// device than the one originally discovered.
    pub fn identity(&self) -> (String, u32, u32) {
        (
            self.info.pci_address.clone(),
            self.info.device_id,
            self.info.rm_gpu_id,
        )
    }

    /// Per section (see [`MONITOR_SECTIONS`]): did it publish a new value
    /// during the open wait? A `false` entry means the driver had not
    /// published a new value inside the budget, so the first samples report
    /// that section as `never_refreshed` instead of showing old data as fresh.
    pub fn initial_freshness(&self) -> [bool; RUSD_FIELDS] {
        self.initial_fresh
    }

    /// One coherent, read-only sample. Session-level failures (dead fds, a
    /// page that stopped publishing, lost device) return `Err` and require
    /// reopening; per-field problems are reported inside the sample.
    pub fn sample(&mut self) -> Result<DeviceSample, Error> {
        let raw = self.raw_sample()?;
        let now = self.mono_ns();
        let sample = interpret_sample(&mut self.trackers, &raw, now, self.stale_after);
        self.track_health(&sample)?;
        Ok(sample)
    }

    /// Ask the caller to reconnect when the whole page goes quiet: a single
    /// section without an update is normal, but every section silent for
    /// several consecutive samples means this session no longer publishes.
    fn track_health(&mut self, sample: &DeviceSample) -> Result<(), Error> {
        if sample.advancing_sections() > 0 {
            self.silent_samples = 0;
            return Ok(());
        }
        self.silent_samples += 1;
        if self.silent_samples >= FROZEN_SAMPLES_BEFORE_RECONNECT {
            return Err(Error::Custom(format!(
                "macOS RM session stopped publishing RUSD data (no section advanced for {} consecutive samples)",
                self.silent_samples
            )));
        }
        Ok(())
    }

    fn mono_ns(&self) -> u64 {
        self.start.elapsed().as_nanos().min(u128::from(u64::MAX)) as u64
    }

    /// Release the mapping, the RM objects and both fds, reporting a release
    /// failure instead of swallowing it. `Drop` still covers callers that do
    /// not call this.
    pub fn close(mut self) -> Result<(), Error> {
        self.close_native()
    }

    #[cfg(all(target_os = "macos", target_arch = "x86_64"))]
    fn close_native(&mut self) -> Result<(), Error> {
        {
            if self.handle.is_null() {
                return Ok(());
            }
            let mut error = [0u8; 256];
            let _guard = transport().lock().unwrap_or_else(|err| err.into_inner());
            // SAFETY: the handle came from a successful open and is closed once.
            let status = unsafe {
                nvoc_macos_rm_session_close(self.handle, error.as_mut_ptr(), error.len())
            };
            self.handle = std::ptr::null_mut();
            if status != 0 {
                return Err(Error::Custom(format!(
                    "macOS RM session close: {}",
                    text(&error)
                )));
            }
        }
        Ok(())
    }

    /// Nothing to release where the backend does not exist.
    #[cfg(not(all(target_os = "macos", target_arch = "x86_64")))]
    fn close_native(&mut self) -> Result<(), Error> {
        Ok(())
    }

    #[cfg(all(target_os = "macos", target_arch = "x86_64"))]
    fn raw_sample(&mut self) -> Result<RawSample, Error> {
        let _guard = lock_transport()?;
        let mut raw = std::mem::MaybeUninit::<RawSample>::uninit();
        let mut error = [0u8; 256];
        // SAFETY: the handle is live and `raw` is a correctly sized output.
        let status = unsafe {
            nvoc_macos_rm_session_sample(
                self.handle,
                raw.as_mut_ptr(),
                std::mem::size_of::<RawSample>(),
                error.as_mut_ptr(),
                error.len(),
            )
        };
        if status != 0 {
            return Err(Error::Custom(format!("macOS RM sample: {}", text(&error))));
        }
        // SAFETY: a zero status means C initialised every field.
        let raw = unsafe { raw.assume_init() };
        if raw.abi_version != ABI_VERSION {
            return Err(Error::Custom(format!(
                "macOS RM sample ABI version {} is not {ABI_VERSION}",
                raw.abi_version
            )));
        }
        Ok(raw)
    }

    #[cfg(not(all(target_os = "macos", target_arch = "x86_64")))]
    fn raw_sample(&mut self) -> Result<RawSample, Error> {
        Err(Error::Custom(
            "macOS RM backend requires x86_64 macOS with the NullMoth driver".into(),
        ))
    }

    /// Best-effort wait until every polled section advanced past the
    /// mapping-time baseline, so the first emitted sample is known new. The
    /// trackers are deliberately left untouched: the first real `sample()`
    /// re-observes the same advancement and reports it as `advancing`.
    #[cfg(all(target_os = "macos", target_arch = "x86_64"))]
    fn wait_initial_freshness(
        &mut self,
        budget: Duration,
        cancelled: &dyn Fn() -> bool,
    ) -> [bool; RUSD_FIELDS] {
        let mut fresh = [false; RUSD_FIELDS];
        let deadline = Instant::now() + budget;
        loop {
            if cancelled() {
                return fresh;
            }
            match self.raw_sample() {
                Ok(raw) => {
                    for (index, seen) in fresh.iter_mut().enumerate() {
                        if raw.field_status[index] == FIELD_OK
                            && raw.timestamps[index] != 0
                            && raw.timestamps[index] != self.trackers[index].ts
                        {
                            *seen = true;
                        }
                    }
                }
                // A session-level failure is reported by the first real sample
                // and handled by the caller's reconnect path; do not mask it.
                Err(_) => return fresh,
            }
            if fresh.iter().all(|seen| *seen) || Instant::now() >= deadline {
                return fresh;
            }
            std::thread::sleep(Duration::from_millis(50));
        }
    }
}

impl Drop for MacosRmSession {
    #[cfg(all(target_os = "macos", target_arch = "x86_64"))]
    fn drop(&mut self) {
        if self.handle.is_null() {
            return;
        }
        let mut error = [0u8; 256];
        // Recover from a poisoned lock so the native resources are still
        // released; leaking the mapping is worse than ignoring poisoning.
        let _guard = transport().lock().unwrap_or_else(|err| err.into_inner());
        // SAFETY: the handle came from a successful open and is closed once.
        unsafe { nvoc_macos_rm_session_close(self.handle, error.as_mut_ptr(), error.len()) };
        self.handle = std::ptr::null_mut();
    }

    #[cfg(not(all(target_os = "macos", target_arch = "x86_64")))]
    fn drop(&mut self) {}
}

#[cfg(test)]
mod tests {
    use super::*;

    fn snapshot() -> Snapshot {
        // SAFETY: private ABI consists exclusively of integers and byte arrays.
        unsafe { std::mem::zeroed() }
    }

    #[test]
    fn typed_queries_reject_targets_without_rm_before_native_access() {
        let target = crate::GpuTarget::without_backends(GpuId(0x100), 0);
        assert!(!target.has_macos_rm());
        let error = crate::run(&target, QueryMacosRmStatus).unwrap_err();
        assert!(error.to_string().contains("has no macOS RM backend"));
        let error = crate::run(&target, QueryMacosRmInfo).unwrap_err();
        assert!(error.to_string().contains("has no macOS RM backend"));
    }

    #[test]
    fn missing_data_serializes_as_null_while_valid_p0_is_preserved() {
        let mut s = snapshot();
        let value = serde_json::to_value(status_from_snapshot(&s)).unwrap();
        assert_eq!(value["pstate"], 0);
        assert!(value["graphics_clock_khz"].is_null());
        assert!(value["temperature_c"].is_null());
        assert!(value["unavailable"]["power_mw"].is_string());
        s.pstate_status = 0x56;
        let value = serde_json::to_value(status_from_snapshot(&s)).unwrap();
        assert!(value["pstate"].is_null());
        assert!(
            value["unavailable"]["pstate"]
                .as_str()
                .unwrap()
                .contains("00000056")
        );
    }

    #[test]
    fn telemetry_keeps_zero_readings_and_converts_signed_fixed_point_temperature() {
        let mut s = snapshot();
        s.telemetry.valid = 0x3f;
        s.telemetry.timestamps = [123; 6];
        s.telemetry.graphics_khz = 1_530_000;
        s.telemetry.memory_khz = 6_000_984;
        s.telemetry.temperature_fx256 = -128;
        s.telemetry.power_mw = 11_357;
        let status = status_from_snapshot(&s);
        assert_eq!(status.graphics_clock_khz, Some(1_530_000));
        assert_eq!(status.memory_clock_khz, Some(6_000_984));
        assert_eq!(status.temperature_c, Some(-0.5));
        assert_eq!(status.power_mw, Some(11_357));
        assert_eq!(status.fan_rpm, Some(0));
        assert_eq!(status.utilization_percent, Some(0));
        assert!(status.unavailable.is_empty());
        assert_eq!(status.rusd_timestamps["temperature_c"], 123);
    }

    #[test]
    fn stale_failed_and_invalid_fields_never_become_measurements() {
        let mut s = snapshot();
        s.telemetry.valid = 0x3f;
        s.telemetry.status = 0x1f;
        let status = status_from_snapshot(&s);
        assert!(status.temperature_c.is_none());
        assert!(status.rusd_timestamps.is_empty());
        assert!(status.unavailable["temperature_c"].contains("0000001f"));
        s.telemetry.status = 0;
        s.telemetry.valid &= !(1 << 2);
        s.telemetry.temperature_fx256 = i32::MAX;
        s.telemetry.utilization_percent = 101;
        let status = status_from_snapshot(&s);
        assert!(status.power_mw.is_none());
        assert!(status.temperature_c.is_none());
        assert!(status.utilization_percent.is_none());
        assert!(status.unavailable["power_mw"].contains("did not update"));
        assert!(status.unavailable["temperature_c"].contains("invalid value"));
        assert!(!status.rusd_timestamps.contains_key("power_mw"));
    }

    #[test]
    fn pci_bus_identity_is_distinct_from_rm_gpu_id_and_failed_queries_are_absent() {
        let mut s = snapshot();
        s.rm_gpu_id = 0x100;
        s.bus = 3;
        s.slot = 2;
        s.memory_status = 0x1f;
        s.total_bytes = 4 * 1024 * 1024 * 1024;
        let device = device_from_snapshot(&s);
        assert_eq!(device.id().0, 3 * 256);
        assert_eq!(device.info().pci_address, "0000:03:02.0");
        assert_eq!(device.info().rm_gpu_id, 0x100);
        assert_eq!(device.info().memory_total_bytes, None);
        assert!(device.info().unavailable["memory"].contains("0000001f"));
    }

    /// A raw sample in which every section is readable and carries the same
    /// stamps. Tests move the stamps to model advancing, quiet and resetting
    /// sections.
    fn raw_sample() -> RawSample {
        RawSample {
            abi_version: ABI_VERSION,
            timestamps: [1_000; RUSD_FIELDS],
            graphics_khz: 1_530_000,
            memory_khz: 6_000_984,
            temperature_fx256: 12_800, // 50.0 C
            power_mw: 11_412,
            limit_mw: 100_000,
            pstate: 1,
            ..RawSample::default()
        }
    }

    /// Trackers seeded the way session_open() seeds them: with the mapping-time
    /// watermark the first sample must advance past.
    fn trackers_at(baseline: u64) -> [FieldTracker; RUSD_FIELDS] {
        [FieldTracker {
            ts: baseline,
            changed_ns: 0,
            seen_change: false,
        }; RUSD_FIELDS]
    }

    #[test]
    fn monitor_keeps_valid_zero_readings_and_marks_the_first_sample_updated() {
        let mut trackers = trackers_at(0);
        let sample = interpret_sample(&mut trackers, &raw_sample(), 0, MONITOR_STALE_AFTER);
        assert_eq!(sample.status.fan_rpm, Some(0));
        assert_eq!(sample.status.utilization_percent, Some(0));
        assert_eq!(sample.status.temperature_c, Some(50.0));
        assert_eq!(sample.status.pstate, Some(1));
        assert!(sample.status.unavailable.is_empty());
        assert_eq!(sample.updated.len(), 7);
        assert!(sample.updated.values().all(|value| *value));
        assert_eq!(sample.status.rusd_timestamps["temperature_c"], 1_000);
    }

    #[test]
    fn monitor_flags_repeat_then_stale_sections_without_fabricating_values() {
        let mut trackers = [FieldTracker::default(); RUSD_FIELDS];
        let raw = raw_sample();
        interpret_sample(&mut trackers, &raw, 0, MONITOR_STALE_AFTER);

        // Same timestamp one second later: still a coherent reading, just not
        // refreshed.
        let sample = interpret_sample(&mut trackers, &raw, 1_000_000_000, MONITOR_STALE_AFTER);
        assert!(sample.status.unavailable.is_empty());
        assert_eq!(sample.status.temperature_c, Some(50.0));
        assert_eq!(sample.updated.len(), 7);
        assert!(sample.updated.values().all(|value| !*value));

        // Unchanged past the staleness window: reported unavailable, never as
        // a repeated or zero value.
        let sample = interpret_sample(&mut trackers, &raw, 6_000_000_000, MONITOR_STALE_AFTER);
        assert!(sample.status.temperature_c.is_none());
        assert!(sample.status.power_mw.is_none());
        assert!(
            sample.status.unavailable["temperature_c"].contains("no RUSD update"),
            "{:?}",
            sample.status.unavailable
        );
        assert!(!sample.status.rusd_timestamps.contains_key("temperature_c"));

        // A later advance clears the stale state and values return.
        let mut newer = raw;
        newer.timestamps = [2_000; RUSD_FIELDS];
        let sample = interpret_sample(&mut trackers, &newer, 6_100_000_000, MONITOR_STALE_AFTER);
        assert_eq!(sample.status.temperature_c, Some(50.0));
        assert!(sample.updated.values().all(|value| *value));
        assert!(sample.status.unavailable.is_empty());
    }

    #[test]
    fn monitor_treats_unstable_or_invalid_fields_as_unavailable() {
        let mut trackers = [FieldTracker::default(); RUSD_FIELDS];
        let mut raw = raw_sample();
        raw.field_status[FIELD_TEMP] = 1;
        raw.timestamps[FIELD_TEMP] = 0;
        raw.utilization_percent = 101;
        raw.graphics_khz = u32::MAX;
        let sample = interpret_sample(&mut trackers, &raw, 0, MONITOR_STALE_AFTER);
        assert!(sample.status.temperature_c.is_none());
        assert!(sample.status.utilization_percent.is_none());
        assert!(sample.status.graphics_clock_khz.is_none());
        assert_eq!(sample.status.memory_clock_khz, Some(6_000_984));
        assert!(
            sample.status.unavailable["temperature_c"].contains("unstable sequence"),
            "{:?}",
            sample.status.unavailable
        );
        assert!(
            sample.status.unavailable["utilization_percent"].contains("invalid value"),
            "{:?}",
            sample.status.unavailable
        );
        assert!(!sample.updated.contains_key("graphics_clock_khz"));
        assert!(sample.updated["memory_clock_khz"]);

        // A section the driver does not publish at all is a different
        // condition from one that raced, and neither is a measurement.
        let mut trackers = trackers_at(0);
        let mut raw = raw_sample();
        raw.field_status[FIELD_POWER] = FIELD_UNAVAILABLE;
        raw.timestamps[FIELD_POWER] = 0;
        let sample = interpret_sample(&mut trackers, &raw, 0, MONITOR_STALE_AFTER);
        assert!(sample.status.power_mw.is_none());
        assert_eq!(sample.freshness["power_mw"], FieldState::Unsupported);
        assert!(
            sample.status.unavailable["power_mw"].contains("no valid timestamp"),
            "{:?}",
            sample.status.unavailable
        );
        assert!(sample.epoch_reset.is_empty());
    }

    #[test]
    fn monitor_distinguishes_never_refreshed_from_not_updated_this_sample() {
        // Opening on an already-old page: the watermark is the baseline, so an
        // unchanged stamp means "nothing new yet", not "fresh zero".
        let mut trackers = trackers_at(1_000);
        let raw = raw_sample();
        let sample = interpret_sample(&mut trackers, &raw, 100_000_000, MONITOR_STALE_AFTER);
        assert!(sample.status.temperature_c.is_none());
        assert_eq!(sample.advancing_sections(), 0);
        for field in [
            "graphics_clock_khz",
            "temperature_c",
            "power_mw",
            "power_limit_mw",
            "fan_rpm",
            "utilization_percent",
        ] {
            assert_eq!(
                sample.freshness[field],
                FieldState::NeverRefreshed,
                "{field}"
            );
            assert!(!sample.updated.contains_key(field));
            assert!(
                sample.status.unavailable[field].contains("since this session opened"),
                "{field}: {:?}",
                sample.status.unavailable
            );
        }
        // P-state is a live control read, not a RUSD section.
        assert_eq!(sample.freshness["pstate"], FieldState::Advancing);
        assert_eq!(sample.status.pstate, Some(1));

        // The first advance publishes values and marks them new.
        let mut newer = raw;
        newer.timestamps = [2_000; RUSD_FIELDS];
        let sample = interpret_sample(&mut trackers, &newer, 600_000_000, MONITOR_STALE_AFTER);
        assert_eq!(sample.status.temperature_c, Some(50.0));
        assert_eq!(sample.advancing_sections(), RUSD_FIELDS);
        assert!(sample.status.unavailable.is_empty());
        assert!(sample.updated.values().all(|updated| *updated));

        // Now the same stamps are "unchanged", not "never refreshed".
        let sample = interpret_sample(&mut trackers, &newer, 700_000_000, MONITOR_STALE_AFTER);
        assert_eq!(sample.freshness["temperature_c"], FieldState::Unchanged);
        assert_eq!(sample.status.temperature_c, Some(50.0));
        assert!(sample.updated.values().all(|updated| !*updated));
        assert!(sample.status.unavailable.is_empty());
    }

    #[test]
    fn monitor_treats_a_backwards_stamp_as_new_data_from_a_reset_epoch() {
        let mut trackers = trackers_at(9_000);
        trackers[FIELD_TEMP].seen_change = true;
        trackers[FIELD_TEMP].changed_ns = 0;
        let mut raw = raw_sample();
        raw.timestamps = [1_000; RUSD_FIELDS];
        let sample = interpret_sample(&mut trackers, &raw, 500_000_000, MONITOR_STALE_AFTER);
        // A restarted clock still carries freshly written data, but the sample
        // says so instead of inventing an age from the stamp.
        assert_eq!(sample.status.temperature_c, Some(50.0));
        assert_eq!(sample.freshness["temperature_c"], FieldState::Advancing);
        assert!(sample.updated.values().all(|updated| *updated));
        assert!(sample.epoch_reset.contains(&"temperature_c".to_string()));
        assert!(
            sample
                .epoch_reset
                .contains(&"graphics_clock_khz".to_string())
        );
    }

    #[test]
    fn sample_json_carries_freshness_epoch_resets_and_serializes_field_states() {
        let mut trackers = trackers_at(9_000);
        let mut raw = raw_sample();
        // Temperature moved backwards (a reset epoch), power is unpublished and
        // the fan still sits on the mapping-time watermark.
        raw.timestamps[FIELD_TEMP] = 1_000;
        raw.field_status[FIELD_POWER] = FIELD_UNAVAILABLE;
        raw.timestamps[FIELD_POWER] = 0;
        raw.timestamps[FIELD_FAN] = 9_000;
        let sample = interpret_sample(&mut trackers, &raw, 10_000_000, MONITOR_STALE_AFTER);
        let value = serde_json::to_value(&sample).unwrap();
        assert_eq!(value["freshness"]["temperature_c"], "advancing");
        assert_eq!(value["freshness"]["power_mw"], "unsupported");
        assert_eq!(value["freshness"]["fan_rpm"], "never_refreshed");
        assert!(
            sample.epoch_reset.contains(&"temperature_c".to_string()),
            "{:?}",
            sample.epoch_reset
        );
        assert!(value["advancing_sections"].is_null());
        assert!(value["temperature_c"].is_number());
        assert!(value["power_mw"].is_null());
    }

    #[test]
    fn rust_abi_layout_matches_the_native_contract() {
        // Mirrors the _Static_asserts in core/native/macos_rm_abi.h and the
        // assertions in core/tests/macos_rm_abi.c; the ignored hardware test
        // additionally compares this array with the native fingerprint.
        assert_eq!(
            abi_layout(),
            [
                360,
                112,
                88,
                272,
                108,
                40,
                16,
                28,
                56,
                64, // sizes and offsets
                u64::from(ABI_VERSION),
                RUSD_FIELDS as u64,
                FIELD_OK as u64,
                FIELD_UNSTABLE as u64,
                FIELD_UNAVAILABLE as u64,
                0,
                1,
                2,
                3,
                4,
                5,
            ]
        );
    }

    #[test]
    fn monitor_reports_pstate_failure_and_flattens_the_sample_json() {
        let mut trackers = [FieldTracker::default(); RUSD_FIELDS];
        let mut raw = raw_sample();
        raw.pstate_status = 0x56;
        let sample = interpret_sample(&mut trackers, &raw, 0, MONITOR_STALE_AFTER);
        assert!(sample.status.pstate.is_none());
        assert!(sample.status.unavailable["pstate"].contains("00000056"));
        let value = serde_json::to_value(&sample).unwrap();
        assert_eq!(value["fan_rpm"], 0);
        assert_eq!(value["temperature_c"], 50.0);
        assert!(value["updated"]["temperature_c"].as_bool().unwrap());
        assert!(value["unavailable"]["pstate"].is_string());
    }

    #[cfg(not(all(target_os = "macos", target_arch = "x86_64")))]
    #[test]
    fn session_open_reports_the_platform_requirement_off_macos() {
        match MacosRmSession::open() {
            Ok(_) => panic!("macOS RM session must not open off x86_64 macOS"),
            Err(error) => assert!(error.to_string().contains("requires x86_64 macOS")),
        }
    }
}
