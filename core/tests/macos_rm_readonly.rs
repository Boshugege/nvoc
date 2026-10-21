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
    QueryMacosRmSettings, QueryMacosRmStatus, QueryThrottleReasons, abi_layout, discover_targets,
    native_abi_layout, nvapi_init_attempted, run, select_targets,
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

/// One shared operation backs the dedicated throttle surface in every front
/// end (CLI `get-throttle-reasons`, pynvoc `query_throttle_reasons`): NVML's
/// nine-reason vocabulary/order, flags decoded from the RUSD mask the status
/// payload carries. Comparing against the raw mask bits proves the op does
/// not just echo its own decoder.
#[test]
#[ignore = "requires a NullMoth GPU on x86_64 macOS"]
fn shared_throttle_reason_query_reports_the_nvml_vocabulary() {
    let _serial = serial();
    let inventory = discover_targets(BackendSet::MacosRm).unwrap();
    let targets = inventory.targets();
    let target = targets[0];
    let status = run(&target, QueryMacosRmStatus).unwrap().output;
    let reasons = run(&target, QueryThrottleReasons).unwrap().output;
    let expected_names = [
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
    let names: Vec<&str> = reasons.iter().map(|reason| reason.name.as_str()).collect();
    assert_eq!(names, expected_names, "NVML vocabulary/order drifted");
    let mask = status
        .throttle_reason_mask
        .expect("throttle_reason_mask on this driver");
    for (bit, reason) in reasons.iter().enumerate() {
        assert_eq!(
            reason.active,
            mask & (1 << bit) != 0,
            "{} active flag disagrees with mask {mask:#010x}",
            reason.name
        );
    }
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

/// The offset channel, as the hardware actually answers it. This is the test to
/// run first after a restart, because it is the only one that can produce the
/// current-offset reading rather than a plausible zero, and because it exercises
/// the seeded selection mask: the handler writes back only the records the mask
/// selects, so a read path that sent a zeroed block would come back `NV_OK` with
/// nothing in it.
///
/// It asserts the *shape* a live answer must have, not the numbers: the values
/// are printed, so a capture of this test is the evidence and a changed constant
/// cannot hide inside an assertion.
#[test]
#[ignore = "requires a NullMoth GPU whose GSP is not in the reset-required state"]
fn the_offset_channel_reports_domains_records_and_its_own_window() {
    use nvoc_core::macos_offset::OffsetBlockRead;

    let _serial = serial();
    let mut session = MacosRmSession::open().expect(
        "open an RM session (0x62 here is a GSP that wants a reset, not a missing command)",
    );
    let snapshot = session
        .read_offset_channel()
        .expect("one refused block must not refuse the channel");
    println!("{snapshot:#?}");
    let refused = |what: &str, status: u32| {
        println!("{what} refused with {status:#010x} (0x62 = the GSP needs a reset)");
    };

    match &snapshot.directory {
        OffsetBlockRead::Parsed(directory) => {
            assert!(
                !directory.entries.is_empty(),
                "a parsed directory lists the domains it has"
            );
            assert_eq!(
                directory
                    .entries
                    .iter()
                    .map(|entry| entry.index)
                    .collect::<Vec<_>>(),
                (0..directory.entries.len()).collect::<Vec<_>>(),
                "entries are reported in index order"
            );
        }
        OffsetBlockRead::Refused(status) => refused("directory 0x20809019", *status),
        OffsetBlockRead::Malformed(err) => panic!("directory malformed: {err:?}"),
    }

    match &snapshot.range {
        OffsetBlockRead::Parsed(range) => {
            for record in &range.records {
                println!(
                    "range record {}: {}..{} kHz (tag {:#010x})",
                    record.index, record.min_khz, record.max_khz, record.tag
                );
            }
        }
        OffsetBlockRead::Refused(status) => refused("range 0x20809025", *status),
        OffsetBlockRead::Malformed(err) => panic!("range malformed: {err:?}"),
    }

    match &snapshot.control {
        OffsetBlockRead::Parsed(control) => {
            // Non-zero because the read path seeds it; the handler does not
            // write this field, so it is the caller's request echoed back.
            assert_ne!(
                control.mask, 0,
                "the read path must ask for records, not read a zeroed block"
            );
            assert!(
                !control.records.is_empty(),
                "a mask the handler accepted must come back with the records it selected"
            );
            for record in &control.records {
                println!(
                    "control record {} stamp {:#06x}: {:?}",
                    record.index, record.type_stamp, record.value_dwords
                );
            }
        }
        OffsetBlockRead::Refused(status) => refused("control 0x2080901b", *status),
        OffsetBlockRead::Malformed(err) => panic!("control malformed: {err:?}"),
    }

    // The naming layer, on the same device. This is the only place that can show
    // which record a public name points at, and the assertion is deliberately
    // one-sided: a name may be *absent* (that is a measurement: no record
    // published this snapshot's live clock for that domain, or two did), but a
    // name that is *present* must be a label on a record the raw read already
    // published, never a number of its own.
    drop(session);
    let inventory = discover_targets(BackendSet::MacosRm).unwrap();
    let targets = inventory.targets();
    let target = targets[0];
    let settings = run(&target, QueryMacosRmSettings)
        .expect("the settings query folds the same offset channel")
        .output;
    match &settings.clock_offsets_named {
        None => println!(
            "naming: the offset channel was not read in this snapshot; see the settings notes"
        ),
        Some(named) => {
            for entry in named {
                println!(
                    "named: {} -> bit {:#x}, published {} kHz, offset {:+} kHz ({})",
                    entry.name,
                    entry.domain_bit,
                    entry.frequency_khz,
                    entry.offset_khz,
                    entry.corroboration
                );
                let published = settings
                    .clock_offsets
                    .as_ref()
                    .expect("a named domain implies the channel was read")
                    .iter()
                    .any(|record| {
                        record.domain_bit == entry.domain_bit
                            && record.frequency_khz == entry.frequency_khz
                            && record.offset_khz == entry.offset_khz
                    });
                assert!(
                    published,
                    "a name labels a record of this read: {entry:?} is not one of {:?}",
                    settings.clock_offsets
                );
            }
        }
    }
    // An unnamed domain must explain itself rather than answer with a zero.
    for name in ["graphics", "memory", "video", "processor"] {
        match settings.named_clock_offset(name) {
            Ok(entry) => println!("{name}: bit {:#x}", entry.domain_bit),
            Err(reason) => {
                println!("{name}: not named -- {reason}");
                assert!(
                    reason.contains("not named") || reason.contains("was not read"),
                    "the reason has to say which layer is missing: {reason}"
                );
            }
        }
    }
}

#[test]
#[ignore = "requires the validated macOS TU116 RM driver; GETs only"]
fn shared_power_limits_read_the_board_policy_without_initializing_nvapi() {
    let _serial = serial();
    let inventory = discover_targets(BackendSet::MacosRm).unwrap();
    let targets = inventory.targets();
    let target = targets[0];
    let policy = target.macos_rm().unwrap().power_policy_settings().unwrap();
    assert_eq!(
        (policy.minimum_mw, policy.default_mw, policy.maximum_mw),
        (70_000, 100_000, 100_000)
    );
    let shared = run(&target, nvoc_core::QueryPowerLimits).unwrap().output;
    assert_eq!((shared.min_watts, shared.max_watts), (70.0, 100.0));
    assert!((70.0..=100.0).contains(&shared.current_watts));
    let report = target.macos_rm().unwrap().power_limit_report().unwrap();
    assert_eq!(
        (report.min_watt, report.default_watt, report.max_watt),
        (Some(70.0), Some(100.0), Some(100.0))
    );
    assert_eq!(
        report.current_watt,
        Some(f64::from(policy.current_mw) / 1000.0)
    );
    assert!(!nvapi_init_attempted());
    println!("policy={policy:?}; shared={shared:?}; report={report:?}");
}

#[test]
#[ignore = "requires the validated macOS TU116 RM driver; fan GETs only"]
fn shared_fan_info_reads_metadata_and_preserves_a_stopped_cooler() {
    let _serial = serial();
    let inventory = discover_targets(BackendSet::MacosRm).unwrap();
    let targets = inventory.targets();
    let target = targets[0];
    let device = target.macos_rm().unwrap();
    let report = device.fan_report().unwrap();
    let shared = run(&target, nvoc_core::QueryFanInfo).unwrap().output;
    let settings = device.fan_control_settings().unwrap();
    assert_eq!(report.count, 1);
    assert_eq!(shared.count, 1);
    assert_eq!((shared.min_speed, shared.max_speed), (Some(0), Some(100)));
    assert_eq!(report.coolers[0].slot, 0);
    assert_eq!(settings.slot, 0);
    assert!((0.0..=100.0).contains(&settings.current_percent));
    assert!((0.0..=100.0).contains(&settings.target_percent));
    assert!(matches!(settings.mode, "auto" | "manual"));
    assert!(device.fan_percent_write_supported());
    assert!(!nvapi_init_attempted());
    println!("settings={settings:?}; report={report:?}; shared={shared:?}");
}
