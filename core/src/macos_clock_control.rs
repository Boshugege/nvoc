//! Transaction for the measured selector-zero additive clock delta.
//! Native I/O lives in macos_rm; this module also exercises failures without GPU writes.
use serde::Serialize;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ClockOffsetWriteReport {
    pub domain: &'static str,
    pub original_khz: i32,
    pub applied_khz: i32,
    pub readback_verified: bool,
    pub other_bytes_preserved: bool,
}

#[cfg(all(target_os = "macos", target_arch = "x86_64"))]
pub(crate) use transaction::{BYTES, ClockControl, apply};

#[cfg(any(test, all(target_os = "macos", target_arch = "x86_64")))]
mod transaction {
    use super::*;
    use crate::Error;
    pub(crate) const BYTES: usize = 2108;
    const RECORD: usize = 60; // Graphics is directory index zero on the validated TU116.
    const VALUE: usize = RECORD + 12;

    pub(crate) trait ClockControl {
        fn get(&mut self) -> Result<[u8; BYTES], Error>;
        fn set(&mut self, block: &[u8; BYTES]) -> Result<(), Error>;
    }

    fn value(block: &[u8; BYTES]) -> i32 {
        i32::from_le_bytes(block[VALUE..VALUE + 4].try_into().expect("fixed size"))
    }

    struct Rollback<'a, T: ClockControl> {
        io: &'a mut T,
        original: [u8; BYTES],
        armed: bool,
    }

    impl<T: ClockControl> Rollback<'_, T> {
        fn restore(&mut self) -> Result<(), Error> {
            // One bounded retry; verify even if SET reports refusal, since it may
            // have done nothing or applied the bytes before returning an error.
            let mut detail = String::new();
            for _ in 0..2 {
                let set = self.io.set(&self.original);
                match self.io.get() {
                    Ok(block) if block == self.original => {
                        self.armed = false;
                        return Ok(());
                    }
                    result => detail = format!("SET {set:?}, readback {result:?}"),
                }
            }
            Err(Error::Custom(format!(
                "RESTORE NOT VERIFIED for graphics: original {} kHz; {detail}",
                value(&self.original)
            )))
        }
    }

    impl<T: ClockControl> Drop for Rollback<'_, T> {
        fn drop(&mut self) {
            if self.armed {
                // Explicit paths report restore failures; Drop only covers unwinding.
                // SIGKILL, process abort and power loss cannot execute this fallback.
                if let Err(error) = self.restore() {
                    eprintln!("macOS RM: {error}");
                }
            }
        }
    }

    pub(crate) fn apply<T: ClockControl>(
        io: &mut T,
        target: i32,
        cancelled: &dyn Fn() -> bool,
    ) -> Result<ClockOffsetWriteReport, Error> {
        if !(-18_750..=18_750).contains(&target) {
            return Err(Error::Custom(
                "additive clock target outside -18750..18750 kHz".into(),
            ));
        }
        if cancelled() {
            return Err(Error::Custom(
                "clock offset cancelled before snapshot".into(),
            ));
        }
        let original = io.get()?;
        if original[4..8] != 1u32.to_le_bytes()
            || original[RECORD..RECORD + 2] != [8, 8]
            || original[RECORD + 8] != 0
        {
            return Err(Error::Custom(
                "graphics control identity/type/additive selector mismatch; no SET issued".into(),
            ));
        }
        let original_khz = value(&original);
        if !(-18_750..=18_750).contains(&original_khz) {
            return Err(Error::Custom(
                "original clock offset outside validated restore range; no SET issued".into(),
            ));
        }
        if (i64::from(target) - i64::from(original_khz)).abs() > 18_750 {
            return Err(Error::Custom(
                "clock step exceeds validated 18750 kHz magnitude; use an intermediate target"
                    .into(),
            ));
        }
        let report = ClockOffsetWriteReport {
            domain: "graphics",
            original_khz,
            applied_khz: target,
            readback_verified: true,
            other_bytes_preserved: true,
        };
        if cancelled() {
            return Err(Error::Custom("clock offset cancelled before SET".into()));
        }
        if target == original_khz {
            return Ok(report);
        }
        let mut wanted = original;
        wanted[VALUE..VALUE + 4].copy_from_slice(&target.to_le_bytes());
        let mut rollback = Rollback {
            io,
            original,
            armed: true,
        };
        let outcome = (|| {
            rollback.io.set(&wanted)?;
            let after = rollback.io.get()?;
            if after != wanted {
                return Err(Error::Custom(format!(
                    "clock readback mismatch: wanted {target} kHz, saw {} kHz or changed other bytes",
                    value(&after)
                )));
            }
            if cancelled() {
                return Err(Error::Custom("clock offset cancelled after SET".into()));
            }
            Ok(())
        })();
        match outcome {
            Ok(()) => {
                rollback.armed = false; // Successful ordinary SET persists like existing backends.
                Ok(report)
            }
            Err(error) => {
                let restored = rollback.restore();
                // Explicit retries are complete. Do not hide a third write in Drop.
                rollback.armed = false;
                match restored {
                    Ok(()) => Err(Error::Custom(format!(
                        "{error}; original block restored and verified"
                    ))),
                    Err(restore) => Err(Error::Custom(format!("{error}; {restore}"))),
                }
            }
        }
    }

    #[cfg(test)]
    mod tests {
        use super::*;
        use std::cell::Cell;

        struct Fake {
            block: [u8; BYTES],
            writes: usize,
            gets: usize,
            fault: &'static str,
        }
        fn fake(fault: &'static str) -> Fake {
            let mut block = [0u8; BYTES];
            block[4..8].copy_from_slice(&1u32.to_le_bytes());
            block[RECORD..RECORD + 2].copy_from_slice(&[8, 8]);
            // Nonzero unrelated bytes must survive both success and rollback.
            block[RECORD + 28] = 41;
            Fake {
                block,
                writes: 0,
                gets: 0,
                fault,
            }
        }
        impl ClockControl for Fake {
            fn get(&mut self) -> Result<[u8; BYTES], Error> {
                self.gets += 1;
                if self.fault == "snapshot" || self.fault == "get" && self.gets == 2 {
                    return Err(Error::Custom("GET failed".into()));
                }
                let mut result = self.block;
                if self.fault == "mismatch" && self.gets == 2 {
                    result[RECORD + 28] ^= 1;
                }
                Ok(result)
            }
            fn set(&mut self, block: &[u8; BYTES]) -> Result<(), Error> {
                self.writes += 1;
                if self.fault == "restore" && self.writes > 1 {
                    return Err(Error::Custom("restore refused".into()));
                }
                self.block = *block;
                if (self.fault == "set" || self.fault == "restore") && self.writes == 1 {
                    return Err(Error::Custom("SET failed after applying bytes".into()));
                }
                Ok(())
            }
        }
        #[test]
        fn success_is_an_absolute_target_and_preserves_every_other_byte() {
            let mut io = fake("");
            io.block[VALUE..VALUE + 4].copy_from_slice(&250i32.to_le_bytes());
            let before = io.block;
            let report = apply(&mut io, 15_000, &|| false).unwrap();
            assert_eq!((report.original_khz, report.applied_khz), (250, 15_000));
            assert_eq!(io.writes, 1);
            let mut expected = before;
            expected[VALUE..VALUE + 4].copy_from_slice(&15_000i32.to_le_bytes());
            assert_eq!(io.block, expected);
            apply(&mut io, 250, &|| false).unwrap();
            assert_eq!(io.block, before);
        }
        #[test]
        fn failing_snapshot_or_invalid_mode_never_sends_set() {
            let mut io = fake("snapshot");
            assert!(apply(&mut io, 250, &|| false).is_err());
            assert_eq!(io.writes, 0);
            let mut io = fake("");
            io.block[RECORD + 8] = 1;
            assert!(apply(&mut io, 250, &|| false).is_err());
            assert_eq!(io.writes, 0);
        }
        #[test]
        fn set_failure_get_failure_and_collateral_change_restore_the_snapshot() {
            for fault in ["set", "get", "mismatch"] {
                let mut io = fake(fault);
                let original = io.block;
                let error = apply(&mut io, 250, &|| false).unwrap_err().to_string();
                assert!(error.contains("restored and verified"), "{fault}: {error}");
                assert_eq!(io.block, original);
                assert_eq!(io.writes, 2);
            }
        }
        #[test]
        fn cancellation_after_set_restores_and_failure_is_explicit() {
            let mut io = fake("");
            let original = io.block;
            let checks = Cell::new(0);
            let error = apply(&mut io, 250, &|| {
                checks.set(checks.get() + 1);
                checks.get() == 3
            })
            .unwrap_err();
            assert!(error.to_string().contains("cancelled after SET"));
            assert_eq!(io.block, original);
            let mut io = fake("restore");
            let error = apply(&mut io, 250, &|| false).unwrap_err().to_string();
            assert!(error.contains("RESTORE NOT VERIFIED"), "{error}");
            assert_eq!(io.writes, 3, "one SET and two explicit restore attempts");
            assert_eq!(value(&io.block), 250);
        }
        #[test]
        fn bounds_and_cancellation_are_checked_before_writing() {
            let mut io = fake("");
            assert!(apply(&mut io, 18_751, &|| false).is_err());
            assert!(apply(&mut io, 250, &|| true).is_err());
            assert_eq!((io.writes, io.gets), (0, 0));
            apply(&mut io, 0, &|| false).unwrap();
            assert_eq!(io.writes, 0);
        }
    }
}
