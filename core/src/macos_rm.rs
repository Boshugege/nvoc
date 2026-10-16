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

// Private stable C ABI, not an NVIDIA RM structure. NVIDIA layouts live only
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
    if snapshot.abi_version != 2 {
        return Err(Error::Custom(
            "macOS RM snapshot ABI version mismatch".into(),
        ));
    }
    Ok(snapshot)
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

/// Budget for the first sample of a session: every polled section should
/// advance past the mapping-time baseline before the first monitor line is
/// emitted, matching the one-shot query's freshness guarantee.
#[cfg(all(target_os = "macos", target_arch = "x86_64"))]
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

#[cfg(all(target_os = "macos", target_arch = "x86_64"))]
const _: () = assert!(std::mem::size_of::<RawSample>() == 112);

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
    pub updated: BTreeMap<String, bool>,
}

#[derive(Debug, Clone, Copy, Default)]
struct FieldTracker {
    /// Last RUSD timestamp (polled) or sequence (non-polled) for this section.
    /// `0` means the session has not observed a value yet.
    ts: u64,
    /// Session-relative monotonic nanoseconds when the value last changed.
    changed_ns: u64,
}

enum SectionRead {
    Read { updated: bool },
    Failed(String),
}

/// Decide whether a polled section produced a usable reading this sample.
/// Freshness is judged from advancement since the previous sample and a
/// session-relative staleness window, never from the RUSD timestamp's absolute
/// value: the driver's timestamps lag wall clock by ~2 s and drift, so only the
/// rate of advance is trustworthy.
fn track_section(
    tracker: &mut FieldTracker,
    raw_status: u32,
    ts: u64,
    now_ns: u64,
    stale_ns: u64,
) -> SectionRead {
    if raw_status != 0 {
        return SectionRead::Failed(
            "RUSD section could not be read coherently (unstable sequence)".into(),
        );
    }
    if ts == 0 {
        return SectionRead::Failed("RUSD section has no valid timestamp yet".into());
    }
    if ts != tracker.ts {
        tracker.ts = ts;
        tracker.changed_ns = now_ns;
        return SectionRead::Read { updated: true };
    }
    let age = now_ns.saturating_sub(tracker.changed_ns);
    if age > stale_ns {
        return SectionRead::Failed(format!(
            "no RUSD update for {} ms (staleness window {} ms)",
            age / 1_000_000,
            stale_ns / 1_000_000
        ));
    }
    SectionRead::Read { updated: false }
}

#[allow(clippy::too_many_arguments)]
fn record_field(
    name: &str,
    section: &SectionRead,
    sane: bool,
    ts: u64,
    unavailable: &mut BTreeMap<String, String>,
    timestamps: &mut BTreeMap<String, u64>,
    updated: &mut BTreeMap<String, bool>,
) -> bool {
    match section {
        SectionRead::Failed(reason) => {
            unavailable.insert(name.into(), reason.clone());
            false
        }
        SectionRead::Read { updated: upd } => {
            if !sane {
                unavailable.insert(name.into(), "RUSD field reported an invalid value".into());
                false
            } else {
                timestamps.insert(name.into(), ts);
                updated.insert(name.into(), *upd);
                true
            }
        }
    }
}

/// Interpret one raw native sample. Pure and deterministic so the freshness,
/// staleness, zero-value and partial-missing rules are CPU testable.
fn interpret_sample(
    trackers: &mut [FieldTracker; RUSD_FIELDS],
    raw: &RawSample,
    now_ns: u64,
    stale_after: Duration,
) -> DeviceSample {
    let stale_ns = u64::try_from(stale_after.as_nanos()).unwrap_or(u64::MAX);
    let mut unavailable = BTreeMap::new();
    let mut timestamps = BTreeMap::new();
    let mut updated = BTreeMap::new();

    let clock = track_section(
        &mut trackers[FIELD_CLOCK],
        raw.field_status[FIELD_CLOCK],
        raw.timestamps[FIELD_CLOCK],
        now_ns,
        stale_ns,
    );
    let graphics_clock_khz = record_field(
        "graphics_clock_khz",
        &clock,
        raw.graphics_khz != u32::MAX,
        raw.timestamps[FIELD_CLOCK],
        &mut unavailable,
        &mut timestamps,
        &mut updated,
    )
    .then_some(raw.graphics_khz);
    let memory_clock_khz = record_field(
        "memory_clock_khz",
        &clock,
        raw.memory_khz != u32::MAX,
        raw.timestamps[FIELD_CLOCK],
        &mut unavailable,
        &mut timestamps,
        &mut updated,
    )
    .then_some(raw.memory_khz);

    let temp = track_section(
        &mut trackers[FIELD_TEMP],
        raw.field_status[FIELD_TEMP],
        raw.timestamps[FIELD_TEMP],
        now_ns,
        stale_ns,
    );
    let temperature_c = record_field(
        "temperature_c",
        &temp,
        (-50 * 256..=150 * 256).contains(&raw.temperature_fx256),
        raw.timestamps[FIELD_TEMP],
        &mut unavailable,
        &mut timestamps,
        &mut updated,
    )
    .then_some(f64::from(raw.temperature_fx256) / 256.0);

    let power = track_section(
        &mut trackers[FIELD_POWER],
        raw.field_status[FIELD_POWER],
        raw.timestamps[FIELD_POWER],
        now_ns,
        stale_ns,
    );
    let power_mw = record_field(
        "power_mw",
        &power,
        raw.power_mw != u32::MAX,
        raw.timestamps[FIELD_POWER],
        &mut unavailable,
        &mut timestamps,
        &mut updated,
    )
    .then_some(raw.power_mw);

    let limit = track_section(
        &mut trackers[FIELD_LIMIT],
        raw.field_status[FIELD_LIMIT],
        raw.timestamps[FIELD_LIMIT],
        now_ns,
        stale_ns,
    );
    let power_limit_mw = record_field(
        "power_limit_mw",
        &limit,
        raw.limit_mw != u32::MAX,
        raw.timestamps[FIELD_LIMIT],
        &mut unavailable,
        &mut timestamps,
        &mut updated,
    )
    .then_some(raw.limit_mw);

    let fan = track_section(
        &mut trackers[FIELD_FAN],
        raw.field_status[FIELD_FAN],
        raw.timestamps[FIELD_FAN],
        now_ns,
        stale_ns,
    );
    let fan_rpm = record_field(
        "fan_rpm",
        &fan,
        raw.fan_rpm != u32::MAX,
        raw.timestamps[FIELD_FAN],
        &mut unavailable,
        &mut timestamps,
        &mut updated,
    )
    .then_some(raw.fan_rpm);

    let util = track_section(
        &mut trackers[FIELD_UTIL],
        raw.field_status[FIELD_UTIL],
        raw.timestamps[FIELD_UTIL],
        now_ns,
        stale_ns,
    );
    let utilization_percent = record_field(
        "utilization_percent",
        &util,
        raw.utilization_percent <= 100,
        raw.timestamps[FIELD_UTIL],
        &mut unavailable,
        &mut timestamps,
        &mut updated,
    )
    .then_some(raw.utilization_percent);

    let pstate = if raw.pstate_status == 0 {
        Some(raw.pstate)
    } else {
        unavailable.insert("pstate".into(), query_error(raw.pstate_status));
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
            rusd_timestamps: timestamps,
            unavailable,
        },
        updated,
    }
}

/// A persistent read-only RM session: the connection chain and the mapped RUSD
/// page are created once and reused for every [`sample`](Self::sample). Drop
/// releases the mapping, the RM objects and both file descriptors.
pub struct MacosRmSession {
    #[cfg(all(target_os = "macos", target_arch = "x86_64"))]
    handle: *mut SessionOpaque,
    info: DeviceInfo,
    trackers: [FieldTracker; RUSD_FIELDS],
    start: Instant,
    stale_after: Duration,
}

impl MacosRmSession {
    pub fn open() -> Result<Self, Error> {
        Self::open_with(MONITOR_STALE_AFTER)
    }

    #[cfg(all(target_os = "macos", target_arch = "x86_64"))]
    fn open_with(stale_after: Duration) -> Result<Self, Error> {
        let mut info = std::mem::MaybeUninit::<Snapshot>::uninit();
        let mut handle: *mut SessionOpaque = std::ptr::null_mut();
        let mut baseline = [0u64; RUSD_FIELDS];
        let mut error = [0u8; 256];
        let status = {
            let _guard = lock_transport()?;
            // SAFETY: C receives correctly sized writable buffers and either
            // fills `info` fully or returns an error with `handle` left null.
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
        if info.abi_version != 2 {
            let mut err = [0u8; 256];
            let _guard = lock_transport()?;
            // SAFETY: the handle came from a successful open and is closed once.
            unsafe { nvoc_macos_rm_session_close(handle, err.as_mut_ptr(), err.len()) };
            return Err(Error::Custom(
                "macOS RM session ABI version mismatch".into(),
            ));
        }
        let device = device_from_snapshot(&info);
        let start = Instant::now();
        let mut session = MacosRmSession {
            handle,
            info: device.info,
            trackers: std::array::from_fn(|i| FieldTracker {
                ts: baseline[i],
                changed_ns: 0,
            }),
            start,
            stale_after,
        };
        session.wait_initial_freshness();
        Ok(session)
    }

    #[cfg(not(all(target_os = "macos", target_arch = "x86_64")))]
    fn open_with(_stale_after: Duration) -> Result<Self, Error> {
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

    /// One coherent, read-only sample. Session-level failures (dead fds, lost
    /// device) return `Err` and require reopening; per-field problems are
    /// reported inside the sample.
    pub fn sample(&mut self) -> Result<DeviceSample, Error> {
        let raw = self.raw_sample()?;
        let now = self.mono_ns();
        Ok(interpret_sample(
            &mut self.trackers,
            &raw,
            now,
            self.stale_after,
        ))
    }

    fn mono_ns(&self) -> u64 {
        self.start.elapsed().as_nanos().min(u128::from(u64::MAX)) as u64
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
        if raw.abi_version != 2 {
            return Err(Error::Custom("macOS RM sample ABI version mismatch".into()));
        }
        Ok(raw)
    }

    #[cfg(not(all(target_os = "macos", target_arch = "x86_64")))]
    fn raw_sample(&mut self) -> Result<RawSample, Error> {
        Err(Error::Custom(
            "macOS RM backend requires x86_64 macOS with the NullMoth driver".into(),
        ))
    }

    /// Best-effort wait for every polled section to advance past the baseline
    /// captured at open, so the first emitted sample is known fresh. The
    /// trackers are left untouched: the advancement is re-observed by the first
    /// real `sample()` call.
    #[cfg(all(target_os = "macos", target_arch = "x86_64"))]
    fn wait_initial_freshness(&mut self) {
        let deadline = Instant::now() + INITIAL_FRESH_BUDGET;
        loop {
            match self.raw_sample() {
                Ok(raw) => {
                    let all_advanced = (0..RUSD_FIELDS).all(|i| {
                        raw.field_status[i] == 0 && raw.timestamps[i] != self.trackers[i].ts
                    });
                    if all_advanced {
                        return;
                    }
                }
                Err(_) => return,
            }
            if Instant::now() >= deadline {
                return;
            }
            std::thread::sleep(Duration::from_millis(100));
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

    fn raw_sample() -> RawSample {
        RawSample {
            abi_version: 2,
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

    #[test]
    fn monitor_keeps_valid_zero_readings_and_marks_the_first_sample_updated() {
        let mut trackers = [FieldTracker::default(); RUSD_FIELDS];
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
