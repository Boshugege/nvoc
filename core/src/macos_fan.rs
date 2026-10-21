//! Single-cooler NVML-equivalent percentage control on measured TU116 firmware.
use crate::Error;
use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FanControlTarget {
    Percent(u32),
    Auto,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct FanControlSettings {
    pub slot: u32,
    pub rpm: u32,
    pub min_percent: f64,
    pub max_percent: f64,
    /// Driver status percentage, distinct from the requested control percentage.
    pub current_percent: f64,
    pub target_percent: f64,
    pub mode: &'static str,
}
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct FanControlWriteReport {
    pub slot: u32,
    pub original_mode: &'static str,
    pub original_target_percent: f64,
    pub applied_mode: &'static str,
    pub applied_target_percent: f64,
    pub readback_verified: bool,
    pub other_bytes_preserved: bool,
}
pub fn validate_macos_fan_request(slot: u32, target: FanControlTarget) -> Result<(), Error> {
    if slot != 0 {
        return Err(Error::Custom(
            "macOS fan control currently validates cooler 0 only".into(),
        ));
    }
    if let FanControlTarget::Percent(value) = target
        && value > 100
    {
        return Err(Error::Custom(
            "fan target must be within 0..100 percent".into(),
        ));
    }
    Ok(())
}
#[cfg(all(target_os = "macos", target_arch = "x86_64"))]
pub(crate) use control::{BYTES, FanControl, INFO_BYTES, STATUS_BYTES, apply, settings};
#[cfg(any(test, all(target_os = "macos", target_arch = "x86_64")))]
mod control {
    use super::*;
    use crate::macos_control_transaction::{ControlBlock, write_block};
    pub(crate) const BYTES: usize = 980;
    pub(crate) const INFO_BYTES: usize = 1432;
    pub(crate) const STATUS_BYTES: usize = 844;
    pub(crate) trait FanControl: ControlBlock<BYTES> {
        fn defaults(&mut self) -> Result<[u8; BYTES], Error>;
    }
    fn word(b: &[u8], at: usize) -> u32 {
        u32::from_le_bytes(b[at..at + 4].try_into().expect("validated block size"))
    }
    fn mode(b: &[u8; BYTES]) -> Result<&'static str, Error> {
        match word(b, 36) {
            0 => Ok("auto"),
            1 => Ok("manual"),
            _ => Err(Error::Custom(
                "fan control mode is outside the validated auto/manual modes".into(),
            )),
        }
    }
    pub(crate) fn settings(
        info: &[u8],
        status: &[u8],
        control: &[u8; BYTES],
    ) -> Result<FanControlSettings, Error> {
        if info.len() != INFO_BYTES || status.len() != STATUS_BYTES {
            return Err(Error::Custom("fan metadata/status size mismatch".into()));
        }
        if word(info, 12) != 1
            || word(status, 4) != 1
            || word(control, 12) != 1
            || word(status, 12) != 0x0303
            || word(control, 20) != 3
            || word(control, 16) != 0
        {
            return Err(Error::Custom(
                "fan slot/mask/type/default selector differs from validated cooler0".into(),
            ));
        }
        if word(status, 24) != 0
            || word(status, 28) != 65536
            || word(status, 32) > 65536
            || word(control, 40) > 65536
        {
            return Err(Error::Custom(
                "fan percentage range or value differs from validated 0..100 percent".into(),
            ));
        }
        Ok(FanControlSettings {
            slot: 0,
            rpm: word(status, 20),
            min_percent: 0.0,
            max_percent: 100.0,
            current_percent: f64::from(word(status, 32)) * 100.0 / 65536.0,
            target_percent: f64::from(word(control, 40)) * 100.0 / 65536.0,
            mode: mode(control)?,
        })
    }
    pub(crate) fn apply<T: FanControl>(
        io: &mut T,
        info: &[u8],
        status: &[u8],
        target: FanControlTarget,
        cancelled: &dyn Fn() -> bool,
    ) -> Result<FanControlWriteReport, Error> {
        validate_macos_fan_request(0, target)?;
        if cancelled() {
            return Err(Error::Custom(
                "fan control cancelled before snapshot".into(),
            ));
        }
        let original = io.get()?;
        let before = settings(info, status, &original)?;
        let mut wanted = original;
        match target {
            FanControlTarget::Percent(value) => {
                wanted[36..40].copy_from_slice(&1u32.to_le_bytes());
                // NVML rounds the Q16 fraction to nearest, not an integer PWM percent readback.
                let q16 = (value * 65536 + 50) / 100;
                wanted[40..44].copy_from_slice(&q16.to_le_bytes());
            }
            FanControlTarget::Auto => {
                let defaults = io.defaults()?;
                if defaults[..16] != original[..16]
                    || word(&defaults, 16) != 1
                    || defaults[20..36] != original[20..36]
                    || defaults[44..] != original[44..]
                    || word(&defaults, 36) != 0
                    || word(&defaults, 40) > 65536
                {
                    return Err(Error::Custom("fan default mode/record differs from validated reset contract; no SET issued".into()));
                }
                // GET-default selector is an input to GET. Keep the original ordinary
                // control header and preserve every calibration/other-control byte.
                wanted[36..44].copy_from_slice(&defaults[36..44]);
            }
        }
        write_block(
            io,
            original,
            wanted,
            cancelled,
            format!(
                "cooler0: original {} {} percent",
                before.mode, before.target_percent
            ),
        )?;
        Ok(FanControlWriteReport {
            slot: 0,
            original_mode: before.mode,
            original_target_percent: before.target_percent,
            applied_mode: mode(&wanted)?,
            applied_target_percent: f64::from(word(&wanted, 40)) * 100.0 / 65536.0,
            readback_verified: true,
            other_bytes_preserved: true,
        })
    }
    #[cfg(test)]
    mod tests {
        use super::*;
        use std::cell::Cell;
        struct Fake {
            b: [u8; BYTES],
            writes: usize,
            fail: bool,
            bad_default: bool,
        }
        fn fixture() -> (Vec<u8>, Vec<u8>, Fake) {
            let mut info = vec![0; INFO_BYTES];
            info[12] = 1;
            let mut status = vec![0; STATUS_BYTES];
            status[4] = 1;
            status[12..16].copy_from_slice(&0x303u32.to_le_bytes());
            status[28..32].copy_from_slice(&65536u32.to_le_bytes());
            let mut b = [0; BYTES];
            b[12] = 1;
            b[20] = 3;
            b[64] = 83;
            (
                info,
                status,
                Fake {
                    b,
                    writes: 0,
                    fail: false,
                    bad_default: false,
                },
            )
        }
        impl ControlBlock<BYTES> for Fake {
            fn get(&mut self) -> Result<[u8; BYTES], Error> {
                Ok(self.b)
            }
            fn set(&mut self, b: &[u8; BYTES]) -> Result<(), Error> {
                self.writes += 1;
                self.b = *b;
                if self.fail && self.writes == 1 {
                    Err(Error::Custom("SET applied then errored".into()))
                } else {
                    Ok(())
                }
            }
        }
        impl FanControl for Fake {
            fn defaults(&mut self) -> Result<[u8; BYTES], Error> {
                let mut b = self.b;
                b[16] = 1;
                b[36..44].fill(0);
                if self.bad_default {
                    b[64] ^= 1;
                }
                Ok(b)
            }
        }
        #[test]
        fn manual_percentage_is_q16_and_auto_preserves_all_other_bytes() {
            let (info, status, mut io) = fixture();
            let before = io.b;
            let r = apply(
                &mut io,
                &info,
                &status,
                FanControlTarget::Percent(35),
                &|| false,
            )
            .unwrap();
            assert_eq!(r.applied_mode, "manual");
            assert!((r.applied_target_percent - 35.0).abs() < 0.001);
            let mut wanted = before;
            wanted[36] = 1;
            wanted[40..44].copy_from_slice(&22938u32.to_le_bytes());
            assert_eq!(io.b, wanted);
            apply(&mut io, &info, &status, FanControlTarget::Auto, &|| false).unwrap();
            assert_eq!(io.b, before);
        }
        #[test]
        fn zero_rpm_is_a_present_cooler_and_feedback_is_not_the_target() {
            let (info, mut status, mut io) = fixture();
            status[32..36].copy_from_slice(&42224u32.to_le_bytes());
            io.b[36] = 1;
            io.b[40..44].copy_from_slice(&22938u32.to_le_bytes());
            let s = settings(&info, &status, &io.b).unwrap();
            assert_eq!(s.rpm, 0);
            assert!(s.current_percent > 64.0);
            assert!(s.target_percent < 35.001);
        }
        #[test]
        fn fan_errors_and_post_write_cancellation_restore_original_mode_and_target() {
            let (info, status, mut io) = fixture();
            let before = io.b;
            io.fail = true;
            assert!(
                apply(
                    &mut io,
                    &info,
                    &status,
                    FanControlTarget::Percent(35),
                    &|| false
                )
                .unwrap_err()
                .to_string()
                .contains("restored and verified")
            );
            assert_eq!(io.b, before);
            let (_, _, mut io) = fixture();
            let checks = Cell::new(0);
            assert!(
                apply(
                    &mut io,
                    &info,
                    &status,
                    FanControlTarget::Percent(35),
                    &|| {
                        checks.set(checks.get() + 1);
                        checks.get() == 3
                    }
                )
                .is_err()
            );
            assert_eq!(io.b, before);
        }
        #[test]
        fn changed_modes_masks_limits_or_default_calibration_do_not_write() {
            let (info, status, mut io) = fixture();
            io.b[36] = 2;
            assert!(apply(&mut io, &info, &status, FanControlTarget::Auto, &|| false).is_err());
            assert_eq!(io.writes, 0);
            io.b[36] = 0;
            io.bad_default = true;
            assert!(apply(&mut io, &info, &status, FanControlTarget::Auto, &|| false).is_err());
            assert_eq!(io.writes, 0);
            let mut bad = status;
            bad[28] = 1;
            assert!(settings(&info, &bad, &io.b).is_err());
            assert!(validate_macos_fan_request(1, FanControlTarget::Auto).is_err());
            assert!(validate_macos_fan_request(0, FanControlTarget::Percent(101)).is_err());
        }
    }
}
