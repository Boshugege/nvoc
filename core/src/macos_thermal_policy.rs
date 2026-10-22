//! Measured TU116 thermal-policy target control, separate from sensor limits.
use crate::Error;
use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct ThermalPolicyEntry {
    pub policy_index: u32,
    pub minimum_celsius: i32,
    pub default_celsius: i32,
    pub maximum_celsius: i32,
    pub current_celsius: i32,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct ThermalPolicySettings {
    pub gpu_max: ThermalPolicyEntry,
    pub acoustic: ThermalPolicyEntry,
}
impl ThermalPolicySettings {
    /// The same NVML threshold vocabulary; unlocated channels stay absent.
    pub fn thresholds(&self) -> Vec<crate::TemperatureThreshold> {
        [
            ("Shutdown", None),
            ("Slowdown", None),
            ("MemoryMax", None),
            ("GpuMax", Some(self.gpu_max.current_celsius as u32)),
            ("AcousticMin", Some(self.acoustic.minimum_celsius as u32)),
            ("AcousticCurr", Some(self.acoustic.current_celsius as u32)),
            ("AcousticMax", Some(self.acoustic.maximum_celsius as u32)),
            ("GpsCurr", None),
        ]
        .into_iter()
        .map(|(name, celsius)| crate::TemperatureThreshold { name, celsius })
        .collect()
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AcousticTarget {
    Celsius(i32),
    Default,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct AcousticTargetWriteReport {
    pub policy_index: u32,
    pub original_celsius: i32,
    pub applied_celsius: i32,
    pub readback_verified: bool,
    pub other_policies_preserved: bool,
    pub derived_fields_verified: bool,
}
pub fn validate_macos_acoustic_target(celsius: i32) -> Result<(), Error> {
    if !(65..=87).contains(&celsius) {
        return Err(Error::Custom(
            "macOS acoustic target must be within the measured policy's 65..87 Celsius window"
                .into(),
        ));
    }
    Ok(())
}
#[cfg(all(target_os = "macos", target_arch = "x86_64"))]
pub(crate) use control::{BYTES, INFO_BYTES, apply, settings};
#[cfg(any(test, all(target_os = "macos", target_arch = "x86_64")))]
mod control {
    use super::*;
    use crate::macos_control_transaction::{ControlBlock, write_block_with_readback};
    pub(crate) const BYTES: usize = 1440;
    pub(crate) const INFO_BYTES: usize = 1784;
    const ACOUSTIC: usize = 32 + 88 * 2;
    fn word(b: &[u8], at: usize) -> u32 {
        u32::from_le_bytes(b[at..at + 4].try_into().expect("validated size"))
    }
    fn put(b: &mut [u8], at: usize, v: u32) {
        b[at..at + 4].copy_from_slice(&v.to_le_bytes());
    }
    fn celsius(b: &[u8], at: usize) -> Result<i32, Error> {
        let fx8 = word(b, at);
        if fx8 > 127 * 256 || !fx8.is_multiple_of(256) {
            return Err(Error::Custom(
                "thermal policy temperature is outside the validated whole-Celsius FX8 encoding"
                    .into(),
            ));
        }
        Ok((fx8 / 256) as i32)
    }
    pub(crate) fn settings(
        info: &[u8],
        block: &[u8; BYTES],
    ) -> Result<ThermalPolicySettings, Error> {
        if info.len() != INFO_BYTES
            || word(info, 4) != 5
            || word(info, 8) != 5
            || info[16..20] != [255, 2, 255, 0]
            || word(block, 4) != 5
            || word(block, 8) != 5
        {
            return Err(Error::Custom(
                "thermal policy mask/selectors differ from the validated GPU-Max0/acoustic2 layout"
                    .into(),
            ));
        }
        let entry = |index: usize| -> Result<ThermalPolicyEntry, Error> {
            let at = 56 + 108 * index;
            let ctl = 32 + 88 * index;
            if word(info, at) != 4 || word(block, ctl) != 4 {
                return Err(Error::Custom(
                    "thermal policy record type differs from measured type4".into(),
                ));
            }
            let e = ThermalPolicyEntry {
                policy_index: index as u32,
                minimum_celsius: celsius(info, at + 8)?,
                default_celsius: celsius(info, at + 12)?,
                maximum_celsius: celsius(info, at + 16)?,
                current_celsius: celsius(block, ctl + 4)?,
            };
            if e.minimum_celsius > e.default_celsius
                || e.default_celsius > e.maximum_celsius
                || !(e.minimum_celsius..=e.maximum_celsius).contains(&e.current_celsius)
            {
                return Err(Error::Custom(
                    "invalid thermal policy bounds/current/default".into(),
                ));
            }
            Ok(e)
        };
        Ok(ThermalPolicySettings {
            gpu_max: entry(0)?,
            acoustic: entry(2)?,
        })
    }
    /// Driver-normalized control readback for the measured type4 acoustic policy.
    /// Only the primary target is sent in SET; these six additional dwords are
    /// recomputed by the driver. All other 1412 bytes must still compare exactly.
    fn expected(mut block: [u8; BYTES], target: i32) -> [u8; BYTES] {
        let fx8 = target as u32 * 256;
        for (at, value) in [
            (4, fx8),
            (28, fx8),
            (40, (3 * 87 * 256 + fx8) / 4),
            (68, fx8 + 256),
            (72, fx8),
            (76, fx8 - 256),
            (80, fx8 - 3 * 256),
        ] {
            put(&mut block, ACOUSTIC + at, value);
        }
        block
    }
    pub(crate) fn apply<T: ControlBlock<BYTES>>(
        io: &mut T,
        info: &[u8],
        target: AcousticTarget,
        cancelled: &dyn Fn() -> bool,
    ) -> Result<AcousticTargetWriteReport, Error> {
        if let AcousticTarget::Celsius(c) = target {
            validate_macos_acoustic_target(c)?;
        }
        if cancelled() {
            return Err(Error::Custom(
                "acoustic target cancelled before snapshot".into(),
            ));
        }
        let original = io.get()?;
        let before = settings(info, &original)?;
        if before.gpu_max.minimum_celsius != 88
            || before.gpu_max.default_celsius != 88
            || before.gpu_max.maximum_celsius != 88
            || before.acoustic.minimum_celsius != 65
            || before.acoustic.default_celsius != 83
            || before.acoustic.maximum_celsius != 87
            || celsius(info, 56 + 108 * 2 + 36)? != 83
            || celsius(info, 56 + 108 * 2 + 52)? != 86
            || celsius(info, 56 + 108 * 2 + 68)? != 87
            || expected(original, before.acoustic.current_celsius) != original
        {
            return Err(Error::Custom("thermal policy bounds/calibration or driver-derived fields differ from the validated model; no SET issued".into()));
        }
        let target = match target {
            AcousticTarget::Celsius(c) => c,
            AcousticTarget::Default => before.acoustic.default_celsius,
        };
        let mut wanted = original;
        put(&mut wanted, ACOUSTIC + 4, target as u32 * 256);
        let readback = expected(wanted, target);
        write_block_with_readback(
            io,
            original,
            wanted,
            readback,
            cancelled,
            format!(
                "acoustic policy2: original {} Celsius, target {target} Celsius",
                before.acoustic.current_celsius
            ),
        )?;
        Ok(AcousticTargetWriteReport {
            policy_index: 2,
            original_celsius: before.acoustic.current_celsius,
            applied_celsius: target,
            readback_verified: true,
            other_policies_preserved: true,
            derived_fields_verified: true,
        })
    }
    #[cfg(test)]
    mod tests {
        use super::*;
        use std::cell::Cell;
        struct Fake {
            block: [u8; BYTES],
            writes: usize,
            fail: bool,
            corrupt: bool,
        }
        impl ControlBlock<BYTES> for Fake {
            fn get(&mut self) -> Result<[u8; BYTES], Error> {
                Ok(self.block)
            }
            fn set(&mut self, b: &[u8; BYTES]) -> Result<(), Error> {
                self.writes += 1;
                self.block = expected(*b, celsius(b, ACOUSTIC + 4)?);
                if self.writes == 1 && self.corrupt {
                    self.block[36] ^= 1;
                }
                if self.writes == 1 && self.fail {
                    Err(Error::Custom("SET applied then errored".into()))
                } else {
                    Ok(())
                }
            }
        }
        fn fixture() -> (Vec<u8>, Fake) {
            let mut info = vec![0; INFO_BYTES];
            put(&mut info, 4, 5);
            put(&mut info, 8, 5);
            info[16..20].copy_from_slice(&[255, 2, 255, 0]);
            for (idx, values) in [(0, [88, 88, 88]), (2, [65, 83, 87])] {
                let at = 56 + 108 * idx;
                put(&mut info, at, 4);
                for (off, value) in [8, 12, 16].into_iter().zip(values) {
                    put(&mut info, at + off, value * 256);
                }
            }
            for (at, v) in [(36, 83), (52, 86), (68, 87)] {
                put(&mut info, 56 + 108 * 2 + at, v * 256);
            }
            let mut block = [0; BYTES];
            put(&mut block, 4, 5);
            put(&mut block, 8, 5);
            put(&mut block, 32, 4);
            put(&mut block, 36, 88 * 256);
            put(&mut block, ACOUSTIC, 4);
            block[400] = 91;
            (
                info,
                Fake {
                    block: expected(block, 83),
                    writes: 0,
                    fail: false,
                    corrupt: false,
                },
            )
        }
        #[test]
        fn thresholds_keep_gpu_max_acoustic_and_missing_gps_distinct() {
            let (info, io) = fixture();
            let s = settings(&info, &io.block).unwrap();
            let t = s.thresholds();
            assert_eq!(
                t.iter().find(|t| t.name == "GpuMax").unwrap().celsius,
                Some(88)
            );
            assert_eq!(
                t.iter().find(|t| t.name == "AcousticCurr").unwrap().celsius,
                Some(83)
            );
            assert_eq!(
                t.iter().find(|t| t.name == "GpsCurr").unwrap().celsius,
                None
            );
            assert_eq!(
                t.iter().find(|t| t.name == "Shutdown").unwrap().celsius,
                None
            );
        }
        #[test]
        fn set_reset_and_extremes_verify_driver_derived_fields_and_preserve_other_policies() {
            let (info, mut io) = fixture();
            let original = io.block;
            for target in [82, 81, 65, 87] {
                let r = apply(&mut io, &info, AcousticTarget::Celsius(target), &|| false).unwrap();
                assert_eq!(r.applied_celsius, target);
                assert_eq!(io.block, expected(original, target));
                assert_eq!(&io.block[..ACOUSTIC], &original[..ACOUSTIC]);
                apply(&mut io, &info, AcousticTarget::Default, &|| false).unwrap();
                assert_eq!(io.block, original);
            }
        }
        #[test]
        fn post_write_errors_unexpected_foreign_changes_and_cancellation_restore_every_byte() {
            for corrupt in [false, true] {
                let (info, mut io) = fixture();
                let original = io.block;
                io.fail = !corrupt;
                io.corrupt = corrupt;
                let e = apply(&mut io, &info, AcousticTarget::Celsius(82), &|| false).unwrap_err();
                assert!(e.to_string().contains("restored and verified"));
                assert_eq!(io.block, original);
            }
            let (info, mut io) = fixture();
            let original = io.block;
            let checks = Cell::new(0);
            assert!(
                apply(&mut io, &info, AcousticTarget::Celsius(82), &|| {
                    checks.set(checks.get() + 1);
                    checks.get() == 3
                })
                .is_err()
            );
            assert_eq!(io.block, original);
        }
        #[test]
        fn bad_metadata_original_calibration_fraction_or_outside_request_never_set() {
            let (info, mut io) = fixture();
            io.block[ACOUSTIC + 28] ^= 1;
            assert!(apply(&mut io, &info, AcousticTarget::Celsius(82), &|| false).is_err());
            assert_eq!(io.writes, 0);
            let (mut info, mut io) = fixture();
            info[17] = 0;
            assert!(apply(&mut io, &info, AcousticTarget::Default, &|| false).is_err());
            assert_eq!(io.writes, 0);
            for c in [-1, 64, 88, i32::MAX] {
                assert!(validate_macos_acoustic_target(c).is_err());
            }
            assert!(settings(&[], &io.block).is_err());
        }
    }
}
