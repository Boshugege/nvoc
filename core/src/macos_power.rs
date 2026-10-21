//! Board power policy decoded from the measured Linux610 NVML/RM contract.
use crate::Error;
use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct PowerPolicySettings {
    pub policy_index: u8,
    pub current_mw: u32,
    pub minimum_mw: u32,
    pub default_mw: u32,
    pub maximum_mw: u32,
}
#[derive(Debug, Clone, Copy)]
pub enum PowerLimitTarget {
    Milliwatts(u32),
    Default,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PowerLimitWriteReport {
    pub original_mw: u32,
    pub applied_mw: u32,
    pub readback_verified: bool,
    pub other_bytes_preserved: bool,
}
/// The currently validated TU116 policy window; checked again from live metadata.
pub fn validate_macos_power_limit_watts(watts: u32) -> Result<u32, Error> {
    if !(70..=100).contains(&watts) {
        return Err(Error::Custom(
            "macOS board power limit must be within the validated 70..100 W window".into(),
        ));
    }
    Ok(watts * 1000)
}

#[cfg(all(target_os = "macos", target_arch = "x86_64"))]
pub(crate) use control::{BYTES, INFO_BYTES, apply, parse_info};

#[cfg(any(test, all(target_os = "macos", target_arch = "x86_64")))]
mod control {
    use super::*;
    use crate::macos_control_transaction::{ControlBlock, write_block};
    pub(crate) const BYTES: usize = 12;
    pub(crate) const INFO_BYTES: usize = 20_000;
    fn word(block: &[u8], at: usize) -> u32 {
        u32::from_le_bytes(block[at..at + 4].try_into().expect("validated block size"))
    }
    pub(crate) fn parse_info(block: &[u8]) -> Result<PowerPolicySettings, Error> {
        if block.len() != INFO_BYTES {
            return Err(Error::Custom("power policy info size mismatch".into()));
        }
        // Linux610 cDeviceGetPowerManagementLimitInfo chooses these in order.
        let index = [block[152], block[154], block[183]]
            .into_iter()
            .find(|i| *i != 0xff)
            .ok_or_else(|| Error::Custom("no board power policy selector".into()))?;
        if index >= 32 || word(block, 4) & (1u32 << index) == 0 {
            return Err(Error::Custom(
                "board power policy selector is not in the published mask".into(),
            ));
        }
        let at = 208 + 252 * usize::from(index);
        if block[at] != 0 || block[at + 1] != 9 || block[at + 2] != 0 {
            return Err(Error::Custom(
                "board power policy type/units differ from the measured milliwatt board policy"
                    .into(),
            ));
        }
        let info = PowerPolicySettings {
            policy_index: index,
            current_mw: 0,
            minimum_mw: word(block, at + 4),
            default_mw: word(block, at + 8),
            maximum_mw: word(block, at + 12),
        };
        if info.minimum_mw == 0
            || info.minimum_mw > info.default_mw
            || info.default_mw > info.maximum_mw
        {
            return Err(Error::Custom("invalid board power policy window".into()));
        }
        Ok(info)
    }
    pub(crate) fn current(block: &[u8; BYTES]) -> Result<u32, Error> {
        if block[..4] != [1, 0xfe, 0, 0] || word(block, 8) != 0 {
            return Err(Error::Custom(
                "board power policy/client selector or reserved fields differ; no SET issued"
                    .into(),
            ));
        }
        Ok(word(block, 4))
    }
    pub(crate) fn apply<T: ControlBlock<BYTES>>(
        io: &mut T,
        window: PowerPolicySettings,
        target: PowerLimitTarget,
        cancelled: &dyn Fn() -> bool,
    ) -> Result<PowerLimitWriteReport, Error> {
        // The measured card's own window, not a guessed rated-TDP percentage.
        if window.policy_index != 2
            || window.minimum_mw != 70_000
            || window.default_mw != 100_000
            || window.maximum_mw != 100_000
        {
            return Err(Error::Custom(
                "power policy differs from the validated TU116 70/100/100 W window; no SET issued"
                    .into(),
            ));
        }
        let target = match target {
            PowerLimitTarget::Milliwatts(v) => v,
            PowerLimitTarget::Default => window.default_mw,
        };
        if !(window.minimum_mw..=window.maximum_mw).contains(&target) {
            return Err(Error::Custom(
                "power target outside the live policy window; no SET issued".into(),
            ));
        }
        if cancelled() {
            return Err(Error::Custom(
                "power limit cancelled before snapshot".into(),
            ));
        }
        let original = io.get()?;
        let original_mw = current(&original)?;
        if !(window.minimum_mw..=window.maximum_mw).contains(&original_mw) {
            return Err(Error::Custom(
                "original power limit outside validated restore window; no SET issued".into(),
            ));
        }
        let mut wanted = original;
        wanted[4..8].copy_from_slice(&target.to_le_bytes());
        write_block(
            io,
            original,
            wanted,
            cancelled,
            format!("board power limit: original {original_mw} mW, target {target} mW"),
        )?;
        Ok(PowerLimitWriteReport {
            original_mw,
            applied_mw: target,
            readback_verified: true,
            other_bytes_preserved: true,
        })
    }

    #[cfg(test)]
    mod tests {
        use super::*;
        use std::cell::Cell;
        fn window() -> PowerPolicySettings {
            PowerPolicySettings {
                policy_index: 2,
                current_mw: 100_000,
                minimum_mw: 70_000,
                default_mw: 100_000,
                maximum_mw: 100_000,
            }
        }
        struct Fake {
            block: [u8; BYTES],
            writes: usize,
            fail_after_write: bool,
        }
        fn fake() -> Fake {
            let mut block = [0; BYTES];
            block[..2].copy_from_slice(&[1, 0xfe]);
            block[4..8].copy_from_slice(&100_000u32.to_le_bytes());
            Fake {
                block,
                writes: 0,
                fail_after_write: false,
            }
        }
        impl ControlBlock<BYTES> for Fake {
            fn get(&mut self) -> Result<[u8; BYTES], Error> {
                Ok(self.block)
            }
            fn set(&mut self, block: &[u8; BYTES]) -> Result<(), Error> {
                self.writes += 1;
                self.block = *block;
                if self.fail_after_write && self.writes == 1 {
                    Err(Error::Custom("applied then failed".into()))
                } else {
                    Ok(())
                }
            }
        }
        #[test]
        fn policy_selector_units_and_window_come_from_the_named_record() {
            let mut b = vec![0u8; INFO_BYTES];
            b[4] = 4;
            b[152] = 2;
            b[154] = 0xff;
            b[183] = 0xff;
            let at = 208 + 252 * 2;
            b[at + 1] = 9;
            for (off, v) in [(4, 70_000u32), (8, 100_000), (12, 100_000)] {
                b[at + off..at + off + 4].copy_from_slice(&v.to_le_bytes());
            }
            assert_eq!(parse_info(&b).unwrap().minimum_mw, 70_000);
            b[at + 2] = 1;
            assert!(parse_info(&b).is_err());
            b[at + 2] = 0;
            b[4] = 0;
            assert!(parse_info(&b).is_err());
            b[4] = 4;
            b[152] = 0xff;
            b[154] = 2;
            assert_eq!(parse_info(&b).unwrap().policy_index, 2);
            b[154] = 0xff;
            assert!(parse_info(&b).is_err());
            assert!(parse_info(&b[..100]).is_err());
        }
        #[test]
        fn absolute_power_target_and_default_reset_preserve_the_other_bytes() {
            let mut io = fake();
            let original = io.block;
            let report = apply(
                &mut io,
                window(),
                PowerLimitTarget::Milliwatts(99_000),
                &|| false,
            )
            .unwrap();
            assert_eq!((report.original_mw, report.applied_mw), (100_000, 99_000));
            let mut expected = original;
            expected[4..8].copy_from_slice(&99_000u32.to_le_bytes());
            assert_eq!(io.block, expected);
            apply(&mut io, window(), PowerLimitTarget::Default, &|| false).unwrap();
            assert_eq!(io.block, original);
        }
        #[test]
        fn power_error_and_cancellation_use_the_same_exact_restore_transaction() {
            let mut io = fake();
            let original = io.block;
            io.fail_after_write = true;
            assert!(
                apply(
                    &mut io,
                    window(),
                    PowerLimitTarget::Milliwatts(99_000),
                    &|| false
                )
                .unwrap_err()
                .to_string()
                .contains("restored and verified")
            );
            assert_eq!(io.block, original);
            let mut io = fake();
            let checks = Cell::new(0);
            assert!(
                apply(
                    &mut io,
                    window(),
                    PowerLimitTarget::Milliwatts(99_000),
                    &|| {
                        checks.set(checks.get() + 1);
                        checks.get() == 3
                    }
                )
                .is_err()
            );
            assert_eq!(io.block, original);
            io.block[1] = 0xfd;
            assert!(
                apply(
                    &mut io,
                    window(),
                    PowerLimitTarget::Milliwatts(99_000),
                    &|| false
                )
                .is_err()
            );
        }
        #[test]
        fn invalid_requests_and_changed_limits_never_write() {
            let mut io = fake();
            assert!(
                apply(
                    &mut io,
                    window(),
                    PowerLimitTarget::Milliwatts(100_001),
                    &|| false
                )
                .is_err()
            );
            let mut w = window();
            w.maximum_mw = 110_000;
            assert!(apply(&mut io, w, PowerLimitTarget::Default, &|| false).is_err());
            assert_eq!(io.writes, 0);
            for watts in [0, 69, 101, u32::MAX] {
                assert!(validate_macos_power_limit_watts(watts).is_err());
            }
            assert_eq!(validate_macos_power_limit_watts(99).unwrap(), 99_000);
        }
    }
}
#[cfg(all(target_os = "macos", target_arch = "x86_64"))]
pub(crate) use control::current;
