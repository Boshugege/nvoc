//! Per-domain clock enumeration on the macOS RM path.
//!
//! Two private CLK-class controls, both proven on the TU116 test card before
//! this module existed:
//!
//! * `0x20809001` (8 bytes) takes `{mode, 0}` and answers `{mask, mode}` where
//!   `mask` is the set of clock-domain bits the chip reports for that mode.
//!   Mode 0 measured `0x03FC007F`, mode 1 measured `0x0038007F` on this card,
//!   so the two modes are kept verbatim rather than named by guesswork.
//! * `0x20809006` (8 bytes) takes `{version = 1, domain bit}` and answers
//!   `{version, kHz}`; the payload word is the live frequency of that domain,
//!   already in kHz (1545076 = ~1545 MHz at stock on the test card).
//!
//! NVML drives the same pair from `dmal/common/common_clock.c` (its
//! `cDeviceMapTargetToActualFrequency` helper passes domain `8320` = class
//! `0x2080`), which is where the ids come from.
//!
//! All-zero versions are rejected with `NV_ERR_INVALID_ARGUMENT` (`0x1F`), so
//! the version word of `0x20809006` is not optional.

use crate::error::Error;

/// Bytes the C entry point fills: mask, then 32 kHz words, then 32 statuses.
pub const DOMAIN_BLOCK_BYTES: usize = 4 + 4 * 32 + 4 * 32;
/// Offset of the kHz array inside the block.
pub const DOMAIN_KHZ_OFFSET: usize = 4;
/// Offset of the per-domain status array inside the block.
pub const DOMAIN_STATUS_OFFSET: usize = 4 + 4 * 32;
/// Command that enumerates the reported clock domains.
pub const DOMAIN_MASK_CMD: u32 = 0x2080_9001;
/// Command that answers one domain's live frequency.
pub const DOMAIN_CLOCK_CMD: u32 = 0x2080_9006;
/// Version word `0x20809006` requires.
pub const DOMAIN_CLOCK_VERSION: u32 = 1;
/// Mode word for the enumeration: 0 is the full set the chip reports.
pub const DOMAIN_MODE_ALL: u32 = 0;

/// What one domain bit answered.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DomainClock {
    /// The driver returned a frequency in kHz.
    Khz(u32),
    /// The driver refused this bit; the RM status is carried verbatim.
    Refused(u32),
}

/// The enumeration plus every per-domain answer, in bit order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DomainClockReport {
    /// Domain bits the enumeration reported.
    pub mask: u32,
    /// One entry per set bit, in ascending bit order.
    pub domains: Vec<(u32, DomainClock)>,
}

/// Decode the fixed block the C entry point returns.
///
/// A short block is refused rather than truncated: a missing bit would
/// otherwise read as "the chip does not report this domain".
pub fn parse_domains(block: &[u8]) -> Result<DomainClockReport, Error> {
    if block.len() != DOMAIN_BLOCK_BYTES {
        return Err(Error::Custom(format!(
            "clock-domain block is {} bytes, expected {DOMAIN_BLOCK_BYTES}",
            block.len()
        )));
    }
    let mask = u32::from_le_bytes([block[0], block[1], block[2], block[3]]);
    let mut domains = Vec::new();
    for bit in 0..32u32 {
        if mask & (1 << bit) == 0 {
            continue;
        }
        let at = DOMAIN_KHZ_OFFSET + bit as usize * 4;
        let khz = u32::from_le_bytes([block[at], block[at + 1], block[at + 2], block[at + 3]]);
        let at = DOMAIN_STATUS_OFFSET + bit as usize * 4;
        let status = u32::from_le_bytes([block[at], block[at + 1], block[at + 2], block[at + 3]]);
        domains.push((
            bit,
            if status == 0 {
                DomainClock::Khz(khz)
            } else {
                DomainClock::Refused(status)
            },
        ));
    }
    Ok(DomainClockReport { mask, domains })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn block(mask: u32, entries: &[(u32, u32, u32)]) -> Vec<u8> {
        let mut buf = vec![0u8; DOMAIN_BLOCK_BYTES];
        buf[..4].copy_from_slice(&mask.to_le_bytes());
        for (bit, khz, status) in entries {
            let at = DOMAIN_KHZ_OFFSET + *bit as usize * 4;
            buf[at..at + 4].copy_from_slice(&khz.to_le_bytes());
            let at = DOMAIN_STATUS_OFFSET + *bit as usize * 4;
            buf[at..at + 4].copy_from_slice(&status.to_le_bytes());
        }
        buf
    }

    #[test]
    fn decodes_only_the_reported_bits_in_order() {
        // Two set bits, the low one refused and the high one answered.
        let parsed = parse_domains(&block(0b1010, &[(1, 0, 0x56), (3, 1_545_076, 0)])).unwrap();
        assert_eq!(parsed.mask, 0b1010);
        assert_eq!(
            parsed.domains,
            vec![
                (1, DomainClock::Refused(0x56)),
                (3, DomainClock::Khz(1_545_076))
            ]
        );
    }

    #[test]
    fn an_empty_mask_reports_nothing() {
        let parsed = parse_domains(&block(0, &[])).unwrap();
        assert_eq!(parsed.mask, 0);
        assert!(parsed.domains.is_empty());
    }

    /// The ids live on the C side; a rename there must not leave these
    /// constants describing a command nobody sends.
    #[test]
    fn the_constants_match_the_c_bridge() {
        let bridge = include_str!("../native/macos_rm.c");
        for needle in [
            format!("0x{DOMAIN_MASK_CMD:08x}u"),
            format!("0x{DOMAIN_CLOCK_CMD:08x}u"),
            format!("payload[0] = {DOMAIN_CLOCK_VERSION}u"),
            format!("payload[0] = {DOMAIN_MODE_ALL}u;"),
        ] {
            assert!(bridge.contains(&needle), "C bridge lost `{needle}`");
        }
    }

    #[test]
    fn a_short_block_is_refused() {
        assert!(parse_domains(&[0u8; 8]).is_err());
    }
}
