//! Locked clocks: the matched min/max frequency window per clock domain and
//! vPstate, as the TU116 firmware takes it through the private macOS RM channel.
//!
//! The payload is the measurement, not a header: 648 bytes with a format word at
//! `+0` (1 on every accepted request, 0 is refused), an entry count at `+4`
//! (1..31, 0 is refused), and 20-byte entries from `+8` of
//! `{vPstate, domain bit, min kHz, max kHz, padding}`. Only the first four
//! dwords of an entry are read; the fifth stays zero on every accepted request.
//!
//! The domain is one bit, not an index: 1, 2, 4, 8, 16, 32 and 64 are the bits
//! this device answers (128 and above come back `0x1f`), and two entries may not
//! name the same bit. Bit 1 is the graphics domain: writing it moves bit 0 of the
//! clock-domain read. The remaining bits are named by position only, because
//! nothing has tied them to a live reading yet.
//!
//! The values are kHz and are not taken verbatim: the driver snaps them onto the
//! domain's own step table and refuses a zero bound, which is why the transport
//! always seeds both bounds and why the readback (not the request) is what a
//! verification compares. The window is a floor and a ceiling -- the observed
//! clock follows the floor unless the ceiling is the smaller of a sane pair, and
//! the firmware keeps the window after the client that set it is gone, so a
//! write is only undone by writing again.

use serde::Serialize;
use std::fmt;

/// Whole control block. The driver rejects any other size.
pub const BYTES: usize = 648;
/// Format word `+0`, count `+4`, first entry `+8`.
pub const HEADER_BYTES: usize = 8;
/// One entry stride.
pub const ENTRY_BYTES: usize = 20;
/// Counts the field at `+4` accepts.
pub const MAX_ENTRIES: u32 = 31;
/// The only format word the driver accepts on a request.
pub const FORMAT_WORD: u32 = 1;

/// Domain bits, named by measured position.
pub const DOMAIN_GRAPHICS: u32 = 1;
pub const DOMAIN_BIT_2: u32 = 2;
pub const DOMAIN_BIT_4: u32 = 4;
pub const DOMAIN_BIT_8: u32 = 8;
pub const DOMAIN_BIT_16: u32 = 16;
pub const DOMAIN_BIT_32: u32 = 32;
pub const DOMAIN_BIT_64: u32 = 64;

/// Bits the device answers at all, in the order the directory lists them.
pub const DOMAINS: [u32; 7] = [
    DOMAIN_GRAPHICS,
    DOMAIN_BIT_2,
    DOMAIN_BIT_4,
    DOMAIN_BIT_8,
    DOMAIN_BIT_16,
    DOMAIN_BIT_32,
    DOMAIN_BIT_64,
];

/// Stable name of a domain bit; unknown bits keep their numeric value.
pub fn domain_name(domain: u32) -> String {
    match domain {
        DOMAIN_GRAPHICS => "graphics".to_string(),
        DOMAIN_BIT_2 => "bit2".to_string(),
        DOMAIN_BIT_4 => "bit4".to_string(),
        DOMAIN_BIT_8 => "bit8".to_string(),
        DOMAIN_BIT_16 => "bit16".to_string(),
        DOMAIN_BIT_32 => "bit32".to_string(),
        DOMAIN_BIT_64 => "bit64".to_string(),
        other => format!("0x{other:x}"),
    }
}

/// One domain/vPstate window in kHz.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct LockWindow {
    /// vPstate the window applies in. The device accepts 0..15 in the field and
    /// refuses the block for values the domain does not know.
    pub pstate: u8,
    /// Single domain bit from [`DOMAINS`].
    pub domain: u32,
    /// Floor in kHz.
    pub min_khz: u32,
    /// Ceiling in kHz.
    pub max_khz: u32,
}

impl LockWindow {
    pub const fn new(pstate: u8, domain: u32, min_khz: u32, max_khz: u32) -> Self {
        Self {
            pstate,
            domain,
            min_khz,
            max_khz,
        }
    }

    /// Whether both bounds are present, which is the pair the driver validates.
    pub const fn bounds_present(&self) -> bool {
        self.min_khz != 0 && self.max_khz != 0
    }
}

impl fmt::Display for LockWindow {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{} vP{} {}-{} kHz",
            domain_name(self.domain),
            self.pstate,
            self.min_khz,
            self.max_khz
        )
    }
}

/// What a completed write did, with the snapshot it kept and the readback it
/// verified. `applied` is the driver's own answer, so a caller can show the
/// snapped values instead of the requested ones.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct LockedClockWriteReport {
    pub requested: Vec<LockWindow>,
    pub snapshot: Vec<LockWindow>,
    pub applied: Vec<LockWindow>,
}

/// Why a request could not be laid out in the block.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LockBuildError {
    NoEntries,
    TooManyEntries(u32),
    DuplicateDomain(u32),
    ZeroBound(u32),
    NotSingleBit(u32),
    PstateOutOfRange(u8),
}

impl fmt::Display for LockBuildError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NoEntries => write!(f, "a lock needs at least one entry"),
            Self::TooManyEntries(count) => {
                write!(
                    f,
                    "{count} entries do not fit, the field holds {MAX_ENTRIES}"
                )
            }
            Self::DuplicateDomain(domain) => write!(
                f,
                "domain {} is listed twice and the device accepts one entry per domain",
                domain_name(*domain)
            ),
            Self::ZeroBound(domain) => write!(
                f,
                "domain {} needs both a min and a max; 0/0 is refused with 0xd",
                domain_name(*domain)
            ),
            Self::NotSingleBit(domain) => {
                write!(f, "0x{domain:x} is not a single domain bit")
            }
            Self::PstateOutOfRange(pstate) => {
                write!(f, "vPstate {pstate} is outside the 0..=15 field")
            }
        }
    }
}

impl std::error::Error for LockBuildError {}

/// Why a readback could not be read as a window list.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LockParseError {
    /// Count above what the block can hold.
    Count(u32),
    /// Format word other than [`FORMAT_WORD`].
    Format(u32),
    /// An entry named something other than a single bit.
    NotSingleBit(u32),
    /// An entry carried a zero bound even though both are validated.
    ZeroBound(u32),
}

impl fmt::Display for LockParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Count(count) => write!(f, "count {count} does not fit the block"),
            Self::Format(word) => write!(f, "format word 0x{word:x} is not {FORMAT_WORD:#x}"),
            Self::NotSingleBit(domain) => {
                write!(f, "entry names domain 0x{domain:x}, not a single bit")
            }
            Self::ZeroBound(domain) => write!(
                f,
                "entry for domain {} came back with a zero bound",
                domain_name(*domain)
            ),
        }
    }
}

impl std::error::Error for LockParseError {}

/// A domain is exactly one bit; a zero or a mask is refused.
pub const fn is_single_bit(domain: u32) -> bool {
    domain != 0 && domain & (domain - 1) == 0
}

fn put_dword(block: &mut [u8; BYTES], offset: usize, value: u32) {
    block[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
}

fn get_dword(block: &[u8; BYTES], offset: usize) -> u32 {
    u32::from_le_bytes([
        block[offset],
        block[offset + 1],
        block[offset + 2],
        block[offset + 3],
    ])
}

/// Lay a window list out in the block the transport sends.
///
/// The whole block is written, so the untouched tail is zero rather than
/// whatever the caller's buffer held.
pub fn build_block(entries: &[LockWindow]) -> Result<[u8; BYTES], LockBuildError> {
    if entries.is_empty() {
        return Err(LockBuildError::NoEntries);
    }
    if entries.len() as u32 > MAX_ENTRIES {
        return Err(LockBuildError::TooManyEntries(entries.len() as u32));
    }
    let mut block = [0u8; BYTES];
    put_dword(&mut block, 0, FORMAT_WORD);
    put_dword(&mut block, 4, entries.len() as u32);
    for (index, entry) in entries.iter().enumerate() {
        if !is_single_bit(entry.domain) {
            return Err(LockBuildError::NotSingleBit(entry.domain));
        }
        if entries[..index]
            .iter()
            .any(|seen| seen.domain == entry.domain)
        {
            return Err(LockBuildError::DuplicateDomain(entry.domain));
        }
        if !entry.bounds_present() {
            return Err(LockBuildError::ZeroBound(entry.domain));
        }
        if entry.pstate > 15 {
            return Err(LockBuildError::PstateOutOfRange(entry.pstate));
        }
        let base = HEADER_BYTES + index * ENTRY_BYTES;
        block[base] = entry.pstate;
        put_dword(&mut block, base + 4, entry.domain);
        put_dword(&mut block, base + 8, entry.min_khz);
        put_dword(&mut block, base + 12, entry.max_khz);
        put_dword(&mut block, base + 16, 0);
    }
    Ok(block)
}

/// Read a block back, snapped values included.
pub fn parse_block(block: &[u8; BYTES]) -> Result<Vec<LockWindow>, LockParseError> {
    let format = get_dword(block, 0);
    if format != FORMAT_WORD {
        return Err(LockParseError::Format(format));
    }
    let count = get_dword(block, 4);
    if count == 0 || count as usize > (BYTES - HEADER_BYTES) / ENTRY_BYTES {
        return Err(LockParseError::Count(count));
    }
    let mut entries = Vec::with_capacity(count as usize);
    for index in 0..count as usize {
        let base = HEADER_BYTES + index * ENTRY_BYTES;
        let domain = get_dword(block, base + 4);
        let min_khz = get_dword(block, base + 8);
        let max_khz = get_dword(block, base + 12);
        if !is_single_bit(domain) {
            return Err(LockParseError::NotSingleBit(domain));
        }
        if min_khz == 0 || max_khz == 0 {
            return Err(LockParseError::ZeroBound(domain));
        }
        entries.push(LockWindow {
            pstate: block[base],
            domain,
            min_khz,
            max_khz,
        });
    }
    Ok(entries)
}

/// Entry-by-entry comparison of a readback against a request.
///
/// Returns the first window whose values differ, so a caller can report what the
/// driver actually took instead of a bare mismatch.
pub fn first_difference(
    requested: &[LockWindow],
    readback: &[LockWindow],
) -> Option<(LockWindow, Option<LockWindow>)> {
    requested
        .iter()
        .enumerate()
        .find_map(|(index, want)| {
            let got = readback.get(index).copied();
            (got != Some(*want)).then_some((*want, got))
        })
        .or_else(|| {
            readback
                .get(requested.len())
                .map(|extra| (requested[requested.len() - 1], Some(*extra)))
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn graphics(min_khz: u32, max_khz: u32) -> LockWindow {
        LockWindow::new(8, DOMAIN_GRAPHICS, min_khz, max_khz)
    }

    #[test]
    fn the_block_carries_the_measured_header_and_entry_shape() {
        let block = build_block(&[graphics(200_000, 1_800_000)]).expect("one entry fits");
        assert_eq!(get_dword(&block, 0), FORMAT_WORD);
        assert_eq!(get_dword(&block, 4), 1);
        assert_eq!(block[HEADER_BYTES], 8, "vPstate");
        assert_eq!(get_dword(&block, HEADER_BYTES + 4), DOMAIN_GRAPHICS);
        assert_eq!(get_dword(&block, HEADER_BYTES + 8), 200_000);
        assert_eq!(get_dword(&block, HEADER_BYTES + 12), 1_800_000);
        assert_eq!(get_dword(&block, HEADER_BYTES + 16), 0, "entry padding");
        assert!(
            block[HEADER_BYTES + ENTRY_BYTES..]
                .iter()
                .all(|byte| *byte == 0),
            "the tail past the entries stays zeroed"
        );
    }

    #[test]
    fn entries_are_twenty_bytes_apart() {
        let block = build_block(&[
            graphics(200_000, 1_800_000),
            LockWindow::new(8, DOMAIN_BIT_16, 100_000, 1_000_000),
        ])
        .expect("two distinct domains fit");
        assert_eq!(get_dword(&block, 4), 2);
        let second = HEADER_BYTES + ENTRY_BYTES;
        assert_eq!(get_dword(&block, second + 4), DOMAIN_BIT_16);
        assert_eq!(get_dword(&block, second + 8), 100_000);
    }

    #[test]
    fn an_empty_request_is_refused() {
        assert_eq!(build_block(&[]), Err(LockBuildError::NoEntries));
    }

    #[test]
    fn the_count_stops_at_the_field_width_not_at_the_block() {
        let mut entries = Vec::new();
        for index in 0..=MAX_ENTRIES {
            entries.push(LockWindow::new(8, 1 << index, 100_000, 200_000));
        }
        assert_eq!(
            build_block(&entries),
            Err(LockBuildError::TooManyEntries(MAX_ENTRIES + 1))
        );
        assert!(build_block(&entries[..MAX_ENTRIES as usize]).is_ok());
    }

    #[test]
    fn a_domain_may_not_repeat() {
        assert_eq!(
            build_block(&[graphics(100_000, 200_000), graphics(300_000, 400_000)]),
            Err(LockBuildError::DuplicateDomain(DOMAIN_GRAPHICS))
        );
    }

    #[test]
    fn an_index_style_domain_and_a_zero_bound_are_refused() {
        assert_eq!(
            build_block(&[LockWindow::new(8, 3, 100_000, 200_000)]),
            Err(LockBuildError::NotSingleBit(3))
        );
        assert_eq!(
            build_block(&[LockWindow::new(8, 0, 100_000, 200_000)]),
            Err(LockBuildError::NotSingleBit(0))
        );
        assert_eq!(
            build_block(&[graphics(0, 200_000)]),
            Err(LockBuildError::ZeroBound(DOMAIN_GRAPHICS))
        );
        assert_eq!(
            build_block(&[graphics(100_000, 0)]),
            Err(LockBuildError::ZeroBound(DOMAIN_GRAPHICS))
        );
    }

    #[test]
    fn a_pstate_past_the_field_is_refused() {
        assert_eq!(
            build_block(&[LockWindow::new(16, DOMAIN_GRAPHICS, 1, 2)]),
            Err(LockBuildError::PstateOutOfRange(16))
        );
    }

    #[test]
    fn a_readback_snapped_onto_the_table_round_trips() {
        // 1000000 kHz is not a step of the graphics table; the device answers
        // 1080000, which is exactly what a readback has to be readable as.
        let mut block = build_block(&[graphics(1_000_000, 1_000_000)]).expect("sane request");
        put_dword(&mut block, HEADER_BYTES + 8, 1_080_000);
        put_dword(&mut block, HEADER_BYTES + 12, 1_080_000);
        assert_eq!(get_dword(&block, HEADER_BYTES + 8), 1_080_000);
        assert_eq!(
            parse_block(&block),
            Ok(vec![graphics(1_080_000, 1_080_000)])
        );
    }

    #[test]
    fn a_readback_without_the_format_word_or_entries_is_unreadable() {
        let mut block = build_block(&[graphics(1, 2)]).expect("sane request");
        put_dword(&mut block, 0, 0);
        assert_eq!(parse_block(&block), Err(LockParseError::Format(0)));
        let mut block = build_block(&[graphics(1, 2)]).expect("sane request");
        put_dword(&mut block, 4, 0);
        assert_eq!(parse_block(&block), Err(LockParseError::Count(0)));
        put_dword(&mut block, 4, 33);
        assert_eq!(parse_block(&block), Err(LockParseError::Count(33)));
    }

    #[test]
    fn a_readback_entry_with_a_zero_bound_is_a_malformed_answer() {
        let mut block = build_block(&[graphics(1, 2)]).expect("sane request");
        put_dword(&mut block, HEADER_BYTES + 12, 0);
        assert_eq!(
            parse_block(&block),
            Err(LockParseError::ZeroBound(DOMAIN_GRAPHICS))
        );
    }

    #[test]
    fn differences_report_what_the_driver_took() {
        let want = vec![graphics(1_000_000, 1_200_000)];
        assert_eq!(first_difference(&want, &want), None);
        let got = vec![graphics(1_080_000, 1_200_000)];
        assert_eq!(
            first_difference(&want, &got),
            Some((
                graphics(1_000_000, 1_200_000),
                Some(graphics(1_080_000, 1_200_000))
            ))
        );
        assert_eq!(
            first_difference(&want, &[]),
            Some((graphics(1_000_000, 1_200_000), None))
        );
        let extra = vec![graphics(1_000_000, 1_200_000), LockWindow::new(8, 2, 1, 2)];
        assert!(first_difference(&want, &extra).is_some());
    }

    #[test]
    fn names_stay_readable_for_unknown_bits() {
        assert_eq!(domain_name(DOMAIN_GRAPHICS), "graphics");
        assert_eq!(domain_name(DOMAIN_BIT_64), "bit64");
        assert_eq!(domain_name(0x80), "0x80");
        assert!(graphics(1, 2).to_string().contains("graphics"));
    }
}
