//! Native, read-only NullMoth RM backend for macOS 15+ on x86_64.
//!
//! Uses the same IOKit transport as the validation probe, linked into this
//! library. No external probe executable, NVAPI, or NVML runtime is required.
//! The initial implementation discovers the first service exposed by NullMoth.
use crate::{Error, GpuId, PciAddress};
use serde::Serialize;
use std::collections::BTreeMap;

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
        let snapshot = read_snapshot()?;
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
    read_snapshot().map(|snapshot| vec![device_from_snapshot(&snapshot)])
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
}

#[cfg(all(target_os = "macos", target_arch = "x86_64"))]
const _: () = assert!(std::mem::size_of::<Snapshot>() == 272);

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
    let mut unavailable: BTreeMap<String, String> = [
        "graphics_clock_khz",
        "memory_clock_khz",
        "temperature_c",
        "power_mw",
        "power_limit_mw",
        "fan_rpm",
        "utilization_percent",
    ]
    .into_iter()
    .map(|field| {
        (
            field.into(),
            "macOS RM telemetry path not yet validated".into(),
        )
    })
    .collect();
    if s.pstate_status != 0 {
        unavailable.insert("pstate".into(), query_error(s.pstate_status));
    }
    DeviceStatus {
        pstate: (s.pstate_status == 0).then_some(s.pstate),
        graphics_clock_khz: None,
        memory_clock_khz: None,
        temperature_c: None,
        power_mw: None,
        power_limit_mw: None,
        fan_rpm: None,
        utilization_percent: None,
        unavailable,
    }
}

#[cfg(all(target_os = "macos", target_arch = "x86_64"))]
fn read_snapshot() -> Result<Snapshot, Error> {
    use std::sync::Mutex;
    static TRANSPORT: Mutex<()> = Mutex::new(());
    unsafe extern "C" {
        fn nvoc_macos_rm_snapshot(
            out: *mut Snapshot,
            size: usize,
            error: *mut u8,
            cap: usize,
        ) -> u32;
    }
    let _guard = TRANSPORT
        .lock()
        .map_err(|_| Error::Custom("macOS RM transport lock poisoned".into()))?;
    let mut snapshot = std::mem::MaybeUninit::<Snapshot>::uninit();
    let mut error = [0u8; 256];
    // SAFETY: C receives correctly-sized writable buffers, initializes the
    // entire POD snapshot on success, and does not retain either pointer.
    // The mutex serializes every access to nvx_xnu's global connection table.
    let status = unsafe {
        nvoc_macos_rm_snapshot(
            snapshot.as_mut_ptr(),
            std::mem::size_of::<Snapshot>(),
            error.as_mut_ptr(),
            error.len(),
        )
    };
    if status != 0 {
        return Err(Error::Custom(format!("macOS RM: {}", text(&error))));
    }
    // SAFETY: success guarantees all fields were initialized by C.
    let snapshot = unsafe { snapshot.assume_init() };
    if snapshot.abi_version != 1 {
        return Err(Error::Custom(
            "macOS RM snapshot ABI version mismatch".into(),
        ));
    }
    Ok(snapshot)
}

#[cfg(not(all(target_os = "macos", target_arch = "x86_64")))]
fn read_snapshot() -> Result<Snapshot, Error> {
    Err(Error::Custom(
        "macOS RM backend requires x86_64 macOS with the NullMoth driver".into(),
    ))
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
}
