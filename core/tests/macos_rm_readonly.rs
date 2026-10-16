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
    BackendSet, GpuId, GpuSelector, MacosRmSession, QueryMacosRmInfo, QueryMacosRmStatus,
    discover_targets, nvapi_init_attempted, run, select_targets,
};

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
    let count_fds = || std::fs::read_dir("/dev/fd").unwrap().count();
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
    let count_fds = || std::fs::read_dir("/dev/fd").unwrap().count();
    let before = count_fds();
    let mut session = MacosRmSession::open().unwrap();
    assert_eq!(session.info().driver_version, "610.57.04");
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
        assert!(sample.updated["temperature_c"], "sample {index} stalled");
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
    drop(session);
    assert!(count_fds() <= before + 1);
}
