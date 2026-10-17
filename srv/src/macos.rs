//! Read-only macOS [`ControlBackend`]: the native NullMoth RM surface.
//!
//! On macOS there is no NVAPI runtime and no NVML, so the NVAPI(+NVML)
//! backend in [`crate::backend`] has nothing to talk to. The driver is
//! reachable only through the RM/RUSD read channel behind NullMoth, which
//! exposes telemetry — clocks, temperatures, power, fan rpm, utilization,
//! throttle reasons — but **no writable control** is reachable: the public
//! 610.57.04 RM control surface has no clock/voltage offset, lock, fan or
//! thermal-policy write command, and the private GETs the read-channel review
//! probed return `NV_ERR_INVALID_ARGUMENT` (0x1f) or `NV_ERR_NOT_SUPPORTED`
//! (0x56) through this transport (evidence:
//! `outputs/review-stage4-7-read-channel-20261017.md` in the delivery
//! bundle).
//!
//! So this backend implements the read half of the trait and fails every
//! actuator closed with one shared reason; it reports
//! [`supports_writes`](ControlBackend::supports_writes) as false, which pins
//! the control loop to observe-only. It deliberately fabricates nothing: a
//! sensor / limit the RM channel does not publish is `None`, never a
//! default-looking zero.
//!
//! The read path reuses `nvoc_core::macos_rm` exactly as the read-only
//! pynvoc backend and the `macos-eval` optimizer step do, so all three
//! consumers report the same numbers for the same tick.

use crate::controller::{ControlBackend, FanReading, SensorBundle};
use crate::monitor::{MonitorSample, OffsetBackend, OffsetDomain};
use nvoc_core::DeviceStatus;
use std::borrow::Cow;

/// One reason string shared by every actuator path, so the HTTP body, the
/// audit entry and the capability matrix cannot drift apart.
const READ_ONLY: &str =
    "read-only macOS backend: no writable RM control is reachable on this driver";

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
        false
    }

    fn as_any_mut(&mut self) -> &mut dyn std::any::Any {
        self
    }

    fn read_temps(&mut self, gpu_index: usize) -> Result<SensorBundle, String> {
        let status = self.status(gpu_index)?;
        Ok(sensors_from_status(&status))
    }

    fn read_fan(&mut self, _gpu_index: usize) -> Option<FanReading> {
        // The RM channel reports fan rpm, not duty %, and `FanReading` only
        // carries a percentage: there is no honest mapping, so the duty stays
        // unknown rather than being derived from rpm.
        None
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
            obj.insert("read_only".to_string(), serde_json::json!(true));
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
            "V/F curve unavailable: no readable V/F RM control on this driver \
             (read-only macOS backend)"
                .to_string(),
        )
    }

    fn read_offset_mhz(
        &mut self,
        _gpu_index: usize,
        _domain: OffsetDomain,
        _backend: OffsetBackend,
    ) -> Result<i32, String> {
        Err(
            "clock offset unavailable: no readable offset RM control on this driver \
             (read-only macOS backend)"
                .to_string(),
        )
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
        _gpu_index: usize,
        _domain: OffsetDomain,
        _backend: OffsetBackend,
        _mhz: i32,
    ) -> Result<(), String> {
        Err(READ_ONLY.to_string())
    }

    fn write_power_limit_w(&mut self, _gpu_index: usize, _watts: u32) -> Result<(), String> {
        Err(READ_ONLY.to_string())
    }

    fn write_temp_limit_c(&mut self, _gpu_index: usize, _celsius: i32) -> Result<(), String> {
        Err(READ_ONLY.to_string())
    }

    fn reset_offset(&mut self, _gpu_index: usize, _domain: OffsetDomain) -> Result<(), String> {
        Err(READ_ONLY.to_string())
    }

    fn reset_power_limit(&mut self, _gpu_index: usize) -> Result<(), String> {
        Err(READ_ONLY.to_string())
    }

    fn reset_temp_limit(&mut self, _gpu_index: usize) -> Result<(), String> {
        Err(READ_ONLY.to_string())
    }

    fn write_fan_percent(&mut self, _gpu_index: usize, _percent: u32) -> Result<(), String> {
        Err(READ_ONLY.to_string())
    }

    fn write_freq_cap_khz(&mut self, _gpu_index: usize, _cap_khz: u32) -> Result<(), String> {
        Err(READ_ONLY.to_string())
    }

    fn restore_fan_auto(&mut self, _gpu_index: usize) -> Result<(), String> {
        // Nothing was ever pinned, so there is nothing to un-pin. Reporting
        // success keeps a stray hand-back call from raising a false
        // `RestoreFailed` failsafe; the loop does not call this anyway
        // (`supports_writes` is false).
        Ok(())
    }

    fn restore_freq_auto(&mut self, _gpu_index: usize) -> Result<(), String> {
        // See `restore_fan_auto`: the soft wall was never set.
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
    fn every_actuator_fails_closed_with_the_shared_reason() {
        let mut backend = MacosRmBackend { gpus: Vec::new() };
        assert!(!backend.supports_writes());
        assert_eq!(backend.backend_kind(), "macos-rm");
        assert_eq!(backend.gpu_count(), 0);
        assert_eq!(backend.name(0).as_ref(), "GPU 0");
        assert!(
            backend
                .as_any_mut()
                .downcast_mut::<MacosRmBackend>()
                .is_some()
        );
        assert_eq!(
            backend.write_fan_percent(0, 100),
            Err(READ_ONLY.to_string())
        );
        assert_eq!(
            backend.write_power_limit_w(0, 150),
            Err(READ_ONLY.to_string())
        );
        assert_eq!(
            backend.write_temp_limit_c(0, 80),
            Err(READ_ONLY.to_string())
        );
        assert_eq!(
            backend.write_offset_mhz(0, OffsetDomain::Core, OffsetBackend::Nvml, 100),
            Err(READ_ONLY.to_string())
        );
        assert_eq!(
            backend.write_freq_cap_khz(0, 1_000_000),
            Err(READ_ONLY.to_string())
        );
        assert_eq!(
            backend.reset_offset(0, OffsetDomain::Mem),
            Err(READ_ONLY.to_string())
        );
        assert_eq!(backend.reset_power_limit(0), Err(READ_ONLY.to_string()));
        assert_eq!(backend.reset_temp_limit(0), Err(READ_ONLY.to_string()));
        // Nothing was ever pinned, so hand-back is a no-op success.
        assert_eq!(backend.restore_fan_auto(0), Ok(()));
        assert_eq!(backend.restore_freq_auto(0), Ok(()));
    }

    #[test]
    fn unavailable_reads_report_a_reason_instead_of_a_fake_window() {
        let mut backend = MacosRmBackend { gpus: Vec::new() };
        assert_eq!(backend.read_power_limit_w(0).unwrap(), None);
        assert_eq!(backend.read_temp_limit_c(0).unwrap(), None);
        assert_eq!(backend.read_freq_ceiling_mhz(0), None);
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
