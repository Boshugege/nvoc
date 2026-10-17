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
    BackendSet, FieldState, GpuId, GpuSelector, MacosRmSession, QueryMacosRmInfo,
    QueryMacosRmStatus, abi_layout, discover_targets, native_abi_layout, nvapi_init_attempted, run,
    select_targets,
};

fn count_fds() -> usize {
    std::fs::read_dir("/dev/fd").unwrap().count()
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
        assert!(status.pstate.is_some());
        assert!(status.graphics_clock_khz.is_some());
        assert!(status.temperature_c.is_some());
        assert!(status.power_mw.is_some());
        assert!(status.unavailable.is_empty());
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
        assert!(status.unavailable.is_empty(), "{:?}", status.unavailable);
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
    // An open session waited for every section to publish a first new value, so
    // the first sample may not claim "never refreshed" for any of them.
    assert_eq!(
        session.initial_freshness(),
        [true; nvoc_core::MONITOR_SECTIONS.len()],
        "a section did not publish a new value inside the open budget"
    );
    // The identity the monitor guards on is the one the session opened on.
    let (pci, device_id, _rm_id) = session.identity();
    assert_eq!(pci, session.info().pci_address);
    assert_eq!(device_id, session.info().device_id);

    let mut previous_timestamp = 0;
    for index in 0..6 {
        std::thread::sleep(std::time::Duration::from_millis(600));
        let sample = session.sample().unwrap();
        assert!(
            sample.status.unavailable.is_empty(),
            "sample {index}: {:?}",
            sample.status.unavailable
        );
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
