//! Single-cooler NVML-equivalent percentage control on measured TU116 firmware.
use crate::Error;
use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FanControlTarget {
    Percent(u32),
    /// Physical RPM, translated to the duty fraction through the cooler's own
    /// published max RPM. The driver has no rpm channel of its own here (mode 3
    /// is accepted but normalises any value to a fixed 50 percent), so this is an
    /// open-loop translation: the request lands on the nearest duty step.
    Rpm(u32),
    Auto,
    /// Ask the firmware for its own zero-RPM stop mode instead of manual 0 percent.
    Stop,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct FanControlSettings {
    pub slot: u32,
    pub rpm: u32,
    /// Cooler max RPM as the driver calibrates it in the control record. Zero
    /// means the record published no scale, which is what auto mode looks like.
    pub max_rpm: u32,
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
    match target {
        FanControlTarget::Percent(value) if value > 100 => {
            return Err(Error::Custom(
                "fan target must be within 0..100 percent".into(),
            ));
        }
        // A ceiling for the argument itself; the real ceiling is the cooler's
        // own max RPM and is applied once the record has been read.
        FanControlTarget::Rpm(value) if value > 6000 => {
            return Err(Error::Custom(
                "fan target must be within 0..6000 rpm".into(),
            ));
        }
        _ => {}
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
    /// The duty feedback is a live, non-atomic measurement: it overshoots full
    /// scale by a few hundred Q16 units at a 100 percent request (measured
    /// 65559), and a block caught mid-transition has been seen holding an
    /// unrelated 893490. The structural check still runs on the selectors,
    /// minimum and maximum; the current reading gets this generous ceiling and
    /// is clamped back to 100 percent, so a transient cannot fail a command.
    const FEEDBACK_CEILING_Q16: u32 = 0x0020_0000;
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
            2 => Ok("fanstop"),
            _ => Err(Error::Custom(
                "fan control mode is outside the validated auto/manual/fanstop modes".into(),
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
            || word(status, 32) > FEEDBACK_CEILING_Q16
            || word(control, 40) > 65536
        {
            // Name the field that left the contract: a bare range complaint on a
            // live board does not say which side of the pair drifted.
            return Err(Error::Custom(format!(
                "fan percentage range or value differs from validated 0..100 percent (status min {} / max {} / current {}, control target {})",
                word(status, 24),
                word(status, 28),
                word(status, 32),
                word(control, 40)
            )));
        }
        Ok(FanControlSettings {
            slot: 0,
            rpm: word(status, 20),
            max_rpm: word(control, 32),
            min_percent: 0.0,
            max_percent: 100.0,
            current_percent: f64::from(word(status, 32).min(65536)) * 100.0 / 65536.0,
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
            FanControlTarget::Rpm(value) => {
                let max_rpm = word(&original, 32);
                if max_rpm < 500 {
                    return Err(Error::Custom(format!(
                        "fan control record publishes no cooler max rpm (read {max_rpm}); an rpm target has no scale"
                    )));
                }
                let value = value.min(max_rpm);
                wanted[36..40].copy_from_slice(&1u32.to_le_bytes());
                // Round to nearest and clamp at full scale: the driver only takes
                // a Q16 duty fraction, so the request cannot land below or above it.
                let q16 = ((u64::from(value) * 65536 + u64::from(max_rpm) / 2) / u64::from(max_rpm))
                    .min(65536) as u32;
                wanted[40..44].copy_from_slice(&q16.to_le_bytes());
            }
            FanControlTarget::Stop => {
                // Mode 2 is what the driver accepts for "let the fan stop"; the target
                // is written as 0 because a stopped curve has no duty to ask for.
                wanted[36..40].copy_from_slice(&2u32.to_le_bytes());
                wanted[40..44].copy_from_slice(&0u32.to_le_bytes());
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
            b[32..36].copy_from_slice(&2800u32.to_le_bytes());
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
        fn fan_stop_writes_mode_two_with_a_zero_target_and_restores_exactly() {
            let (info, status, mut io) = fixture();
            let before = io.b;
            let r = apply(&mut io, &info, &status, FanControlTarget::Stop, &|| false).unwrap();
            assert_eq!(r.applied_mode, "fanstop");
            assert_eq!(r.applied_target_percent, 0.0);
            let mut wanted = before;
            wanted[36..40].copy_from_slice(&2u32.to_le_bytes());
            wanted[40..44].copy_from_slice(&0u32.to_le_bytes());
            assert_eq!(io.b, wanted);
            // The reader has to name the mode it just wrote, or a stopped fan reads as
            // "outside the validated modes" on the next snapshot.
            let reading = settings(&info, &status, &io.b).unwrap();
            assert_eq!(reading.mode, "fanstop");
            apply(&mut io, &info, &status, FanControlTarget::Auto, &|| false).unwrap();
            assert_eq!(io.b, before);
        }
        #[test]
        fn stop_from_a_stopped_block_sends_no_set_and_an_unknown_mode_is_named() {
            let (info, status, mut io) = fixture();
            io.b[36..40].copy_from_slice(&2u32.to_le_bytes());
            io.b[40..44].copy_from_slice(&0u32.to_le_bytes());
            apply(&mut io, &info, &status, FanControlTarget::Stop, &|| false).unwrap();
            assert_eq!(io.writes, 0, "an identical block must not be re-sent");
            io.b[36..40].copy_from_slice(&7u32.to_le_bytes());
            let err = settings(&info, &status, &io.b).unwrap_err().to_string();
            assert!(err.contains("auto/manual/fanstop"), "{err}");
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
        fn full_scale_feedback_may_overshoot_and_is_clamped_back_to_100() {
            let (info, mut status, mut io) = fixture();
            io.b[36..40].copy_from_slice(&1u32.to_le_bytes());
            io.b[40..44].copy_from_slice(&65536u32.to_le_bytes());
            status[32..36].copy_from_slice(&65559u32.to_le_bytes());
            let s = settings(&info, &status, &io.b).unwrap();
            assert_eq!(s.current_percent, 100.0);
            assert_eq!(s.target_percent, 100.0);
            // A block caught mid-transition has held an unrelated large value.
            status[32..36].copy_from_slice(&893_490u32.to_le_bytes());
            assert_eq!(
                settings(&info, &status, &io.b).unwrap().current_percent,
                100.0
            );
            // Far above the ceiling is still a different contract.
            status[32..36].copy_from_slice(&0x0021_0000u32.to_le_bytes());
            let err = settings(&info, &status, &io.b).unwrap_err().to_string();
            assert!(err.contains("current 2162688"), "{err}");
        }
        #[test]
        fn rpm_targets_are_translated_through_the_records_own_max_rpm() {
            let (info, status, mut io) = fixture();
            let before = io.b;
            // Half of the published 2800 rpm is exactly mid scale, rounded.
            let r = apply(
                &mut io,
                &info,
                &status,
                FanControlTarget::Rpm(1400),
                &|| false,
            )
            .unwrap();
            assert_eq!(r.applied_mode, "manual");
            assert!((r.applied_target_percent - 50.0).abs() < 0.001);
            let mut wanted = before;
            wanted[36..40].copy_from_slice(&1u32.to_le_bytes());
            wanted[40..44].copy_from_slice(&32768u32.to_le_bytes());
            assert_eq!(io.b, wanted);
            // Above the ceiling clamps to full scale instead of overflowing 100 percent.
            apply(
                &mut io,
                &info,
                &status,
                FanControlTarget::Rpm(6000),
                &|| false,
            )
            .unwrap();
            assert_eq!(u32::from_le_bytes(io.b[40..44].try_into().unwrap()), 65536);
            apply(&mut io, &info, &status, FanControlTarget::Auto, &|| false).unwrap();
            assert_eq!(io.b, before);
        }
        #[test]
        fn an_rpm_target_without_a_published_scale_is_refused_before_the_set() {
            let (info, status, mut io) = fixture();
            io.b[32..36].fill(0);
            let err = apply(
                &mut io,
                &info,
                &status,
                FanControlTarget::Rpm(1200),
                &|| false,
            )
            .unwrap_err()
            .to_string();
            assert!(err.contains("no cooler max rpm"), "{err}");
            assert_eq!(io.writes, 0);
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
            io.b[36] = 7;
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
