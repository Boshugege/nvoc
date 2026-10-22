//! PERF power-state source request (PERF_GET/SET_POWERSTATE).
//!
//! The pair is `0x2080205a` GET / `0x2080205b` SET, a single 4-byte dword drawn
//! from the published `NV2080_CTRL_PERF_POWER_SOURCE` enum: AC(0) or BATTERY(1).
//! Both ids are in the validated GSP method table at size 4 with the SET-able
//! flag the other writable perf controls carry, and the driver echoes the stored
//! dword in either direction -- which is what makes an exact readback possible
//! for a control whose only parameter is the value itself.
//!
//! Honest boundary: this is the *source* the performance controller is told
//! about, not a clock or a voltage. Measured on the validated host, BATTERY is
//! accepted and echoes back, but a constant NVK load stayed at 1950 MHz in both
//! states and `PERF_GET_CURRENT_PSTATE` stayed at P0 throughout, so no clock
//! effect is claimed here. The neighbouring P-state force attempts are recorded
//! in the matrix: `PERF_SET_AUX_POWER_STATE` (`0x20802092`, P0..P4) is accepted
//! with NV_OK for every level and changes nothing on this desktop either, and
//! the NVML range-lock carrier (`0x2080e078`, 16 B count+pointer) answers
//! NV_ERR_NOT_SUPPORTED (0x56), so it is not attempted here.

use crate::Error;

/// The whole request is one dword.
pub const BYTES: usize = 4;

/// `NV2080_CTRL_PERF_POWER_SOURCE`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PowerState {
    Ac,
    Battery,
}

impl PowerState {
    pub const ALL: [Self; 2] = [Self::Ac, Self::Battery];

    pub fn code(self) -> u32 {
        match self {
            Self::Ac => 0,
            Self::Battery => 1,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Self::Ac => "ac",
            Self::Battery => "battery",
        }
    }

    /// The platform default on a desktop host: the POST state is AC.
    pub const DEFAULT: Self = Self::Ac;

    pub fn parse(text: &str) -> Result<Self, Error> {
        let folded = text.trim().to_ascii_lowercase().replace('_', "-");
        let t = folded.strip_suffix("-power").unwrap_or(&folded);
        Self::ALL
            .into_iter()
            .find(|s| s.name().eq_ignore_ascii_case(t))
            .ok_or_else(|| {
                Error::Custom(format!("unknown power state {text:?}; expected ac|battery"))
            })
    }

    pub fn from_code(code: u32) -> Option<Self> {
        Self::ALL.into_iter().find(|s| s.code() == code)
    }
}

/// What the reply says the source is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PowerStateSettings {
    pub state: PowerState,
}

/// Proof of the round trip: the state the write replaced, and the one it left.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PowerStateWriteReport {
    pub original: PowerState,
    pub applied: PowerState,
}

fn word(block: &[u8; BYTES]) -> u32 {
    u32::from_le_bytes(*block)
}

/// Decode the reply. The driver reports exactly the two published values; a
/// third value would mean the enum moved, so it is an error rather than a
/// best-effort guess.
pub fn parse(block: &[u8; BYTES]) -> Result<PowerStateSettings, Error> {
    let code = word(block);
    let state = PowerState::from_code(code).ok_or_else(|| {
        Error::Custom(format!(
            "power-state reply {code} is outside the published AC(0)/BATTERY(1) enum"
        ))
    })?;
    Ok(PowerStateSettings { state })
}

/// Read the current source without writing anything.
pub fn read<T: crate::macos_control_transaction::ControlBlock<BYTES>>(
    io: &mut T,
) -> Result<PowerStateSettings, Error> {
    let block = io.get()?;
    parse(&block)
}

/// Ask for `state`. Errors and cancellation restore the original dword;
/// success leaves the new state standing (the driver keeps it, the reset form
/// sends AC back).
pub fn set<T: crate::macos_control_transaction::ControlBlock<BYTES>>(
    io: &mut T,
    state: PowerState,
    cancelled: &dyn Fn() -> bool,
) -> Result<PowerStateWriteReport, Error> {
    if cancelled() {
        return Err(Error::Custom(
            "power-state request cancelled before snapshot".into(),
        ));
    }
    let original = io.get()?;
    let before = parse(&original)?;
    let wanted = state.code().to_le_bytes();
    crate::macos_control_transaction::write_block(
        io,
        original,
        wanted,
        cancelled,
        format!(
            "power-state: original {}, target {}",
            before.state.name(),
            state.name()
        ),
    )?;
    Ok(PowerStateWriteReport {
        original: before.state,
        applied: state,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Fake {
        block: [u8; BYTES],
        fail_set: Option<u32>,
        sets: u32,
    }

    impl Fake {
        fn new(state: u32) -> Self {
            Self {
                block: state.to_le_bytes(),
                fail_set: None,
                sets: 0,
            }
        }
    }

    impl crate::macos_control_transaction::ControlBlock<BYTES> for Fake {
        fn get(&mut self) -> Result<[u8; BYTES], Error> {
            Ok(self.block)
        }
        fn set(&mut self, block: &[u8; BYTES]) -> Result<(), Error> {
            self.sets += 1;
            if let Some(status) = self.fail_set.take() {
                return Err(Error::Custom(format!("SET failed: RM 0x{status:08x}")));
            }
            self.block = *block;
            Ok(())
        }
    }

    #[test]
    fn parses_both_published_sources_and_refuses_a_third_value() {
        assert_eq!(parse(&0u32.to_le_bytes()).unwrap().state, PowerState::Ac);
        assert_eq!(
            parse(&1u32.to_le_bytes()).unwrap().state,
            PowerState::Battery
        );
        assert!(parse(&2u32.to_le_bytes()).is_err());
        assert!(parse(&0xffff_ffffu32.to_le_bytes()).is_err());
    }

    #[test]
    fn parses_the_cli_spellings_in_either_case() {
        assert_eq!(PowerState::parse("AC").unwrap(), PowerState::Ac);
        assert_eq!(PowerState::parse(" battery ").unwrap(), PowerState::Battery);
        assert_eq!(
            PowerState::parse("battery-power").unwrap(),
            PowerState::Battery
        );
        assert!(PowerState::parse("dc").is_err());
    }

    #[test]
    fn set_writes_only_the_value_and_reports_what_it_replaced() {
        let mut io = Fake::new(0);
        let report = set(&mut io, PowerState::Battery, &|| false).unwrap();
        assert_eq!(report.original, PowerState::Ac);
        assert_eq!(report.applied, PowerState::Battery);
        assert_eq!(io.block, PowerState::Battery.code().to_le_bytes());
        assert_eq!(io.sets, 1);
    }

    #[test]
    fn asking_for_the_state_that_is_already_set_is_a_no_op_but_still_reports() {
        // The shared transaction skips the SET when the wanted block equals the
        // snapshot; the report still names what was replaced, so callers do not
        // have to special-case a redundant request.
        let mut io = Fake::new(1);
        let report = set(&mut io, PowerState::Battery, &|| false).unwrap();
        assert_eq!(report.original, PowerState::Battery);
        assert_eq!(report.applied, PowerState::Battery);
        assert_eq!(io.sets, 0);
        assert_eq!(io.block, PowerState::Battery.code().to_le_bytes());
    }

    #[test]
    fn an_error_after_the_write_restores_the_original_dword() {
        struct FailAfter {
            block: [u8; BYTES],
        }
        impl crate::macos_control_transaction::ControlBlock<BYTES> for FailAfter {
            fn get(&mut self) -> Result<[u8; BYTES], Error> {
                Ok(self.block)
            }
            fn set(&mut self, _block: &[u8; BYTES]) -> Result<(), Error> {
                Err(Error::Custom("boom".into()))
            }
        }
        let mut failing = FailAfter { block: [0; BYTES] };
        let result = set(&mut failing, PowerState::Battery, &|| false);
        // The transaction had to restore, and the transport refuses SET, so it
        // must surface the failure instead of claiming success.
        assert!(result.is_err());
    }

    #[test]
    fn cancellation_before_the_snapshot_does_not_write() {
        let mut io = Fake::new(0);
        let result = set(&mut io, PowerState::Battery, &|| true);
        assert!(result.is_err());
        assert_eq!(io.sets, 0);
        assert_eq!(io.block, PowerState::Ac.code().to_le_bytes());
    }
}
