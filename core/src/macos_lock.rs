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

/// The GET that owns the locked-clock write as its twin: `0x20809040`, whose
/// answer is a format word and a count of the windows locked right now.
pub const STATE_BYTES: usize = 4808;

/// How many leading dwords of that block a report keeps. Enough to see whether
/// a table starts at offset 8 without carrying the whole 4.7 kB through JSON.
pub const STATE_HEAD_WORDS: usize = 32;
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
    /// The head of the twin read's answer (`0x20809040`) while the write is in
    /// place, plus the offset-4 dword that `parse_state_count` reads.
    ///
    /// Raw on purpose. `parse_state_records` reads the records out of the full
    /// block and this keeps the first dwords for anyone who needs to see what
    /// the parse was based on. Offset 4 counts locked-clock SETs since the
    /// driver loaded, times 48: a fresh boot reads 0, the first write 48, the
    /// second 96 (§二百二十三). It is not the record count.
    pub twin_block_head: Option<(Vec<u32>, Option<u32>)>,
    /// The records those dwords spell out, when the answer had any. The layout
    /// is measured (§二百二十一): 48-byte records from `+16`, each carrying the
    /// window that write asked for. This is the driver's stored state, not a
    /// readback of the request, which is why it is reported next to it and not
    /// instead of it.
    pub twin_records: Vec<StateRecord>,
    /// Whether the newest stored record carries the window that was applied.
    ///
    /// `None` when the answer had no records. `Some(false)` is a real finding:
    /// the driver stored something other than what it said it applied.
    pub storage_matches_applied: Option<bool>,
}

/// What a read of the stored locked-clock table answered.
///
/// This is a read of the driver's own table, not a normalisation of a request,
/// so it can be taken without asking for a window first. Empty records with a
/// `count_at_4` of zero is the state a fresh boot shows.
#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Serialize)]
pub struct LockedStateReport {
    /// The dword at `+4`: 48 times the number of writes since the driver loaded.
    pub count_at_4: Option<u32>,
    /// The 48-byte records, oldest first.
    pub records: Vec<StateRecord>,
}

impl LockedStateReport {
    /// The window in the newest record, in kHz.
    pub fn stored_window(&self) -> Option<(u32, u32)> {
        last_stored_window(&self.records)
    }

    /// What about this answer does not fit the layout the dumps established.
    ///
    /// The parse is deliberately forgiving -- a table is still worth reading
    /// when one record will not decode -- so the reader has to be told when the
    /// answer it is looking at disagrees with itself. One boot answered
    /// `+4 = 288` with only two records in the block, which is six writes by
    /// the counter and two by the records; nothing in this program noticed, and
    /// a reader had to spot it by hand. `None` means "nothing to report", which
    /// includes an unknown counter: not knowing the count is not an anomaly.
    pub fn table_anomaly(&self) -> Option<String> {
        if let Some(count) = self.count_at_4 {
            if count % STATE_RECORD_BYTES as u32 != 0 {
                return Some(format!(
                    "+4 reads {count}, which is not a multiple of {STATE_RECORD_BYTES}: the dword \
                     counts writes only if it steps by {STATE_RECORD_BYTES}"
                ));
            }
            let writes = count / STATE_RECORD_BYTES as u32;
            if writes != self.records.len() as u32 {
                return Some(format!(
                    "+4 reads {count}, which is {writes} writes of {STATE_RECORD_BYTES}, but the \
                     block holds {} record(s); the table and the counter disagree",
                    self.records.len()
                ));
            }
        }
        for (index, record) in self.records.iter().enumerate() {
            let expected = STATE_RECORD_BASE + index * STATE_RECORD_BYTES;
            if record.offset != expected {
                return Some(format!(
                    "record {index} sits at {:#x}, not {expected:#x}: the parse walks \
                     {STATE_RECORD_BYTES}-byte records from {STATE_RECORD_BASE:#x}",
                    record.offset
                ));
            }
        }
        None
    }
}

/// Bytes one record of the `0x20809040` answer occupies, and where the first
/// one starts.
///
/// Measured, not assumed: a read-only dump with two writes in place showed
/// `01000002, 1, min, max, 0, 0, 16, mem, mem, 0, x, y` at `+16` and at `+64`,
/// then zero out to the end of the 4808-byte block.
pub const STATE_RECORD_BYTES: usize = 48;
/// Byte offset of the first record in that answer.
pub const STATE_RECORD_BASE: usize = 16;
/// Dwords kept per record.
pub const STATE_RECORD_WORDS: usize = 12;

/// One record of the `0x20809040` answer, kept as its twelve raw dwords.
///
/// The accessors below name only what the dumps actually showed; the two
/// trailing dwords differ between records and look like stamps, so they stay
/// raw instead of being given a meaning.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
pub struct StateRecord {
    /// Byte offset of this record inside the block.
    pub offset: usize,
    /// The record's twelve dwords, little-endian as they arrive.
    pub words: [u32; STATE_RECORD_WORDS],
}

impl StateRecord {
    /// `+0`: `0x01000002` in every record seen so far.
    pub fn type_word(&self) -> u32 {
        self.words[0]
    }

    /// `+4`: 1 in every record seen so far.
    pub fn count(&self) -> u32 {
        self.words[1]
    }

    /// `+8`: low end of the window this record carries.
    pub fn min_khz(&self) -> u32 {
        self.words[2]
    }

    /// `+12`: high end of the same window.
    pub fn max_khz(&self) -> u32 {
        self.words[3]
    }

    /// `+28`/`+32`: the memory clock, when the two dwords agree and are not
    /// zero. `None` otherwise, because a disagreement is not a reading.
    pub fn memory_khz(&self) -> Option<u32> {
        match (self.words[7], self.words[8]) {
            (0, _) | (_, 0) => None,
            (low, high) if low == high => Some(low),
            _ => None,
        }
    }
}

/// The window in the newest record of a `0x20809040` answer.
///
/// This is what the driver has *stored*, which is the only thing here that is
/// independent of the request: `readback_verified` compares the driver's answer
/// with the driver's own normalisation of the same request, while this compares
/// a stored record with the window that was meant to be applied. It says nothing
/// about the clock itself, and nothing about a record the driver may append for
/// a different domain, so it is reported as its own field and never folded into
/// `readback_verified`.
pub fn last_stored_window(records: &[StateRecord]) -> Option<(u32, u32)> {
    records
        .last()
        .map(|record| (record.min_khz(), record.max_khz()))
}

/// Whether the newest stored record carries the window that was applied.
///
/// `None` when either side is empty. `Some(false)` is a real finding: the driver
/// stored a window other than the one it answered with. Only the first applied
/// window is compared, because that is the one this backend writes and the one
/// the records have been seen to carry; more windows would need a record each and
/// that mapping has not been measured.
pub fn storage_matches(applied: &[LockWindow], records: &[StateRecord]) -> Option<bool> {
    let wanted = applied.first()?;
    let stored = last_stored_window(records)?;
    Some(stored == (wanted.min_khz, wanted.max_khz))
}

/// Walk the records out of a `0x20809040` answer.
///
/// Stops at the first slot whose type word is zero: that is what the dumps show
/// after the last record, and continuing past it would invent records out of
/// padding. A block shorter than one record yields nothing.
pub fn parse_state_records(block: &[u8]) -> Vec<StateRecord> {
    let word = |offset: usize| {
        u32::from_le_bytes([
            block[offset],
            block[offset + 1],
            block[offset + 2],
            block[offset + 3],
        ])
    };
    let mut records = Vec::new();
    let mut offset = STATE_RECORD_BASE;
    while offset + STATE_RECORD_BYTES <= block.len() {
        if word(offset) == 0 {
            break;
        }
        let mut words = [0u32; STATE_RECORD_WORDS];
        for (index, slot) in words.iter_mut().enumerate() {
            *slot = word(offset + index * 4);
        }
        records.push(StateRecord { offset, words });
        offset += STATE_RECORD_BYTES;
    }
    records
}

/// Read the window count out of a `0x20809040` block.
///
/// Only the count is parsed: the entry layout has never been seen with a
/// non-zero count, and guessing it would be exactly the kind of invented
/// structure this backend refuses elsewhere.
pub fn parse_state_count(block: &[u8]) -> Result<u32, String> {
    if block.len() < 8 {
        return Err(format!("locked-state block is {} bytes", block.len()));
    }
    let word = |offset: usize| {
        u32::from_le_bytes([
            block[offset],
            block[offset + 1],
            block[offset + 2],
            block[offset + 3],
        ])
    };
    let format = word(0);
    if format != FORMAT_WORD {
        return Err(format!("locked-state format word 0x{format:x}"));
    }
    Ok(word(4))
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

/// Why a pre-write read cannot be used as the comparison target.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PreflightFault {
    /// The driver answered a different entry count than it was given.
    Count,
    /// The driver moved the entry to another vPstate or domain.
    Identity,
    /// The driver normalised a bound to zero, or inverted the window.
    Bounds,
}

impl fmt::Display for PreflightFault {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Count => "the driver answers a different entry count",
            Self::Identity => "the driver moves the entry to another vPstate or domain",
            Self::Bounds => "the driver leaves no usable window behind",
        })
    }
}

/// Checks the pre-write read before it is trusted as a comparison target.
///
/// The driver's answer is *not* the request: it replaces the floor with the
/// window's effective floor, which is the stored floor raised to the live clock,
/// and echoes the ceiling. That makes the answer state-dependent, so it is only
/// rejected when it could not be written back at all -- a moved identity, an
/// erased bound or an inverted window (a floor above the ceiling). A floor that
/// merely moved is accepted and becomes the value the readback is held to.
pub fn check_preflight(
    requested: &[LockWindow],
    answered: &[LockWindow],
) -> Result<(), (PreflightFault, LockWindow, Option<LockWindow>)> {
    if requested.len() != answered.len() {
        let want = requested[requested.len().saturating_sub(1)];
        return Err((PreflightFault::Count, want, answered.first().copied()));
    }
    for (want, got) in requested.iter().zip(answered) {
        if want.pstate != got.pstate || want.domain != got.domain {
            return Err((PreflightFault::Identity, *want, Some(*got)));
        }
        if !got.bounds_present() || got.min_khz > got.max_khz {
            return Err((PreflightFault::Bounds, *want, Some(*got)));
        }
    }
    Ok(())
}

/// The readback comparison used after the write.
///
/// Identity and ceiling are exact; the floor only has to stay inside the window,
/// because the driver recomputes it from the clock the write itself just moved.
pub fn first_stable_difference(
    wanted: &[LockWindow],
    readback: &[LockWindow],
) -> Option<(LockWindow, Option<LockWindow>)> {
    wanted
        .iter()
        .enumerate()
        .find_map(|(index, want)| {
            let got = readback.get(index).copied();
            let stable = got.is_some_and(|got| {
                got.pstate == want.pstate
                    && got.domain == want.domain
                    && got.max_khz == want.max_khz
                    && got.min_khz <= got.max_khz
            });
            (!stable).then_some((*want, got))
        })
        .or_else(|| {
            readback
                .get(wanted.len())
                .map(|extra| (wanted[wanted.len() - 1], Some(*extra)))
        })
}

#[cfg(test)]
mod tests {

    fn record_at(offset: usize) -> StateRecord {
        StateRecord {
            offset,
            words: [
                0x0100_0002,
                1,
                1_620_000,
                1_800_000,
                0,
                0,
                16,
                0,
                0,
                0,
                0,
                0,
            ],
        }
    }

    #[test]
    fn a_table_that_agrees_with_itself_has_nothing_to_report() {
        let report = LockedStateReport {
            count_at_4: Some(96),
            records: vec![
                record_at(STATE_RECORD_BASE),
                record_at(STATE_RECORD_BASE + 48),
            ],
        };
        assert_eq!(report.table_anomaly(), None);
        // A fresh boot: nothing stored, nothing to say.
        assert_eq!(LockedStateReport::default().table_anomaly(), None);
    }

    #[test]
    fn a_counter_that_disagrees_with_the_records_is_reported() {
        // The boot that answered 288 with two records: six writes by the
        // counter, two by the block.
        let report = LockedStateReport {
            count_at_4: Some(288),
            records: vec![
                record_at(STATE_RECORD_BASE),
                record_at(STATE_RECORD_BASE + 48),
            ],
        };
        let anomaly = report.table_anomaly().expect("anomaly");
        assert!(anomaly.contains("288"), "{anomaly}");
        assert!(anomaly.contains("6 writes"), "{anomaly}");
        assert!(anomaly.contains("2 record(s)"), "{anomaly}");
    }

    #[test]
    fn a_counter_that_is_not_a_multiple_of_the_stride_is_reported() {
        let report = LockedStateReport {
            count_at_4: Some(50),
            records: vec![record_at(STATE_RECORD_BASE)],
        };
        let anomaly = report.table_anomaly().expect("anomaly");
        assert!(anomaly.contains("not a multiple"), "{anomaly}");
    }

    #[test]
    fn an_unknown_counter_is_not_an_anomaly() {
        // Not knowing the count says nothing about the records.
        let report = LockedStateReport {
            count_at_4: None,
            records: vec![record_at(STATE_RECORD_BASE)],
        };
        assert_eq!(report.table_anomaly(), None);
    }

    #[test]
    fn a_record_off_the_stride_is_reported() {
        let report = LockedStateReport {
            count_at_4: None,
            records: vec![
                record_at(STATE_RECORD_BASE),
                record_at(STATE_RECORD_BASE + 52),
            ],
        };
        let anomaly = report.table_anomaly().expect("anomaly");
        assert!(anomaly.contains("record 1"), "{anomaly}");
    }
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
    fn a_preflight_accepts_a_raised_floor_but_not_an_inverted_window() {
        let want = vec![graphics(1_000_000, 1_200_000)];
        // The driver raises the floor to the live clock: still writable.
        let raised = vec![graphics(1_082_343, 1_200_000)];
        assert_eq!(check_preflight(&want, &raised), Ok(()));
        // A floor above the ceiling has no window left.
        let inverted = vec![graphics(1_202_343, 1_200_000)];
        assert_eq!(
            check_preflight(&want, &inverted),
            Err((
                PreflightFault::Bounds,
                graphics(1_000_000, 1_200_000),
                Some(graphics(1_202_343, 1_200_000))
            ))
        );
        // Zeroed bounds and moved identity are faults of their own.
        assert!(matches!(
            check_preflight(&want, &[graphics(0, 1_200_000)]),
            Err((PreflightFault::Bounds, _, _))
        ));
        assert_eq!(
            check_preflight(&want, &[LockWindow::new(4, DOMAIN_GRAPHICS, 1, 2)]),
            Err((
                PreflightFault::Identity,
                graphics(1_000_000, 1_200_000),
                Some(LockWindow::new(4, DOMAIN_GRAPHICS, 1, 2))
            ))
        );
        assert_eq!(
            check_preflight(&want, &[]),
            Err((PreflightFault::Count, graphics(1_000_000, 1_200_000), None))
        );
    }

    #[test]
    fn a_readback_may_move_the_floor_but_not_the_ceiling_or_the_domain() {
        let want = vec![graphics(1_080_000, 1_620_000)];
        // Same identity and ceiling, floor recomputed by the driver.
        assert_eq!(
            first_stable_difference(&want, &[graphics(1_620_000, 1_620_000)]),
            None
        );
        assert_eq!(
            first_stable_difference(&want, &[graphics(1_080_000, 1_700_000)]),
            Some((
                graphics(1_080_000, 1_620_000),
                Some(graphics(1_080_000, 1_700_000))
            ))
        );
        assert_eq!(
            first_stable_difference(&want, &[LockWindow::new(8, 2, 1_080_000, 1_620_000)]),
            Some((
                graphics(1_080_000, 1_620_000),
                Some(LockWindow::new(8, 2, 1_080_000, 1_620_000))
            ))
        );
        assert!(first_stable_difference(&want, &[]).is_some());
    }

    #[test]
    fn names_stay_readable_for_unknown_bits() {
        assert_eq!(domain_name(DOMAIN_GRAPHICS), "graphics");
        assert_eq!(domain_name(DOMAIN_BIT_64), "bit64");
        assert_eq!(domain_name(0x80), "0x80");
        assert!(graphics(1, 2).to_string().contains("graphics"));
    }
}

#[cfg(test)]
mod state_count_tests {
    use super::*;

    fn block_with(format: u32, count: u32) -> Vec<u8> {
        let mut block = vec![0u8; STATE_BYTES];
        block[0..4].copy_from_slice(&format.to_le_bytes());
        block[4..8].copy_from_slice(&count.to_le_bytes());
        block
    }

    #[test]
    fn a_format_word_and_count_are_read_back() {
        assert_eq!(parse_state_count(&block_with(FORMAT_WORD, 0)).unwrap(), 0);
        assert_eq!(parse_state_count(&block_with(FORMAT_WORD, 48)).unwrap(), 48);
    }

    #[test]
    fn a_zeroed_block_is_refused_rather_than_read_as_no_locks() {
        let err = parse_state_count(&block_with(0, 0)).unwrap_err();
        assert!(err.contains("format word"), "{err}");
    }

    fn block_with_records(records: &[[u32; 12]]) -> Vec<u8> {
        let mut block = vec![0u8; STATE_BYTES];
        for (index, record) in records.iter().enumerate() {
            let base = STATE_RECORD_BASE + index * STATE_RECORD_BYTES;
            for (slot, word) in record.iter().enumerate() {
                block[base + slot * 4..base + slot * 4 + 4].copy_from_slice(&word.to_le_bytes());
            }
        }
        block
    }

    fn record(min: u32, max: u32) -> [u32; 12] {
        [
            0x0100_0002,
            1,
            min,
            max,
            0,
            0,
            16,
            6_000_984,
            6_000_984,
            0,
            0,
            0,
        ]
    }

    #[test]
    fn the_records_of_a_two_window_answer_are_read_back_in_order() {
        let block =
            block_with_records(&[record(1_700_000, 1_700_000), record(1_800_000, 1_800_000)]);
        let records = parse_state_records(&block);
        assert_eq!(records.len(), 2);
        assert_eq!(records[0].offset, STATE_RECORD_BASE);
        assert_eq!(records[0].min_khz(), 1_700_000);
        assert_eq!(records[0].max_khz(), 1_700_000);
        assert_eq!(records[0].memory_khz(), Some(6_000_984));
        assert_eq!(records[1].offset, STATE_RECORD_BASE + STATE_RECORD_BYTES);
        assert_eq!(records[1].max_khz(), 1_800_000);
    }

    #[test]
    fn an_answer_with_nothing_stored_has_no_records() {
        assert!(parse_state_records(&[0u8; STATE_BYTES]).is_empty());
        // Short of one record: nothing to read, nothing invented.
        assert!(parse_state_records(&[0xffu8; STATE_RECORD_BASE + 8]).is_empty());
    }

    #[test]
    fn a_record_with_disagreeing_memory_dwords_reports_no_memory_clock() {
        let mut one = record(1_700_000, 1_700_000);
        one[8] = 6_000_985;
        let records = parse_state_records(&block_with_records(&[one]));
        assert_eq!(records[0].memory_khz(), None);
        let mut zeroed = record(1_700_000, 1_700_000);
        zeroed[7] = 0;
        zeroed[8] = 0;
        assert_eq!(
            parse_state_records(&block_with_records(&[zeroed]))[0].memory_khz(),
            None
        );
    }

    #[test]
    fn the_newest_record_is_the_one_compared_with_the_applied_window() {
        let block =
            block_with_records(&[record(1_700_000, 1_700_000), record(1_620_000, 1_800_000)]);
        assert_eq!(
            last_stored_window(&parse_state_records(&block)),
            Some((1_620_000, 1_800_000))
        );
        assert_eq!(last_stored_window(&[]), None);
    }

    fn window(min_khz: u32, max_khz: u32) -> LockWindow {
        LockWindow {
            pstate: 8,
            domain: DOMAIN_GRAPHICS,
            min_khz,
            max_khz,
        }
    }

    #[test]
    fn a_stored_record_is_compared_against_the_applied_window() {
        let block =
            block_with_records(&[record(1_700_000, 1_700_000), record(1_620_000, 1_800_000)]);
        let records = parse_state_records(&block);
        assert_eq!(
            storage_matches(&[window(1_620_000, 1_800_000)], &records),
            Some(true)
        );
        assert_eq!(
            storage_matches(&[window(1_620_000, 1_750_000)], &records),
            Some(false)
        );
        // Nothing on either side is not a pass.
        assert_eq!(storage_matches(&[], &records), None);
        assert_eq!(storage_matches(&[window(1_620_000, 1_800_000)], &[]), None);
    }

    #[test]
    fn a_state_report_names_the_window_it_stores() {
        let block = block_with_records(&[record(1_620_000, 1_800_000)]);
        let report = LockedStateReport {
            count_at_4: Some(48),
            records: parse_state_records(&block),
        };
        assert_eq!(report.stored_window(), Some((1_620_000, 1_800_000)));
        assert_eq!(LockedStateReport::default().stored_window(), None);
    }

    #[test]
    fn a_short_block_is_refused() {
        assert!(parse_state_count(&[1u8, 0, 0, 0]).is_err());
    }
}
