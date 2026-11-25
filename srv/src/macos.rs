//! Read-only macOS [`ControlBackend`]: the native NullMoth RM surface.
//!
//! On macOS there is no NVAPI runtime and no NVML, so the NVAPI(+NVML)
//! backend in [`crate::backend`] has nothing to talk to. The driver is
//! reachable only through the RM/RUSD read channel behind NullMoth, which
//! exposes telemetry — clocks, temperatures, power, fan rpm, utilization,
//! throttle reasons — and a private control channel now partially mapped.
//!
//! The read half is implemented; the actuators that have a validated,
//! transactional route are wired to the shared core operations, and the ones
//! that do not fail closed with a reason naming what is missing. Two facts keep
//! that honest, because the usual shorthand ("this driver has no write
//! command") is wrong here and the difference matters to a user:
//!
//! * The write command **exists**. The GSP export table names `0x2080d01c` as
//!   the twin of the `0x2080901b` control GET, and its identity is a *set* id
//!   (`(cmd & 0xC000) == 0xC000`). What a client cannot do today is *reach*
//!   it: an unprivileged caller measured `NV_ERR_INSUFFICIENT_PERMISSIONS`
//!   (0x1b), the host-visible table has no privileged row, and the macOS
//!   native layer (`core/native/macos_rm.c`) carries no SET id at all.
//! * Private GETs are **not** uniformly rejected. The read-channel review's
//!   early `0x1f` / `0x56` answers came from caller-side size guesses; the
//!   driver accepts exactly the size its export table declares, which is how
//!   the 776-byte clock document (`0x20809061`) came back `NV_OK` and
//!   published per-domain offsets.
//!
//! [`supports_writes`](ControlBackend::supports_writes) is `true`: four
//! families have end-to-end verified routes (graphics/memory additive clock
//! offsets, the board power limit, cooler0 percent + firmware auto, and the
//! acoustic temperature target), and the control loop drives those. It is a
//! per-backend answer, not the coarse
//! `nvoc_core::macos_rm::SETTINGS_WRITE_SUPPORTED` switch, which describes
//! whether *bulk* settings/restore can be applied and is still false. The
//! frequency-cap actuator stays closed on purpose: writing a locked-clock
//! window has no verified clear route on this driver (`0x2080c535` returns
//! `0x40`), so exposing it would leave a card locked with no way back.
//! This backend deliberately fabricates nothing: a sensor / limit the RM
//! channel does not publish is `None`, never a default-looking zero.
//!
//! The read path reuses `nvoc_core::macos_rm` exactly as the read-only
//! pynvoc backend and the `macos-eval` optimizer step do, so all three
//! consumers report the same numbers for the same tick.

use crate::controller::{ControlBackend, FanReading, SensorBundle};
use crate::monitor::{MonitorSample, OffsetBackend, OffsetDomain};
use nvoc_core::DeviceStatus;
use std::borrow::Cow;

/// One reason string shared by every actuator path, so the HTTP body, the
/// audit entry and the capability matrix cannot drift apart. It names the two
/// layers that are actually missing rather than claiming the driver has no
/// write command: the write path exists (`0x2080d01c`), but reaching it needs
/// privilege this client does not have and a client-side SET entry point that
/// is not wired yet.
/// Why the frequency-cap actuator is still closed. Kept next to the actuator so
/// the service body, the audit entry and the capability matrix cannot drift:
/// the write exists, the *undo* does not.
const NO_FREQUENCY_UNDO: &str = "the macOS locked-clock window is writable (0x2080d041) but has no \
     verified clear route on this driver (the reset twin 0x2080c535 answers 0x40), so the service \
     does not expose a one-way lock";

/// One discovered GPU: the core handle plus the resolved display name.
struct MacosGpu {
    device: nvoc_core::macos_rm::Device,
    name: String,
}

/// Read-only [`ControlBackend`] backed by the NullMoth RM session.
pub struct MacosRmBackend {
    gpus: Vec<MacosGpu>,
}

impl MacosRmBackend {
    /// Discover every GPU the RM channel publishes. On non-macOS builds the
    /// underlying snapshot read reports `Unsupported`, so this returns the
    /// same error the capability layer expects instead of succeeding empty.
    pub fn discover() -> Result<Self, String> {
        let devices = nvoc_core::macos_rm::discover()
            .map_err(|e| format!("macOS RM GPU discovery failed: {e}"))?;
        let gpus = devices
            .into_iter()
            .map(|device| {
                let info = device.info();
                let name = info
                    .name
                    .clone()
                    .filter(|n| !n.trim().is_empty())
                    .unwrap_or_else(|| format!("NVIDIA GPU @ {}", info.pci_address));
                MacosGpu { device, name }
            })
            .collect();
        Ok(Self { gpus })
    }

    fn gpu(&self, index: usize) -> Result<&MacosGpu, String> {
        self.gpus.get(index).ok_or_else(|| {
            format!(
                "GPU {index} out of range: macOS RM backend sees {} GPU(s)",
                self.gpus.len()
            )
        })
    }

    /// One coherent, read-only snapshot for `index`. `Device::status` opens,
    /// samples and closes the RM chain, and re-checks identity, so a device
    /// that vanished behind us is reported rather than mislabelled.
    fn status(&self, index: usize) -> Result<DeviceStatus, String> {
        self.gpu(index)?
            .device
            .status()
            .map_err(|e| format!("GPU {index}: macOS RM status: {e}"))
    }

    /// One coherent, read-only settings projection for `index`.
    ///
    /// This is the same `QueryMacosRmSettings` the CLI and pynvoc read, so the
    /// OC page cannot publish a window the other two consumers do not see.
    fn settings(&self, index: usize) -> Result<nvoc_core::DeviceSettings, String> {
        let gpu = self.gpu(index)?;
        let target = nvoc_core::GpuTarget::macos_rm_only(gpu.device.id(), index, &gpu.device);
        nvoc_core::run(&target, nvoc_core::QueryMacosRmSettings)
            .map(|report| report.output)
            .map_err(|e| format!("GPU {index}: macOS RM settings: {e}"))
    }

    /// Additive clock offset in kHz, one domain at a time.
    ///
    /// The MHz entry points convert first, so both the OC page and any
    /// kHz-aware caller land on the same shared operation and the same
    /// transaction; the window (-18750..18750 kHz) is the driver's own and is
    /// enforced by core, not re-implemented here.
    fn write_offset_khz(
        &self,
        gpu_index: usize,
        domain: OffsetDomain,
        khz: i32,
    ) -> Result<(), String> {
        let clock_domain = match domain {
            OffsetDomain::Core => nvoc_core::ClockDomain::Graphics,
            OffsetDomain::Mem => nvoc_core::ClockDomain::Memory,
        };
        self.run_op(
            gpu_index,
            nvoc_core::SetMacosRmClockOffset {
                domain: clock_domain,
                khz,
                cancelled: &|| false,
            },
        )
        .map(|_| ())
    }

    /// Run one shared core operation against one of our own devices.
    ///
    /// Every macOS write in this backend goes through here, so the actuators
    /// cannot grow a private escape sequence: the transaction semantics
    /// (snapshot -> single-domain SET -> read-back -> restore on failure) live
    /// in `nvoc_core` and are the same ones the CLI and pynvoc use.
    fn run_op<O>(&self, index: usize, op: O) -> Result<O::Output, String>
    where
        O: nvoc_core::GpuOperation,
    {
        let gpu = self.gpu(index)?;
        let target = nvoc_core::GpuTarget::macos_rm_only(gpu.device.id(), index, &gpu.device);
        nvoc_core::run(&target, op)
            .map(|report| report.output)
            .map_err(|e| format!("{e}"))
    }
}

/// Map the RM status onto the controller's legacy core/memory/board view.
/// There is no ThermChannel hotspot on this transport, so `hotspot_c` stays
/// `None` and a `Hotspot` sensor selection degrades to core (as the trait
/// documents).
fn sensors_from_status(status: &DeviceStatus) -> SensorBundle {
    SensorBundle {
        core_c: status.temperature_c.map(|v| v as f32),
        hotspot_c: None,
        memory_c: status.temperature_memory_c.map(|v| v as f32),
        board_c: status.temperature_board_c.map(|v| v as f32),
    }
}

/// Map the RM status onto the dashboard sample. `volt_mv` and `fan_pct` stay
/// `None`: the RM channel publishes no core-voltage or fan-duty reading, and
/// the fan rpm it does publish has no home in this struct.
fn monitor_from_status(status: &DeviceStatus) -> MonitorSample {
    MonitorSample {
        util_pct: status.utilization_percent.map(|v| v as f32),
        volt_mv: None,
        mem_clock_mhz: status.memory_clock_khz.map(|khz| khz as f32 / 1000.0),
        pstate: status.pstate.map(|p| format!("P{p}")),
        power_w: status.power_mw.map(|mw| mw as f32 / 1000.0),
        core_clock_mhz: status.graphics_clock_khz.map(|khz| khz as f32 / 1000.0),
        temp_c: status.temperature_c.map(|v| v as f32),
        fan_pct: None,
    }
}

impl ControlBackend for MacosRmBackend {
    fn gpu_count(&self) -> usize {
        self.gpus.len()
    }

    fn name(&self, index: usize) -> Cow<'_, str> {
        self.gpus
            .get(index)
            .map(|g| Cow::Borrowed(g.name.as_str()))
            .unwrap_or(Cow::Owned(format!("GPU {index}")))
    }

    fn backend_kind(&self) -> &'static str {
        "macos-rm"
    }

    fn supports_writes(&self) -> bool {
        // Per-backend answer: the four families below have verified routes, so
        // the control loop may drive them. This is deliberately *not* the
        // coarse `SETTINGS_WRITE_SUPPORTED` switch, which is about bulk
        // settings/restore and stays false.
        true
    }

    fn as_any_mut(&mut self) -> &mut dyn std::any::Any {
        self
    }

    fn read_temps(&mut self, gpu_index: usize) -> Result<SensorBundle, String> {
        let status = self.status(gpu_index)?;
        Ok(sensors_from_status(&status))
    }

    fn read_fan(&mut self, gpu_index: usize) -> Option<FanReading> {
        // The duty the *driver* holds is published: the cooler0 control record
        // carries a target percent (that is what `get-fan-info` prints as
        // `target_percent`), so it is reported as the duty reading instead of
        // being derived from rpm. When the record publishes no target the duty
        // stays unknown rather than becoming a default-looking zero.
        let report = self.gpu(gpu_index).ok()?.device.fan_report().ok()?;
        let percent = report.target_percent?;
        Some(FanReading {
            percent: Some(percent.round().clamp(0.0, 100.0) as u32),
        })
    }

    fn read_power_watts(&mut self, gpu_index: usize) -> Option<f32> {
        let status = self.status(gpu_index).ok()?;
        status.power_mw.map(|mw| mw as f32 / 1000.0)
    }

    fn read_core_clock_mhz(&mut self, gpu_index: usize) -> Option<f32> {
        let status = self.status(gpu_index).ok()?;
        status.graphics_clock_khz.map(|khz| khz as f32 / 1000.0)
    }

    fn read_freq_ceiling_mhz(&mut self, _gpu_index: usize) -> Option<f32> {
        // No V/F table is readable through this transport (the vfpoints GET
        // is rejected), so there is no ceiling to report.
        None
    }

    fn read_monitor(&mut self, gpu_index: usize) -> Result<MonitorSample, String> {
        let status = self.status(gpu_index)?;
        Ok(monitor_from_status(&status))
    }

    fn read_gpu_info_json(&mut self, gpu_index: usize) -> Result<serde_json::Value, String> {
        let info = self.gpu(gpu_index)?.device.info();
        let mut value = serde_json::to_value(info).map_err(|e| format!("serialize info: {e}"))?;
        // A live snapshot adds the fields that only exist on the status
        // surface. It is best effort: the identity block above is the answer
        // even when telemetry is momentarily unavailable.
        let live = self.status(gpu_index).ok();
        if let Some(obj) = value.as_object_mut() {
            obj.insert("backend".to_string(), serde_json::json!("macos-rm"));
            // The About page needs to know what it may offer. This backend is
            // not read-only any more, and it is not fully writable either: list
            // the families that have verified routes instead of one blanket
            // boolean that would be wrong in one direction or the other.
            obj.insert("read_only".to_string(), serde_json::json!(false));
            obj.insert(
                "writable_families".to_string(),
                serde_json::json!([
                    "clock_offset",
                    "power_limit",
                    "fan_percent",
                    "acoustic_target"
                ]),
            );
            if let Some(status) = &live {
                obj.insert(
                    "power_limit_w".to_string(),
                    serde_json::json!(status.power_limit_mw.map(|mw| mw as f64 / 1000.0)),
                );
                obj.insert("fan_rpm".to_string(), serde_json::json!(status.fan_rpm));
                obj.insert("fan_rpms".to_string(), serde_json::json!(status.fan_rpms));
                obj.insert(
                    "throttle_reasons".to_string(),
                    serde_json::json!(status.throttle_reasons),
                );
            }
        }
        Ok(value)
    }

    fn read_vf_curve(&mut self, _gpu_index: usize) -> Result<Vec<(f32, f32)>, String> {
        Err(
            "V/F curve not located yet: no readable V/F RM control has been pinned on this \
             driver, so nothing is asserted (the offset channel is the one under test; see \
             docs/reverse-engineering/nvapi/rm-param-marshall-corrections-r610.md)"
                .to_string(),
        )
    }

    fn read_offset_mhz(
        &mut self,
        gpu_index: usize,
        domain: OffsetDomain,
        _backend: OffsetBackend,
    ) -> Result<i32, String> {
        // The offset is readable, and which record is which *can* be named: the
        // clock document publishes a frequency per record, and a record is named
        // only when that frequency equals the live clock of that public domain
        // (`DeviceSettings::clock_offset_view`). So the OC page gets the number
        // the driver holds, with the comparison behind its name attached, and a
        // domain whose record this read did not corroborate is refused rather
        // than reported as some other domain's offset.
        let name = match domain {
            OffsetDomain::Core => "graphics",
            OffsetDomain::Mem => "memory",
        };
        let settings = self.settings(gpu_index)?;
        let view = settings.clock_offset_view(name).map_err(|e| {
            format!("{name} offset not named in this read, so none is asserted: {e}")
        })?;
        Ok(view.offset_khz)
    }

    fn read_offset_window_mhz(&mut self, gpu_index: usize) -> Option<(f32, f32)> {
        let limits = self.settings(gpu_index).ok()?.clock_offset_limits_khz?;
        Some((
            limits.min_khz as f32 / 1000.0,
            limits.max_khz as f32 / 1000.0,
        ))
    }

    fn read_power_limit_w(&mut self, _gpu_index: usize) -> Result<Option<(u32, u32, u32)>, String> {
        // The enforced limit is readable (and is reported through `/api/info`
        // and pynvoc's settings), but min/max are not, and this slot exists
        // for *writable* slider bounds. Reporting `(current, current, current)`
        // would assert a window we cannot honour, so the window stays unknown.
        Ok(None)
    }

    fn read_temp_limit_c(&mut self, _gpu_index: usize) -> Result<Option<(i32, i32, i32)>, String> {
        // Temperature sensors are readable; the thermal wall is not, and it
        // is not writable either, so there is no window to report.
        Ok(None)
    }

    fn write_offset_mhz(
        &mut self,
        gpu_index: usize,
        domain: OffsetDomain,
        backend: OffsetBackend,
        mhz: i32,
    ) -> Result<(), String> {
        // `backend` selects NVAPI vs NVML offset semantics; this transport has
        // one additive channel, so both route to the same RM write instead of
        // silently ignoring the request.
        let _ = backend;
        self.write_offset_khz(gpu_index, domain, mhz.saturating_mul(1000))
    }

    fn write_power_limit_w(&mut self, gpu_index: usize, watts: u32) -> Result<(), String> {
        let milliwatts =
            nvoc_core::validate_macos_power_limit_watts(watts).map_err(|e| format!("{e}"))?;
        self.run_op(
            gpu_index,
            nvoc_core::SetMacosRmPowerLimit {
                target: nvoc_core::PowerLimitTarget::Milliwatts(milliwatts),
                cancelled: &|| false,
            },
        )
        .map(|_| ())
    }

    fn write_temp_limit_c(&mut self, gpu_index: usize, celsius: i32) -> Result<(), String> {
        self.run_op(
            gpu_index,
            nvoc_core::SetMacosRmAcousticTarget {
                target: nvoc_core::AcousticTarget::Celsius(celsius),
                cancelled: &|| false,
            },
        )
        .map(|_| ())
    }

    fn reset_offset(&mut self, gpu_index: usize, domain: OffsetDomain) -> Result<(), String> {
        self.write_offset_khz(gpu_index, domain, 0)
    }

    fn reset_power_limit(&mut self, gpu_index: usize) -> Result<(), String> {
        // `PowerLimitTarget::Default` asks the driver for its own limit rather
        // than pinning a number this client invented.
        self.run_op(
            gpu_index,
            nvoc_core::SetMacosRmPowerLimit {
                target: nvoc_core::PowerLimitTarget::Default,
                cancelled: &|| false,
            },
        )
        .map(|_| ())
    }

    fn reset_temp_limit(&mut self, gpu_index: usize) -> Result<(), String> {
        self.run_op(
            gpu_index,
            nvoc_core::SetMacosRmAcousticTarget {
                target: nvoc_core::AcousticTarget::Default,
                cancelled: &|| false,
            },
        )
        .map(|_| ())
    }

    fn write_fan_percent(&mut self, gpu_index: usize, percent: u32) -> Result<(), String> {
        nvoc_core::validate_macos_fan_request(0, nvoc_core::FanControlTarget::Percent(percent))
            .map_err(|e| format!("{e}"))?;
        self.run_op(
            gpu_index,
            nvoc_core::SetMacosRmFanControl {
                slot: 0,
                target: nvoc_core::FanControlTarget::Percent(percent),
                cancelled: &|| false,
            },
        )
        .map(|_| ())
    }

    fn write_freq_cap_khz(&mut self, _gpu_index: usize, _cap_khz: u32) -> Result<(), String> {
        Err(NO_FREQUENCY_UNDO.to_string())
    }

    fn restore_fan_auto(&mut self, gpu_index: usize) -> Result<(), String> {
        self.run_op(
            gpu_index,
            nvoc_core::SetMacosRmFanControl {
                slot: 0,
                target: nvoc_core::FanControlTarget::Auto,
                cancelled: &|| false,
            },
        )
        .map(|_| ())
    }

    fn restore_freq_auto(&mut self, _gpu_index: usize) -> Result<(), String> {
        // The soft wall is never set (see `write_freq_cap_khz`), so there is
        // nothing to clear; success keeps a stray hand-back from raising a
        // false `RestoreFailed` failsafe.
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A synthetic RM status with every sensor the mapping consumes.
    fn sample_status() -> DeviceStatus {
        DeviceStatus {
            pstate: Some(0),
            graphics_clock_khz: Some(1_965_000),
            memory_clock_khz: Some(7_001_000),
            temperature_c: Some(57.5),
            temperature_memory_c: Some(60.25),
            temperature_board_c: Some(48.0),
            power_mw: Some(13_500),
            power_limit_mw: Some(100_000),
            fan_rpm: Some(2_615),
            fan_rpms: vec![2_615],
            utilization_percent: Some(99),
            throttle_reasons: vec!["sw_power_cap".into()],
            ..DeviceStatus::default()
        }
    }

    #[test]
    fn sensors_map_to_the_legacy_bundle() {
        let bundle = sensors_from_status(&sample_status());
        assert_eq!(bundle.core_c, Some(57.5));
        assert_eq!(bundle.memory_c, Some(60.25));
        assert_eq!(bundle.board_c, Some(48.0));
        // No ThermChannel hotspot on this transport: never fabricated.
        assert_eq!(bundle.hotspot_c, None);
        // A Hotspot selection degrades to core instead of reading a phantom.
        assert_eq!(bundle.pick(crate::config::SensorKind::Hotspot), Some(57.5));
    }

    #[test]
    fn monitor_map_converts_units_and_leaves_absent_fields_none() {
        let m = monitor_from_status(&sample_status());
        assert_eq!(m.util_pct, Some(99.0));
        assert_eq!(m.core_clock_mhz, Some(1965.0));
        assert_eq!(m.mem_clock_mhz, Some(7001.0));
        assert_eq!(m.power_w, Some(13.5));
        assert_eq!(m.temp_c, Some(57.5));
        assert_eq!(m.pstate.as_deref(), Some("P0"));
        // No core-voltage or fan-duty reading on this transport.
        assert_eq!(m.volt_mv, None);
        assert_eq!(m.fan_pct, None);
    }

    #[test]
    fn missing_sensors_stay_none_not_zero() {
        let empty = DeviceStatus::default();
        let bundle = sensors_from_status(&empty);
        assert_eq!(bundle, SensorBundle::default());
        let m = monitor_from_status(&empty);
        assert!(m.util_pct.is_none() && m.power_w.is_none() && m.temp_c.is_none());
    }

    #[test]
    fn the_frequency_cap_stays_closed_because_it_cannot_be_undone() {
        // A one-way lock is worse than no lock: this driver has a writable
        // window (0x2080d041) and no verified clear route, so the actuator
        // names the missing undo instead of pretending the write is available.
        assert!(NO_FREQUENCY_UNDO.contains("0x2080d041"));
        assert!(NO_FREQUENCY_UNDO.contains("0x2080c535"));
        let mut backend = MacosRmBackend { gpus: Vec::new() };
        assert_eq!(
            backend.write_freq_cap_khz(0, 1_620_000),
            Err(NO_FREQUENCY_UNDO.to_string())
        );
        // Hand-back stays a no-op success: nothing was ever pinned.
        assert_eq!(backend.restore_freq_auto(0), Ok(()));
    }

    #[test]
    fn the_writable_families_are_announced_and_differ_from_the_bulk_switch() {
        // Two different questions: `SETTINGS_WRITE_SUPPORTED` is about bulk
        // settings/restore (still false), `supports_writes` is about the four
        // families this backend can actually apply one transaction at a time.
        let bulk = nvoc_core::macos_rm::SETTINGS_WRITE_SUPPORTED;
        assert!(!bulk, "bulk settings/restore is still gated: {bulk}");
        let mut backend = MacosRmBackend { gpus: Vec::new() };
        assert!(backend.supports_writes());
        assert_ne!(backend.supports_writes(), bulk);
        assert_eq!(backend.backend_kind(), "macos-rm");
        assert_eq!(backend.gpu_count(), 0);
        assert_eq!(backend.name(0).as_ref(), "GPU 0");
        assert!(
            backend
                .as_any_mut()
                .downcast_mut::<MacosRmBackend>()
                .is_some()
        );
    }

    #[test]
    fn arguments_are_refused_before_the_device_is_touched() {
        // With no GPUs attached a validation failure must still be the *first*
        // error: a caller with 150 W or 101 percent gets the range, not "GPU 0
        // out of range", so the gate order is visible in the message.
        let mut backend = MacosRmBackend { gpus: Vec::new() };
        let power = backend.write_power_limit_w(0, 150).unwrap_err();
        assert!(
            power.contains('1') && power.to_lowercase().contains("w"),
            "{power}"
        );
        assert!(!power.contains("out of range"), "{power}");
        let fan = backend.write_fan_percent(0, 101).unwrap_err();
        assert!(fan.contains("100"), "{fan}");
        assert!(!fan.contains("out of range"), "{fan}");
    }

    #[test]
    fn a_missing_gpu_is_named_instead_of_writing_to_nothing() {
        // The actuators are routable now, so the failure mode changed: an index
        // that does not exist must say so rather than quietly succeeding.
        let mut backend = MacosRmBackend { gpus: Vec::new() };
        for err in [
            backend.write_fan_percent(0, 40).unwrap_err(),
            backend.restore_fan_auto(0).unwrap_err(),
            backend.write_power_limit_w(0, 90).unwrap_err(),
            backend.reset_power_limit(0).unwrap_err(),
            backend.write_temp_limit_c(0, 80).unwrap_err(),
            backend.reset_temp_limit(0).unwrap_err(),
            backend
                .write_offset_mhz(0, OffsetDomain::Core, OffsetBackend::Nvml, 100)
                .unwrap_err(),
            backend.reset_offset(0, OffsetDomain::Mem).unwrap_err(),
        ] {
            assert!(err.contains("out of range"), "{err}");
        }
    }

    #[test]
    fn unavailable_reads_report_a_reason_instead_of_a_fake_window() {
        let mut backend = MacosRmBackend { gpus: Vec::new() };
        assert_eq!(backend.read_power_limit_w(0).unwrap(), None);
        assert_eq!(backend.read_temp_limit_c(0).unwrap(), None);
        assert_eq!(backend.read_freq_ceiling_mhz(0), None);
        // Still None with no GPU attached: the duty now comes from the driver's
        // own control record, so a missing device must not read as 0 percent.
        assert!(backend.read_fan(0).is_none());
        assert!(backend.read_vf_curve(0).is_err());
        assert!(
            backend
                .read_offset_mhz(0, OffsetDomain::Core, OffsetBackend::Nvapi)
                .is_err()
        );
        // An out-of-range index is an error, not a silent default.
        assert!(backend.read_temps(0).is_err());
    }
}
