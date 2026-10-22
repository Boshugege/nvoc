//! RATED_TDP arbitration requests (PERF_RATED_TDP_SET_CONTROL).
//!
//! The public pair is `0x2080206e` GET / `0x2080206f` SET, both at the
//! 12-byte `{client, action, vPstate}` trio. Measured trap: the SDK also
//! publishes an a0-shaped legacy twin (`0x2080a06e` / `0x2080e06f`) and it
//! answers NV_OK, but the a0 GET returns a *stub* block -- client 0, action
//! `0xFFFFFFFF`, vPstate 0 -- for every client asked for, so a read routed
//! through it looks like "nothing is set" and never matches the SET readback.
//! The class-20 ids return the real trio; only `0x2080206e` is used to read.
//!
//! The request is *arbitration*, not wattage: it names a client and one action
//! against the rated TDP. Measured on the validated host, `GLOBAL:FORCE_LIMIT`
//! held under a constant NVK load cut draw from 12.87-13.46 W to 11.95-11.98 W
//! and clamped the graphics clock to 1755 MHz, while RUSD's `enforcedmW` stayed
//! 100000 the whole time -- so the enforced wall is *not* the readback of this
//! control (matrix section 143).

use crate::Error;

/// Both tiers carry the same 12-byte trio: client, action, vPstate.
pub const BYTES: usize = 12;

/// `NV2080_CTRL_PERF_RATED_TDP_CLIENT`. `RM` (0) is RM-internal and refused by
/// the driver, so it is not offered here.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RatedTdpClient {
    War,
    Global,
    Os,
    Profile,
}

impl RatedTdpClient {
    pub const ALL: [Self; 4] = [Self::War, Self::Global, Self::Os, Self::Profile];
    pub fn code(self) -> u32 {
        match self {
            Self::War => 1,
            Self::Global => 2,
            Self::Os => 3,
            Self::Profile => 4,
        }
    }
    pub fn name(self) -> &'static str {
        match self {
            Self::War => "war",
            Self::Global => "global",
            Self::Os => "os",
            Self::Profile => "profile",
        }
    }
    pub fn parse(text: &str) -> Result<Self, Error> {
        Self::ALL
            .into_iter()
            .find(|c| c.name().eq_ignore_ascii_case(text))
            .ok_or_else(|| {
                Error::Custom(format!(
                    "unknown RATED_TDP client {text:?}; expected war|global|os|profile"
                ))
            })
    }
    fn from_code(code: u32) -> Option<Self> {
        Self::ALL.into_iter().find(|c| c.code() == code)
    }
}

/// `NV2080_CTRL_PERF_RATED_TDP_ACTION`. The `FORCE_` forms are the same values;
/// the driver reports whichever it stored, so both spellings parse.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RatedTdpAction {
    Default,
    Exceed,
    Limit,
    Lock,
    Floor,
}

impl RatedTdpAction {
    pub const ALL: [Self; 5] = [
        Self::Default,
        Self::Exceed,
        Self::Limit,
        Self::Lock,
        Self::Floor,
    ];
    pub fn code(self) -> u32 {
        match self {
            Self::Default => 0,
            Self::Exceed => 1,
            Self::Limit => 2,
            Self::Lock => 3,
            Self::Floor => 4,
        }
    }
    pub fn name(self) -> &'static str {
        match self {
            Self::Default => "default",
            Self::Exceed => "force-exceed",
            Self::Limit => "force-limit",
            Self::Lock => "force-lock",
            Self::Floor => "force-floor",
        }
    }
    pub fn parse(text: &str) -> Result<Self, Error> {
        // Accept both the driver's force spellings and plain ones, in either
        // case and with `_` or `-` separators (`FORCE_LIMIT` == `force-limit`).
        let folded = text.trim().to_ascii_lowercase().replace('_', "-");
        let t = folded.strip_prefix("force-").unwrap_or(&folded);
        Self::ALL
            .into_iter()
            .find(|a| {
                a.name()
                    .trim_start_matches("force-")
                    .eq_ignore_ascii_case(t)
            })
            .ok_or_else(|| {
                Error::Custom(format!(
                    "unknown RATED_TDP action {text:?}; expected default|exceed|limit|lock|floor"
                ))
            })
    }
    fn from_code(code: u32) -> Option<Self> {
        Self::ALL.into_iter().find(|a| a.code() == code)
    }
}

/// What the reply carries for one client.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RatedTdpSettings {
    pub client: RatedTdpClient,
    pub action: RatedTdpAction,
    pub v_pstate: u32,
}

/// Proof of the round trip: the stored action came back and is restored on drop.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RatedTdpWriteReport {
    pub original_action: RatedTdpAction,
    pub applied_action: RatedTdpAction,
}

fn word(block: &[u8], at: usize) -> u32 {
    u32::from_le_bytes(block[at..at + 4].try_into().expect("validated block size"))
}

/// Decode one client's reply. Measured layout on this driver: the GET reply
/// does **not** echo the client -- dword 0 comes back `0` (RM, the internal
/// client) for every client asked for, dword 1 is the action and dword 2 the
/// vPstate. The SET reply *does* echo the full request, which is what makes the
/// forward readback below byte-exact. So dword 0 is accepted when it is 0 and
/// checked when the driver does fill it in.
pub fn parse(block: &[u8; BYTES], client: RatedTdpClient) -> Result<RatedTdpSettings, Error> {
    let echoed = word(block, 0);
    if echoed != 0 && echoed != client.code() {
        let named = RatedTdpClient::from_code(echoed)
            .map(|echo| echo.name())
            .unwrap_or("unknown");
        return Err(Error::Custom(format!(
            "RATED_TDP reply names client {echoed} ({named}) but {} was asked for",
            client.name()
        )));
    }
    let action = RatedTdpAction::from_code(word(block, 4)).ok_or_else(|| {
        Error::Custom(format!(
            "RATED_TDP reply action {} is outside the published 0..4 enum",
            word(block, 4)
        ))
    })?;
    Ok(RatedTdpSettings {
        client,
        action,
        v_pstate: word(block, 8),
    })
}

/// Read the current request for `client` without writing anything. The control
/// object is per-client (its `get` fills that client), so identity is checked
/// against the argument rather than trusted.
pub fn read<T: crate::macos_control_transaction::ControlBlock<BYTES>>(
    io: &mut T,
    client: RatedTdpClient,
) -> Result<RatedTdpSettings, Error> {
    let block = io.get()?;
    parse(&block, client)
}

/// Ask `client` for `action`. Errors and cancellation restore the exact 12
/// original bytes; success leaves the request standing.
pub fn set<T: crate::macos_control_transaction::ControlBlock<BYTES>>(
    io: &mut T,
    client: RatedTdpClient,
    action: RatedTdpAction,
    cancelled: &dyn Fn() -> bool,
) -> Result<RatedTdpWriteReport, Error> {
    if cancelled() {
        return Err(Error::Custom(
            "RATED_TDP request cancelled before snapshot".into(),
        ));
    }
    // The GET reply's dword 0 is not the client, but a SET payload must carry
    // one -- restoring the reply verbatim would address the internal RM client.
    // Stamping the client into the snapshot makes restore and the forward
    // comparison both address the right client.
    let mut original = io.get()?;
    let before = parse(&original, client)?;
    original[0..4].copy_from_slice(&client.code().to_le_bytes());
    let mut wanted = [0u8; BYTES];
    wanted[0..4].copy_from_slice(&client.code().to_le_bytes());
    wanted[4..8].copy_from_slice(&action.code().to_le_bytes());
    wanted[8..12].copy_from_slice(&before.v_pstate.to_le_bytes());
    crate::macos_control_transaction::write_block(
        io,
        original,
        wanted,
        cancelled,
        format!(
            "RATED_TDP client {}: original action {}, target {}",
            client.name(),
            before.action.name(),
            action.name()
        ),
    )?;
    Ok(RatedTdpWriteReport {
        original_action: before.action,
        applied_action: action,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Fake {
        block: [u8; BYTES],
        fail_set: Option<u32>,
        gets: u32,
    }

    impl Fake {
        fn new(action: u32, v_pstate: u32) -> Self {
            let mut block = [0u8; BYTES];
            block[0..4].copy_from_slice(&2u32.to_le_bytes());
            block[4..8].copy_from_slice(&action.to_le_bytes());
            block[8..12].copy_from_slice(&v_pstate.to_le_bytes());
            Self {
                block,
                fail_set: None,
                gets: 0,
            }
        }
        fn action(&self) -> u32 {
            word(&self.block, 4)
        }
    }

    impl crate::macos_control_transaction::ControlBlock<BYTES> for Fake {
        fn get(&mut self) -> Result<[u8; BYTES], Error> {
            self.gets += 1;
            Ok(self.block)
        }
        fn set(&mut self, block: &[u8; BYTES]) -> Result<(), Error> {
            if let Some(status) = self.fail_set.take() {
                return Err(Error::Custom(format!("SET failed: RM 0x{status:08x}")));
            }
            self.block = *block;
            Ok(())
        }
    }

    #[test]
    fn parses_the_published_trio_and_keeps_v_pstate() {
        let settings = parse(&Fake::new(2, 1).block, RatedTdpClient::Global).unwrap();
        assert_eq!(settings.action, RatedTdpAction::Limit);
        assert_eq!(settings.v_pstate, 1);
    }

    #[test]
    fn a_foreign_client_or_unknown_action_is_refused_but_the_zero_echo_is_tolerated() {
        // GET answers 0 in dword 0 for every client; that must not be an error.
        let mut zeroed = Fake::new(0, 1).block;
        zeroed[0] = 0;
        assert_eq!(
            parse(&zeroed, RatedTdpClient::Global).unwrap().client,
            RatedTdpClient::Global
        );
        let mut wrong_client = Fake::new(0, 1).block;
        wrong_client[0] = 3;
        assert!(parse(&wrong_client, RatedTdpClient::Global).is_err());
        assert!(parse(&Fake::new(9, 1).block, RatedTdpClient::Global).is_err());
    }

    #[test]
    fn set_carries_the_request_and_only_touches_client_action_and_v_pstate() {
        let mut io = Fake::new(0, 1);
        let report = set(
            &mut io,
            RatedTdpClient::Global,
            RatedTdpAction::Limit,
            &|| false,
        )
        .unwrap();
        assert_eq!(report.original_action, RatedTdpAction::Default);
        assert_eq!(report.applied_action, RatedTdpAction::Limit);
        assert_eq!(io.block, [2, 0, 0, 0, 2, 0, 0, 0, 1, 0, 0, 0]);
    }

    #[test]
    fn a_failed_set_and_a_cancellation_both_restore_the_original_bytes() {
        let mut io = Fake::new(0, 1);
        io.fail_set = Some(0x1f);
        assert!(
            set(
                &mut io,
                RatedTdpClient::Global,
                RatedTdpAction::Lock,
                &|| false
            )
            .is_err()
        );
        assert_eq!(io.action(), 0);

        let mut io = Fake::new(0, 1);
        assert!(
            set(
                &mut io,
                RatedTdpClient::Global,
                RatedTdpAction::Lock,
                &|| true
            )
            .is_err()
        );
        assert_eq!(io.gets, 0);
    }

    #[test]
    fn spellings_round_trip_including_force_forms() {
        for action in RatedTdpAction::ALL {
            assert_eq!(RatedTdpAction::parse(action.name()).unwrap(), action);
        }
        assert_eq!(
            RatedTdpAction::parse("FORCE_LIMIT").unwrap(),
            RatedTdpAction::Limit
        );
        assert!(RatedTdpAction::parse("turbo").is_err());
        assert_eq!(
            RatedTdpClient::parse("GLOBAL").unwrap(),
            RatedTdpClient::Global
        );
        assert!(RatedTdpClient::parse("rm").is_err());
    }
}
