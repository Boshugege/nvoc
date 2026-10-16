//! Hardware validation; never changes clock, power, fan, or boost policy.
#![cfg(all(target_os = "macos", target_arch = "x86_64"))]

use nvoc_core::{
    BackendSet, GpuId, GpuSelector, QueryMacosRmInfo, QueryMacosRmStatus, discover_targets,
    nvapi_init_attempted, run, select_targets,
};

#[test]
#[ignore = "requires an initialized NullMoth GPU on x86_64 macOS"]
fn native_inventory_and_queries_do_not_initialize_nvapi() {
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
        assert_eq!(status.graphics_clock_khz, None);
        assert_eq!(status.temperature_c, None);
        assert_eq!(status.power_mw, None);
    }
    assert!(!nvapi_init_attempted());
}
