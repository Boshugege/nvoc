//! Hardware validation; never changes clock, power, fan, or boost policy.
#![cfg(all(target_os = "macos", target_arch = "x86_64"))]

/// The NVOC transport and the driver's RUSD poller are process-global. Two
/// concurrent read-only clients can starve each other's 1.5 s freshness
/// budget, so the hardware tests take turns instead of running in parallel.
static SERIAL: std::sync::Mutex<()> = std::sync::Mutex::new(());

fn serial() -> std::sync::MutexGuard<'static, ()> {
    SERIAL.lock().unwrap_or_else(|err| err.into_inner())
}

use nvoc_core::{
    BackendSet, DeviceStatus, FieldState, GpuId, GpuSelector, MacosRmSession, QueryMacosRmInfo,
    QueryMacosRmStatus, abi_layout, discover_targets, native_abi_layout, nvapi_init_attempted, run,
    select_targets,
};

fn count_fds() -> usize {
    std::fs::read_dir("/dev/fd").unwrap().count()
}

/// Fields the 610.57.04 driver does not publish on this GPU: the board has no
/// memory/board/PSU temperature sensors and the average-power and SM-domain
/// blocks stay RUSD_TIMESTAMP_INVALID even when polled. They must arrive as
/// `unavailable` with a reason, never as a fabricated value; every other field
/// has to be a real reading.
const UNPUBLISHED_ON_THIS_GPU: [&str; 6] = [
    "temperature_memory_c",
    "temperature_board_c",
    "temperature_power_supply_c",
    "average_power_mw",
    "sm_clock_khz",
    "sm_target_clock_mhz",
];

/// Every field this GPU does publish must be present, and anything withheld
/// must carry a non-empty reason and be one of the known-absent fields.
fn assert_required_fields_present(status: &DeviceStatus) {
    assert!(status.pstate.is_some(), "pstate");
    assert!(status.graphics_clock_khz.is_some(), "graphics_clock_khz");
    assert!(status.memory_clock_khz.is_some(), "memory_clock_khz");
    assert!(status.video_clock_khz.is_some(), "video_clock_khz");
    assert!(
        status.graphics_target_clock_mhz.is_some(),
        "graphics_target_clock_mhz"
    );
    assert!(
        status.video_target_clock_mhz.is_some(),
        "video_target_clock_mhz"
    );
    assert!(status.temperature_c.is_some(), "temperature_c");
    assert!(status.power_mw.is_some(), "power_mw");
    assert!(status.power_limit_mw.is_some(), "power_limit_mw");
    assert!(
        status.power_limit_requested_mw.is_some(),
        "power_limit_requested_mw"
    );
    assert!(status.power_policy_tgp_mw.is_some(), "power_policy_tgp_mw");
    assert!(
        status.throttle_reason_mask.is_some(),
        "throttle_reason_mask"
    );
    assert!(status.fan_rpm.is_some(), "fan_rpm");
    assert!(status.utilization_percent.is_some(), "utilization_percent");
    assert!(
        status.engine_utilization_percent.contains_key("gr"),
        "engine_utilization_percent"
    );
    assert!(status.pcie_link_gen.is_some(), "pcie_link_gen");
    assert!(status.memory_used_bytes.is_some(), "memory_used_bytes");
    for (field, reason) in &status.unavailable {
        assert!(!reason.is_empty(), "{field} has an empty reason");
    }
    let unexpected: Vec<&str> = status
        .unavailable
        .keys()
        .map(String::as_str)
        .filter(|name| !UNPUBLISHED_ON_THIS_GPU.contains(name))
        .collect();
    assert!(
        unexpected.is_empty(),
        "unexpected unavailable fields: {unexpected:?} (all: {:?})",
        status.unavailable
    );
}

#[test]
#[ignore = "requires an initialized NullMoth GPU on x86_64 macOS"]
fn native_inventory_and_queries_do_not_initialize_nvapi() {
    let _serial = serial();
    let inventory = discover_targets(BackendSet::MacosRm).unwrap();
    let targets = inventory.targets();
    assert!(!targets.is_empty());
    let target = targets[0];
    assert!(target.has_macos_rm());
    assert!(!target.has_nvapi());
    assert!(!target.has_nvml());
    let info = run(&target, QueryMacosRmInfo).unwrap().output;
    assert_eq!(info.vendor_id, 0x10de);
    assert_eq!(info.driver_version, "610.57.04");
    assert!(info.memory_total_bytes.unwrap() > 0);
    assert_eq!(GpuId::from_pci_str(&info.pci_address).unwrap(), target.id);
    let selected = select_targets(&targets, &GpuSelector::from_specs(["0".into()])).unwrap();
    assert_eq!(selected[0].id, target.id);
    for _ in 0..3 {
        let status = run(&target, QueryMacosRmStatus).unwrap().output;
        assert_required_fields_present(&status);
    }
    assert!(!nvapi_init_attempted());
}

#[test]
#[ignore = "requires a NullMoth GPU with RUSD telemetry"]
fn rusd_refreshes_across_queries_without_leaking_fds() {
    let _serial = serial();
    let inventory = discover_targets(BackendSet::MacosRm).unwrap();
    let targets = inventory.targets();
    let target = targets[0];
    let before = count_fds();
    let mut previous_timestamp = 0;
    for _ in 0..8 {
        let status = run(&target, QueryMacosRmStatus).unwrap().output;
        assert_required_fields_present(&status);
        assert!((-50.0..=150.0).contains(&status.temperature_c.unwrap()));
        assert!(status.utilization_percent.unwrap() <= 100);
        let timestamp = status.rusd_timestamps["temperature_c"];
        assert!(timestamp > previous_timestamp);
        previous_timestamp = timestamp;
    }
    assert!(count_fds() <= before + 1);
}

#[test]
#[ignore = "requires a NullMoth GPU with RUSD telemetry"]
fn persistent_session_samples_advance_without_leaking_fds() {
    let _serial = serial();
    let before = count_fds();
    let mut session = MacosRmSession::open().unwrap();
    assert_eq!(session.info().driver_version, "610.57.04");
    // The open wait reached a verdict for every section: the polled ones
    // published a new value and the ones this driver does not implement
    // settled as unavailable, so the first sample may not claim
    // "never refreshed" for any of them.
    assert_eq!(
        session.initial_settled(),
        [true; nvoc_core::MONITOR_SECTIONS.len()],
        "a section did not settle inside the open budget"
    );
    // The anchor report names one entry per section, in section order.
    assert_eq!(nvoc_core::MONITOR_SECTIONS.len(), 14);
    // The identity the monitor guards on is the one the session opened on.
    let (pci, device_id, _rm_id) = session.identity();
    assert_eq!(pci, session.info().pci_address);
    assert_eq!(device_id, session.info().device_id);

    let mut previous_timestamp = 0;
    for index in 0..6 {
        std::thread::sleep(std::time::Duration::from_millis(600));
        let sample = session.sample().unwrap();
        assert_required_fields_present(&sample.status);
        assert!(sample.epoch_reset.is_empty(), "sample {index}: clock reset");
        assert_eq!(
            sample.freshness["temperature_c"],
            FieldState::Advancing,
            "sample {index} did not advance"
        );
        assert!(sample.updated["temperature_c"], "sample {index} stalled");
        assert!(sample.advancing_sections() > 0);
        let temperature = sample.status.temperature_c.unwrap();
        assert!((-50.0..=150.0).contains(&temperature));
        assert!(sample.status.utilization_percent.unwrap() <= 100);
        assert!(sample.status.pstate.is_some());
        let timestamp = sample.status.rusd_timestamps["temperature_c"];
        assert!(
            timestamp > previous_timestamp,
            "sample {index}: {timestamp} <= {previous_timestamp}"
        );
        previous_timestamp = timestamp;
    }
    // Explicit close reports its own result instead of relying on Drop.
    session.close().unwrap();
    assert!(count_fds() <= before + 1);
}

#[test]
#[ignore = "requires a NullMoth GPU with RUSD telemetry"]
fn sessions_can_be_closed_and_reopened_without_growing_resources() {
    let _serial = serial();
    let before = count_fds();
    let mut identity = None;
    for cycle in 0..3 {
        let session = MacosRmSession::open().unwrap();
        let current = session.identity();
        if let Some(previous) = &identity {
            assert_eq!(previous, &current, "cycle {cycle} changed device identity");
        }
        identity = Some(current);
        session.close().unwrap();
    }
    // Closing must release the chain and the mapping, not just the Rust value.
    assert!(count_fds() <= before + 1);
}

#[test]
#[ignore = "requires a NullMoth GPU on x86_64 macOS"]
fn native_and_rust_abi_layouts_agree() {
    let _serial = serial();
    // The C side pins the same numbers with _Static_assert; this compares the
    // two languages at run time through the shared fingerprint entry point, so
    // the extern declaration in core/src/macos_rm.rs cannot drift silently.
    assert_eq!(native_abi_layout().unwrap(), abi_layout());
}
