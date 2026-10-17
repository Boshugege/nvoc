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

#[derive(Debug, Clone, Default, Serialize)]
pub struct DeviceStatus {
    pub pstate: Option<u32>,
    /// Actual (effective) clock of each public domain the driver reports.
    pub graphics_clock_khz: Option<u32>,
    pub memory_clock_khz: Option<u32>,
    pub video_clock_khz: Option<u32>,
    pub sm_clock_khz: Option<u32>,
    /// Target clock the driver currently requests for the same domains.
    pub graphics_target_clock_mhz: Option<u32>,
    pub memory_target_clock_mhz: Option<u32>,
    pub video_target_clock_mhz: Option<u32>,
    pub sm_target_clock_mhz: Option<u32>,
    pub temperature_c: Option<f64>,
    pub temperature_memory_c: Option<f64>,
    pub temperature_board_c: Option<f64>,
    pub temperature_power_supply_c: Option<f64>,
    /// Instantaneous GPU (die) power draw in mW.
    pub power_mw: Option<u32>,
    /// Current enforced power limit (the live TGP wall) in mW.
    pub power_limit_mw: Option<u32>,
    /// Power limit the driver was asked for, which clamping may exceed.
    pub power_limit_requested_mw: Option<u32>,
    /// Driver's own average GPU power window, when it publishes one.
    pub average_power_mw: Option<u32>,
    /// Total GPU power from the power-policy block (nvml's "power policy").
    pub power_policy_tgp_mw: Option<u32>,
    /// Raw clkThrottleInfo.reasonMask (cl00de.h RUSD_CLK_THROTTLE_REASON_*).
    pub throttle_reason_mask: Option<u32>,
    /// Decoded names of the active throttle reasons.
    pub throttle_reasons: Vec<String>,
    /// Highest live fan reading in rpm (0 means "stopped", not "unknown").
    pub fan_rpm: Option<u32>,
    /// One entry per live cooler the driver reports.
    pub fan_rpms: Vec<u32>,
    pub utilization_percent: Option<u32>,
    pub memory_utilization_percent: Option<u32>,
    /// Busy percent per engine, keyed the way the driver names them (gr, fb,
    /// nvenc, nvdec, nvjpg, nvofa).
    pub engine_utilization_percent: BTreeMap<String, f64>,
    /// Negotiated / maximum PCI Express generation, decoded from
    /// RUSD_BUS_DATA_PCIE_GEN_INFO.
    pub pcie_link_gen: Option<u32>,
    pub pcie_max_link_gen: Option<u32>,
    /// Video memory in use (FB_GET_INFO_V2 heap size - heap free).
    pub memory_used_bytes: Option<u64>,
    /// RUSD per-section update timestamp, or even sequence for non-polled data.
    pub rusd_timestamps: BTreeMap<String, u64>,
    pub unavailable: BTreeMap<String, String>,
}

/// Capability flags for the settings surface, so a consumer can disable a
/// control for the right reason instead of guessing.
#[derive(Debug, Clone, Copy, Serialize)]
pub struct SettingsCapabilities {
    /// The backend can report the driver's current configuration.
    pub settings_read: bool,
    /// The backend can change configuration. Still false on macOS: no writable
    /// RM control is reachable on this driver (see [`DeviceSettings::notes`]).
    pub settings_write: bool,
    /// The backend can put a saved configuration back. Only meaningful once
    /// `settings_write` is true.
    pub settings_restore: bool,
}

/// One reason string shared by every configuration family the RM channel
/// cannot read on this driver, so the payload cannot drift from the evidence.
const SETTINGS_UNREADABLE_REASON: &str = "no readable RM control on this driver (see notes)";

/// Read-only projection of the driver-visible configuration.
///
/// This deliberately contains only what the RM session actually reports, and
/// names every configuration family it does *not* report with a reason. A
/// missing family is never represented by a default-looking zero.
#[derive(Debug, Clone, Serialize)]
pub struct DeviceSettings {
    pub pstate: Option<u32>,
    /// Current enforced power limit (the live TGP wall) in mW.
    pub power_limit_mw: Option<u32>,
    /// Power limit the driver was asked for, which clamping may exceed.
    pub power_limit_requested_mw: Option<u32>,
    pub pcie_link_gen: Option<u32>,
    pub memory_total_bytes: Option<u64>,
    pub memory_used_bytes: Option<u64>,
    pub board_id: Option<u32>,
    pub chip_sku: Option<String>,
    pub project: Option<String>,
    /// Configuration families this backend cannot read (or write) yet, keyed by
    /// the family name, with the cause.
    pub unavailable: BTreeMap<String, String>,
    pub capabilities: SettingsCapabilities,
    /// Evidence behind the capability claims: what the backend does read, and
    /// which controls were probed with which RM status.
    pub notes: Vec<String>,
}

fn settings_from(info: &DeviceInfo, status: &DeviceStatus) -> DeviceSettings {
    // Every family that would need a read we do not have. The keys are the
    // public vocabulary (`get-settings` / pynvoc), not RM command names.
    let unavailable: BTreeMap<String, String> = [
        "core_clock_offset",
        "memory_clock_offset",
        "voltage_boost",
        "thermal_limit",
        "fan_control",
        "pstate_lock",
        "vf_curve",
        "power_limit_write",
        "settings_restore",
    ]
    .into_iter()
    .map(|family| (family.to_string(), SETTINGS_UNREADABLE_REASON.to_string()))
    .collect();

    DeviceSettings {
        pstate: status.pstate,
        power_limit_mw: status.power_limit_mw,
        power_limit_requested_mw: status.power_limit_requested_mw,
        pcie_link_gen: status.pcie_link_gen,
        memory_total_bytes: info.memory_total_bytes,
        memory_used_bytes: status.memory_used_bytes,
        board_id: info.board_id,
        chip_sku: info.chip_sku.clone(),
        project: info.project.clone(),
        unavailable,
        capabilities: SettingsCapabilities {
            settings_read: true,
            settings_write: false,
            settings_restore: false,
        },
        notes: vec![
            "read: P-state, enforced/requested power limit, PCIe generation, board/SKU and memory sizes come from the same RM snapshot as get-status; sensor readings are separate (see get-status).".into(),
            "write: refused before the driver is opened. No writable RM control is reachable on 610.57.04; setting a limit would also need a verified restore path first.".into(),
            "offsets/locks/VF/fan/thermal policies: NV2080_CTRL_CMD_PERF_GET_LEVEL_INFO_V2 (0x2080200b, exact SDK struct) and the campaign-sourced GETs rails-get-info 0x2080B201, rails-get-status 0x2080B202 and vfpoints-get 0x208090A2 all return NV_ERR_INVALID_ARGUMENT (0x1f) through this transport; EDPp limit info and pmumon clock samples return NV_ERR_NOT_SUPPORTED (0x56).".into(),
        ],
    }
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
        let snapshot = self.refreshed_snapshot()?;
        Ok(status_from_snapshot(&snapshot))
    }

    /// Reads the driver-visible configuration without changing anything.
    ///
    /// Uses the same single RM snapshot as [`Device::status`], so the
    /// configuration readback and the telemetry cannot disagree about the
    /// device, and a device that changed identity behind the session is
    /// rejected instead of being reported under the old name.
    pub fn settings(&self) -> Result<DeviceSettings, Error> {
        let snapshot = self.refreshed_snapshot()?;
        let info = device_from_snapshot(&snapshot).info;
        let status = status_from_snapshot(&snapshot);
        Ok(settings_from(&info, &status))
    }

    /// One fresh snapshot, with the identity re-checked against this handle.
    fn refreshed_snapshot(&self) -> Result<Snapshot, Error> {
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
        Ok(snapshot)
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

/// Read-only configuration query for the common `run` / `run_many` operation API.
#[derive(Debug, Clone, Copy)]
pub struct QueryMacosRmSettings;

impl crate::GpuOperation for QueryMacosRmSettings {
    type Output = DeviceSettings;

    fn kind(&self) -> crate::OperationKind {
        crate::OperationKind::QueryGpuSettings
    }

    fn run(&self, target: &crate::GpuTarget<'_>) -> Result<Self::Output, Error> {
        target.macos_rm()?.settings()
    }
}

/// Version of the private C ABI in core/native/macos_rm_abi.h. Bump both
/// sides together: the C header pins the same number and the hardware test
/// compares the two fingerprints.
const ABI_VERSION: u32 = 3;

/// Slots returned by [`abi_layout`] and by the native
/// nvoc_macos_rm_abi_layout(): sizes, field offsets, the ABI version and the
/// per-field status codes Rust matches on. Keeping them identical is what the
/// C header's `_Static_assert`s and the hardware test enforce.
pub const ABI_LAYOUT_SLOTS: usize = 41;

/// RUSD section indices and value-block sizes, matching macos_rm_abi.h (the
/// header is the source of truth; the layout fingerprint and the compile-time
/// asserts below keep the two sides identical).
const RUSD_FIELDS: usize = 14;
const SEC_CLOCK: usize = 0;
const SEC_TEMP_GPU: usize = 1;
const SEC_TEMP_MEMORY: usize = 2;
const SEC_TEMP_BOARD: usize = 3;
const SEC_TEMP_POWER_SUPPLY: usize = 4;
const SEC_POWER: usize = 5;
const SEC_POWER_LIMIT: usize = 6;
const SEC_AVG_POWER: usize = 7;
const SEC_POWER_POLICY: usize = 8;
const SEC_THROTTLE: usize = 9;
const SEC_FAN: usize = 10;
const SEC_UTIL: usize = 11;
const SEC_PCI: usize = 12;
const SEC_PROC_UTIL: usize = 13;
const CLK_DOMAINS: usize = 4;
const TEMP_SENSORS: usize = 4;
const FAN_MAX_COOLERS: usize = 16;
const ENGINES: usize = 6;
/// cl00de.h NV00DE_RUSD_POLL_* mask the native layer requests.
const POLL_MASK: u32 = 0xFB;

/// Field status codes reported by the native side (macos_rm_abi.h).
const FIELD_OK: u32 = 0;
const FIELD_UNSTABLE: u32 = 1;
const FIELD_UNAVAILABLE: u32 = 2;
const FIELD_NOT_FRESH: u32 = 3;

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
    sample: RawSample,
}

/// Raw per-field telemetry exactly as produced by the native layer: one
/// section status + timestamp table plus the value blocks. Every field is a
/// plain integer so the C/Rust layouts can be size-checked, and the driver's
/// own units are preserved (kHz, MHz, mW, 8.8 fixed-point C, percent x100).
#[repr(C)]
#[derive(Default, Clone, Copy)]
struct RawSample {
    abi_version: u32,
    field_status: [u32; RUSD_FIELDS],
    timestamps: [u64; RUSD_FIELDS],
    clk_target_mhz: [u32; CLK_DOMAINS],
    clk_actual_khz: [u32; CLK_DOMAINS],
    temperature_fx256: [i32; TEMP_SENSORS],
    power_mw: u32,
    power_limit_requested_mw: u32,
    power_limit_mw: u32,
    average_power_mw: u32,
    power_policy_tgp_mw: u32,
    throttle_reason_mask: u32,
    fan_rpm: [u32; FAN_MAX_COOLERS],
    utilization_percent: u32,
    memory_utilization_percent: u32,
    engine_utilization_pct100: [u32; ENGINES],
    pci_gen_info: u32,
    pstate: u32,
    pstate_status: u32,
    memory_used_kb: u32,
    memory_used_status: u32,
}

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
    interpret_oneshot(&s.telemetry.sample, s.telemetry.status)
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

/// The Rust half of the C/Rust layout fingerprint. Slot meanings match
/// NVOC_RM_LAYOUT_SLOTS in core/native/macos_rm_abi.h and the assertions in
/// core/tests/macos_rm_abi.c; `native_abi_layout()` returns the C half.
pub fn abi_layout() -> [u64; ABI_LAYOUT_SLOTS] {
    [
        std::mem::size_of::<Snapshot>() as u64,
        std::mem::size_of::<RawSample>() as u64,
        std::mem::size_of::<Telemetry>() as u64,
        std::mem::offset_of!(Snapshot, telemetry) as u64,
        std::mem::offset_of!(Snapshot, driver) as u64,
        std::mem::offset_of!(Telemetry, sample) as u64,
        std::mem::offset_of!(RawSample, timestamps) as u64,
        std::mem::offset_of!(RawSample, temperature_fx256) as u64,
        std::mem::offset_of!(RawSample, clk_target_mhz) as u64,
        std::mem::offset_of!(RawSample, clk_actual_khz) as u64,
        std::mem::offset_of!(RawSample, power_mw) as u64,
        std::mem::offset_of!(RawSample, fan_rpm) as u64,
        std::mem::offset_of!(RawSample, engine_utilization_pct100) as u64,
        std::mem::offset_of!(RawSample, pci_gen_info) as u64,
        std::mem::offset_of!(RawSample, pstate) as u64,
        std::mem::offset_of!(RawSample, memory_used_kb) as u64,
        ABI_VERSION as u64,
        RUSD_FIELDS as u64,
        FIELD_OK as u64,
        FIELD_UNSTABLE as u64,
        FIELD_UNAVAILABLE as u64,
        FIELD_NOT_FRESH as u64,
        CLK_DOMAINS as u64,
        TEMP_SENSORS as u64,
        FAN_MAX_COOLERS as u64,
        ENGINES as u64,
        POLL_MASK as u64,
        SEC_CLOCK as u64,
        SEC_TEMP_GPU as u64,
        SEC_TEMP_MEMORY as u64,
        SEC_TEMP_BOARD as u64,
        SEC_TEMP_POWER_SUPPLY as u64,
        SEC_POWER as u64,
        SEC_POWER_LIMIT as u64,
        SEC_AVG_POWER as u64,
        SEC_POWER_POLICY as u64,
        SEC_THROTTLE as u64,
        SEC_FAN as u64,
        SEC_UTIL as u64,
        SEC_PCI as u64,
        SEC_PROC_UTIL as u64,
    ]
}

/// Compile-time mirror of the `_Static_assert`s in macos_rm_abi.h. A layout
/// change on either side fails the build instead of silently mis-reading
/// driver memory.
const _: () = {
    assert!(std::mem::size_of::<Snapshot>() == 648);
    assert!(std::mem::size_of::<RawSample>() == 368);
    assert!(std::mem::size_of::<Telemetry>() == 376);
    assert!(std::mem::offset_of!(Snapshot, telemetry) == 272);
    assert!(std::mem::offset_of!(Snapshot, driver) == 108);
    assert!(std::mem::offset_of!(Telemetry, sample) == 8);
    assert!(std::mem::offset_of!(RawSample, timestamps) == 64);
    assert!(std::mem::offset_of!(RawSample, clk_target_mhz) == 176);
    assert!(std::mem::offset_of!(RawSample, clk_actual_khz) == 192);
    assert!(std::mem::offset_of!(RawSample, temperature_fx256) == 208);
    assert!(std::mem::offset_of!(RawSample, power_mw) == 224);
    assert!(std::mem::offset_of!(RawSample, fan_rpm) == 248);
    assert!(std::mem::offset_of!(RawSample, utilization_percent) == 312);
    assert!(std::mem::offset_of!(RawSample, engine_utilization_pct100) == 320);
    assert!(std::mem::offset_of!(RawSample, pci_gen_info) == 344);
    assert!(std::mem::offset_of!(RawSample, pstate) == 348);
    assert!(std::mem::offset_of!(RawSample, memory_used_kb) == 356);
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
    /// Whole-subsystem reason (the RUSD page could not be allocated/mapped)
    /// that replaces the per-section text when set.
    override_reason: Option<String>,
}

/// Which wait produced a `NeverRefreshed` verdict, so the reason text names the
/// right budget: the one-shot query waits once before answering, the monitor
/// keeps waiting for the whole session.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FreshnessScope {
    InitialQuery,
    Session,
}

impl SectionCheck {
    fn reason(&self, stale_ms: u64, scope: FreshnessScope) -> String {
        if let Some(reason) = &self.override_reason {
            return reason.clone();
        }
        match self.state {
            FieldState::Unsupported => {
                "RUSD section has no valid timestamp (driver does not publish it)".into()
            }
            FieldState::Unstable => {
                "RUSD section changed while it was copied (unstable sequence)".into()
            }
            FieldState::NeverRefreshed => match scope {
                FreshnessScope::InitialQuery => {
                    "no RUSD update within the initial freshness budget".into()
                }
                FreshnessScope::Session => format!(
                    "no RUSD update since this session opened ({} ms)",
                    self.silent_ms
                ),
            },
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
                override_reason: None,
            };
        }
        _ => {
            return SectionCheck {
                state: FieldState::Unstable,
                silent_ms: silent_ns / 1_000_000,
                epoch_reset: false,
                override_reason: None,
            };
        }
    }
    if ts == 0 {
        return SectionCheck {
            state: FieldState::Unsupported,
            silent_ms: silent_ns / 1_000_000,
            epoch_reset: false,
            override_reason: None,
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
            override_reason: None,
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
        override_reason: None,
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
        value: ValueCheck,
        stale_ms: u64,
        scope: FreshnessScope,
    ) -> bool {
        self.freshness.insert(name.into(), check.state);
        if check.epoch_reset {
            self.epoch_resets.push(name.into());
        }
        if !check.state.publishes_value() {
            self.unavailable
                .insert(name.into(), check.reason(stale_ms, scope));
            return false;
        }
        if let Some(reason) = value.reason() {
            self.unavailable.insert(name.into(), reason.into());
            return false;
        }
        self.timestamps.insert(name.into(), ts);
        self.updated.insert(name.into(), check.state.is_advancing());
        true
    }

    /// Note a section's advancement for the whole-page "went silent" health
    /// check (the clock section feeds eight value fields but is one section).
    fn note_section(&mut self, check: &SectionCheck) {
        if check.state.is_advancing() {
            self.advancing_sections += 1;
        }
    }

    /// Record a field that comes from a live RM control rather than a RUSD
    /// section: it is new whenever its query succeeded.
    fn publish_live(&mut self, name: &str, status: u32) -> bool {
        if status == 0 {
            self.freshness.insert(name.into(), FieldState::Advancing);
            self.updated.insert(name.into(), true);
            true
        } else {
            self.freshness.insert(name.into(), FieldState::Unsupported);
            self.unavailable.insert(name.into(), query_error(status));
            false
        }
    }
}

/// Whether a present value may be published. Separated from the freshness
/// check because "the driver does not report this" and "the driver reported
/// something impossible" are different conditions.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ValueCheck {
    Ok,
    /// Out of the plausible range for the field, e.g. a fan at 294 118 rpm
    /// from an unpopulated RUSD slot. Never published as a measurement.
    Invalid,
    /// The driver reports no value for this sub-field (a clock domain it does
    /// not implement, a temperature sensor with no sensor fitted).
    NotReported,
}

impl ValueCheck {
    fn reason(self) -> Option<&'static str> {
        match self {
            ValueCheck::Ok => None,
            ValueCheck::Invalid => Some("RUSD field reported an implausible value"),
            ValueCheck::NotReported => Some("driver reports no value for this field"),
        }
    }
}

/// Plausibility bounds. A value outside these is treated as "the driver
/// published garbage" instead of being shown as a reading: the fan array in
/// particular carries unpopulated slots that read as huge rpm counts.
const MAX_FAN_RPM: u32 = 20_000;
const MAX_CLOCK_KHZ: u32 = 20_000_000; // 20 GHz, far above any real GPU clock
const MAX_TARGET_MHZ: u32 = 20_000;
const MAX_POWER_MW: u32 = 2_000_000; // 2 kW
const TEMP_MIN_FX256: i32 = -50 * 256;
const TEMP_MAX_FX256: i32 = 150 * 256;

fn temperature_sane(fx256: i32) -> bool {
    (TEMP_MIN_FX256..=TEMP_MAX_FX256).contains(&fx256)
}

/// cl00de.h RUSD_CLK_THROTTLE_REASON_*, bit order preserved.
const THROTTLE_REASONS: [(u32, &str); 9] = [
    (1 << 0, "gpu_idle"),
    (1 << 1, "application_clock_setting"),
    (1 << 2, "sw_power_cap"),
    (1 << 3, "hw_slowdown"),
    (1 << 4, "sync_boost"),
    (1 << 5, "sw_thermal_slowdown"),
    (1 << 6, "hw_thermal_slowdown"),
    (1 << 7, "hw_power_brakes_slowdown"),
    (1 << 8, "display_clock_setting"),
];

fn decode_throttle_reasons(mask: u32) -> Vec<String> {
    THROTTLE_REASONS
        .iter()
        .filter(|(bit, _)| mask & bit != 0)
        .map(|(_, name)| (*name).to_owned())
        .collect()
}

/// Human-facing throttle-reason view for the dedicated surface: the full
/// nine-reason vocabulary in the same order NVML reports it, each paired with
/// its active flag decoded from the RUSD `clkThrottleInfo.reasonMask`.
///
/// [`decode_throttle_reasons`] keeps the raw snake_case tags the status payload
/// has always carried; this is the NVML-compatible twin so `get-throttle-reasons`
/// returns the same rows on macOS that it does on NVML/NVAPI. A `None` mask
/// (throttle section unavailable) reports every reason as inactive rather than
/// inventing an active one.
pub fn throttle_reason_states(mask: Option<u32>) -> Vec<(&'static str, bool)> {
    const DISPLAY_NAMES: [&str; 9] = [
        "GPU Idle",
        "App Clock Setting",
        "SW Power Cap",
        "HW Slowdown",
        "Sync Boost",
        "SW Thermal",
        "HW Thermal",
        "HW Power Brake",
        "Display Clock",
    ];
    let mask = mask.unwrap_or(0);
    THROTTLE_REASONS
        .iter()
        .zip(DISPLAY_NAMES)
        .map(|((bit, _), name)| (name, mask & *bit != 0))
        .collect()
}

/// Engine names in the order NVOC_RM_ENGINES carries them (gr, fb, nvenc,
/// nvdec, nvjpg, nvofa), matching RUSD_PROC_UTIL's own field names.
const ENGINE_NAMES: [&str; ENGINES] = ["gr", "fb", "nvenc", "nvdec", "nvjpg", "nvofa"];

/// RUSD_BUS_DATA_PCIE_GEN_INFO bit fields, mirroring
/// NV2080_CTRL_BUS_INFO_PCIE_LINK_CAP_* in ctrl2080bus.h.
const PCIE_GEN_INFO_GEN_SHIFT: u32 = 12;
const PCIE_GEN_INFO_CURR_LEVEL_SHIFT: u32 = 16;

/// Decode the PCIe generation fields. `None` means the driver left the whole
/// word at 0, i.e. it does not report the link at all.
fn decode_pci_gen_info(info: u32) -> (Option<u32>, Option<u32>) {
    if info == 0 {
        return (None, None);
    }
    let capable_gen = (info >> PCIE_GEN_INFO_GEN_SHIFT) & 0xf;
    let current = (info >> PCIE_GEN_INFO_CURR_LEVEL_SHIFT) & 0xf;
    (Some(current + 1), Some(capable_gen + 1))
}

/// Turn one raw sample into the published status. `checks[i]` is section i's
/// verdict; the same function serves the one-shot query (verdicts derived
/// directly from the native field statuses) and the continuous monitor
/// (verdicts from per-section advancement tracking).
fn publish_all(
    fields: &mut SampleFields,
    raw: &RawSample,
    checks: &[SectionCheck; RUSD_FIELDS],
    scope: FreshnessScope,
    stale_ms: u64,
) -> DeviceStatus {
    // ---- CLOCK: four public domains, one section -------------------------
    let clock = &checks[SEC_CLOCK];
    fields.note_section(clock);
    let clock_ts = raw.timestamps[SEC_CLOCK];
    let mut actual_khz = [None; CLK_DOMAINS];
    let mut target_mhz = [None; CLK_DOMAINS];
    for (domain, index) in [("graphics", 0usize), ("memory", 1), ("video", 2), ("sm", 3)] {
        let actual = raw.clk_actual_khz[index];
        let target = raw.clk_target_mhz[index];
        // A domain the driver does not implement reports 0/0. Publishing that
        // as "0 MHz" would look like a stopped clock.
        let value = if actual == 0 && target == 0 {
            ValueCheck::NotReported
        } else if actual > MAX_CLOCK_KHZ || target > MAX_TARGET_MHZ {
            ValueCheck::Invalid
        } else {
            ValueCheck::Ok
        };
        let name = format!("{domain}_clock_khz");
        if fields.publish(&name, clock, clock_ts, value, stale_ms, scope) {
            actual_khz[index] = Some(actual);
        }
        let name = format!("{domain}_target_clock_mhz");
        if fields.publish(&name, clock, clock_ts, value, stale_ms, scope) {
            target_mhz[index] = Some(target);
        }
    }

    // ---- temperatures: one section per sensor ----------------------------
    let mut temperature = [None; TEMP_SENSORS];
    for (index, (section, name)) in [
        (SEC_TEMP_GPU, "temperature_c"),
        (SEC_TEMP_MEMORY, "temperature_memory_c"),
        (SEC_TEMP_BOARD, "temperature_board_c"),
        (SEC_TEMP_POWER_SUPPLY, "temperature_power_supply_c"),
    ]
    .into_iter()
    .enumerate()
    {
        let check = &checks[section];
        fields.note_section(check);
        let fx256 = raw.temperature_fx256[index];
        let value = if temperature_sane(fx256) {
            ValueCheck::Ok
        } else {
            ValueCheck::Invalid
        };
        if fields.publish(name, check, raw.timestamps[section], value, stale_ms, scope) {
            temperature[index] = Some(f64::from(fx256) / 256.0);
        }
    }

    // ---- power block ------------------------------------------------------
    let power = &checks[SEC_POWER];
    fields.note_section(power);
    let power_value = |value: u32| {
        if value <= MAX_POWER_MW {
            ValueCheck::Ok
        } else {
            ValueCheck::Invalid
        }
    };
    let power_mw = fields
        .publish(
            "power_mw",
            power,
            raw.timestamps[SEC_POWER],
            power_value(raw.power_mw),
            stale_ms,
            scope,
        )
        .then_some(raw.power_mw);

    let limit = &checks[SEC_POWER_LIMIT];
    fields.note_section(limit);
    let power_limit_mw = fields
        .publish(
            "power_limit_mw",
            limit,
            raw.timestamps[SEC_POWER_LIMIT],
            power_value(raw.power_limit_mw),
            stale_ms,
            scope,
        )
        .then_some(raw.power_limit_mw);
    let power_limit_requested_mw = fields
        .publish(
            "power_limit_requested_mw",
            limit,
            raw.timestamps[SEC_POWER_LIMIT],
            power_value(raw.power_limit_requested_mw),
            stale_ms,
            scope,
        )
        .then_some(raw.power_limit_requested_mw);

    let average = &checks[SEC_AVG_POWER];
    fields.note_section(average);
    let average_power_mw = fields
        .publish(
            "average_power_mw",
            average,
            raw.timestamps[SEC_AVG_POWER],
            power_value(raw.average_power_mw),
            stale_ms,
            scope,
        )
        .then_some(raw.average_power_mw);

    let policy = &checks[SEC_POWER_POLICY];
    fields.note_section(policy);
    let power_policy_tgp_mw = fields
        .publish(
            "power_policy_tgp_mw",
            policy,
            raw.timestamps[SEC_POWER_POLICY],
            power_value(raw.power_policy_tgp_mw),
            stale_ms,
            scope,
        )
        .then_some(raw.power_policy_tgp_mw);

    // ---- clock throttle reasons -----------------------------------------
    let throttle = &checks[SEC_THROTTLE];
    fields.note_section(throttle);
    let throttle_reason_mask = fields
        .publish(
            "throttle_reason_mask",
            throttle,
            raw.timestamps[SEC_THROTTLE],
            ValueCheck::Ok,
            stale_ms,
            scope,
        )
        .then_some(raw.throttle_reason_mask);

    // ---- fans: one section, 16 slots the driver may leave unpopulated ----
    let fan = &checks[SEC_FAN];
    fields.note_section(fan);
    let fan_ts = raw.timestamps[SEC_FAN];
    let mut fan_rpms: Vec<u32> = Vec::new();
    let mut implausible = false;
    for rpm in raw.fan_rpm.iter().copied() {
        if rpm == 0 {
            continue; // an empty cooler slot, or a stopped fan
        }
        if rpm > MAX_FAN_RPM {
            implausible = true;
            continue;
        }
        fan_rpms.push(rpm);
    }
    // Every non-zero slot was nonsense: the section itself is not usable.
    let fan_value = if implausible && fan_rpms.is_empty() {
        ValueCheck::Invalid
    } else {
        ValueCheck::Ok
    };
    let fan_max = fan_rpms.iter().copied().max().unwrap_or(0);
    let fan_rpm = fields
        .publish("fan_rpm", fan, fan_ts, fan_value, stale_ms, scope)
        .then_some(fan_max);
    let fan_rpms = fields
        .publish("fan_rpms", fan, fan_ts, fan_value, stale_ms, scope)
        .then_some(fan_rpms);

    // ---- device utilization ----------------------------------------------
    let util = &checks[SEC_UTIL];
    fields.note_section(util);
    let percent = |value: u32| {
        if value <= 100 {
            ValueCheck::Ok
        } else {
            ValueCheck::Invalid
        }
    };
    let utilization_percent = fields
        .publish(
            "utilization_percent",
            util,
            raw.timestamps[SEC_UTIL],
            percent(raw.utilization_percent),
            stale_ms,
            scope,
        )
        .then_some(raw.utilization_percent);
    let memory_utilization_percent = fields
        .publish(
            "memory_utilization_percent",
            util,
            raw.timestamps[SEC_UTIL],
            percent(raw.memory_utilization_percent),
            stale_ms,
            scope,
        )
        .then_some(raw.memory_utilization_percent);

    // ---- per-engine utilization (procUtil, percent x100) -----------------
    let proc = &checks[SEC_PROC_UTIL];
    fields.note_section(proc);
    let engine_sane = raw
        .engine_utilization_pct100
        .iter()
        .all(|pct100| *pct100 <= 100 * 100);
    let mut engine_utilization_percent = BTreeMap::new();
    if fields.publish(
        "engine_utilization_percent",
        proc,
        raw.timestamps[SEC_PROC_UTIL],
        if engine_sane {
            ValueCheck::Ok
        } else {
            ValueCheck::Invalid
        },
        stale_ms,
        scope,
    ) {
        for (name, pct100) in ENGINE_NAMES.iter().zip(raw.engine_utilization_pct100) {
            engine_utilization_percent.insert((*name).to_owned(), f64::from(pct100) / 100.0);
        }
    }

    // ---- PCIe link --------------------------------------------------------
    let pci = &checks[SEC_PCI];
    fields.note_section(pci);
    let (pcie_link_gen, pcie_max_link_gen) = decode_pci_gen_info(raw.pci_gen_info);
    let pci_value = if pcie_link_gen.is_some() {
        ValueCheck::Ok
    } else {
        ValueCheck::NotReported
    };
    let pcie_link_gen = fields
        .publish(
            "pcie_link_gen",
            pci,
            raw.timestamps[SEC_PCI],
            pci_value,
            stale_ms,
            scope,
        )
        .then_some(pcie_link_gen)
        .flatten();
    let pcie_max_link_gen = fields
        .publish(
            "pcie_max_link_gen",
            pci,
            raw.timestamps[SEC_PCI],
            pci_value,
            stale_ms,
            scope,
        )
        .then_some(pcie_max_link_gen)
        .flatten();

    // ---- live RM controls (not RUSD sections) ----------------------------
    let pstate = fields
        .publish_live("pstate", raw.pstate_status)
        .then_some(raw.pstate);
    let memory_used_bytes = fields
        .publish_live("memory_used_bytes", raw.memory_used_status)
        .then_some(u64::from(raw.memory_used_kb) * 1024);

    DeviceStatus {
        pstate,
        graphics_clock_khz: actual_khz[0],
        memory_clock_khz: actual_khz[1],
        video_clock_khz: actual_khz[2],
        sm_clock_khz: actual_khz[3],
        graphics_target_clock_mhz: target_mhz[0],
        memory_target_clock_mhz: target_mhz[1],
        video_target_clock_mhz: target_mhz[2],
        sm_target_clock_mhz: target_mhz[3],
        temperature_c: temperature[0],
        temperature_memory_c: temperature[1],
        temperature_board_c: temperature[2],
        temperature_power_supply_c: temperature[3],
        power_mw,
        power_limit_mw,
        power_limit_requested_mw,
        average_power_mw,
        power_policy_tgp_mw,
        throttle_reason_mask,
        throttle_reasons: throttle_reason_mask
            .map(decode_throttle_reasons)
            .unwrap_or_default(),
        fan_rpm,
        fan_rpms: fan_rpms.unwrap_or_default(),
        utilization_percent,
        memory_utilization_percent,
        engine_utilization_percent,
        pcie_link_gen,
        pcie_max_link_gen,
        memory_used_bytes,
        rusd_timestamps: std::mem::take(&mut fields.timestamps),
        unavailable: std::mem::take(&mut fields.unavailable),
    }
}

/// One-shot interpretation: the native side already resolved freshness while
/// waiting for the initial sample, so its per-section status maps directly to
/// a verdict.
fn interpret_oneshot(raw: &RawSample, map_status: u32) -> DeviceStatus {
    let mut fields = SampleFields::default();
    let override_reason =
        (map_status != 0).then(|| format!("RUSD allocation/mapping failed: 0x{map_status:08x}"));
    let checks = std::array::from_fn(|index| SectionCheck {
        state: match raw.field_status[index] {
            // A section the native layer called readable must carry the stamp
            // it was read under; no stamp means nothing usable was captured.
            FIELD_OK if map_status == 0 && raw.timestamps[index] != 0 => FieldState::Advancing,
            FIELD_OK => FieldState::Unsupported,
            FIELD_UNAVAILABLE => FieldState::Unsupported,
            FIELD_NOT_FRESH => FieldState::NeverRefreshed,
            _ => FieldState::Unstable,
        },
        silent_ms: 0,
        epoch_reset: false,
        override_reason: override_reason.clone(),
    });
    publish_all(&mut fields, raw, &checks, FreshnessScope::InitialQuery, 0)
}

/// Interpret one raw native sample for the continuous monitor. Pure and
/// deterministic so the freshness, staleness, zero-value, epoch-reset,
/// plausibility and partial-missing rules are CPU testable.
fn interpret_sample(
    trackers: &mut [FieldTracker; RUSD_FIELDS],
    raw: &RawSample,
    now_ns: u64,
    stale_after: Duration,
) -> DeviceSample {
    let stale_ns = u64::try_from(stale_after.as_nanos()).unwrap_or(u64::MAX);
    let stale_ms = stale_ns / 1_000_000;
    let mut fields = SampleFields::default();
    let checks = std::array::from_fn(|index| {
        track_section(
            &mut trackers[index],
            raw.field_status[index],
            raw.timestamps[index],
            now_ns,
            stale_ns,
        )
    });
    let status = publish_all(&mut fields, raw, &checks, FreshnessScope::Session, stale_ms);
    DeviceSample {
        status,
        updated: fields.updated,
        freshness: fields.freshness,
        epoch_reset: fields.epoch_resets,
        advancing_sections: fields.advancing_sections,
    }
}

/// RUSD sections in the order reported by [`MacosRmSession::initial_settled`]
/// and by the native field indices.
pub const MONITOR_SECTIONS: [&str; RUSD_FIELDS] = [
    "clock",
    "temperature_gpu",
    "temperature_memory",
    "temperature_board",
    "temperature_power_supply",
    "power",
    "power_limit",
    "average_power",
    "power_policy",
    "throttle",
    "fan",
    "utilization",
    "pci",
    "proc_util",
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
    /// Per section: did the open wait reach a final verdict? True means the
    /// section published a new value inside the budget, or the driver reports
    /// it as unpublished (there is nothing to wait for). A `false` entry is a
    /// polled section that had not produced a new value yet, so the first
    /// samples report it as `never_refreshed` instead of showing old data.
    initial_settled: [bool; RUSD_FIELDS],
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
    /// [`initial_settled`](Self::initial_settled) says which ones are still
    /// waiting.
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
            initial_settled: [false; RUSD_FIELDS],
            silent_samples: 0,
        };
        session.initial_settled = session.wait_initial_settled(initial_budget, cancelled);
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

    /// Per section (see [`MONITOR_SECTIONS`]): did the open wait reach a final
    /// verdict for it? `false` means the section is polled but had not
    /// published a new value inside the budget, so the first samples report it
    /// as `never_refreshed` instead of showing old data as fresh. Sections the
    /// driver does not publish settle immediately: there is nothing to wait
    /// for, and they are reported as `unsupported`, not as a pending wait.
    pub fn initial_settled(&self) -> [bool; RUSD_FIELDS] {
        self.initial_settled
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
    /// mapping-time baseline, so the first emitted sample is known new. A
    /// section the driver reports as unavailable settles at once: it is not
    /// "about to publish", it simply has no data on this platform. The
    /// trackers are deliberately left untouched: the first real `sample()`
    /// re-observes the same advancement and reports it as `advancing`.
    #[cfg(all(target_os = "macos", target_arch = "x86_64"))]
    fn wait_initial_settled(
        &mut self,
        budget: Duration,
        cancelled: &dyn Fn() -> bool,
    ) -> [bool; RUSD_FIELDS] {
        let mut settled = [false; RUSD_FIELDS];
        let deadline = Instant::now() + budget;
        loop {
            if cancelled() {
                return settled;
            }
            match self.raw_sample() {
                Ok(raw) => {
                    for (index, seen) in settled.iter_mut().enumerate() {
                        // Either the driver has no data for this section at all,
                        // or it published a timestamp this session has not seen
                        // yet. Both mean the section is settled for this open.
                        let unavailable = raw.field_status[index] == FIELD_UNAVAILABLE;
                        let advanced = raw.field_status[index] == FIELD_OK
                            && raw.timestamps[index] != 0
                            && raw.timestamps[index] != self.trackers[index].ts;
                        if unavailable || advanced {
                            *seen = true;
                        }
                    }
                }
                // A session-level failure is reported by the first real sample
                // and handled by the caller's reconnect path; do not mask it.
                Err(_) => return settled,
            }
            if settled.iter().all(|seen| *seen) || Instant::now() >= deadline {
                return settled;
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

    /// The settings projection may only carry what the RM session reports, and
    /// every family it cannot read must be named in `unavailable` with writes
    /// and restore reported as unavailable.
    #[test]
    fn settings_projection_reports_reads_and_gaps() {
        let info = DeviceInfo {
            name: Some("NVIDIA GeForce GTX 1650 SUPER".into()),
            driver_version: "610.57.04".into(),
            rm_gpu_id: 0x100,
            pci_address: "0000:01:00.0".into(),
            vendor_id: 0x10DE,
            device_id: 0x2187,
            subsystem_id: Some(0x4018_0062),
            revision: Some(0xA1),
            board_id: Some(376),
            chip_sku: Some("250".into()),
            project: Some("G166".into()),
            memory_total_bytes: Some(4_294_967_296),
            memory_usable_bytes: Some(4_075_880_448),
            bar1_bytes: Some(268_435_456),
            l2_cache_bytes: Some(1_048_576),
            supported_class_count: Some(95),
            unavailable: BTreeMap::new(),
        };
        let mut status = DeviceStatus {
            pstate: Some(8),
            graphics_clock_khz: Some(1_530_000),
            memory_clock_khz: Some(6_000_984),
            video_clock_khz: Some(1_410_000),
            sm_clock_khz: None,
            graphics_target_clock_mhz: Some(1530),
            memory_target_clock_mhz: Some(6000),
            video_target_clock_mhz: Some(1410),
            sm_target_clock_mhz: None,
            temperature_c: Some(44.125),
            temperature_memory_c: None,
            temperature_board_c: None,
            temperature_power_supply_c: None,
            power_mw: Some(11_559),
            power_limit_mw: Some(100_000),
            power_limit_requested_mw: Some(100_000),
            average_power_mw: None,
            power_policy_tgp_mw: Some(11_426),
            throttle_reason_mask: Some(0),
            throttle_reasons: Vec::new(),
            fan_rpm: Some(0),
            fan_rpms: Vec::new(),
            utilization_percent: Some(0),
            memory_utilization_percent: Some(0),
            engine_utilization_percent: BTreeMap::new(),
            pcie_link_gen: Some(3),
            pcie_max_link_gen: Some(3),
            memory_used_bytes: Some(209_649_664),
            rusd_timestamps: BTreeMap::new(),
            unavailable: BTreeMap::new(),
        };
        status.pstate = Some(1);

        let settings = settings_from(&info, &status);
        assert_eq!(settings.pstate, Some(1));
        assert_eq!(settings.power_limit_mw, Some(100_000));
        assert_eq!(settings.memory_total_bytes, Some(4_294_967_296));
        assert_eq!(settings.memory_used_bytes, Some(209_649_664));
        assert_eq!(settings.board_id, Some(376));
        assert_eq!(settings.chip_sku.as_deref(), Some("250"));
        assert!(settings.capabilities.settings_read);
        assert!(!settings.capabilities.settings_write);
        assert!(!settings.capabilities.settings_restore);
        for family in [
            "core_clock_offset",
            "memory_clock_offset",
            "voltage_boost",
            "thermal_limit",
            "fan_control",
            "pstate_lock",
            "vf_curve",
            "power_limit_write",
            "settings_restore",
        ] {
            assert!(
                settings.unavailable.contains_key(family),
                "missing gap entry for {family}"
            );
        }
        // The gap reason must point at the recorded probe evidence.
        assert!(
            settings
                .notes
                .iter()
                .any(|note| note.contains("0x2080B201"))
        );
        assert!(settings.notes.iter().any(|note| note.contains("0x1f")));
    }

    /// A raw sample in which every section is readable and carries the same
    /// stamps. Tests move the stamps to model advancing, quiet and resetting
    /// sections, and overwrite values to model missing or implausible readings.
    fn raw_sample() -> RawSample {
        RawSample {
            abi_version: ABI_VERSION,
            timestamps: [1_000; RUSD_FIELDS],
            clk_target_mhz: [1_530, 6_000, 1_410, 1_800],
            clk_actual_khz: [1_530_000, 6_000_984, 1_410_000, 1_800_000],
            temperature_fx256: [12_800, 12_544, 11_776, 11_520], // 50/49/46/45 C
            power_mw: 11_412,
            power_limit_requested_mw: 100_000,
            power_limit_mw: 100_000,
            average_power_mw: 11_000,
            power_policy_tgp_mw: 11_500,
            throttle_reason_mask: 0,
            fan_rpm: [1_200, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0],
            utilization_percent: 7,
            memory_utilization_percent: 3,
            engine_utilization_pct100: [700, 300, 0, 0, 0, 0],
            pci_gen_info: 0x0022_2000, // ASPM L0S+L1, gen/curr/gpu_gen = Gen3
            pstate: 1,
            memory_used_kb: 200_000,
            ..RawSample::default()
        }
    }

    /// Fields this sample marked as new, in BTreeMap (sorted) order.
    fn updated_fields(sample: &DeviceSample) -> Vec<&str> {
        sample
            .updated
            .iter()
            .filter(|(_, updated)| **updated)
            .map(|(name, _)| name.as_str())
            .collect()
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
        // A zeroed section table means "nothing was read", never "0 MHz".
        assert_eq!(value["pstate"], 0);
        assert!(value["graphics_clock_khz"].is_null());
        assert!(value["temperature_c"].is_null());
        assert!(value["unavailable"]["power_mw"].is_string());
        s.telemetry.sample.pstate_status = 0x56;
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
        s.telemetry.sample = raw_sample();
        s.telemetry.sample.temperature_fx256[0] = -128; // -0.5 C
        let status = status_from_snapshot(&s);
        assert_eq!(status.graphics_clock_khz, Some(1_530_000));
        assert_eq!(status.memory_clock_khz, Some(6_000_984));
        assert_eq!(status.video_clock_khz, Some(1_410_000));
        assert_eq!(status.sm_clock_khz, Some(1_800_000));
        assert_eq!(status.temperature_c, Some(-0.5));
        assert_eq!(status.temperature_memory_c, Some(49.0));
        assert_eq!(status.power_mw, Some(11_412));
        assert_eq!(status.fan_rpm, Some(1_200));
        assert_eq!(status.utilization_percent, Some(7));
        assert_eq!(status.memory_used_bytes, Some(200_000 * 1024));
        assert!(status.unavailable.is_empty());
        assert_eq!(status.rusd_timestamps["temperature_c"], 1_000);
    }

    #[test]
    fn zero_valued_and_implausible_fields_are_reported_not_measured() {
        let mut s = snapshot();
        s.telemetry.sample = raw_sample();
        // SM/board clocks a driver does not implement read back as 0/0.
        s.telemetry.sample.clk_target_mhz[3] = 0;
        s.telemetry.sample.clk_actual_khz[3] = 0;
        // A fan slot the driver never populated reads as an absurd rpm count.
        s.telemetry.sample.fan_rpm = [294_118, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0];
        // An impossible temperature and an over-100 utilization are rejected.
        s.telemetry.sample.temperature_fx256[1] = 1_000 * 256;
        s.telemetry.sample.utilization_percent = 101;
        let status = status_from_snapshot(&s);
        assert!(status.sm_clock_khz.is_none());
        assert!(status.sm_target_clock_mhz.is_none());
        assert!(
            status.unavailable["sm_clock_khz"].contains("reports no value"),
            "{:?}",
            status.unavailable
        );
        assert!(status.fan_rpm.is_none());
        assert!(status.fan_rpms.is_empty());
        assert!(status.unavailable["fan_rpm"].contains("implausible"));
        assert!(status.temperature_memory_c.is_none());
        assert!(status.unavailable["temperature_memory_c"].contains("implausible"));
        assert!(status.utilization_percent.is_none());
        assert!(status.unavailable["utilization_percent"].contains("implausible"));
        // Fields that were fine are untouched.
        assert_eq!(status.graphics_clock_khz, Some(1_530_000));
        assert_eq!(status.temperature_c, Some(50.0));
    }

    #[test]
    fn throttle_reasons_and_pcie_generation_are_decoded() {
        let mut s = snapshot();
        s.telemetry.sample = raw_sample();
        s.telemetry.sample.throttle_reason_mask = (1 << 0) | (1 << 2) | (1 << 6);
        let status = status_from_snapshot(&s);
        assert_eq!(status.throttle_reason_mask, Some(0b1000101));
        assert_eq!(
            status.throttle_reasons,
            ["gpu_idle", "sw_power_cap", "hw_thermal_slowdown"]
        );
        assert_eq!(status.pcie_link_gen, Some(3));
        assert_eq!(status.pcie_max_link_gen, Some(3));

        // A zero reason mask is "nothing throttling", not "no data".
        s.telemetry.sample.throttle_reason_mask = 0;
        assert!(status_from_snapshot(&s).throttle_reasons.is_empty());

        // The whole gen-info word at 0 means the driver does not report a link.
        assert_eq!(decode_pci_gen_info(0), (None, None));
        assert_eq!(decode_pci_gen_info(0x0000_1000), (Some(1), Some(2)));

        // The NVML-facing view keeps the full nine-reason list in NVML order,
        // with the active flags decoded from the same mask.
        let states = throttle_reason_states(Some(0b1000101));
        assert_eq!(states.len(), 9);
        assert_eq!(states[0], ("GPU Idle", true));
        assert_eq!(states[1], ("App Clock Setting", false));
        assert_eq!(states[2], ("SW Power Cap", true));
        assert_eq!(states[6], ("HW Thermal", true));
        assert_eq!(states[8], ("Display Clock", false));
        // An unavailable mask never fabricates an active reason.
        assert!(
            throttle_reason_states(None)
                .iter()
                .all(|(_, active)| !active)
        );
    }

    #[test]
    fn engine_utilization_is_normalized_from_percent_x100() {
        let mut s = snapshot();
        s.telemetry.sample = raw_sample();
        s.telemetry.sample.engine_utilization_pct100 = [9_148, 25, 0, 0, 0, 0];
        let status = status_from_snapshot(&s);
        assert_eq!(status.engine_utilization_percent["gr"], 91.48);
        assert_eq!(status.engine_utilization_percent["fb"], 0.25);
        assert_eq!(status.engine_utilization_percent["nvdec"], 0.0);

        s.telemetry.sample.engine_utilization_pct100[0] = 10_001;
        let status = status_from_snapshot(&s);
        assert!(status.engine_utilization_percent.is_empty());
        assert!(status.unavailable["engine_utilization_percent"].contains("implausible"));
    }

    #[test]
    fn stale_failed_and_invalid_fields_never_become_measurements() {
        let mut s = snapshot();
        s.telemetry.sample = raw_sample();
        // The whole RUSD page failed to allocate/map: every section says so.
        s.telemetry.status = 0x1f;
        for status in s.telemetry.sample.field_status.iter_mut() {
            *status = FIELD_UNAVAILABLE;
        }
        let status = status_from_snapshot(&s);
        assert!(status.temperature_c.is_none());
        assert!(status.rusd_timestamps.is_empty());
        assert!(status.unavailable["temperature_c"].contains("0000001f"));
        // Live controls fail on their own status, not the page status.
        s.telemetry.sample.memory_used_status = 0x57;
        let status = status_from_snapshot(&s);
        assert!(status.memory_used_bytes.is_none());
        assert!(status.unavailable["memory_used_bytes"].contains("00000057"));

        // A section that stayed on its mapping-time watermark is called out
        // with the one-shot wording, not the monitor's "session opened".
        s.telemetry.status = 0;
        s.telemetry.sample.field_status[SEC_POWER] = FIELD_NOT_FRESH;
        s.telemetry.sample.timestamps[SEC_POWER] = 0;
        let status = status_from_snapshot(&s);
        assert!(status.power_mw.is_none());
        assert!(
            status.unavailable["power_mw"].contains("initial freshness budget"),
            "{:?}",
            status.unavailable
        );
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

    #[test]
    fn monitor_keeps_valid_zero_readings_and_marks_the_first_sample_updated() {
        let mut trackers = trackers_at(0);
        let sample = interpret_sample(&mut trackers, &raw_sample(), 0, MONITOR_STALE_AFTER);
        assert_eq!(sample.status.temperature_c, Some(50.0));
        assert_eq!(sample.status.pstate, Some(1));
        assert!(sample.status.unavailable.is_empty());
        assert_eq!(sample.advancing_sections(), RUSD_FIELDS);
        // 8 clock + 4 temperature + 2 limit + power + avg + policy + throttle
        // + fan_rpm + fan_rpms + 2 utilization + engines + 2 pcie + the two
        // live RM controls (pstate, memory_used_bytes).
        assert_eq!(sample.updated.len(), 27);
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
        assert_eq!(sample.advancing_sections(), 0);
        assert_eq!(updated_fields(&sample), ["memory_used_bytes", "pstate"]);

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
        raw.field_status[SEC_TEMP_GPU] = FIELD_UNSTABLE;
        raw.timestamps[SEC_TEMP_GPU] = 0;
        raw.utilization_percent = 101;
        raw.clk_actual_khz[0] = u32::MAX;
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
            sample.status.unavailable["utilization_percent"].contains("implausible"),
            "{:?}",
            sample.status.unavailable
        );
        assert!(!sample.updated.contains_key("graphics_clock_khz"));
        assert!(sample.updated["memory_clock_khz"]);

        // A section the driver does not publish at all is a different
        // condition from one that raced, and neither is a measurement.
        let mut trackers = trackers_at(0);
        let mut raw = raw_sample();
        raw.field_status[SEC_POWER] = FIELD_UNAVAILABLE;
        raw.timestamps[SEC_POWER] = 0;
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
            "pcie_link_gen",
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
        // Live controls are RM queries, not RUSD sections, so they are new on
        // every successful sample.
        assert_eq!(sample.freshness["pstate"], FieldState::Advancing);
        assert_eq!(sample.status.pstate, Some(1));
        assert_eq!(sample.status.memory_used_bytes, Some(200_000 * 1024));

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
        assert_eq!(updated_fields(&sample), ["memory_used_bytes", "pstate"]);
        assert!(sample.status.unavailable.is_empty());
    }

    #[test]
    fn monitor_treats_a_backwards_stamp_as_new_data_from_a_reset_epoch() {
        let mut trackers = trackers_at(9_000);
        trackers[SEC_TEMP_GPU].seen_change = true;
        trackers[SEC_TEMP_GPU].changed_ns = 0;
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
        raw.timestamps[SEC_TEMP_GPU] = 1_000;
        raw.field_status[SEC_POWER] = FIELD_UNAVAILABLE;
        raw.timestamps[SEC_POWER] = 0;
        raw.timestamps[SEC_FAN] = 9_000;
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
        assert_eq!(value["engine_utilization_percent"]["gr"], 7.0);
        // The fan section still sits on the open watermark, so it has no
        // reading to publish.
        assert!(value["fan_rpms"].as_array().unwrap().is_empty());
    }

    #[test]
    fn rust_abi_layout_matches_the_native_contract() {
        // Mirrors the _Static_asserts in core/native/macos_rm_abi.h and the
        // assertions in core/tests/macos_rm_abi.c; the ignored hardware test
        // additionally compares this array with the native fingerprint.
        assert_eq!(
            abi_layout(),
            [
                648,
                368,
                376,
                272,
                108,
                8,
                64,
                208,
                176,
                192,
                224,
                248,
                320,
                344,
                348,
                356, // sizes and offsets
                u64::from(ABI_VERSION),
                RUSD_FIELDS as u64,
                FIELD_OK as u64,
                FIELD_UNSTABLE as u64,
                FIELD_UNAVAILABLE as u64,
                FIELD_NOT_FRESH as u64,
                CLK_DOMAINS as u64,
                TEMP_SENSORS as u64,
                FAN_MAX_COOLERS as u64,
                ENGINES as u64,
                POLL_MASK as u64,
                0,
                1,
                2,
                3,
                4,
                5,
                6,
                7,
                8,
                9,
                10,
                11,
                12,
                13,
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
