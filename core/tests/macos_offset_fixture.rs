//! The offset parsers checked against bytes captured from the device.
//!
//! The unit tests in `macos_offset` check that the parser implements the
//! documented geometry; these check that the documented geometry is the
//! device's. The fixtures are the literal payloads of two pre-wedge `NV_OK`
//! runs of `probe-macos-rm` on the target card (GTX 1650 SUPER, TU116,
//! 610.57.04), captured 2026-10-17 and stored on the test machine under
//! `outputs/stage-gsp-clkctl-20261017T092536Z/clkctl-hex.txt`. They are
//! committed here because a parser is only as good as the bytes it has been
//! shown: an offset table read off a live device cannot be re-derived from any
//! header on this platform.
//!
//! What these tests can and cannot say:
//!
//! * they pin the two blocks whose *payload* was captured raw -- the domain
//!   directory and the offset range -- field by field, so a drift in any record
//!   stride or field offset fails here rather than on the GPU;
//! * the control block's payload was never dumped raw (only its write set was),
//!   so its geometry is pinned by the synthetic test in `macos_offset` plus the
//!   directory's type sequence asserted below, not by a byte fixture;
//! * none of this says which dword carries the *current offset*. Every value
//!   dword reads zero at stock, so no byte fixture can settle that.

use nvoc_core::macos_offset::{
    OffsetBlockRead, OffsetParseError, parse_control, parse_directory, parse_range,
};

/// `0x20809019 @12336`, status `NV_OK`, exactly as the driver returned it.
const DIRECTORY: &[u8] = include_bytes!("data/macos-offset-directory.bin");
/// `0x20809025 @7188`, status `NV_OK`, exactly as the driver returned it.
const RANGE: &[u8] = include_bytes!("data/macos-offset-range.bin");

#[test]
fn the_directory_fixture_decodes_to_the_ten_recorded_entries() {
    let directory = parse_directory(DIRECTORY).expect("the fixture is the exact size");
    // (index, domain bit, type stamp) in directory order. The domain bits are
    // the driver's, not a guess: they are why the public core/memory domains
    // cannot simply be read off this table yet -- bit 1 and bit 16 are both
    // type 0x0808, and no header on this platform names either.
    let expected = [
        (0usize, 1u32, 0x0808u16),
        (1, 2, 0x0909),
        (2, 16, 0x0808),
        (3, 4, 0x0909),
        (4, 8, 0x0909),
        (5, 1_048_576, 0x0909),
        (6, 524_288, 0x0303),
        (7, 64, 0x0808),
        (8, 2_097_152, 0x0909),
        (9, 32, 0x0909),
    ];
    let got = directory
        .entries
        .iter()
        .map(|e| (e.index, e.domain_bit, e.type_stamp))
        .collect::<Vec<_>>();
    assert_eq!(got, expected);
    // Entries past the tenth exist in the container but were never written.
    assert_eq!(directory.head.len(), 12);
}

#[test]
fn the_directory_type_sequence_is_what_the_write_set_argument_used() {
    // The mask 0xFF run selects entries 0..7, whose types the directory gives
    // as 08, 09, 08, 09, 09, 09, 03, 08. That sequence is the load-bearing part
    // of the account of the 106 written dwords (seven type-08/09 records with
    // twelve value dwords each, plus one type-03 record that writes its stamp
    // only). If the directory's types ever change, that arithmetic has to be
    // redone -- so it is asserted here instead of living in prose.
    let directory = parse_directory(DIRECTORY).expect("the fixture is the exact size");
    let types = directory
        .entries
        .iter()
        .take(8)
        .map(|e| e.type_stamp)
        .collect::<Vec<_>>();
    assert_eq!(
        types,
        vec![
            0x0808, 0x0909, 0x0808, 0x0909, 0x0909, 0x0909, 0x0303, 0x0808
        ]
    );
}

#[test]
fn the_range_fixture_carries_the_recorded_bounds() {
    let range = parse_range(RANGE).expect("the fixture is the exact size");
    // Three records, identical bounds: -18750 .. +18750 kHz. The tags count
    // 0x01030101, 0x01040101, 0x01050101 -- an unidentified but consistent
    // sequence, which is why the range is reported per record and not yet
    // attributed to a named domain.
    assert_eq!(
        range
            .records
            .iter()
            .map(|r| (r.index, r.tag, r.max_vf_khz, r.min_khz, r.max_khz))
            .collect::<Vec<_>>(),
        vec![
            (0, 0x0103_0101, 1_638_400, -18_750, 18_750),
            (1, 0x0104_0101, 1_638_400, -18_750, 18_750),
            (2, 0x0105_0101, 1_638_400, -18_750, 18_750),
        ]
    );
}

#[test]
fn the_fixtures_are_the_sizes_their_commands_declare() {
    // A fixture is only evidence while it is the exact block the driver was
    // asked for: truncating one would make the size check in the parser pass
    // for the wrong reason.
    assert_eq!(
        DIRECTORY.len(),
        nvoc_core::macos_offset::OFFSET_DIRECTORY_BYTES
    );
    assert_eq!(RANGE.len(), nvoc_core::macos_offset::OFFSET_RANGE_BYTES);
}

#[test]
fn a_reading_is_never_confused_with_a_value() {
    // The control block has no raw fixture, so the strongest statement this
    // file can make about it is that it refuses to invent one: a short buffer
    // is an error, never a shorter block of zeroes.
    match parse_control(&DIRECTORY[..100]) {
        Err(err) => {
            assert_eq!(err.got, 100);
            assert_eq!(err.need, nvoc_core::macos_offset::OFFSET_CONTROL_BYTES);
        }
        Ok(_) => panic!("a 100-byte buffer must not parse as a control block"),
    }

    // And the three outcomes the public wrapper can produce are distinct by
    // construction. That is the whole reason the status is carried next to the
    // payload: a GSP answering 0x62 must never look like "the offset is zero".
    let parsed: OffsetBlockRead<u32> = OffsetBlockRead::Parsed(0);
    let refused: OffsetBlockRead<u32> = OffsetBlockRead::Refused(0x62);
    let malformed: OffsetBlockRead<u32> = OffsetBlockRead::Malformed(OffsetParseError {
        need: nvoc_core::macos_offset::OFFSET_CONTROL_BYTES,
        got: 100,
    });
    assert_ne!(parsed, refused);
    assert_ne!(parsed, malformed);
    assert_ne!(refused, malformed);
}
