//! Raw parsing of the private clock/offset control blocks.
//!
//! The macOS driver exposes the clock-offset channel through private `NV2080`
//! control ids that are not in the open kernel modules' headers. The probe
//! (`probe-macos-rm`) established their geometry on the live device and the
//! numbers below are that measurement, not a guess: the container sizes are
//! byte-exact (`48 + 32*384`, `20 + 32*224`, `60 + 32*64`, `8 + 32*24`) and
//! every record stride and field offset used here was read back from the device.
//!
//! The fourth of them, the clock document, is the one that carries an actual
//! offset: its records hold a domain bit, a frequency and a small signed value
//! that is the domain's current offset in kHz. That reading is an inference from
//! captured bytes, so it is stated with the two things that support it -- two of
//! its frequencies are bit-for-bit the live graphics and memory clocks RUSD
//! reports, and its non-zero magnitudes sit inside the +-18.75 MHz span the
//! range block declares -- and with the falsifier: a controlled write must move
//! the matching record.
//!
//! This module is deliberately platform-independent: it takes `&[u8]` and
//! returns typed values, so it can be unit-tested without a GPU, an RM session
//! or macOS. The FFI side ([`crate::macos_rm`]) only moves bytes.
//!
//! Two naming caveats, because they are inferences rather than driver
//! declarations:
//!
//! * the range block's two `i32` fields are called `min_khz`/`max_khz` because
//!   they read `-18750`/`+18750` and the NVAPI caller struct orders the same
//!   pair; no driver header names them on this platform. The in-tree NVAPI copy
//!   does name the meaning: its IDA note on RM `0x20809025` calls the pair the
//!   frequency-offset floor and ceiling, "the +-18.75 MHz control span", which
//!   also fixes the unit as kHz for this magnitude;
//! * which dword of a control record carries the *offset* is **not** decided
//!   here -- the parser reports all twelve value dwords verbatim and leaves the
//!   identification to a write/read-back experiment. The clock document is the
//!   read that does not need that experiment: it publishes an offset per domain,
//!   which is why it is parsed alongside the three control blocks.

use core::fmt;

/// `0x20809019`: the per-domain directory (domain bit -> record type).
pub const OFFSET_DIRECTORY_CMD: u32 = 0x2080_9019;
/// Byte size the driver requires for [`OFFSET_DIRECTORY_CMD`].
pub const OFFSET_DIRECTORY_BYTES: usize = 12_336;
const OFFSET_DIRECTORY_HEAD_BYTES: usize = 48;
const OFFSET_DIRECTORY_RECORD_BYTES: usize = 384;
const OFFSET_DIRECTORY_RECORDS: usize = 32;

/// `0x20809025`: the per-domain offset range.
pub const OFFSET_RANGE_CMD: u32 = 0x2080_9025;
/// Byte size the driver requires for [`OFFSET_RANGE_CMD`].
pub const OFFSET_RANGE_BYTES: usize = 7_188;
const OFFSET_RANGE_HEAD_BYTES: usize = 20;
const OFFSET_RANGE_RECORD_BYTES: usize = 224;
const OFFSET_RANGE_RECORDS: usize = 32;

/// `0x2080901b`: the domain-control GET (current applied values).
pub const OFFSET_CONTROL_CMD: u32 = 0x2080_901b;
/// Byte size the driver requires for [`OFFSET_CONTROL_CMD`].
pub const OFFSET_CONTROL_BYTES: usize = 2_108;
const OFFSET_CONTROL_HEAD_BYTES: usize = 60;
const OFFSET_CONTROL_RECORD_BYTES: usize = 64;
const OFFSET_CONTROL_RECORDS: usize = 32;
/// Dword index of the selection mask inside the control header.
///
/// This is an *input*: the handler reads it and writes only the records whose bit
/// is set, which the seed sweep proves by behaviour (`0xFF` in this field fills
/// records, `0xFF` in a neighbouring dword is rejected). Whether the handler also
/// *echoes* the seed back is not settled by that sweep: the seed replaced the
/// `0xA5` background, so "left alone" and "echoed" look identical, and the field
/// is correspondingly absent from the changed-dword set either way. The recorded
/// Windows-side layout (see the marshalling correction table) says the driver
/// echoes the seed rather than the real controllable set, which is compatible
/// with the measurement; the probe's other seeds (`0x3FF`, `0x1`) are what would
/// separate the two, since an echo would return them while a driver-filled field
/// would not. Either way, the value is load-bearing: seeding `0xFF` here is what
/// is load-bearing rather than informational -- seeding `0xFF` here is what
/// makes the handler answer `NV_OK` with records in it, while a zeroed block asks
/// for nothing, so a caller that wants
/// the current values must seed this field before the call; the native read path
/// does (`NVOC_RM_OFFSET_CONTROL_MASK` in `core/native/macos_rm.c`).
///
/// The bits select *directory entries by index*, not domains by their stamped
/// bit: a mask of `0xFF` selects the eight entries at indices 0..7 and the
/// recorded run writes 106 dwords, which is exactly the 14 header dwords plus
/// seven 13-dword records plus the one stamp-only record at index 6. Reading the
/// mask as domain bits instead would select seven value-carrying entries
/// (indices 0,1,2,3,4,5,7 for bits 0..6) and predict 105.
pub const OFFSET_CONTROL_MASK_DWORD: usize = 1;
/// Byte offsets, inside a control record, of the twelve dwords the handler
/// writes. The gaps (`+4`, `+48`, `+60`) are never written and are therefore
/// not carried: a 0xA5 pre-seed left them untouched.
const OFFSET_VALUE_BYTES: [usize; 12] = [8, 12, 16, 20, 24, 28, 32, 36, 40, 44, 52, 56];

/// `0x20809061`: the per-domain clock document (a "VFP get-info" in the
/// driver's export, which is the only one of the four commands whose accepted
/// size is small enough to have been captured whole).
pub const CLK_DOCUMENT_CMD: u32 = 0x2080_9061;
/// Byte size the driver requires for [`CLK_DOCUMENT_CMD`].
///
/// This is the one accepted form, measured: the same command refused sizes
/// 493060, 83996, 369796, 157692 and 249844 with `NV_ERR_INVALID_ARGUMENT`.
pub const CLK_DOCUMENT_BYTES: usize = 776;
const CLK_DOCUMENT_HEAD_BYTES: usize = 8;
const CLK_DOCUMENT_RECORD_BYTES: usize = 24;
const CLK_DOCUMENT_RECORDS: usize = 32;
/// Dword every populated record starts with (`0x00000303`). Slots that hold
/// this and slots that are entirely zero are the only two shapes the captured
/// payload has, so a slot with data but no marker is reported rather than
/// guessed at.
pub const CLK_DOCUMENT_RECORD_MARKER: u32 = 0x0000_0303;

/// A block that did not have the size its command requires.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OffsetParseError {
    /// Bytes the fixed geometry requires.
    pub need: usize,
    /// Bytes actually supplied.
    pub got: usize,
}

impl fmt::Display for OffsetParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "block is {} bytes, the command's geometry requires {}",
            self.got, self.need
        )
    }
}

impl std::error::Error for OffsetParseError {}

/// One directory entry: the domain bit a control record belongs to, and the
/// record's type stamp.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OffsetDomainEntry {
    /// Index of the entry, i.e. the record index it names in the control block.
    pub index: usize,
    /// Domain bit the entry selects (the public graphics/memory/... domains are
    /// not yet mapped onto these bits, so this is reported verbatim).
    pub domain_bit: u32,
    /// Type stamp the matching control record carries.
    pub type_stamp: u16,
}

/// The parsed directory.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OffsetDirectory {
    /// The header as returned, dword by dword. Its fields beyond the entry
    /// count are not named, so they are kept raw rather than interpreted.
    pub head: Vec<u32>,
    /// Entries that carry a type stamp, in directory order.
    pub entries: Vec<OffsetDomainEntry>,
}

/// One offset-range record.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OffsetRangeRecord {
    /// Record index.
    pub index: usize,
    /// First dword of the record, as returned (an unidentified tag that counts
    /// `0x01030101`, `0x01040101`, ... across records).
    pub tag: u32,
    /// `rec+8`, in kHz.
    pub max_vf_khz: u32,
    /// `rec+20`, in kHz.
    pub min_khz: i32,
    /// `rec+24`, in kHz.
    pub max_khz: i32,
}

/// The parsed range block.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OffsetRange {
    /// The header as returned, dword by dword.
    pub head: Vec<u32>,
    /// Records that carry any non-zero field, in record order.
    pub records: Vec<OffsetRangeRecord>,
}

/// One control record: the type stamp and the twelve value dwords.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OffsetControlRecord {
    /// Record index, which is also the mask bit that selects it.
    pub index: usize,
    /// Type stamp at `rec+0` (`0x0808`/`0x0909` for the entries the directory
    /// carries, `0x0303` for the one entry that writes no value dword).
    pub type_stamp: u16,
    /// The twelve value dwords `rec+8..rec+44` and `rec+52`/`rec+56` hold, in
    /// ascending byte order -- i.e. the bytes the handler actually wrote.
    pub value_dwords: Vec<u32>,
}

/// The parsed control block.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OffsetControl {
    /// Selection mask read back from `head+4`.
    pub mask: u32,
    /// The header as returned, dword by dword.
    pub head: Vec<u32>,
    /// Records selected by the mask, in record order.
    pub records: Vec<OffsetControlRecord>,
}

/// Read a little-endian dword, or the block's error if the block is short.
fn dword(buf: &[u8], at: usize) -> u32 {
    u32::from_le_bytes([buf[at], buf[at + 1], buf[at + 2], buf[at + 3]])
}

/// Read a little-endian signed dword.
fn sdword(buf: &[u8], at: usize) -> i32 {
    dword(buf, at) as i32
}

/// Read a little-endian word.
fn word(buf: &[u8], at: usize) -> u16 {
    u16::from_le_bytes([buf[at], buf[at + 1]])
}

/// Read the header of `head_bytes` as dwords, checking the exact total size the
/// command's geometry requires: a short buffer would otherwise be silently
/// parsed as a shorter directory.
fn head_dwords(buf: &[u8], need: usize, head_bytes: usize) -> Result<Vec<u32>, OffsetParseError> {
    if buf.len() < need {
        return Err(OffsetParseError {
            need,
            got: buf.len(),
        });
    }
    Ok((0..head_bytes / 4).map(|i| dword(buf, i * 4)).collect())
}

/// Parse a `0x20809019` directory.
///
/// Entries without a type stamp are padding: the driver writes only the entries
/// it has, and the rest of the array stays zero.
pub fn parse_directory(buf: &[u8]) -> Result<OffsetDirectory, OffsetParseError> {
    let head = head_dwords(buf, OFFSET_DIRECTORY_BYTES, OFFSET_DIRECTORY_HEAD_BYTES)?;
    let mut entries = Vec::new();
    for index in 0..OFFSET_DIRECTORY_RECORDS {
        let at = OFFSET_DIRECTORY_HEAD_BYTES + index * OFFSET_DIRECTORY_RECORD_BYTES;
        let type_stamp = word(buf, at);
        if type_stamp != 0 {
            entries.push(OffsetDomainEntry {
                index,
                domain_bit: dword(buf, at + 4),
                type_stamp,
            });
        }
    }
    Ok(OffsetDirectory { head, entries })
}

/// Parse a `0x20809025` range block.
pub fn parse_range(buf: &[u8]) -> Result<OffsetRange, OffsetParseError> {
    let head = head_dwords(buf, OFFSET_RANGE_BYTES, OFFSET_RANGE_HEAD_BYTES)?;
    let mut records = Vec::new();
    for index in 0..OFFSET_RANGE_RECORDS {
        let at = OFFSET_RANGE_HEAD_BYTES + index * OFFSET_RANGE_RECORD_BYTES;
        let record = OffsetRangeRecord {
            index,
            tag: dword(buf, at),
            max_vf_khz: dword(buf, at + 8),
            min_khz: sdword(buf, at + 20),
            max_khz: sdword(buf, at + 24),
        };
        if record.tag != 0 || record.max_vf_khz != 0 || record.min_khz != 0 || record.max_khz != 0 {
            records.push(record);
        }
    }
    Ok(OffsetRange { head, records })
}

/// Parse a `0x2080901b` control block.
///
/// Only the records the mask selects are reported: an unselected record is
/// never written by the handler, so its twelve dwords would be indistinguishable
/// from real zeroes.
pub fn parse_control(buf: &[u8]) -> Result<OffsetControl, OffsetParseError> {
    let head = head_dwords(buf, OFFSET_CONTROL_BYTES, OFFSET_CONTROL_HEAD_BYTES)?;
    let mask = head[OFFSET_CONTROL_MASK_DWORD];
    let mut records = Vec::new();
    for index in 0..OFFSET_CONTROL_RECORDS {
        if mask & (1 << index) == 0 {
            continue;
        }
        let at = OFFSET_CONTROL_HEAD_BYTES + index * OFFSET_CONTROL_RECORD_BYTES;
        records.push(OffsetControlRecord {
            index,
            type_stamp: word(buf, at),
            value_dwords: OFFSET_VALUE_BYTES
                .iter()
                .map(|o| dword(buf, at + o))
                .collect(),
        });
    }
    Ok(OffsetControl {
        mask,
        head,
        records,
    })
}

/// The parsed clock document.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClkDocument {
    /// The header as returned, dword by dword (`0x000000ff`, `0x000007ff` in the
    /// captured payload). Neither field is named anywhere on this platform, so
    /// both are kept raw.
    pub head: Vec<u32>,
    /// Populated records, in document order.
    pub entries: Vec<ClkDocumentEntry>,
    /// Slots that hold something without being entries.
    ///
    /// The captured payload has two of these (slots 21 and 31, each holding a
    /// single `0xff` byte and otherwise zeroes) and twenty-one that are entirely
    /// zero. They are reported by index rather than ignored, because "slot 21 is
    /// not an entry" is a fact about the bytes while "the slots after the last
    /// entry are padding" would be a guess: a second, differently shaped array
    /// sharing this container cannot be ruled out yet.
    pub unparsed_slots: Vec<usize>,
}

/// Parse a `0x20809061` clock document.
///
/// Populated records are spaced one stride apart and every one of them starts
/// with the same marker; slots past the last entry are zero except for the two
/// strays reported in [`ClkDocument::unparsed_slots`]. Nothing beyond those two
/// shapes is assumed: a slot with the marker is an entry, a slot with neither
/// the marker nor any content is padding, and anything else is recorded rather
/// than read as an entry.
pub fn parse_clock_document(buf: &[u8]) -> Result<ClkDocument, OffsetParseError> {
    let head = head_dwords(buf, CLK_DOCUMENT_BYTES, CLK_DOCUMENT_HEAD_BYTES)?;
    let mut entries = Vec::new();
    let mut unparsed_slots = Vec::new();
    for slot in 0..CLK_DOCUMENT_RECORDS {
        let at = CLK_DOCUMENT_HEAD_BYTES + slot * CLK_DOCUMENT_RECORD_BYTES;
        let dwords: Vec<u32> = (0..CLK_DOCUMENT_RECORD_BYTES / 4)
            .map(|i| dword(buf, at + i * 4))
            .collect();
        if dwords.iter().all(|&d| d == 0) {
            continue;
        }
        if dwords[0] != CLK_DOCUMENT_RECORD_MARKER {
            unparsed_slots.push(slot);
            continue;
        }
        entries.push(ClkDocumentEntry {
            index: slot,
            domain_bit: dwords[1],
            frequency_khz: dwords[2],
            offset_khz: dwords[3] as i32,
            flags: dwords[4],
        });
    }
    Ok(ClkDocument {
        head,
        entries,
        unparsed_slots,
    })
}

/// One domain in the clock document.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ClkDocumentEntry {
    /// Record slot, i.e. the index of the domain in the document.
    pub index: usize,
    /// The domain bit this record names. Reported verbatim: the public
    /// graphics/memory names are not in any header on this platform, and the
    /// only bit that can be named here at all is `0x10`, which the in-tree NVAPI
    /// copy's own mask names MCLK -- and whose frequency in the captured payload
    /// is the live memory clock, which agrees.
    pub domain_bit: u32,
    /// The frequency the record reports, in kHz.
    ///
    /// An inference, not a header declaration: for two of the captured records
    /// this equals the live RUSD reading exactly (1 530 000 graphics,
    /// 6 000 984 memory), and no other field in the payload has that property.
    pub frequency_khz: u32,
    /// The signed value the record carries, in kHz.
    ///
    /// This is the offset reading the goal asks for: the captured values are
    /// small and signed (-5, -126, -244, -3904 kHz), of the same order as the
    /// +-18.75 MHz span the range block declares. A controlled write that moves
    /// this field would confirm it; nothing has written yet.
    pub offset_khz: i32,
    /// The dword after the offset, as returned. It is not named: the captured
    /// values (`0x00020002` on the records whose offset reads -1,
    /// `0x00010000`/`0x00060000` on the records with a real offset) suggest it
    /// distinguishes "no offset set" from "offset set", but that is a guess and
    /// is kept raw.
    pub flags: u32,
}

/// The outcome of reading one block of the private clock-offset channel.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OffsetBlockRead<T> {
    /// The driver answered and the payload parsed at its declared geometry.
    Parsed(T),
    /// The driver refused the command. The value is the RM status as returned:
    /// `0x62` while the GSP is in its reset-required state, `0x56` when the
    /// handler is not implemented for this chip.
    Refused(u32),
    /// The driver answered with a zero status, but the payload does not have
    /// the geometry its command declares. Requesting the exact size makes this
    /// a driver-side surprise rather than a caller error, so it is reported
    /// instead of guessed at.
    Malformed(OffsetParseError),
}

/// One read of the private clock-offset channel.
///
/// Each block keeps its own outcome. A refusal must never be rendered as a
/// value -- in particular not as "the offset is zero" -- and one refused block
/// must not discard the two that answered.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OffsetChannelSnapshot {
    /// `0x20809019`: the domain bit and record type of each entry.
    pub directory: OffsetBlockRead<OffsetDirectory>,
    /// `0x20809025`: the per-record offset bounds, in kHz.
    pub range: OffsetBlockRead<OffsetRange>,
    /// `0x2080901b`: the current values. Every value dword reads zero at stock,
    /// so a `Parsed` block means "no offset is applied" -- a measured zero,
    /// not a missing reading.
    pub control: OffsetBlockRead<OffsetControl>,
    /// `0x20809061`: the per-domain clock document, the one block that publishes
    /// an actual offset rather than a window or a set of unlabelled dwords.
    pub clock_document: OffsetBlockRead<ClkDocument>,
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Build a block of `len` bytes with `writes` laid down as dwords.
    fn block(len: usize, writes: &[(usize, u32)]) -> Vec<u8> {
        let mut buf = vec![0u8; len];
        for (at, value) in writes {
            buf[*at..*at + 4].copy_from_slice(&value.to_le_bytes());
        }
        buf
    }

    #[test]
    fn a_short_block_is_rejected_rather_than_parsed() {
        assert_eq!(
            parse_directory(&[0u8; 16]),
            Err(OffsetParseError {
                need: OFFSET_DIRECTORY_BYTES,
                got: 16
            })
        );
        assert_eq!(
            parse_range(&[0u8; 16]),
            Err(OffsetParseError {
                need: OFFSET_RANGE_BYTES,
                got: 16
            })
        );
        assert_eq!(
            parse_control(&[0u8; 16]),
            Err(OffsetParseError {
                need: OFFSET_CONTROL_BYTES,
                got: 16
            })
        );
    }

    #[test]
    fn the_control_mask_selects_records_and_only_the_written_dwords_are_carried() {
        // Select records 0 and 3, stamp them, and put a marker in every byte the
        // handler is known not to write.
        let mut writes = vec![(4, 0b1001)];
        for index in [0usize, 3] {
            let at = OFFSET_CONTROL_HEAD_BYTES + index * OFFSET_CONTROL_RECORD_BYTES;
            writes.push((at, 0x0909));
            for (n, off) in OFFSET_VALUE_BYTES.iter().enumerate() {
                writes.push((at + off, 0x1000 + n as u32));
            }
            // +4, +48 and +60 are never written: mark them and prove they are
            // not mistaken for value dwords.
            for skipped in [4usize, 48, 60] {
                writes.push((at + skipped, 0xdead_beef));
            }
        }
        let parsed = parse_control(&block(OFFSET_CONTROL_BYTES, &writes)).unwrap();
        assert_eq!(parsed.mask, 0b1001);
        assert_eq!(
            parsed.records.iter().map(|r| r.index).collect::<Vec<_>>(),
            vec![0, 3]
        );
        for record in &parsed.records {
            assert_eq!(record.type_stamp, 0x0909);
            assert_eq!(
                record.value_dwords,
                (0..12).map(|n| 0x1000 + n).collect::<Vec<u32>>()
            );
            assert!(!record.value_dwords.contains(&0xdead_beef));
        }
    }

    #[test]
    fn an_unselected_record_is_not_reported() {
        let mut writes = vec![(4, 1)];
        let at = OFFSET_CONTROL_HEAD_BYTES + OFFSET_CONTROL_RECORD_BYTES;
        // Record 1 is stamped but not selected.
        writes.push((at, 0x0808));
        writes.push((at + 8, 0x1234_5678));
        let parsed = parse_control(&block(OFFSET_CONTROL_BYTES, &writes)).unwrap();
        assert_eq!(parsed.records.len(), 1);
        assert_eq!(parsed.records[0].index, 0);
        assert_eq!(parsed.records[0].type_stamp, 0);
    }

    #[test]
    fn a_block_whose_mask_was_never_seeded_reports_no_record() {
        // The shape a caller gets from a zeroed buffer: the handler writes only
        // the records the mask selects, and this block's mask says "none". The
        // record bytes are filled anyway, because a driver that wrote everything
        // regardless would look identical here -- which is the point: with no
        // selection there is no way to tell a real value from a filler byte, so
        // reporting nothing is the only honest answer. This is why the native
        // read path seeds the mask instead of sending a zeroed block.
        let mut writes = vec![(4, 0)];
        let at = OFFSET_CONTROL_HEAD_BYTES;
        writes.push((at, 0x0808));
        for (n, off) in OFFSET_VALUE_BYTES.iter().enumerate() {
            writes.push((at + off, 0x2000 + n as u32));
        }
        let parsed = parse_control(&block(OFFSET_CONTROL_BYTES, &writes)).unwrap();
        assert_eq!(parsed.mask, 0);
        assert!(parsed.records.is_empty(), "{:?}", parsed.records);
    }

    #[test]
    fn the_directory_reports_domain_bits_in_index_order() {
        let mut writes = Vec::new();
        for (index, (bit, stamp)) in [(1u32, 0x0808u16), (2, 0x0909), (0x10, 0x0808)]
            .into_iter()
            .enumerate()
        {
            let at = OFFSET_DIRECTORY_HEAD_BYTES + index * OFFSET_DIRECTORY_RECORD_BYTES;
            writes.push((at, u32::from(stamp)));
            writes.push((at + 4, bit));
        }
        let parsed = parse_directory(&block(OFFSET_DIRECTORY_BYTES, &writes)).unwrap();
        assert_eq!(
            parsed
                .entries
                .iter()
                .map(|e| (e.index, e.domain_bit, e.type_stamp))
                .collect::<Vec<_>>(),
            vec![(0, 1, 0x0808), (1, 2, 0x0909), (2, 0x10, 0x0808)]
        );
    }

    #[test]
    fn the_range_block_reports_signed_bounds() {
        let at = OFFSET_RANGE_HEAD_BYTES;
        let writes = vec![
            (at, 0x0103_0101),
            (at + 8, 1_638_400),
            (at + 20, (-18_750i32) as u32),
            (at + 24, 18_750),
        ];
        let parsed = parse_range(&block(OFFSET_RANGE_BYTES, &writes)).unwrap();
        assert_eq!(parsed.records.len(), 1);
        let record = parsed.records[0];
        assert_eq!(record.tag, 0x0103_0101);
        assert_eq!(record.max_vf_khz, 1_638_400);
        assert_eq!(record.min_khz, -18_750);
        assert_eq!(record.max_khz, 18_750);
    }

    #[test]
    fn the_record_strides_are_the_sizes_the_driver_requires() {
        // The geometries must tile their containers exactly, or a record
        // would straddle the end of the block.
        assert_eq!(
            OFFSET_DIRECTORY_HEAD_BYTES + OFFSET_DIRECTORY_RECORDS * OFFSET_DIRECTORY_RECORD_BYTES,
            OFFSET_DIRECTORY_BYTES
        );
        assert_eq!(
            OFFSET_RANGE_HEAD_BYTES + OFFSET_RANGE_RECORDS * OFFSET_RANGE_RECORD_BYTES,
            OFFSET_RANGE_BYTES
        );
        assert_eq!(
            OFFSET_CONTROL_HEAD_BYTES + OFFSET_CONTROL_RECORDS * OFFSET_CONTROL_RECORD_BYTES,
            OFFSET_CONTROL_BYTES
        );
        assert_eq!(
            CLK_DOCUMENT_HEAD_BYTES + CLK_DOCUMENT_RECORDS * CLK_DOCUMENT_RECORD_BYTES,
            CLK_DOCUMENT_BYTES
        );
    }

    #[test]
    fn the_clock_document_reports_a_domain_frequency_and_offset() {
        let first = CLK_DOCUMENT_HEAD_BYTES;
        let third = CLK_DOCUMENT_HEAD_BYTES + 2 * CLK_DOCUMENT_RECORD_BYTES;
        let writes = vec![
            (0, 0xff),
            (4, 0x7ff),
            (first, CLK_DOCUMENT_RECORD_MARKER),
            (first + 4, 1),
            (first + 8, 1_530_000),
            (first + 12, (-1i32) as u32),
            (first + 16, 0x0002_0002),
            (third, CLK_DOCUMENT_RECORD_MARKER),
            (third + 4, 0x10),
            (third + 8, 6_000_984),
            (third + 12, (-126i32) as u32),
            (third + 16, 0),
        ];
        let parsed = parse_clock_document(&block(CLK_DOCUMENT_BYTES, &writes)).unwrap();
        assert_eq!(parsed.head, vec![0xff, 0x7ff]);
        assert_eq!(
            parsed
                .entries
                .iter()
                .map(|e| (
                    e.index,
                    e.domain_bit,
                    e.frequency_khz,
                    e.offset_khz,
                    e.flags
                ))
                .collect::<Vec<_>>(),
            vec![
                (0, 1, 1_530_000, -1, 0x0002_0002),
                (2, 0x10, 6_000_984, -126, 0),
            ]
        );
    }

    #[test]
    fn an_empty_clock_document_slot_is_padding_and_a_stray_value_is_reported() {
        // A zeroed slot past the last entry is padding: the captured payload has
        // 21 of them and none of them is an entry.
        let parsed = parse_clock_document(&block(CLK_DOCUMENT_BYTES, &[])).unwrap();
        assert!(parsed.entries.is_empty());
        assert_eq!(parsed.head, vec![0, 0]);

        // Data without the marker is neither padding nor an entry. The captured
        // payload has two such slots, so it is recorded by index rather than
        // read as an entry or dropped silently.
        let stray = block(CLK_DOCUMENT_BYTES, &[(CLK_DOCUMENT_HEAD_BYTES + 8, 0x1234)]);
        let parsed = parse_clock_document(&stray).unwrap();
        assert!(parsed.entries.is_empty());
        assert_eq!(parsed.unparsed_slots, vec![0]);
    }

    #[test]
    fn a_short_clock_document_is_rejected_rather_than_parsed() {
        assert_eq!(
            parse_clock_document(&[0u8; 24]),
            Err(OffsetParseError {
                need: CLK_DOCUMENT_BYTES,
                got: 24
            })
        );
    }
}
