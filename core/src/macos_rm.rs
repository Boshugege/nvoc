//! Native, read-only NullMoth RM backend for macOS 15+ on x86_64.
//!
//! Uses the same IOKit transport as the validation probe, linked into this
//! library. No external probe executable, NVAPI, or NVML runtime is required.
//! The initial implementation discovers the first service exposed by NullMoth.
use crate::macos_offset;
use crate::{ClockDomain, Error, GpuId, PciAddress};
use serde::Serialize;
use std::collections::BTreeMap;
use std::time::{Duration, Instant};

#[derive(Debug, Clone, Default, Serialize)]
pub struct DeviceInfo {
    pub name: Option<String>,
    pub driver_version: String,
    pub rm_gpu_id: u32,
    pub pci_address: String,
    pub vendor_id: u32,
    pub device_id: u32,
    pub subsystem_id: Option<u32>,
    pub revision: Option<u32>,
    pub board_id: Option<u32>,
    pub chip_sku: Option<String>,
    pub project: Option<String>,
    pub memory_total_bytes: Option<u64>,
    pub memory_usable_bytes: Option<u64>,
    pub bar1_bytes: Option<u64>,
    pub l2_cache_bytes: Option<u64>,
    pub supported_class_count: Option<u32>,
    pub unavailable: BTreeMap<String, String>,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct DeviceStatus {
    pub pstate: Option<u32>,
    /// Actual (effective) clock of each public domain the driver reports.
    pub graphics_clock_khz: Option<u32>,
    pub memory_clock_khz: Option<u32>,
    pub video_clock_khz: Option<u32>,
    pub sm_clock_khz: Option<u32>,
    /// Target clock the driver currently requests for the same domains.
    pub graphics_target_clock_mhz: Option<u32>,
    pub memory_target_clock_mhz: Option<u32>,
    pub video_target_clock_mhz: Option<u32>,
    pub sm_target_clock_mhz: Option<u32>,
    pub temperature_c: Option<f64>,
    pub temperature_memory_c: Option<f64>,
    pub temperature_board_c: Option<f64>,
    pub temperature_power_supply_c: Option<f64>,
    /// Instantaneous GPU (die) power draw in mW.
    pub power_mw: Option<u32>,
    /// Current enforced power limit (the live TGP wall) in mW.
    pub power_limit_mw: Option<u32>,
    /// Power limit the driver was asked for, which clamping may exceed.
    pub power_limit_requested_mw: Option<u32>,
    /// Driver's own average GPU power window, when it publishes one.
    pub average_power_mw: Option<u32>,
    /// Total GPU power from the power-policy block (nvml's "power policy").
    pub power_policy_tgp_mw: Option<u32>,
    /// Raw clkThrottleInfo.reasonMask (cl00de.h RUSD_CLK_THROTTLE_REASON_*).
    pub throttle_reason_mask: Option<u32>,
    /// Decoded names of the active throttle reasons.
    pub throttle_reasons: Vec<String>,
    /// Highest live fan reading in rpm (0 means "stopped", not "unknown").
    pub fan_rpm: Option<u32>,
    /// One entry per live cooler the driver reports.
    pub fan_rpms: Vec<u32>,
    /// The same readings with the driver's own cooler slot kept, which is what
    /// the fan-info entry needs: the position in `fan_rpms` is a position after
    /// the plausibility filter, not the cooler index.
    pub fan_coolers: Vec<FanCoolerReading>,
    pub utilization_percent: Option<u32>,
    pub memory_utilization_percent: Option<u32>,
    /// Busy percent per engine, keyed the way the driver names them (gr, fb,
    /// nvenc, nvdec, nvjpg, nvofa).
    pub engine_utilization_percent: BTreeMap<String, f64>,
    /// Negotiated / maximum PCI Express generation, decoded from
    /// RUSD_BUS_DATA_PCIE_GEN_INFO.
    pub pcie_link_gen: Option<u32>,
    pub pcie_max_link_gen: Option<u32>,
    /// Video memory in use (FB_GET_INFO_V2 heap size - heap free).
    pub memory_used_bytes: Option<u64>,
    /// RUSD per-section update timestamp, or even sequence for non-polled data.
    pub rusd_timestamps: BTreeMap<String, u64>,
    pub unavailable: BTreeMap<String, String>,
}

/// Capability flags for the settings surface, so a consumer can disable a
/// control for the right reason instead of guessing.
///
/// `Default` is the "nothing was asked yet" state: every flag false. It exists
/// so a caller that only wants to fill in the flags it read does not have to
/// write out flags it did not -- never as a claim about a backend.
#[derive(Debug, Clone, Copy, Default, Serialize)]
pub struct SettingsCapabilities {
    /// The backend can report the driver's current configuration.
    pub settings_read: bool,
    /// The backend can change configuration. Still false on macOS, for two
    /// independent reasons: the GSP method table does contain the GET/SET twins
    /// (0x2080901B pairs with 0x2080D01C) but every private write id carries the
    /// legacy privileged bits, so an unprivileged client is refused with
    /// `0x1b` before the GSP is reached; and even behind that gate a write
    /// stays gated until the current value, a bounded step and the restore path
    /// are verified on the device. See [`DeviceSettings::notes`].
    pub settings_write: bool,
    /// The backend can put a saved configuration back. Only meaningful once
    /// `settings_write` is true.
    pub settings_restore: bool,
}

/// Whether this backend can change configuration at all, as a property of the
/// backend rather than of one device reading.
///
/// Kept separate from [`SettingsCapabilities`] so a consumer that only wants to
/// know whether a write is possible does not have to produce a settings
/// projection first, and so the projection and every such consumer cannot drift
/// apart: `settings_from` fills the struct from these constants. Turning writes
/// on is therefore a change here and nowhere else.
pub const SETTINGS_WRITE_SUPPORTED: bool = false;

/// Whether this backend can put a saved configuration back. Only meaningful once
/// [`SETTINGS_WRITE_SUPPORTED`] is true.
pub const SETTINGS_RESTORE_SUPPORTED: bool = false;

/// Why an entry that *writes* through this backend is refused while the read
/// half answers.
///
/// The missing layer here is a *verification*, not a control and not a
/// privilege: the write id is located (see [`WRITE_NEEDS_PRIVILEGE_REASON`]), the
/// current value and the driver's window are already readable
/// (`get-pstate-global-freq-offset`, `get-settings`), and what no read can supply
/// is the proof that the field moved -- a single bounded step, the same field
/// read back, and the restore target being the value that same read found, which
/// is what [`SETTINGS_WRITE_SUPPORTED`] waits for.
///
/// It exists so a write entry can say this instead of a platform-level "NVAPI
/// and NVML do not exist here", which is true of the *host* and misleading about
/// the *entry*: the same backend answers the matching read.
pub const MACOS_RM_WRITE_GATE_REASON: &str = "this backend is read-only until one write has been verified on the device: the current value and the driver's own window are already read by the matching entry (get-pstate-global-freq-offset, get-settings), but only issuing a bounded step and reading the same field back can show that the field moved, and the restore target has to be the value that same read found; until then the backend's write gate is closed by construction (SETTINGS_WRITE_SUPPORTED is false), so the missing layer is the verification, not the control";

/// Why a family that needs a *write* is listed as unavailable: the write half of
/// these ids carries the legacy privileged bits, so an unprivileged client is
/// refused before the GSP is consulted. This is a precondition that has been
/// located (the host forwarder's own `(cmd & 0xC000) == 0xC000` test selects
/// exactly the table's write half and nothing else: 92 of the 1118 rows of the
/// recounted export, 89 of the 889 of the disassembly dump), not a missing
/// control.
pub const WRITE_NEEDS_PRIVILEGE_REASON: &str = "needs a write, and a private SET is refused below root on this host: the write half sits one category byte over (the read id's 0x80-0xBF legacy-non-privileged byte gains 0x40 and its message id advances by one: 0x2080901B -> 0x2080D01C, the twin the method table itself records), and that 0xC0-0xFF half is what the host forwarder refuses below root with 0x1b, so no SET has been issued (see notes)";

/// The power-limit write: the same privilege gap as every other write, and one
/// *documented* precondition that is specific to it. The SDK's RATED_TDP control
/// is an arbitration request rather than a wattage -- the caller picks a client
/// (RM, WAR_BUG_1785342, GLOBAL, OS, PROFILE) and an action (DEFAULT,
/// FORCE_EXCEED, FORCE_LIMIT, FORCE_LOCK, FORCE_FLOOR) -- and the header states
/// that SET_CONTROL cannot set the RM client while RM arbitrates between the
/// clients and may fall back to FORCE_EXCEED. A write can therefore succeed and
/// still leave the observed limit unchanged, which is exactly why a frequency
/// change must not be taken as proof that a write took effect.
pub const POWER_WRITE_NEEDS_PRIVILEGE_REASON: &str = "needs a write, and a private SET is refused below root on this host (the same privilege gap as every other write), plus a precondition specific to this one: the SDK's RATED_TDP control is an arbitration request rather than a wattage -- the caller picks a client (RM, WAR_BUG_1785342, GLOBAL, OS, PROFILE) and an action (DEFAULT, FORCE_EXCEED, FORCE_LIMIT, FORCE_LOCK, FORCE_FLOOR) -- and SET_CONTROL is documented as unable to set the RM client, while RM arbitrates between clients and may fall back to FORCE_EXCEED, so a write that succeeds can still leave the observed limit unchanged and a frequency change must not be taken as proof that it took effect (see notes)";

/// Fan control, the family whose published surface really is absent: the SDK's
/// own `ctrl2080fan.h` is a FINN-generated file with no definitions, and no
/// `NV2080_CTRL_CMD_*FAN*`/`*COOLER*` macro exists anywhere in the control-header
/// tree. That is a fact about the published headers, not about the driver -- and
/// since this round there is a *named* candidate instead of an unknown one, taken
/// from the in-tree NVAPI copy rather than from a scan. Whether it has been
/// answered yet is decided by artifacts, not by prose: see the reason string and
/// `probe-macos-rm/tools/audit-live-attempts.py`.
pub const FAN_CONTROL_UNREADABLE_REASON: &str = "no fan value is published yet, but this family now has a named candidate rather than an unknown one: the RM SDK declares no fan control at all (its only fan artifact is a 33-line FINN stub with no declarations, and no NV2080_CTRL_CMD_*FAN/COOLER* macro exists in the tree), so the id was taken from the in-tree NVAPI copy, which names the fan-policy command it wraps as RM 0x20800532 reached through escape 0x07000038 and filling a 232-byte internal fanPolicyInfo table (dword 0 = policy mask, byte +4 = active policy index); on this host only the private twin 0x20808532 is in the method table (paramSize 336, read half), and the read that was attempted with it answered 0x62 NV_ERR_RESET_REQUIRED while the public id answered 0x56 NV_ERR_NOT_SUPPORTED -- the same wall every GSP-forwarded control hits while the reset is pending, and the answer this host gives for public ids the chip lacks, so neither verdict says the command is missing; those two answers are prose-only, though, because the census of every .txt and .jsonl capture under outputs/ (probe-macos-rm/tools/audit-live-attempts.py) finds neither id in a returned status, so the candidate is prepared as probe --fan-read (336 B on an 0xA5 background, so an unwritten field keeps 0xA5 and a written zero stays distinguishable) and is part of the read battery, which makes the next healthy run the first artifact-backed measurement of this family; until then nothing about fan rpm, its range or its restore path is claimed (see notes)";

/// The VF family is the opposite case: its reads exist, are non-privileged, and
/// have simply never been issued, because the current priority is the offset
/// channel. Saying it is missing would be wrong in a way a later round would
/// have to retract.
pub const VF_CURVE_UNREADABLE_REASON: &str = "the VF read half exists and is non-privileged (0x2080901F@19416, 0x20809023@66060 and their siblings) but none of it has been read yet: deferred by the current priority, not missing (see notes)";

/// The thermal family *is* read -- the sensor path answered with a temperature
/// before the wedge -- so what is absent for a limit is a different field, and
/// that distinction is exactly what the retired blanket sentence lost.
pub const THERMAL_LIMIT_UNREADABLE_REASON: &str = "the thermal family itself is readable (0x20800513 answered a sensor reading before the wedge), and the *configured* limit is a different field that has not been located yet -- but it is located enough to say where it is not: the SDK's thermal control family is exactly two ids (0x20800513 and its non-privileged twin 0x20808513) and its opcodes cover targets, sensors, readings, reading ranges and a sensor target index, so no limit or policy opcode exists in the family we already call successfully; on the client side the configured limit is a *policy* -- a per-policy id, a Celsius value and a remove-TDP-limit flag, with PFF alongside it -- so the shape of the value is known and only the RM control that carries the policy query is still unlocated, and the in-tree NVAPI copy quotes no RM id for it at all (thermal.rs has zero, the whole gpu.rs has ten) (see notes)";

/// The boost path is a SET, so it sits behind the same gate as the write
/// families; nothing about it has been read, and nothing is claimed.
/// The temperature-*threshold* entry asks for NVML's severity table (slow,
/// warning, shutdown and the hardware pair). This backend answers the thermal
/// family successfully, but the table itself has no source yet, so the entry is
/// refused rather than left to fail with a platform-level message that would
/// read as "no thermal data on this path" -- which the reading disproves.
pub const THERMAL_THRESHOLDS_UNREADABLE_REASON: &str = "the threshold table has no source here, and the family that would carry it has been read: 0x20800513 answered NV_OK with one target and one sensor reading 47 on the driver's own [0,127] range (its non-privileged twin 0x20808513 is the same read), and the SDK's thermal control family is exactly those two ids -- its opcodes cover targets, sensors, readings, reading ranges and a sensor target index, so no severity/threshold opcode exists in the family this host answers; the thresholds are therefore unlocated rather than absent, and the *configured* policy limit is a different field again (see thermal_limit); what does answer on macOS is the live reading of every sensor this backend knows, through get-legacy-temp-sensor (see notes)";

/// NVML's pstate entry emits a table of P-state to clock pairs. This backend
/// publishes the *current* P-state and per-domain offsets, so the table has no
/// source; the reads that could feed one are private, and one of them was
/// measured empty rather than assumed absent.
pub const PSTATE_FREQ_TABLE_UNREADABLE_REASON: &str = "the per-pstate clock table has no source here: the RM snapshot carries the *current* P-state (get-status, get-settings) and the per-domain clock offsets (get-pstate-global-freq-offset), not a table of P-state to frequency pairs; the reads that could feed one are private, and the one that was measured came back empty rather than missing -- 0x2080901d answered NV_OK at its own 32680-byte size with every value dword written as zero on an 0xA5 background (a written zero, not an untouched field), while the public per-level control 0x2080200B still answers 0x1f on this driver; so the entry is not implemented here for want of a read source, and the two entries that do answer on macOS report the current state rather than the table (see notes)";

pub const VOLTAGE_BOOST_UNREADABLE_REASON: &str = "the boost *write* is a SET and sits behind the same privilege gate -- but the family is not read-blind: the client side exposes a read of it as the volt-rails control (a percentage), and the read we actually issued is the internal boost-sync info, which the driver answered 0x56 NOT_SUPPORTED, so this is refused rather than absent; the in-tree copy also carries a controllable window measured live on this very card model at R465 (minimum 300 mV, step 6.25 mV, maximum 1300 mV, default 1000 mV, matching the rail clamp floor), which must be re-measured at R610 rather than reused; the RM endpoint behind the volt-rails escape is still not located (see notes)";

/// The two clock-offset families are the one family where a summary sentence
/// would overstate the gap: the driver answers a per-domain offset
/// *range*, and the current offset is measured **zero** rather than missing --
/// on a background that cannot miss a write, the domain-control GET writes its
/// value dwords and they are all zero, so no offset is applied. The measurement
/// is exactly as wide as the selection mask that produced it: the mask at +4 is
/// an input whose value changes the reply (the seed sweep is the behaviour
/// proof), the recorded run seeded it with `0xFF`, and that reaches the first
/// eight of the ten entries the directory itself declares -- its head carries
/// `+4 = 0x3ff`, ten consecutive row bits, plus the union of those rows' domain
/// bits -- so the zero is measured for those eight and unchecked for the last
/// two, which the rows name as MSD (id 21) and HOST (id 5). The domains this
/// backend is about, core (id 0, row 0) and memory (id 4, row 2), are both
/// inside the measured window: the two unchecked rows are not them, which is
/// what limits this caveat to the peripheral domains. Whether the handler also *echoes* the seed back is not settled by that
/// run: the seed overwrote the probe's `0xA5` background, so "left alone" and
/// "echoed" look identical in it; the runbook's `0x1` and `0x3FF` seeds separate
/// them, and `0x1` additionally distinguishes index-addressing from
/// domain-bit-addressing. What is
/// still open is *which* of those twelve dwords is the offset (the slot identity
/// needed to write one), so the family stays unwritable. The session does carry
/// the block now, so the *window* is reported as measured data in
/// [`DeviceSettings::clock_offset_limits_khz`]; only the offset itself stays out
/// of the numbers, because saying "0" would assert a slot identity this backend
/// does not have.
pub const CLOCK_OFFSET_UNREADABLE_REASON: &str = "current offset measured 0 for the records the measured selection mask reaches (0x2080901B seeded at +4 with 0xFF wrote its value dwords as zero on an 0xA5 background, where a field the handler skipped would have kept 0xA5, so the zero is a write and not a missing reading); the directory itself declares ten entries (its head carries +4 = 0x3ff, ten consecutive row bits, alongside the union of those rows' domain bits 0x38007f) and 0xFF selects the first eight, so the last two are unmeasured until the offset runbook's wider mask answers -- those two are the MSD (id 21) and HOST (id 5) rows, so the core and memory domains this backend reports (rows 0 and 2, ids 0 and 4) are inside the measured window; the window the driver enforces is reported as measured data in clock_offset_limits_khz; which of the twelve value dwords carries the offset is not yet pinned, so no SET is issued -- the offset itself *is* reported, but from a different read than that one (0x20809061, see clock_offsets in this projection), which is why the older 0x2080901B slot identity is what stays out of the numbers and out of the queued write; the documented NVAPI view of this same command calls slot 0 the signed kHz offset with its neighbours range/voltage terms, but that ordering is for the caller's 8-slot record and not for our 12-dword one, and its own author marks the order unconfirmed, so the queued write (bit 0, value at record +8 = slot 0) tests that hypothesis first rather than a random dword; on the same authority the write set is per record type, and the type-03 record took only its stamp here while the V1 set includes 3, so the runbook must not assume a type-03 record accepts a value (see notes)";

/// One domain's offset, as the clock document reports it.
///
/// This is the *current value* the write gate asks for, and the only offset this
/// backend reports with a number attached. Two fields carry the corroboration
/// rather than a guess: `frequency_khz` is what the same record published for the
/// domain (two of them are bit-for-bit the live graphics and memory clocks RUSD
/// reported in the same session), and `domain_bit` is the device's own, not a
/// name this frontend invented.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct ClockOffsetReading {
    /// The device's domain bit, as the document carries it. No name is attached
    /// here: the in-tree NVAPI copy names only `0x10`, which its records agree is
    /// memory because the frequency it published is the live memory clock.
    pub domain_bit: u32,
    /// The frequency the record published for that domain, in kHz.
    pub frequency_khz: u32,
    /// The signed offset the record published, in kHz.
    pub offset_khz: i32,
}

/// A domain this backend can *name*, because the record it read published the
/// same frequency that the live clock of the same snapshot reported.
///
/// No public OGKM header names a clock domain (see [`ClockOffsetDomain`]), so no
/// name here comes from a table: `name` is the frontend's own vocabulary -- the
/// same word `get-status` uses for the live clock the comparison ran against --
/// and `corroboration` carries the measurement, so a reader can reject the name
/// without re-reading the device. The comparison is also why only some domains
/// can ever appear: a domain the status reports no live clock for cannot be
/// named, and it is left out rather than guessed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct NamedClockOffset {
    /// `graphics`, `memory`, `video` or `processor`: the public domain whose
    /// live clock the record's own published frequency matched.
    pub name: String,
    /// The device's domain bit for that record, as the document carries it.
    pub domain_bit: u32,
    /// The frequency the record published, in kHz (equal to that live clock).
    pub frequency_khz: u32,
    /// The signed offset the record published, in kHz.
    pub offset_khz: i32,
    /// The comparison behind the name, spelled out so it can be falsified.
    pub corroboration: String,
}

/// The offset window the driver reports for a clock domain, in kHz.
///
/// Only present while the range block was read *and* every readable record of
/// it agreed. The records are not yet attributed to named domains (their tags
/// count `0x01030101/0x01040101/0x01050101` while the directory enumerates
/// domain *bits*), so a disagreement would force a claim this backend cannot
/// make -- and the window is reported as absent rather than picked.
///
/// This is a limit, not a setting: what the driver currently *applies* lives in
/// the control block, whose value dwords read zero at stock.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct ClockOffsetLimits {
    /// Offset floor, as the device's range record carries it. In kHz, and an
    /// *offset* span rather than an absolute limit: the in-tree NVAPI copy's IDA
    /// note names this same `-18750` "freq offset floor" and reads the pair as
    /// the +-18.75 MHz control span on RM 0x20809025.
    pub min_khz: i32,
    /// Offset ceiling, same record and same note.
    pub max_khz: i32,
    /// How many range records the window was read from, so an aggregate cannot
    /// be mistaken for a single domain's.
    pub records: usize,
}

/// One clock domain the driver exposes to the offset channel.
///
/// `index` is the entry's position in the device's own directory, which is also
/// the bit the `0x2080901B` selection mask uses for it -- the mask sweep showed
/// a mask of `0xFF` selecting exactly the entries at indices 0..7, seven of
/// which write twelve value dwords and one of which (the `0x0303` entry, at
/// index 6) writes its stamp only.
///
/// `domain_bit` is what the directory stamps on the entry, and it is a *bit
/// position*, not an index: the ten this device uses are 0..6 and 19..21. There
/// is no clock-domain enum anywhere in the public OGKM headers, so no name is
/// claimed here. The one cross-reference that exists (`NV_GPU_CLOCK_DOMAIN_ID`,
/// 32 wide, in the NVAPI/Windows namespace) names 0..6
/// GPC/XBAR/SYS/HUB/M/HOST/DISP and 19..21 LEG/PWR/MSD -- the same two clusters
/// -- which is why bit 0 is *probably* the core engine and bit 4 the memory
/// domain. That is a hypothesis with a falsification path (read a single-domain
/// offset once a write is permitted), not a measurement, so the numbers here
/// stay as the device returned them.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct ClockOffsetDomain {
    /// Directory entry index, also the offset-control mask bit.
    pub index: usize,
    /// Bit position the directory stamps on this entry (see the type docs).
    pub domain_bit: u32,
    /// Entry type stamp at `rec+0`: `0x0808`/`0x0909` entries carry twelve
    /// value dwords in the control block, `0x0303` carries only its stamp.
    pub type_stamp: u16,
}

/// Read-only projection of the driver-visible configuration.
///
/// This deliberately contains only what the RM session actually reports, and
/// names every configuration family it does *not* report with a reason. A
/// missing family is never represented by a default-looking zero.
///
/// `Default` is therefore the "nothing has been read yet" state, not a reading:
/// every field is absent except the capability flags, which are false until a
/// session fills them in. It is what lets a caller outside this crate build the
/// projection by hand instead of having to spell out every field.
#[derive(Debug, Clone, Default, Serialize)]
pub struct DeviceSettings {
    pub pstate: Option<u32>,
    /// Current enforced power limit (the live TGP wall) in mW.
    pub power_limit_mw: Option<u32>,
    /// Power limit the driver was asked for, which clamping may exceed.
    pub power_limit_requested_mw: Option<u32>,
    pub pcie_link_gen: Option<u32>,
    pub memory_total_bytes: Option<u64>,
    pub memory_used_bytes: Option<u64>,
    pub board_id: Option<u32>,
    pub chip_sku: Option<String>,
    pub project: Option<String>,
    /// The offset span the driver reports, in kHz, read from the device's range
    /// block (RM 0x20809025) and named after that block's own fields: the
    /// in-tree NVAPI copy's IDA note calls the pair the offset floor and ceiling,
    /// the +-18.75 MHz control span. `None` when that block was refused,
    /// malformed, empty or its records disagreed; the offset channel note says
    /// which.
    pub clock_offset_limits_khz: Option<ClockOffsetLimits>,
    /// The domains the driver says it can offset: the device's own directory,
    /// as returned. `None` when that block was refused or malformed, so an
    /// unread directory is never an empty capability list.
    pub clock_offset_domains: Option<Vec<ClockOffsetDomain>>,
    /// The offsets the driver currently applies, per domain, from the clock
    /// document (`0x20809061`). `None` when that block was refused or malformed,
    /// so an unread offset is never an offset of zero -- which is the whole
    /// reason this is not a plain `Vec`.
    ///
    /// Read-only evidence, not a capability: the write gate is unchanged, and the
    /// queued write is what would prove these numbers move when one is set.
    pub clock_offsets: Option<Vec<ClockOffsetReading>>,
    /// The subset of `clock_offsets` this backend can put a *name* on, because
    /// the record's published frequency equalled the live clock of the same
    /// snapshot and no other record did. `None` means the offset channel was not
    /// read at all; an empty list means it was read and named nothing, which is
    /// a measurement and not a missing value.
    pub clock_offsets_named: Option<Vec<NamedClockOffset>>,
    /// Configuration families this backend cannot read (or write) yet, keyed by
    /// the family name, with the cause.
    pub unavailable: BTreeMap<String, String>,
    pub capabilities: SettingsCapabilities,
    /// Evidence behind the capability claims: what the backend does read, and
    /// which controls were probed with which RM status.
    pub notes: Vec<String>,
}

/// What an entry can publish about one domain's offset: the value now, the span
/// the driver declares, and the value a restore would have to write back.
///
/// All three are *reads*; nothing here writes. The span is `None` when the range
/// block was not read, which is not a span of zero -- the same rule the settings
/// field follows -- and `window_reason` then quotes the read's own outcome.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ClockOffsetView {
    /// The public name the same-snapshot comparison corroborated.
    pub domain: String,
    /// The device's bit for that record, as the document carries it.
    pub domain_bit: u32,
    /// The offset the clock document publishes for this domain, in kHz.
    pub offset_khz: i32,
    /// The frequency that record published, in kHz: the value that named the
    /// domain, kept so a reader can redo the comparison that did it.
    pub frequency_khz: u32,
    pub corroboration: String,
    /// The span the range block reported, in kHz. A *declared* span, not a
    /// per-domain bound: `window_records` says how many records agreed on it.
    pub window_min_khz: Option<i32>,
    pub window_max_khz: Option<i32>,
    pub window_records: Option<usize>,
    /// Why there is no span, when there is none: the offset channel's own
    /// outcome, not a summary of it.
    pub window_reason: Option<String>,
    /// The value a restore must write back after a write, which is the offset
    /// read here. Reported now so the write gate has its target before it opens.
    pub restore_khz: i32,
}

fn settings_from(info: &DeviceInfo, status: &DeviceStatus) -> DeviceSettings {
    // Every family that would need a read we do not have. The keys are the
    // public vocabulary (`get-settings` / pynvoc), not RM command names.
    let unavailable: BTreeMap<String, String> = [
        "core_clock_offset",
        "memory_clock_offset",
        "voltage_boost",
        "thermal_limit",
        "fan_control",
        "pstate_lock",
        "vf_curve",
        "power_limit_write",
        "settings_restore",
    ]
    .into_iter()
    .map(|family| {
        // One sentence per family, because the first real `get-settings
        // --macos-rm` run showed what a single shared sentence costs: it printed
        // "no readable RM control for this family on this driver" for the
        // families whose control demonstrably *is* dispatched. An unread or
        // unauthorised thing must not be reported as a missing one.
        let reason = match family {
            "core_clock_offset" | "memory_clock_offset" => CLOCK_OFFSET_UNREADABLE_REASON,
            "fan_control" => FAN_CONTROL_UNREADABLE_REASON,
            "vf_curve" => VF_CURVE_UNREADABLE_REASON,
            "thermal_limit" => THERMAL_LIMIT_UNREADABLE_REASON,
            "voltage_boost" => VOLTAGE_BOOST_UNREADABLE_REASON,
            "power_limit_write" => POWER_WRITE_NEEDS_PRIVILEGE_REASON,
            "pstate_lock" | "settings_restore" => WRITE_NEEDS_PRIVILEGE_REASON,
            _ => WRITE_NEEDS_PRIVILEGE_REASON,
        };
        (family.to_string(), reason.to_string())
    })
    .collect();

    DeviceSettings {
        pstate: status.pstate,
        power_limit_mw: status.power_limit_mw,
        power_limit_requested_mw: status.power_limit_requested_mw,
        pcie_link_gen: status.pcie_link_gen,
        memory_total_bytes: info.memory_total_bytes,
        memory_used_bytes: status.memory_used_bytes,
        board_id: info.board_id,
        chip_sku: info.chip_sku.clone(),
        project: info.project.clone(),
        // Filled in by `apply_offset_channel` from a live read; the pure
        // projection leaves it absent rather than guessing a window.
        clock_offset_limits_khz: None,
        clock_offset_domains: None,
        clock_offsets: None,
        clock_offsets_named: None,
        unavailable,
        capabilities: SettingsCapabilities {
            settings_read: true,
            settings_write: SETTINGS_WRITE_SUPPORTED,
            settings_restore: SETTINGS_RESTORE_SUPPORTED,
        },
        notes: vec![
            "responder: these ids are answered by the GSP firmware, not by a host RM table row (NVRM forwards legacy ids with bit 0x8000 set), so every parameter block is sized against the GSP's own export table rather than the NVAPI caller struct.".into(),
            "read: P-state, enforced/requested power limit, PCIe generation, board/SKU and memory sizes come from the same RM snapshot as get-status; sensor readings are separate (see get-status).".into(),
            "write: refused before the driver is opened, and on this host a private clock/voltage SET is not even reachable from an unprivileged client: the write half of these ids is one category byte over (0x2080901B -> 0x2080D01C: the 0x80-0xBF legacy-non-privileged byte gains 0x40 and the message id advances by one), and NVRM's legacy forwarder refuses 0xC000 commands below root with 0x1b before the GSP sees them. Behind that gate the GSP handler exists (its own preconditions are enumerated in docs/reverse-engineering/nvapi/gsp-method-table-and-legacy-forwarding.md section 4.4). A write would additionally need a readable current value, a bounded step and a verified restore, and the GSP must not be in the RESET_REQUIRED state.".into(),
            "clock domains/offsets: 0x20809019 answers NV_OK with a directory whose 32 record slots carry 10 populated mask bits, and 0x20809025 answers NV_OK with a per-domain offset range (three entries, +-18750 kHz). The NVAPI caller struct is a shape hint, not a verified mapping: this driver rejects the caller-side sizes (2520 and 2404) with 0x1f, the record stride it accepts is 384 bytes against the caller struct's 72, and the field at +4 is the record's mask bit where the caller struct has a clock type. The caller struct is also marked Pascal only. The +-int16 pair of the Gpc/M directory rows (+-1000 and -1000/+3000) is *not* the caller-visible window: the in-tree NVAPI copy's IDA note puts rangeMax/Min at RM+80/+84 of the caller's own record layout, and every populated row of the directory carries type 0 there, which means no range information. The window this backend reports instead comes from the range block, and the copy locates its meaning: 0x20809025 is its RM id, its ClkFreqController record carries 1638400 max VF kHz and a -18750/+18750 pair that the note calls the frequency-offset floor and ceiling (the +-18.75 MHz control span), so the pair is in kHz and reads as an offset span rather than an absolute limit. What is not reconciled is the record base: the copy's caller geometry is 0x4C + 32x0x60 where this driver's RM block is 20 + 32x224, and the two records are matched by content, not by a proven field map. 0x2080901B also answers NV_OK, but every value slot of its 64-byte records is written as zero at stock (an 0xA5 pre-seed shows the writes as 0xA5A5A5A5 -> 0), so which slot holds the current offset cannot be fixed from a stock read alone -- and the type stamp is the only non-zero field it returns. The write set is type-conditional and all-or-nothing (the seven type-08/09 entries of a full selection write twelve value dwords each, the single type-03 entry writes its stamp only), so the type split narrows the block but still cannot name the offset slot.".into(),
            "voltage/rails: rails-get-info 0x2080B201 answers NV_OK at its own 2444-byte size (the earlier 0x1f came from the NVAPI-era size, not from a missing command), while perf-get-level-info-v2 0x2080200B still returns 0x1f. EDPp limit info and pmumon clock samples return NV_ERR_NOT_SUPPORTED (0x56).".into(),
        ],
    }
}

/// How one offset block ended, in the vocabulary the evidence note uses. Kept
/// as a function rather than a closure so it can stay generic over the block.
fn block_outcome<T>(block: &str, read: &macos_offset::OffsetBlockRead<T>) -> String {
    use macos_offset::OffsetBlockRead::{Malformed, Parsed, Refused};
    match read {
        Parsed(_) => format!("{block} NV_OK"),
        Refused(status) => format!("{block} refused {status:#010x}"),
        Malformed(err) => format!("{block} malformed ({err})"),
    }
}

/// Folds one reading of the private clock-offset channel into the settings
/// projection.
///
/// Two rules drive this, both from the integration plan:
///
/// * **A refusal is not a value.** The window is set only from a parsed range
///   block; a refused or malformed block leaves the field `None` and states the
///   RM status, so a wedge cannot appear as "the limit is 0 kHz".
/// * **No claim about a domain the records have not earned.** The range records
///   are not attributed to named domains, so the window is reported only while
///   every readable record agrees, and it carries how many records it came from.
///
/// The outcome of all four blocks is recorded in one note, the way the rest of
/// this projection reports evidence, so a caller reading `None` can see whether
/// the block was never requested, refused, or did not look like a range block.
fn apply_offset_channel(
    settings: &mut DeviceSettings,
    channel: &macos_offset::OffsetChannelSnapshot,
) {
    use macos_offset::OffsetBlockRead::{Malformed, Parsed, Refused};

    let mut outcomes = vec![
        block_outcome("directory 0x20809019:", &channel.directory),
        block_outcome("range 0x20809025:", &channel.range),
        block_outcome("control 0x2080901B:", &channel.control),
        block_outcome("clock document 0x20809061:", &channel.clock_document),
    ];

    // Which domains the driver is willing to offset at all. Reported as the
    // device returned them: an index, a bit and a type stamp, no names.
    match &channel.directory {
        Parsed(directory) if directory.entries.is_empty() => {
            outcomes.push("the directory carried no entry, so no domain is listed".into());
        }
        Parsed(directory) => {
            settings.clock_offset_domains = Some(
                directory
                    .entries
                    .iter()
                    .map(|entry| ClockOffsetDomain {
                        index: entry.index,
                        domain_bit: entry.domain_bit,
                        type_stamp: entry.type_stamp,
                    })
                    .collect(),
            );
        }
        Refused(_) | Malformed(_) => {
            outcomes.push("no domain is listed because the directory was not read".into());
        }
    }

    match &channel.range {
        Parsed(range) if range.records.is_empty() => {
            outcomes.push("no range record was selected, so no window is reported".into());
        }
        Parsed(range) => {
            let first = &range.records[0];
            let all_agree = range
                .records
                .iter()
                .all(|r| r.min_khz == first.min_khz && r.max_khz == first.max_khz);
            if all_agree {
                settings.clock_offset_limits_khz = Some(ClockOffsetLimits {
                    min_khz: first.min_khz,
                    max_khz: first.max_khz,
                    records: range.records.len(),
                });
            } else {
                // Named-domain attribution is what would let these be reported
                // separately; until it exists, picking one would be a claim.
                outcomes.push(format!(
                    "the {} range records disagree (first {}..{} kHz), so no single window is reported",
                    range.records.len(),
                    first.min_khz,
                    first.max_khz
                ));
            }
        }
        Refused(_) | Malformed(_) => {
            outcomes.push("no window is reported for this reason".into());
        }
    }

    // The one block that publishes an offset rather than a window or a set of
    // unlabelled dwords, so it is the one that fills a structured field. It is
    // also listed in the note below, the same way the other blocks report their
    // outcome, so a caller reading `None` can see whether the block was refused,
    // malformed or simply not requested.
    match &channel.clock_document {
        Parsed(document) if document.entries.is_empty() => {
            outcomes.push("the clock document carried no entry, so no offset is listed".into());
        }
        Parsed(document) => {
            // Reported bit by bit: only `0x10` can be *named* from the in-tree
            // NVAPI copy (as MCLK, which the frequency it published agrees with),
            // so promoting the rest to named domains would guess at names rather
            // than report measured values.
            settings.clock_offsets = Some(
                document
                    .entries
                    .iter()
                    .map(|entry| ClockOffsetReading {
                        domain_bit: entry.domain_bit,
                        frequency_khz: entry.frequency_khz,
                        offset_khz: entry.offset_khz,
                    })
                    .collect(),
            );
            let listed = document
                .entries
                .iter()
                .map(|entry| {
                    format!(
                        "bit {:#x} at {} kHz with offset {} kHz",
                        entry.domain_bit, entry.frequency_khz, entry.offset_khz
                    )
                })
                .collect::<Vec<_>>()
                .join("; ");
            outcomes.push(format!("current offsets: {listed}"));
        }
        Refused(_) | Malformed(_) => {
            outcomes.push("no offset is listed for this reason".into());
        }
    }

    settings
        .notes
        .push(format!("offset channel: {}", outcomes.join(", ")));
}

/// The public domains this backend can try to name, each with the live clock of
/// the same snapshot that the comparison runs against.
///
/// These are the frontend's words (the ones `get-status` uses), not RM domain
/// names: which record they end up pointing at is decided by the measurement
/// below, not by this table.
fn namable_domains(status: &DeviceStatus) -> [(&'static str, Option<u32>); 4] {
    [
        ("graphics", status.graphics_clock_khz),
        ("memory", status.memory_clock_khz),
        ("video", status.video_clock_khz),
        ("processor", status.sm_clock_khz),
    ]
}

/// Attaches a name to the offset records whose own published frequency equals
/// the live clock of the same snapshot, and to no others.
///
/// This is the one comparison that can name a domain without a header that names
/// clock domains: the document publishes, per domain bit, the frequency it holds
/// *and* the offset it applies, and `get-status` reports the live clock for the
/// public domains. Equality is required to be exact and unique -- a tie names
/// neither record rather than picking one -- so the failure direction is "left
/// unnamed", never "misnamed". Every outcome is written into `notes`, so a reader
/// of a `None`/empty list can see which comparison was attempted.
///
/// This does *not* read the offset off a frequency move (the rule that a realtime
/// frequency change must never stand in for a successful offset write): both
/// numbers come from the same snapshot, and the offset reported is the document's
/// own field; the frequency only decides *which* record the name refers to.
fn corroborate_named_offsets(settings: &mut DeviceSettings, status: &DeviceStatus) {
    let Some(offsets) = settings.clock_offsets.clone() else {
        // The channel was not read this time; `apply_offset_channel` already put
        // the per-block outcome in the notes.
        return;
    };
    let mut named: Vec<NamedClockOffset> = Vec::new();
    for (name, live) in namable_domains(status) {
        let Some(live) = live.filter(|khz| *khz > 0) else {
            settings.notes.push(format!(
                "offset naming: {name} is not named -- this snapshot reports no live {name} clock to compare a record against"
            ));
            continue;
        };
        let matching: Vec<&ClockOffsetReading> = offsets
            .iter()
            .filter(|entry| entry.frequency_khz == live)
            .collect();
        match matching.as_slice() {
            [entry] => {
                named.push(NamedClockOffset {
                    name: name.to_string(),
                    domain_bit: entry.domain_bit,
                    frequency_khz: entry.frequency_khz,
                    offset_khz: entry.offset_khz,
                    corroboration: format!(
                        "the record published {live} kHz, which is this snapshot's live {name} clock, and it is the only one of the {} records that did",
                        offsets.len()
                    ),
                });
                settings.notes.push(format!(
                    "offset naming: bit {:#x} carries the {name} domain -- its published frequency ({live} kHz) is this snapshot's live {name} clock and no other record matched",
                    entry.domain_bit
                ));
            }
            [] => settings.notes.push(format!(
                "offset naming: {name} stays unnamed -- no record published this snapshot's live {name} clock ({live} kHz)"
            )),
            ties => settings.notes.push(format!(
                "offset naming: {name} stays unnamed -- {} records published this snapshot's live {name} clock ({live} kHz), so naming one would be a choice rather than a measurement",
                ties.len()
            )),
        }
    }
    // Two public names can legitimately match the *same* record: at idle the
    // graphics and SM clocks are often the same number, and then the evidence
    // for each name is exactly "that record published my live clock". Report
    // both and say why, rather than letting one name silently win.
    let mut claimed: BTreeMap<u32, &str> = BTreeMap::new();
    for entry in &named {
        if let Some(first) = claimed.insert(entry.domain_bit, entry.name.as_str()) {
            settings.notes.push(format!(
                "offset naming: bit {:#x} answers to both {first} and {} -- this snapshot reported {first} and {} clocks of the same {:#x} kHz, so neither name is more specific than the other",
                entry.domain_bit, entry.name, entry.name, entry.frequency_khz,
            ));
        }
    }
    settings.clock_offsets_named = Some(named);
}

/// Why one public domain has no name attached, in the layer terms the entries
/// report: the driver answers, the naming is a comparison this backend can only
/// run where its own status reports a live clock for that domain.
pub fn unnamed_offset_reason(name: &str) -> String {
    format!(
        "{name} is not named in this reading. The driver does answer -- the per-domain \
         records are in clock_offsets and the window in clock_offset_limits_khz -- but no \
         public OGKM header names a clock domain, so this backend attaches a name only \
         where it measured one: a record whose own published frequency is the live {name} \
         clock of the same snapshot, and the only such record. Either none matched, or \
         several did (then neither is named), or the device reports no live {name} clock. \
         A single bounded domain write is still the falsification path that settles a \
         domain no comparison can."
    )
}

/// The public domain names the entries accept, in the order the naming layer
/// tries them. A name outside this list is rejected at parse time rather than
/// silently becoming "unnamed": a typo must not read like a device finding.
pub const PUBLIC_OFFSET_DOMAINS: [&str; 4] = ["graphics", "memory", "video", "processor"];

/// The public name of a clock domain: the spelling [`PUBLIC_OFFSET_DOMAINS`]
/// uses, which is also the key [`DeviceSettings::clock_offset_view`] resolves.
///
/// `None` for a domain no offset entry has a name for -- the caller then has a
/// device bit, not a name, and should say so rather than look up a made-up one.
/// This is the one mapping between the NVAPI/NVML clock-domain enum an entry
/// parses and the name a macOS snapshot is looked up by, kept here so an entry
/// outside this crate (the Python bindings) cannot grow a second copy of it; the
/// CLI's own `domain_label` delegates to it for the same reason.
pub fn public_offset_domain(domain: ClockDomain) -> Option<&'static str> {
    match domain {
        ClockDomain::Graphics => Some("graphics"),
        ClockDomain::Memory => Some("memory"),
        ClockDomain::Processor => Some("processor"),
        ClockDomain::Video => Some("video"),
        _ => None,
    }
}

/// Which record a dry-run step refers to.
///
/// Both forms exist because they prove different things. A public name is only
/// usable where the naming layer corroborated it against the live clock (see
/// [`NamedClockOffset`]). A device bit needs no corroboration: the document
/// publishes a record for it and the caller names the device's own coordinate --
/// which is exactly what the write's selection mask carries, so the bit form is
/// the one that works before any naming.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OffsetStepTarget {
    Domain(String),
    DomainBit(u32),
}

impl OffsetStepTarget {
    /// The request as the caller spelled it, for the report.
    pub fn token(&self) -> String {
        match self {
            Self::Domain(name) => name.clone(),
            Self::DomainBit(bit) => format!("bit:{bit:#x}"),
        }
    }
}

/// Why a dry-run step can or cannot be issued.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum OffsetStepVerdict {
    /// The record is known and the step lands inside the window the driver
    /// reported. It does *not* mean the step was applied.
    Ready,
    /// The offset channel was not read, so nothing is known about any record.
    ChannelUnread,
    /// A public name no record corroborated in this snapshot.
    DomainUnnamed,
    /// A device bit the document published no record for.
    RecordAbsent,
    /// The step is computable but the range block was not read, so the driver's
    /// bounds are unknown and a *bounded* step cannot be claimed.
    WindowUnknown,
    /// The resulting offset would fall outside the reported window.
    OutsideWindow,
}

/// What one requested offset change would do, decided from a read-only snapshot.
///
/// Nothing here writes. The point is to answer, before a write is worth issuing,
/// the questions the write gate asks: which record the caller means, what it
/// holds now, whether the requested step stays inside the window the driver
/// reported, and -- when it cannot answer -- which of those facts is missing.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct OffsetStep {
    /// The request as the caller spelled it (`graphics`, `bit:0x1`).
    pub requested: String,
    /// The device bit, once the target resolved to one.
    pub domain_bit: Option<u32>,
    /// The public name known for that bit in this snapshot, if any. Never
    /// invented: it comes from [`NamedClockOffset`] only.
    pub domain: Option<String>,
    /// How the target was resolved, so a reader can judge the identification.
    pub identity: String,
    pub current_offset_khz: Option<i32>,
    pub delta_khz: i32,
    pub target_offset_khz: Option<i32>,
    /// The window the range block reported, in kHz (absent when not read).
    pub window_min_khz: Option<i32>,
    pub window_max_khz: Option<i32>,
    pub verdict: OffsetStepVerdict,
    /// Why the verdict is what it is, in the terms of the deciding layer.
    pub detail: String,
}

/// Plan one bounded offset step from a read-only snapshot.
///
/// This is the dry-run of the write path, and it is deliberately not a policy:
/// the caller supplies the step (`delta_khz`), the device supplies the current
/// value and the window, and the only judgements made here are the two the
/// evidence supports -- is the record identified, and does the sum land inside
/// the window the driver itself reported. `applied` is a property of the plan
/// that carries these steps, never of a step.
pub fn plan_offset_step(
    settings: &DeviceSettings,
    target: &OffsetStepTarget,
    delta_khz: i32,
) -> OffsetStep {
    let mut step = OffsetStep {
        requested: target.token(),
        domain_bit: None,
        domain: None,
        identity: String::new(),
        current_offset_khz: None,
        delta_khz,
        target_offset_khz: None,
        window_min_khz: settings.clock_offset_limits_khz.as_ref().map(|w| w.min_khz),
        window_max_khz: settings.clock_offset_limits_khz.as_ref().map(|w| w.max_khz),
        verdict: OffsetStepVerdict::ChannelUnread,
        detail: String::new(),
    };

    let Some(records) = settings.clock_offsets.as_ref() else {
        step.identity = "nothing: the offset channel was not read in this snapshot".into();
        step.detail = format!(
            "the offset channel was not read, so the current value of {} is unknown; see the \
             settings notes for the per-block outcome",
            target.token()
        );
        return step;
    };

    // Resolve the request to a record. A name has to be corroborated; a bit is
    // the device's own coordinate and only has to exist.
    let record = match target {
        OffsetStepTarget::Domain(name) => match settings.named_clock_offset(name) {
            Ok(named) => {
                step.domain = Some(named.name.clone());
                step.identity = format!(
                    "named by the same-snapshot comparison: {}",
                    named.corroboration
                );
                records
                    .iter()
                    .find(|record| record.domain_bit == named.domain_bit)
            }
            Err(reason) => {
                step.identity = format!("asked by name; no record matched ({})", reason);
                step.verdict = OffsetStepVerdict::DomainUnnamed;
                step.detail = reason;
                return step;
            }
        },
        OffsetStepTarget::DomainBit(bit) => {
            let found = records.iter().find(|record| record.domain_bit == *bit);
            let name = settings
                .clock_offsets_named
                .as_ref()
                .and_then(|named| named.iter().find(|entry| entry.domain_bit == *bit))
                .map(|entry| entry.name.clone());
            step.domain = name.clone();
            step.identity = match &name {
                Some(name) => format!(
                    "asked by device bit, which the same-snapshot comparison also named {name}"
                ),
                None => {
                    "asked by device bit; no public name was corroborated for it in this snapshot"
                        .to_string()
                }
            };
            match found {
                Some(_) => found,
                None => {
                    step.verdict = OffsetStepVerdict::RecordAbsent;
                    step.detail = format!(
                        "the clock document published {} records and none carries bit {bit:#x}",
                        records.len()
                    );
                    return step;
                }
            }
        }
    };

    let Some(record) = record else {
        step.verdict = OffsetStepVerdict::RecordAbsent;
        step.detail = format!(
            "the clock document published no record for {}",
            target.token()
        );
        return step;
    };

    step.domain_bit = Some(record.domain_bit);
    step.current_offset_khz = Some(record.offset_khz);
    let wanted = i64::from(record.offset_khz) + i64::from(delta_khz);
    step.target_offset_khz = i32::try_from(wanted).ok();

    let (Some(min), Some(max)) = (step.window_min_khz, step.window_max_khz) else {
        step.verdict = OffsetStepVerdict::WindowUnknown;
        step.detail = "the range block (0x20809025) was not read, so the driver's bounds are \
                       unknown and a bounded step cannot be claimed"
            .into();
        return step;
    };

    if wanted < i64::from(min) || wanted > i64::from(max) {
        step.verdict = OffsetStepVerdict::OutsideWindow;
        step.detail = format!(
            "{} {} kHz + {:+} kHz = {} kHz falls outside the reported window {}..{} kHz",
            target.token(),
            record.offset_khz,
            delta_khz,
            wanted,
            min,
            max
        );
        return step;
    }

    step.verdict = OffsetStepVerdict::Ready;
    step.detail = format!(
        "bit {:#x} holds {} kHz; {:+} kHz gives {} kHz, inside the reported window {}..{} kHz. \
         Nothing has been written: the step is bounded and identified, which is what the write \
         gate asks before a write is worth issuing",
        record.domain_bit, record.offset_khz, delta_khz, wanted, min, max
    );
    step
}

/// Parse the dry-run target grammar every entry shares: one of
/// [`PUBLIC_OFFSET_DOMAINS`], or `bit:<n>` for one of the device's own domain
/// bits (decimal or `0x`-prefixed hexadecimal).
pub fn parse_offset_target(token: &str) -> Result<OffsetStepTarget, String> {
    let trimmed = token.trim();
    let lower = trimmed.to_ascii_lowercase();
    if let Some(rest) = lower.strip_prefix("bit:") {
        let rest = rest.trim();
        let (digits, radix) = match rest.strip_prefix("0x") {
            Some(hex) => (hex, 16),
            None => (rest, 10),
        };
        return u32::from_str_radix(digits, radix)
            .map(OffsetStepTarget::DomainBit)
            .map_err(|_| {
                format!("{trimmed:?} is not a device bit: expected decimal or 0x-prefixed hex after \"bit:\"")
            });
    }
    if PUBLIC_OFFSET_DOMAINS.contains(&lower.as_str()) {
        return Ok(OffsetStepTarget::Domain(lower));
    }
    Err(format!(
        "{trimmed:?} is neither a public domain ({}) nor a device bit (bit:<n>); the public \
         names are the ones the naming layer can corroborate against a live clock, and a device \
         bit is the coordinate the write's selection mask uses",
        PUBLIC_OFFSET_DOMAINS.join(", ")
    ))
}

/// Parse a step size: a signed integer in kHz, optionally suffixed `khz` or
/// `mhz` (case-insensitive). The unit may not be omitted silently -- `khz` is
/// the default, and the suffix exists so a caller can say `+8mhz` and mean it.
pub fn parse_offset_delta(token: &str) -> Result<i32, String> {
    let trimmed = token.trim();
    let lower = trimmed.to_ascii_lowercase();
    let (digits, scale) = if let Some(head) = lower.strip_suffix("mhz") {
        (head, 1000i64)
    } else if let Some(head) = lower.strip_suffix("khz") {
        (head, 1i64)
    } else {
        (lower.as_str(), 1i64)
    };
    let value: i64 = digits.trim().parse().map_err(|_| {
        format!(
            "{trimmed:?} is not a step size: expected a signed integer in kHz, optionally \
             suffixed \"khz\" or \"mhz\" (for example +8mhz or -2000)"
        )
    })?;
    let khz = value.checked_mul(scale).ok_or_else(|| {
        format!("{trimmed:?} overflows kHz: {value} x {scale} does not fit a 32-bit kHz value")
    })?;
    i32::try_from(khz).map_err(|_| format!("{trimmed:?} is outside the 32-bit kHz range"))
}

impl DeviceSettings {
    /// The entry's view of one domain: current offset, declared span, restore
    /// target, each either measured or absent-with-a-reason.
    ///
    /// The name is resolved by [`Self::named_clock_offset`], so an uncorroborated
    /// domain is refused here for the same reason it is refused there, and no
    /// fallback number is invented on the way out.
    pub fn clock_offset_view(&self, name: &str) -> Result<ClockOffsetView, String> {
        let named = self.named_clock_offset(name)?;
        Ok(ClockOffsetView {
            domain: named.name.clone(),
            domain_bit: named.domain_bit,
            offset_khz: named.offset_khz,
            frequency_khz: named.frequency_khz,
            corroboration: named.corroboration.clone(),
            window_min_khz: self.clock_offset_limits_khz.as_ref().map(|w| w.min_khz),
            window_max_khz: self.clock_offset_limits_khz.as_ref().map(|w| w.max_khz),
            window_records: self.clock_offset_limits_khz.as_ref().map(|w| w.records),
            window_reason: self.window_absence_reason(),
            restore_khz: named.offset_khz,
        })
    }

    /// Why no offset span was read, or `None` when one was.
    ///
    /// Domain-independent on purpose: a caller reporting the whole projection
    /// (the Python settings object, the optimizer) has no domain name to ask
    /// with, and it must still be able to say *why* the window is missing.
    pub fn window_absence_reason(&self) -> Option<String> {
        match self.clock_offset_limits_khz {
            Some(_) => None,
            None => Some(self.range_outcome()),
        }
    }

    /// The range block's own outcome, taken from the offset channel note.
    ///
    /// The note joins one outcome per block with `", "`, and an outcome can hold
    /// its own comma, so the split re-joins a fragment whose parentheses are
    /// still open. Every fragment that names the range or the window is kept, in
    /// the order the note has them, and nothing is paraphrased: the caller gets
    /// the layer's own words.
    fn range_outcome(&self) -> String {
        const POINTER: &str =
            "no span was read: the range block's outcome is in the offset channel note";
        let Some(body) = self
            .notes
            .iter()
            .find_map(|note| note.strip_prefix("offset channel: "))
        else {
            return POINTER.to_string();
        };
        let mut depth = 0i32;
        let mut fragment = String::new();
        let mut kept: Vec<String> = Vec::new();
        for part in body.split(", ") {
            if !fragment.is_empty() {
                fragment.push_str(", ");
            }
            fragment.push_str(part);
            depth += part.matches('(').count() as i32 - part.matches(')').count() as i32;
            if depth <= 0 {
                if fragment.contains("range") || fragment.contains("window") {
                    kept.push(fragment.clone());
                }
                fragment.clear();
            }
        }
        if !fragment.is_empty() && (fragment.contains("range") || fragment.contains("window")) {
            kept.push(fragment);
        }
        if kept.is_empty() {
            POINTER.to_string()
        } else {
            kept.join(", ")
        }
    }

    /// The corroborated offset for one public domain name, or the reason this
    /// backend has none to report for it.
    pub fn named_clock_offset(&self, name: &str) -> Result<&NamedClockOffset, String> {
        match &self.clock_offsets_named {
            None => Err(format!(
                "the offset channel was not read in this snapshot, so {name} is not named; see notes"
            )),
            Some(named) => named
                .iter()
                .find(|entry| entry.name == name)
                .ok_or_else(|| unnamed_offset_reason(name)),
        }
    }
}

/// One live fan reading: the driver's own cooler slot plus the rpm it reports.
///
/// The slot is kept because it is the only stable identity this layer offers.
/// The order of the published readings is a position after the plausibility
/// filter, so it is not the cooler index; a consumer that has to name a cooler
/// needs the driver's coordinate, the same way the offset records need their
/// device domain bit rather than a position in a list.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct FanCoolerReading {
    pub slot: u32,
    pub rpm: u32,
}

/// The fan face of this backend: the readings it publishes, and the fields of
/// the common fan-info contract it has no source for.
///
/// Kept separate from [`DeviceStatus`] because it answers a question about the
/// entry contract ("can this backend fill fan-info, and with what?") rather
/// than about the device. `unavailable` names the fields this backend cannot
/// fill and why, in the same shape the rest of this backend uses for fields it
/// cannot report -- never by leaving a consumer to guess from a missing key or,
/// worse, to read a fabricated zero.
#[derive(Debug, Clone, Serialize)]
pub struct FanReport {
    /// Coolers that reported a live reading in this snapshot. Not "coolers
    /// fitted": see `notes`.
    pub count: usize,
    pub coolers: Vec<FanCoolerReading>,
    /// The three percentage fields of the common fan-info contract. `None`
    /// because this backend has no source for them, with the reason in
    /// `unavailable`: a null that a consumer can look up, never a zero standing
    /// in for a value that was not read.
    pub min_percent: Option<u32>,
    pub max_percent: Option<u32>,
    pub current_percent: Option<u32>,
    pub unavailable: BTreeMap<String, String>,
    pub notes: Vec<String>,
}

/// The RUSD fan section publishes a tachometer reading per cooler slot and
/// nothing else: no duty percent, and no rpm range to turn one into a percent
/// without inventing a scale. So the three fields the NVML fan-info entry
/// reports as percentages have no source here, while rpm does.
pub const FAN_PERCENT_UNREADABLE_REASON: &str = "the RUSD fan section publishes a tachometer reading per cooler slot, with no duty percent and no rpm range, so a percentage would need an invented scale; the rpm reading the driver does publish is reported instead";

/// The public power-limit contract asks for a min/current/max wattage range.
/// This backend reads a requested limit, an enforced wall and a policy wattage
/// out of the RUSD power section, and no constraint pair, so the two ends come
/// back null with this instead of a bound the driver never published.
///
/// The named next candidate is a measured negative rather than "not found in a
/// header": the public-control read that would carry a level range,
/// `NV2080_CTRL_CMD_PERF_GET_LEVEL_INFO_V2` (`0x2080200B`), answers
/// `NV_ERR_NOT_SUPPORTED`-era `0x1f` on this driver, and the clock range this
/// backend does have comes from the clock channel's own block rather than from
/// the power one.
pub const POWER_RANGE_UNREAD_REASON: &str = "the RUSD power section publishes the requested limit, the enforced wall and a policy wattage, but no min/max constraint pair, so this backend has no bound to report for this end; which read would carry one is 尚未定位 (0x2080200B PERF_GET_LEVEL_INFO_V2 answers 0x1f on this driver)";

/// The driver's `power_policy_tgp_mw` field, measured rather than assumed: on
/// the healthy session it read 11.277-11.309 W while `power_limit_mw` read
/// 100.000 W and instantaneous power read 11.254-11.359 W, so it sits at
/// instantaneous scale and must not be presented as the card's TGP bound.
pub const POWER_POLICY_SCALE_NOTE: &str = "policy_tgp_watt is the driver's own power_policy_tgp_mw reported under its field name rather than as a bound: measured on the healthy session it read 11.277-11.309 W against a 100.000 W wall and 11.254-11.359 W instantaneous power, so what quantity the policy field carries is 尚未定位 and it is not used as an upper limit here";

/// One reading of the public power-limit contract.
///
/// The shape mirrors the entry this backend feeds (min/current/max watts, the
/// same keys the NVML plane reports) and adds the two wattages this driver
/// publishes beyond it. `unavailable` names every field this snapshot could not
/// fill, including the two ends that have no source at all, so "not read" and
/// "not published by the driver" are both distinguishable from a zero.
#[derive(Debug, Clone, Serialize)]
pub struct PowerLimitReport {
    pub source: &'static str,
    pub min_watt: Option<f64>,
    pub current_watt: Option<f64>,
    pub max_watt: Option<f64>,
    pub requested_watt: Option<f64>,
    pub policy_tgp_watt: Option<f64>,
    pub unavailable: BTreeMap<String, String>,
    pub notes: Vec<String>,
}

/// One sensor of the legacy thermal entry.
///
/// `min_c`/`max_c` are the entry's per-sensor physical range and `controller`
/// its controller attribution; both keep the entry's key names and stay null
/// here, with the reason in `unavailable`.
#[derive(Debug, Clone, Serialize)]
pub struct TempSensorView {
    /// The driver's own target type for this sensor, from its measured
    /// decoding: 1=GPU, 2=MEMORY, 4=POWER_SUPPLY, 8=BOARD.
    pub target: &'static str,
    /// The RUSD field the reading comes from, so the provenance is visible and
    /// the pairing is by name rather than by position.
    pub field: &'static str,
    pub current_c: Option<f64>,
    /// Which read produced `current_c`: `rusd:<field>` for this snapshot's own
    /// temperature section, `0x20800513:sensor<n>` for the public thermal read.
    /// Null when there is no value at all.
    pub current_source: Option<String>,
    pub min_c: Option<f64>,
    pub max_c: Option<f64>,
    pub controller: Option<&'static str>,
    pub unavailable: BTreeMap<String, String>,
}

/// The legacy thermal entry carries a per-sensor physical range. This backend's
/// snapshot does not: the RUSD temperature sections publish a reading only.
///
/// The range itself is **readable**, and this is a measured amount rather than
/// a guess: `NV2080_CTRL_CMD_THERMAL_SYSTEM_EXECUTE_V2` (`0x20800513`) answered
/// NV_OK and reported sensor 0 with range [0,127] (target type 1=GPU, provider
/// 0), so what is missing is the client wire-up of that read, not a bound the
/// driver never publishes.
/// What a caller's public thermal read produced. The report keeps "not
/// attempted" apart from "attempted and refused": both leave the range empty,
/// but only the first still means "not wired in", and a refused read that read
/// as "not wired in" would hide a working client behind a sick GSP.
#[derive(Debug, Clone, Copy)]
pub enum ThermalReadOutcome<'a> {
    /// This caller did not issue the read.
    NotAttempted,
    /// The read was asked for and did not complete, with the device's reason:
    /// the driver refusing the control, or the session's own RUSD mapping being
    /// refused (0x62 while the GSP is in its reset-required state) before the
    /// control is ever sent. Both mean "no range this time", neither means "no
    /// range on this platform", and the reason says which one it was.
    Refused(&'a str),
    /// The read answered.
    Answered(&'a ThermalReadout),
}

/// The sensor the public thermal read answered for a given target type.
///
/// A sensor counts only when its reading instruction ran clean and the driver
/// named its target type: without the type there is no way to tell which entry
/// a range belongs to, and a range from an instruction that never ran would be
/// a zero the driver never published.
fn sensor_for_target(read: &ThermalReadout, target_type: u32) -> Option<SensorReading> {
    read.sensors
        .iter()
        .enumerate()
        .take(read.reported_sensors as usize)
        .find(|(_, sensor)| {
            sensor.executed == 1 && sensor.target_known == 1 && sensor.target_type == target_type
        })
        .map(|(index, sensor)| SensorReading {
            index,
            executed: sensor.executed,
            provider_index: sensor.provider_index,
            reading: sensor.reading,
            range_min: sensor.range_min,
            range_max: sensor.range_max,
            range_known: sensor.range_known,
        })
}

/// One sensor of the public thermal read, resolved to the entry it belongs to.
#[derive(Clone, Copy)]
struct SensorReading {
    index: usize,
    executed: u32,
    provider_index: u32,
    reading: i32,
    range_min: i32,
    range_max: i32,
    range_known: u32,
}

pub const THERMAL_RANGE_UNREAD_REASON: &str = "the live reading comes from the RUSD temperature section, which publishes a reading and no bound; the per-sensor range is readable (measured: 0x20800513 EXECUTE_V2 reported sensor 0 range [0,127], target type 1=GPU, provider 0) but this snapshot does not wire that read in yet, so this end is a missing client read rather than a missing driver value";

/// The entry's `controller` names which controller acts on the sensor. The
/// public thermal read reports a provider index (measured: 0) whose relation to
/// this field is not established, so it stays null instead of being mapped by
/// guesswork.
pub const THERMAL_CONTROLLER_UNREAD_REASON: &str = "the RUSD temperature sections carry the reading only; the public thermal read reports a provider index (measured: 0) whose relation to this entry's controller field is not established, so it is left null rather than mapped by guesswork";

impl DeviceStatus {
    /// What this snapshot can say about the fans, and what it cannot.
    pub fn fan_report(&self) -> FanReport {
        let unavailable = ["min_percent", "max_percent", "current_percent"]
            .into_iter()
            .map(|field| (field.to_string(), FAN_PERCENT_UNREADABLE_REASON.to_string()))
            .collect();
        let mut notes = Vec::new();
        match self.unavailable.get("fan_rpm") {
            // The section itself was unusable, so an empty cooler list means
            // "not read", not "nothing turned" -- the two must not be merged.
            Some(reason) => notes.push(format!(
                "the fan section of this snapshot was not usable ({reason}), so an empty cooler \
                 list here means \"not read\", not \"no fan\""
            )),
            None => notes.push(
                "count and coolers list only the coolers that reported a plausible non-zero rpm \
                 in this snapshot: a stopped fan reads 0 rpm, which this layer cannot tell apart \
                 from an unpopulated slot"
                    .to_string(),
            ),
        }
        if !self.unavailable.is_empty() {
            notes.push("see unavailable for the readings this snapshot could not fill".to_string());
        }
        FanReport {
            count: self.fan_coolers.len(),
            coolers: self.fan_coolers.clone(),
            min_percent: None,
            max_percent: None,
            current_percent: None,
            unavailable,
            notes,
        }
    }

    /// Projects the wattages this backend reads onto the public power-limit
    /// contract.
    ///
    /// `current_watt` is the enforced wall (`power_limit_mw` in the snapshot):
    /// the same quantity as NVML's power-management limit, read from the
    /// driver's own telemetry instead of from NVML. The min/max ends have no
    /// source on this path (see [`POWER_RANGE_UNREAD_REASON`]), and an unread
    /// section yields nulls carrying that section's own reason -- never 0 W,
    /// which would claim the driver enforced a zero-watt wall.
    pub fn power_limit_report(&self) -> PowerLimitReport {
        let watts = |mw: Option<u32>| mw.map(|mw| f64::from(mw) / 1000.0);
        let current_watt = watts(self.power_limit_mw);
        let requested_watt = watts(self.power_limit_requested_mw);
        let policy_tgp_watt = watts(self.power_policy_tgp_mw);
        let mut unavailable: BTreeMap<String, String> = BTreeMap::new();
        unavailable.insert(
            "min_watt".to_string(),
            POWER_RANGE_UNREAD_REASON.to_string(),
        );
        unavailable.insert(
            "max_watt".to_string(),
            POWER_RANGE_UNREAD_REASON.to_string(),
        );
        // One key per contract field whose published field did not survive this
        // snapshot's checks, with the reason that field itself carries. A field
        // that was read keeps its number even when a neighbour was not.
        for (field, published, read) in [
            ("current_watt", "power_limit_mw", current_watt),
            ("requested_watt", "power_limit_requested_mw", requested_watt),
            ("policy_tgp_watt", "power_policy_tgp_mw", policy_tgp_watt),
        ] {
            if read.is_none() {
                unavailable.insert(
                    field.to_string(),
                    self.unavailable
                        .get(published)
                        .cloned()
                        .unwrap_or_else(|| format!("{published} was not read in this snapshot")),
                );
            }
        }
        let notes = vec![
            "current_watt is the enforced wall the snapshot publishes as power_limit_mw in \
             milliwatts, converted by /1000; requested_watt is the limit the driver was asked \
             for (power_limit_requested_mw). On the measured healthy session both read 100.000 W."
                .to_string(),
            POWER_POLICY_SCALE_NOTE.to_string(),
            "the NVAPI plane's TDP percent range (min/default/max) has no counterpart here: the \
             vocabulary of this section is milliwatts."
                .to_string(),
        ];
        PowerLimitReport {
            source: "macos-rm",
            min_watt: None,
            current_watt,
            max_watt: None,
            requested_watt,
            policy_tgp_watt,
            unavailable,
            notes,
        }
    }

    /// One view of the legacy thermal entry: the live reading of every sensor
    /// this backend knows about, the reading range the public thermal read
    /// publishes for it, and a reason for every field that stays empty.
    ///
    /// The per-sensor keys are the entry's own (`target`, `current_c`, `min_c`,
    /// `max_c`, `controller`); `field`, `current_source` and `unavailable` are
    /// additive, because a null has to say *which* null it is -- "not read in
    /// this snapshot", "this driver never published it" and "the read answered
    /// for another sensor" are different answers and a bare null cannot tell
    /// them apart.
    ///
    /// Two reads feed this: the RUSD temperature sections for the live readings
    /// and the public thermal read (`thermal`) for each sensor's reading range.
    /// `current_source` names which one produced each number, so merging them
    /// never turns into an unattributable value.
    ///
    /// The target names come from the driver's own target-type bitmask
    /// (ctrl2080thermal.h, measured on the device: 1=GPU, 2=MEMORY,
    /// 4=POWER_SUPPLY, 8=BOARD), and the pairing is by field name: the driver's
    /// type order is not the RUSD field order (4 is the power-supply type where
    /// the block's fourth section is the board), so a positional mapping would
    /// mislabel two of them.
    pub fn legacy_temp_sensor_report(
        &self,
        thermal: ThermalReadOutcome<'_>,
    ) -> Vec<TempSensorView> {
        [
            ("gpu", THERMAL_TARGET_GPU, "temperature_c", self.temperature_c),
            (
                "memory",
                THERMAL_TARGET_MEMORY,
                "temperature_memory_c",
                self.temperature_memory_c,
            ),
            (
                "board",
                THERMAL_TARGET_BOARD,
                "temperature_board_c",
                self.temperature_board_c,
            ),
            (
                "power_supply",
                THERMAL_TARGET_POWER_SUPPLY,
                "temperature_power_supply_c",
                self.temperature_power_supply_c,
            ),
        ]
        .into_iter()
        .map(|(target, target_type, field, current_c)| {
            // The sensor the public read answered for, if it answered for this
            // target: the driver groups sensors under a target, and only that
            // target's range belongs to this entry.
            let read = match thermal {
                ThermalReadOutcome::Answered(read) => sensor_for_target(read, target_type),
                _ => None,
            };

            let range_reason = match (thermal, read.as_ref()) {
                // The read ran and answered for this target: no reason needed,
                // the range below is the driver's own.
                (ThermalReadOutcome::Answered(_), Some(_)) => None,
                (ThermalReadOutcome::Answered(read), None) => Some(format!(
                    "the public thermal read answered ({} target(s), {} sensor(s), {} read) and named no sensor of target type {target_type}, so it does not report this sensor's range",
                    read.target_count, read.sensor_count, read.reported_sensors
                )),
                (ThermalReadOutcome::Refused(reason), _) => Some(format!(
                    "the public thermal read did not complete ({reason}), so this sensor's range is unread this time; the read is wired in and needs a healthy GSP"
                )),
                (ThermalReadOutcome::NotAttempted, _) => {
                    Some(THERMAL_RANGE_UNREAD_REASON.to_string())
                }
            };
            let controller_reason = match read {
                Some(sensor) => format!(
                    "the public thermal read reports provider index {} for this sensor; the RUSD temperature sections carry the reading only, and how that provider index maps onto this entry's controller field is not established, so it is left null rather than mapped by guesswork",
                    sensor.provider_index
                ),
                None => THERMAL_CONTROLLER_UNREAD_REASON.to_string(),
            };

            let mut unavailable = BTreeMap::new();
            if let Some(reason) = &range_reason {
                unavailable.insert("min_c".to_string(), reason.clone());
                unavailable.insert("max_c".to_string(), reason.clone());
            }
            unavailable.insert("controller".to_string(), controller_reason);

            // The live value: this snapshot's own section first, and the public
            // read's reading only when that section has nothing, with the source
            // named either way.
            let (current_c, current_source) = match current_c {
                Some(celsius) => (Some(celsius), Some(format!("rusd:{field}"))),
                None => match read.filter(|sensor| sensor.executed == 1) {
                    Some(sensor) => (
                        Some(f64::from(sensor.reading)),
                        Some(format!("0x20800513:sensor{}", sensor.index)),
                    ),
                    None => (None, None),
                },
            };
            if current_c.is_none() {
                unavailable.insert(
                    "current_c".to_string(),
                    self.unavailable
                        .get(field)
                        .cloned()
                        .unwrap_or_else(|| format!("{field} was not read in this snapshot")),
                );
            }

            let (min_c, max_c) = match read.filter(|sensor| sensor.range_known == 1) {
                Some(sensor) => (
                    Some(f64::from(sensor.range_min)),
                    Some(f64::from(sensor.range_max)),
                ),
                None => (None, None),
            };

            TempSensorView {
                target,
                field,
                current_c,
                current_source,
                min_c,
                max_c,
                controller: None,
                unavailable,
            }
        })
        .collect()
    }
}

#[derive(Debug, Clone)]
pub struct Device {
    id: GpuId,
    info: DeviceInfo,
}

impl Device {
    pub fn id(&self) -> GpuId {
        self.id
    }

    pub fn info(&self) -> &DeviceInfo {
        &self.info
    }

    /// Refreshes RM state and closes all native handles before returning.
    pub fn status(&self) -> Result<DeviceStatus, Error> {
        let snapshot = self.refreshed_snapshot()?;
        Ok(status_from_snapshot(&snapshot))
    }

    /// Reads the driver-visible configuration without changing anything.
    ///
    /// Uses the same single RM snapshot as [`Device::status`], so the
    /// configuration readback and the telemetry cannot disagree about the
    /// device, and a device that changed identity behind the session is
    /// rejected instead of being reported under the old name.
    pub fn settings(&self) -> Result<DeviceSettings, Error> {
        let snapshot = self.refreshed_snapshot()?;
        let info = device_from_snapshot(&snapshot).info;
        let status = status_from_snapshot(&snapshot);
        let mut settings = settings_from(&info, &status);
        // The offset channel is only reachable through a session, and it is the
        // one part of this read the GSP can refuse on its own (a wedge answers
        // 0x62 per block). Neither may fail the settings read: both land in the
        // notes as evidence instead of turning into a value.
        match MacosRmSession::open().and_then(|mut session| session.read_offset_channel()) {
            Ok(channel) => apply_offset_channel(&mut settings, &channel),
            Err(error) => settings
                .notes
                .push(format!("offset channel: not read ({error})")),
        }
        // Names, if the snapshot supports any: the comparison is against the
        // live clocks of *this* snapshot, so it has to run here and not inside
        // the block fold, which has no status.
        corroborate_named_offsets(&mut settings, &status);
        Ok(settings)
    }

    /// The legacy thermal view: the live readings of this snapshot plus the
    /// reading ranges the public thermal read publishes for them.
    ///
    /// Read-only on both sides. The public read needs a healthy GSP, so its
    /// refusal is reported as that refusal and not as a missing value; the RUSD
    /// readings come from the same snapshot as the identity check, so a device
    /// that changed behind the session is rejected here exactly as elsewhere.
    pub fn thermal_view(&self) -> Result<Vec<TempSensorView>, Error> {
        let snapshot = self.refreshed_snapshot()?;
        let status = status_from_snapshot(&snapshot);
        let read = MacosRmSession::open().and_then(|mut session| session.read_thermal());
        let outcome = match &read {
            Ok(readout) => ThermalReadOutcome::Answered(readout),
            Err(error) => ThermalReadOutcome::Refused(&error.to_string()),
        };
        Ok(status.legacy_temp_sensor_report(outcome))
    }

    /// One fresh snapshot, with the identity re-checked against this handle.
    fn refreshed_snapshot(&self) -> Result<Snapshot, Error> {
        let snapshot = read_snapshot(true)?;
        let current = device_from_snapshot(&snapshot);
        if current.info.pci_address != self.info.pci_address
            || current.info.device_id != self.info.device_id
            || current.info.rm_gpu_id != self.info.rm_gpu_id
        {
            return Err(Error::Custom(
                "macOS RM device changed; rediscover GPUs".into(),
            ));
        }
        Ok(snapshot)
    }
}

pub fn discover() -> Result<Vec<Device>, Error> {
    read_snapshot(false).map(|snapshot| vec![device_from_snapshot(&snapshot)])
}

/// Typed macOS info query for the common `run` / `run_many` operation API.
#[derive(Debug, Clone, Copy)]
pub struct QueryMacosRmInfo;

impl crate::GpuOperation for QueryMacosRmInfo {
    type Output = DeviceInfo;

    fn kind(&self) -> crate::OperationKind {
        crate::OperationKind::QueryGpuInfo
    }

    fn run(&self, target: &crate::GpuTarget<'_>) -> Result<Self::Output, Error> {
        Ok(target.macos_rm()?.info().clone())
    }
}

/// Refreshes the macOS status without falling back to another GPU backend.
#[derive(Debug, Clone, Copy)]
pub struct QueryMacosRmStatus;

impl crate::GpuOperation for QueryMacosRmStatus {
    type Output = DeviceStatus;

    fn kind(&self) -> crate::OperationKind {
        crate::OperationKind::QueryGpuStatus
    }

    fn run(&self, target: &crate::GpuTarget<'_>) -> Result<Self::Output, Error> {
        target.macos_rm()?.status()
    }
}

/// Read-only configuration query for the common `run` / `run_many` operation API.
#[derive(Debug, Clone, Copy)]
pub struct QueryMacosRmSettings;

impl crate::GpuOperation for QueryMacosRmSettings {
    type Output = DeviceSettings;

    fn kind(&self) -> crate::OperationKind {
        crate::OperationKind::QueryGpuSettings
    }

    fn run(&self, target: &crate::GpuTarget<'_>) -> Result<Self::Output, Error> {
        target.macos_rm()?.settings()
    }
}

/// The legacy thermal view through the common `run` / `run_many` operation API.
/// Read-only: it issues the public thermal read as well as the RUSD snapshot, and
/// issues no SET of any kind.
#[derive(Debug, Clone, Copy)]
pub struct QueryMacosRmThermalView;

impl crate::GpuOperation for QueryMacosRmThermalView {
    type Output = Vec<TempSensorView>;

    fn kind(&self) -> crate::OperationKind {
        crate::OperationKind::QueryGpuStatus
    }

    fn run(&self, target: &crate::GpuTarget<'_>) -> Result<Self::Output, Error> {
        target.macos_rm()?.thermal_view()
    }
}

/// Version of the private C ABI in core/native/macos_rm_abi.h. Bump both
/// sides together: the C header pins the same number and the hardware test
/// compares the two fingerprints.
const ABI_VERSION: u32 = 4;

/// Slots returned by [`abi_layout`] and by the native
/// nvoc_macos_rm_abi_layout(): sizes, field offsets, the ABI version and the
/// per-field status codes Rust matches on. Keeping them identical is what the
/// C header's `_Static_assert`s and the hardware test enforce.
pub const ABI_LAYOUT_SLOTS: usize = 44;

/// RUSD section indices and value-block sizes, matching macos_rm_abi.h (the
/// header is the source of truth; the layout fingerprint and the compile-time
/// asserts below keep the two sides identical).
const RUSD_FIELDS: usize = 14;
const SEC_CLOCK: usize = 0;
const SEC_TEMP_GPU: usize = 1;
const SEC_TEMP_MEMORY: usize = 2;
const SEC_TEMP_BOARD: usize = 3;
const SEC_TEMP_POWER_SUPPLY: usize = 4;
const SEC_POWER: usize = 5;
const SEC_POWER_LIMIT: usize = 6;
const SEC_AVG_POWER: usize = 7;
const SEC_POWER_POLICY: usize = 8;
const SEC_THROTTLE: usize = 9;
const SEC_FAN: usize = 10;
const SEC_UTIL: usize = 11;
const SEC_PCI: usize = 12;
const SEC_PROC_UTIL: usize = 13;
const CLK_DOMAINS: usize = 4;
const TEMP_SENSORS: usize = 4;
const FAN_MAX_COOLERS: usize = 16;
const ENGINES: usize = 6;
/// cl00de.h NV00DE_RUSD_POLL_* mask the native layer requests.
const POLL_MASK: u32 = 0xFB;

/// Field status codes reported by the native side (macos_rm_abi.h).
const FIELD_OK: u32 = 0;
const FIELD_UNSTABLE: u32 = 1;
const FIELD_UNAVAILABLE: u32 = 2;
const FIELD_NOT_FRESH: u32 = 3;

/// Private stable C ABI, not an NVIDIA RM structure. NVIDIA layouts live only
// in native/macos_rm.c, compiled against the driver's matching OGKM headers.
#[repr(C)]
struct Snapshot {
    abi_version: u32,
    rm_gpu_id: u32,
    domain: u32,
    bus: u32,
    slot: u32,
    function: u32,
    vendor_id: u32,
    device_id: u32,
    subsystem_id: u32,
    revision: u32,
    board_id: u32,
    class_count: u32,
    total_bytes: u64,
    usable_bytes: u64,
    bar1_bytes: u64,
    l2_bytes: u64,
    pstate: u32,
    memory_status: u32,
    pstate_status: u32,
    pci_status: u32,
    bios_status: u32,
    name_status: u32,
    class_status: u32,
    driver: [u8; 64],
    name: [u8; 64],
    sku: [u8; 16],
    project: [u8; 16],
    telemetry: Telemetry,
}

#[repr(C)]
#[derive(Default)]
struct Telemetry {
    status: u32,
    sample: RawSample,
}

/// Per-sensor result of the public thermal read: what the driver published and
/// whether that instruction ran at all. The two are kept apart because an
/// instruction that did not run leaves its value at zero, and a range of zero
/// would otherwise be indistinguishable from a driver that answered zero.
#[repr(C)]
#[derive(Debug, Default, Clone, Copy)]
struct ThermalSensorReadout {
    executed: u32,
    result: u32,
    target_index: u32,
    provider_index: u32,
    target_type: u32,
    target_known: u32,
    reading: i32,
    range_min: i32,
    range_max: i32,
    range_known: u32,
}

/// The public thermal read (NV2080_CTRL_CMD_THERMAL_SYSTEM_EXECUTE_V2): the
/// driver's own target type per target index, plus the per-sensor reading,
/// range, target and provider. Read-only; nothing here is written.
#[repr(C)]
#[derive(Debug, Default, Clone, Copy)]
pub struct ThermalReadout {
    status: u32,
    target_count: u32,
    sensor_count: u32,
    reported_sensors: u32,
    target_types_known: u32,
    target_types: [u32; THERMAL_SENSORS_MAX],
    sensors: [ThermalSensorReadout; THERMAL_SENSORS_MAX],
}

/// Sensors the thermal read asks about, matching the C cap. The control's own
/// instruction list holds 32, so this transport cap is what binds.
const THERMAL_SENSORS_MAX: usize = 8;

/// The driver's target types, from ctrl2080thermal.h (and measured on the
/// device: target 0 came back as type 1). The values are a bitmask, which is
/// why the names must be looked up rather than counted: the type order puts the
/// power supply third where the RUSD temperature block puts the board.
const THERMAL_TARGET_GPU: u32 = 1;
const THERMAL_TARGET_MEMORY: u32 = 2;
const THERMAL_TARGET_POWER_SUPPLY: u32 = 4;
const THERMAL_TARGET_BOARD: u32 = 8;

/// Raw per-field telemetry exactly as produced by the native layer: one
/// section status + timestamp table plus the value blocks. Every field is a
/// plain integer so the C/Rust layouts can be size-checked, and the driver's
/// own units are preserved (kHz, MHz, mW, 8.8 fixed-point C, percent x100).
#[repr(C)]
#[derive(Default, Clone, Copy)]
struct RawSample {
    abi_version: u32,
    field_status: [u32; RUSD_FIELDS],
    timestamps: [u64; RUSD_FIELDS],
    clk_target_mhz: [u32; CLK_DOMAINS],
    clk_actual_khz: [u32; CLK_DOMAINS],
    temperature_fx256: [i32; TEMP_SENSORS],
    power_mw: u32,
    power_limit_requested_mw: u32,
    power_limit_mw: u32,
    average_power_mw: u32,
    power_policy_tgp_mw: u32,
    throttle_reason_mask: u32,
    fan_rpm: [u32; FAN_MAX_COOLERS],
    utilization_percent: u32,
    memory_utilization_percent: u32,
    engine_utilization_pct100: [u32; ENGINES],
    pci_gen_info: u32,
    pstate: u32,
    pstate_status: u32,
    memory_used_kb: u32,
    memory_used_status: u32,
}

fn text(bytes: &[u8]) -> String {
    let end = bytes
        .iter()
        .position(|&byte| byte == 0)
        .unwrap_or(bytes.len());
    String::from_utf8_lossy(&bytes[..end]).into_owned()
}

fn query_error(status: u32) -> String {
    format!("RM query failed: 0x{status:08x}")
}

fn device_from_snapshot(s: &Snapshot) -> Device {
    let pci = PciAddress {
        domain: s.domain,
        bus: s.bus,
        device: s.slot,
        function: s.function,
    };
    let unavailable = [
        ("name", s.name_status),
        ("memory", s.memory_status),
        ("pci_details", s.pci_status),
        ("bios", s.bios_status),
        ("classes", s.class_status),
    ]
    .into_iter()
    .filter(|(_, status)| *status != 0)
    .map(|(field, status)| (field.into(), query_error(status)))
    .collect();
    Device {
        id: GpuId::from_pci_address(pci),
        info: DeviceInfo {
            name: (s.name_status == 0)
                .then(|| text(&s.name))
                .filter(|name| !name.is_empty()),
            driver_version: text(&s.driver),
            rm_gpu_id: s.rm_gpu_id,
            pci_address: pci.to_string(),
            vendor_id: s.vendor_id,
            device_id: s.device_id,
            subsystem_id: (s.pci_status == 0).then_some(s.subsystem_id),
            revision: (s.pci_status == 0).then_some(s.revision),
            board_id: (s.bios_status == 0).then_some(s.board_id),
            chip_sku: (s.bios_status == 0).then(|| text(&s.sku)),
            project: (s.bios_status == 0).then(|| text(&s.project)),
            memory_total_bytes: (s.memory_status == 0).then_some(s.total_bytes),
            memory_usable_bytes: (s.memory_status == 0).then_some(s.usable_bytes),
            bar1_bytes: (s.memory_status == 0).then_some(s.bar1_bytes),
            l2_cache_bytes: (s.memory_status == 0).then_some(s.l2_bytes),
            supported_class_count: (s.class_status == 0).then_some(s.class_count),
            unavailable,
        },
    }
}

fn status_from_snapshot(s: &Snapshot) -> DeviceStatus {
    interpret_oneshot(&s.telemetry.sample, s.telemetry.status)
}

/// nvx_xnu owns a process-global fd table, so every native call (one-shot and
/// session) must be serialized through this single lock.
#[cfg(all(target_os = "macos", target_arch = "x86_64"))]
fn transport() -> &'static std::sync::Mutex<()> {
    static TRANSPORT: std::sync::Mutex<()> = std::sync::Mutex::new(());
    &TRANSPORT
}

#[cfg(all(target_os = "macos", target_arch = "x86_64"))]
fn lock_transport() -> Result<std::sync::MutexGuard<'static, ()>, Error> {
    transport()
        .lock()
        .map_err(|_| Error::Custom("macOS RM transport lock poisoned".into()))
}

#[cfg(all(target_os = "macos", target_arch = "x86_64"))]
unsafe extern "C" {
    fn nvoc_macos_rm_snapshot(
        out: *mut Snapshot,
        size: usize,
        telemetry: u32,
        error: *mut u8,
        cap: usize,
    ) -> u32;
    fn nvoc_macos_rm_session_open(
        out: *mut *mut SessionOpaque,
        info: *mut Snapshot,
        info_size: usize,
        baseline: *mut u64,
        baseline_count: usize,
        error: *mut u8,
        cap: usize,
    ) -> u32;
    fn nvoc_macos_rm_session_sample(
        handle: *mut SessionOpaque,
        out: *mut RawSample,
        size: usize,
        error: *mut u8,
        cap: usize,
    ) -> u32;
    fn nvoc_macos_rm_session_thermal(
        handle: *mut SessionOpaque,
        out: *mut ThermalReadout,
        size: usize,
        error: *mut u8,
        cap: usize,
    ) -> u32;
    fn nvoc_macos_rm_session_close(handle: *mut SessionOpaque, error: *mut u8, cap: usize) -> u32;
    fn nvoc_macos_rm_session_offset(
        handle: *mut SessionOpaque,
        kind: u32,
        out: *mut u8,
        size: usize,
        error: *mut u8,
        cap: usize,
    ) -> u32;
    fn nvoc_macos_rm_abi_layout(out: *mut u64, count: usize) -> usize;
}

#[cfg(all(target_os = "macos", target_arch = "x86_64"))]
fn read_snapshot(telemetry: bool) -> Result<Snapshot, Error> {
    let _guard = lock_transport()?;
    let mut snapshot = std::mem::MaybeUninit::<Snapshot>::uninit();
    let mut error = [0u8; 256];
    // SAFETY: C receives correctly-sized writable buffers, initializes the
    // entire POD snapshot on success, and does not retain either pointer.
    // The mutex serializes every access to nvx_xnu's global connection table.
    let status = unsafe {
        nvoc_macos_rm_snapshot(
            snapshot.as_mut_ptr(),
            std::mem::size_of::<Snapshot>(),
            u32::from(telemetry),
            error.as_mut_ptr(),
            error.len(),
        )
    };
    if status != 0 {
        return Err(Error::Custom(format!("macOS RM: {}", text(&error))));
    }
    // SAFETY: success guarantees all fields were initialized by C.
    let snapshot = unsafe { snapshot.assume_init() };
    if snapshot.abi_version != ABI_VERSION {
        return Err(Error::Custom(
            "macOS RM snapshot ABI version mismatch".into(),
        ));
    }
    Ok(snapshot)
}

/// The native half of the layout fingerprint, read through the shared ABI
/// header. Compared with [`abi_layout`] by the ignored hardware test.
#[cfg(all(target_os = "macos", target_arch = "x86_64"))]
pub fn native_abi_layout() -> Result<[u64; ABI_LAYOUT_SLOTS], Error> {
    let _guard = lock_transport()?;
    let mut out = [0u64; ABI_LAYOUT_SLOTS];
    // SAFETY: the C entry point only writes into the slice we pass and reports
    // how many slots this build actually has.
    let count = unsafe { nvoc_macos_rm_abi_layout(out.as_mut_ptr(), out.len()) };
    if count != ABI_LAYOUT_SLOTS {
        return Err(Error::Custom(format!(
            "macOS RM ABI layout: native build reports {count} slots, Rust expects {ABI_LAYOUT_SLOTS}"
        )));
    }
    Ok(out)
}

#[cfg(not(all(target_os = "macos", target_arch = "x86_64")))]
pub fn native_abi_layout() -> Result<[u64; ABI_LAYOUT_SLOTS], Error> {
    Err(Error::Custom(
        "macOS RM backend requires x86_64 macOS with the NullMoth driver".into(),
    ))
}

#[cfg(not(all(target_os = "macos", target_arch = "x86_64")))]
fn read_snapshot(_telemetry: bool) -> Result<Snapshot, Error> {
    Err(Error::Custom(
        "macOS RM backend requires x86_64 macOS with the NullMoth driver".into(),
    ))
}

// ---------------------------------------------------------------------------
// Continuous monitoring: a persistent read-only session plus the per-sample
// freshness/update interpretation that decides field validity.
// ---------------------------------------------------------------------------

/// A polled section that has not advanced for this long is reported as
/// unavailable instead of repeating its last value forever. The driver's
/// measured poll cadence is 500 ms (see
/// docs/reverse-engineering/macos-gtx1650s-access-2026-10-16.md), so this is a
/// wide margin that will not fire on a healthy idle GPU.
pub const MONITOR_STALE_AFTER: Duration = Duration::from_secs(5);

/// Default budget for the first sample of a session: every polled section
/// should advance past the mapping-time baseline before the first monitor line
/// is emitted, matching the one-shot query's freshness guarantee.
const INITIAL_FRESH_BUDGET: Duration = Duration::from_millis(1500);

/// The Rust half of the C/Rust layout fingerprint. Slot meanings match
/// NVOC_RM_LAYOUT_SLOTS in core/native/macos_rm_abi.h and the assertions in
/// core/tests/macos_rm_abi.c; `native_abi_layout()` returns the C half.
pub fn abi_layout() -> [u64; ABI_LAYOUT_SLOTS] {
    [
        std::mem::size_of::<Snapshot>() as u64,
        std::mem::size_of::<RawSample>() as u64,
        std::mem::size_of::<Telemetry>() as u64,
        std::mem::offset_of!(Snapshot, telemetry) as u64,
        std::mem::offset_of!(Snapshot, driver) as u64,
        std::mem::offset_of!(Telemetry, sample) as u64,
        std::mem::offset_of!(RawSample, timestamps) as u64,
        std::mem::offset_of!(RawSample, temperature_fx256) as u64,
        std::mem::offset_of!(RawSample, clk_target_mhz) as u64,
        std::mem::offset_of!(RawSample, clk_actual_khz) as u64,
        std::mem::offset_of!(RawSample, power_mw) as u64,
        std::mem::offset_of!(RawSample, fan_rpm) as u64,
        std::mem::offset_of!(RawSample, engine_utilization_pct100) as u64,
        std::mem::offset_of!(RawSample, pci_gen_info) as u64,
        std::mem::offset_of!(RawSample, pstate) as u64,
        std::mem::offset_of!(RawSample, memory_used_kb) as u64,
        ABI_VERSION as u64,
        RUSD_FIELDS as u64,
        FIELD_OK as u64,
        FIELD_UNSTABLE as u64,
        FIELD_UNAVAILABLE as u64,
        FIELD_NOT_FRESH as u64,
        CLK_DOMAINS as u64,
        TEMP_SENSORS as u64,
        FAN_MAX_COOLERS as u64,
        ENGINES as u64,
        POLL_MASK as u64,
        SEC_CLOCK as u64,
        SEC_TEMP_GPU as u64,
        SEC_TEMP_MEMORY as u64,
        SEC_TEMP_BOARD as u64,
        SEC_TEMP_POWER_SUPPLY as u64,
        SEC_POWER as u64,
        SEC_POWER_LIMIT as u64,
        SEC_AVG_POWER as u64,
        SEC_POWER_POLICY as u64,
        SEC_THROTTLE as u64,
        SEC_FAN as u64,
        SEC_UTIL as u64,
        SEC_PCI as u64,
        SEC_PROC_UTIL as u64,
        std::mem::size_of::<ThermalReadout>() as u64,
        std::mem::size_of::<ThermalSensorReadout>() as u64,
        std::mem::offset_of!(ThermalReadout, sensors) as u64,
    ]
}

/// Compile-time mirror of the `_Static_assert`s in macos_rm_abi.h. A layout
/// change on either side fails the build instead of silently mis-reading
/// driver memory.
const _: () = {
    assert!(std::mem::size_of::<Snapshot>() == 648);
    assert!(std::mem::size_of::<RawSample>() == 368);
    assert!(std::mem::size_of::<Telemetry>() == 376);
    assert!(std::mem::offset_of!(Snapshot, telemetry) == 272);
    assert!(std::mem::offset_of!(Snapshot, driver) == 108);
    assert!(std::mem::offset_of!(Telemetry, sample) == 8);
    assert!(std::mem::offset_of!(RawSample, timestamps) == 64);
    assert!(std::mem::offset_of!(RawSample, clk_target_mhz) == 176);
    assert!(std::mem::offset_of!(RawSample, clk_actual_khz) == 192);
    assert!(std::mem::offset_of!(RawSample, temperature_fx256) == 208);
    assert!(std::mem::offset_of!(RawSample, power_mw) == 224);
    assert!(std::mem::offset_of!(RawSample, fan_rpm) == 248);
    assert!(std::mem::offset_of!(RawSample, utilization_percent) == 312);
    assert!(std::mem::offset_of!(RawSample, engine_utilization_pct100) == 320);
    assert!(std::mem::offset_of!(RawSample, pci_gen_info) == 344);
    assert!(std::mem::offset_of!(RawSample, pstate) == 348);
    assert!(std::mem::offset_of!(RawSample, memory_used_kb) == 356);
    assert!(std::mem::size_of::<ThermalSensorReadout>() == 40);
    assert!(std::mem::size_of::<ThermalReadout>() == 372);
    assert!(std::mem::offset_of!(ThermalReadout, target_types) == 20);
    assert!(std::mem::offset_of!(ThermalReadout, sensors) == 52);
    assert!(std::mem::offset_of!(ThermalSensorReadout, reading) == 24);
};

/// Opaque native session handle; only ever touched through the extern ABI.
#[cfg(all(target_os = "macos", target_arch = "x86_64"))]
#[repr(C)]
struct SessionOpaque {
    _private: [u8; 0],
}

/// One monitored status reading: the status plus, per field, whether the
/// underlying RUSD section advanced since the previous sample. A `false` entry
/// means the last coherent value is still shown but has not refreshed.
#[derive(Debug, Clone, Serialize)]
pub struct DeviceSample {
    #[serde(flatten)]
    pub status: DeviceStatus,
    /// Per field: did this field's RUSD section advance on this sample?
    pub updated: BTreeMap<String, bool>,
    /// Per field freshness classification; see [`FieldState`]. This is how a
    /// value that was never refreshed since the session opened is told apart
    /// from one that merely did not change on this sample.
    pub freshness: BTreeMap<String, FieldState>,
    /// Fields whose stamp/sequence went backwards on this sample: the driver
    /// reset its clock or re-initialised the shared page.
    pub epoch_reset: Vec<String>,
    /// How many RUSD sections advanced. Not serialized: it feeds the internal
    /// "the whole page went silent" reconnect decision.
    #[serde(skip)]
    advancing_sections: usize,
}

impl DeviceSample {
    /// RUSD sections that advanced on this sample (pstate is a live RM control
    /// re-read every sample and is not counted here).
    pub fn advancing_sections(&self) -> usize {
        self.advancing_sections
    }
}

/// Consecutive samples with no section advancing at all before the session is
/// considered dead and the caller is asked to reconnect. One quiet field is
/// normal; every section silent this long is not.
const FROZEN_SAMPLES_BEFORE_RECONNECT: u32 = 6;

/// Freshness of one received field, per field and per sample.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum FieldState {
    /// The section advanced on this sample: the published value is new.
    Advancing,
    /// The section did not advance on this sample but is still inside the
    /// staleness window: the last value is repeated with `updated: false`.
    Unchanged,
    /// The section has never advanced since this session opened, so no value
    /// has been confirmed new yet.
    NeverRefreshed,
    /// The section advanced earlier in this session but has been silent past
    /// the staleness window: its last value is no longer trustworthy.
    Stale,
    /// The driver publishes no valid timestamp for this section right now
    /// (cl00de.h RUSD_TIMESTAMP_INVALID), so retrying will not help yet.
    Unsupported,
    /// The section changed while it was copied: no coherent value this round.
    Unstable,
}

impl FieldState {
    /// A value may only be published for these states.
    fn publishes_value(self) -> bool {
        matches!(self, FieldState::Advancing | FieldState::Unchanged)
    }

    fn is_advancing(self) -> bool {
        matches!(self, FieldState::Advancing)
    }
}

/// Per-section monitor state. `ts` is seeded with the session's mapping-time
/// baseline, so a value that was already old when the session opened is never
/// mistaken for a fresh reading.
#[derive(Debug, Clone, Copy, Default)]
struct FieldTracker {
    /// Last RUSD timestamp (polled section) or sequence (non-polled section).
    ts: u64,
    /// Session-relative monotonic nanoseconds when the value last changed.
    changed_ns: u64,
    /// True once the section advanced at least once since the session opened.
    seen_change: bool,
}

/// One section's verdict for this sample.
struct SectionCheck {
    state: FieldState,
    /// Milliseconds the section has been silent: since it last changed, or
    /// since the session opened when it never changed.
    silent_ms: u64,
    /// The stamp/sequence went backwards: the driver reset its clock or
    /// re-initialised the page. The value is still new, but the timeline
    /// restarted and no absolute age may be derived from it.
    epoch_reset: bool,
    /// Whole-subsystem reason (the RUSD page could not be allocated/mapped)
    /// that replaces the per-section text when set.
    override_reason: Option<String>,
}

/// Which wait produced a `NeverRefreshed` verdict, so the reason text names the
/// right budget: the one-shot query waits once before answering, the monitor
/// keeps waiting for the whole session.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FreshnessScope {
    InitialQuery,
    Session,
}

impl SectionCheck {
    fn reason(&self, stale_ms: u64, scope: FreshnessScope) -> String {
        if let Some(reason) = &self.override_reason {
            return reason.clone();
        }
        match self.state {
            FieldState::Unsupported => {
                "RUSD section has no valid timestamp (driver does not publish it)".into()
            }
            FieldState::Unstable => {
                "RUSD section changed while it was copied (unstable sequence)".into()
            }
            FieldState::NeverRefreshed => match scope {
                FreshnessScope::InitialQuery => {
                    "no RUSD update within the initial freshness budget".into()
                }
                FreshnessScope::Session => format!(
                    "no RUSD update since this session opened ({} ms)",
                    self.silent_ms
                ),
            },
            FieldState::Stale => format!(
                "no RUSD update for {} ms (staleness window {} ms)",
                self.silent_ms, stale_ms
            ),
            FieldState::Advancing | FieldState::Unchanged => String::new(),
        }
    }
}

/// Classify one RUSD section. Freshness comes from advancement since the
/// baseline plus host monotonic time, never from the stamp's absolute value:
/// the driver's stamps lag wall clock and drift, and non-polled sections carry
/// sequence numbers rather than nanoseconds (class/cl00de.h).
fn track_section(
    tracker: &mut FieldTracker,
    raw_status: u32,
    ts: u64,
    now_ns: u64,
    stale_ns: u64,
) -> SectionCheck {
    let silent_ns = now_ns.saturating_sub(tracker.changed_ns);
    match raw_status {
        FIELD_OK => {}
        FIELD_UNAVAILABLE => {
            return SectionCheck {
                state: FieldState::Unsupported,
                silent_ms: silent_ns / 1_000_000,
                epoch_reset: false,
                override_reason: None,
            };
        }
        _ => {
            return SectionCheck {
                state: FieldState::Unstable,
                silent_ms: silent_ns / 1_000_000,
                epoch_reset: false,
                override_reason: None,
            };
        }
    }
    if ts == 0 {
        return SectionCheck {
            state: FieldState::Unsupported,
            silent_ms: silent_ns / 1_000_000,
            epoch_reset: false,
            override_reason: None,
        };
    }
    if ts != tracker.ts {
        // Any change is new data, including a backwards jump: a clock or
        // sequence reset still published a freshly written section.
        let epoch_reset = ts < tracker.ts;
        tracker.ts = ts;
        tracker.changed_ns = now_ns;
        tracker.seen_change = true;
        return SectionCheck {
            state: FieldState::Advancing,
            silent_ms: 0,
            epoch_reset,
            override_reason: None,
        };
    }
    let state = if !tracker.seen_change {
        FieldState::NeverRefreshed
    } else if silent_ns > stale_ns {
        FieldState::Stale
    } else {
        FieldState::Unchanged
    };
    SectionCheck {
        state,
        silent_ms: silent_ns / 1_000_000,
        epoch_reset: false,
        override_reason: None,
    }
}

/// Everything one sample records about its fields.
#[derive(Default)]
struct SampleFields {
    unavailable: BTreeMap<String, String>,
    timestamps: BTreeMap<String, u64>,
    updated: BTreeMap<String, bool>,
    freshness: BTreeMap<String, FieldState>,
    epoch_resets: Vec<String>,
    advancing_sections: usize,
}

impl SampleFields {
    /// Record one field. Returns whether a value may be published for it.
    fn publish(
        &mut self,
        name: &str,
        check: &SectionCheck,
        ts: u64,
        value: ValueCheck,
        stale_ms: u64,
        scope: FreshnessScope,
    ) -> bool {
        self.freshness.insert(name.into(), check.state);
        if check.epoch_reset {
            self.epoch_resets.push(name.into());
        }
        if !check.state.publishes_value() {
            self.unavailable
                .insert(name.into(), check.reason(stale_ms, scope));
            return false;
        }
        if let Some(reason) = value.reason() {
            self.unavailable.insert(name.into(), reason.into());
            return false;
        }
        self.timestamps.insert(name.into(), ts);
        self.updated.insert(name.into(), check.state.is_advancing());
        true
    }

    /// Note a section's advancement for the whole-page "went silent" health
    /// check (the clock section feeds eight value fields but is one section).
    fn note_section(&mut self, check: &SectionCheck) {
        if check.state.is_advancing() {
            self.advancing_sections += 1;
        }
    }

    /// Record a field whose payload lives *inside* the RUSD sample while its
    /// validity word rides along with the sample: usable only when the snapshot
    /// itself mapped, so the snapshot-level reason is checked before the field's
    /// own status word.
    ///
    /// That word is precisely what cannot be trusted when the mapping failed: a
    /// zero-filled sample reads as status 0, i.e. "fine", with a payload of 0.
    fn publish_snapshot_backed(
        &mut self,
        name: &str,
        status: u32,
        rusd_map_reason: Option<&str>,
    ) -> bool {
        if let Some(reason) = rusd_map_reason {
            self.freshness.insert(name.into(), FieldState::Unsupported);
            self.unavailable.insert(name.into(), reason.to_string());
            return false;
        }
        self.publish_live(name, status)
    }

    /// Record a field that comes from a live RM control rather than a RUSD
    /// section: it is new whenever its query succeeded.
    fn publish_live(&mut self, name: &str, status: u32) -> bool {
        if status == 0 {
            self.freshness.insert(name.into(), FieldState::Advancing);
            self.updated.insert(name.into(), true);
            true
        } else {
            self.freshness.insert(name.into(), FieldState::Unsupported);
            self.unavailable.insert(name.into(), query_error(status));
            false
        }
    }
}

/// Whether a present value may be published. Separated from the freshness
/// check because "the driver does not report this" and "the driver reported
/// something impossible" are different conditions.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ValueCheck {
    Ok,
    /// Out of the plausible range for the field, e.g. a fan at 294 118 rpm
    /// from an unpopulated RUSD slot. Never published as a measurement.
    Invalid,
    /// The driver reports no value for this sub-field (a clock domain it does
    /// not implement, a temperature sensor with no sensor fitted).
    NotReported,
}

impl ValueCheck {
    fn reason(self) -> Option<&'static str> {
        match self {
            ValueCheck::Ok => None,
            ValueCheck::Invalid => Some("RUSD field reported an implausible value"),
            ValueCheck::NotReported => Some("driver reports no value for this field"),
        }
    }
}

/// Plausibility bounds. A value outside these is treated as "the driver
/// published garbage" instead of being shown as a reading: the fan array in
/// particular carries unpopulated slots that read as huge rpm counts.
const MAX_FAN_RPM: u32 = 20_000;
const MAX_CLOCK_KHZ: u32 = 20_000_000; // 20 GHz, far above any real GPU clock
const MAX_TARGET_MHZ: u32 = 20_000;
const MAX_POWER_MW: u32 = 2_000_000; // 2 kW
const TEMP_MIN_FX256: i32 = -50 * 256;
const TEMP_MAX_FX256: i32 = 150 * 256;

fn temperature_sane(fx256: i32) -> bool {
    (TEMP_MIN_FX256..=TEMP_MAX_FX256).contains(&fx256)
}

/// cl00de.h RUSD_CLK_THROTTLE_REASON_*, bit order preserved.
const THROTTLE_REASONS: [(u32, &str); 9] = [
    (1 << 0, "gpu_idle"),
    (1 << 1, "application_clock_setting"),
    (1 << 2, "sw_power_cap"),
    (1 << 3, "hw_slowdown"),
    (1 << 4, "sync_boost"),
    (1 << 5, "sw_thermal_slowdown"),
    (1 << 6, "hw_thermal_slowdown"),
    (1 << 7, "hw_power_brakes_slowdown"),
    (1 << 8, "display_clock_setting"),
];

fn decode_throttle_reasons(mask: u32) -> Vec<String> {
    THROTTLE_REASONS
        .iter()
        .filter(|(bit, _)| mask & bit != 0)
        .map(|(_, name)| (*name).to_owned())
        .collect()
}

/// Human-facing throttle-reason view for the dedicated surface: the full
/// nine-reason vocabulary in the same order NVML reports it, each paired with
/// its active flag decoded from the RUSD `clkThrottleInfo.reasonMask`.
///
/// [`decode_throttle_reasons`] keeps the raw snake_case tags the status payload
/// has always carried; this is the NVML-compatible twin so `get-throttle-reasons`
/// returns the same rows on macOS that it does on NVML/NVAPI. A `None` mask
/// (throttle section unavailable) reports every reason as inactive rather than
/// inventing an active one.
pub fn throttle_reason_states(mask: Option<u32>) -> Vec<(&'static str, bool)> {
    const DISPLAY_NAMES: [&str; 9] = [
        "GPU Idle",
        "App Clock Setting",
        "SW Power Cap",
        "HW Slowdown",
        "Sync Boost",
        "SW Thermal",
        "HW Thermal",
        "HW Power Brake",
        "Display Clock",
    ];
    let mask = mask.unwrap_or(0);
    THROTTLE_REASONS
        .iter()
        .zip(DISPLAY_NAMES)
        .map(|((bit, _), name)| (name, mask & *bit != 0))
        .collect()
}

/// Engine names in the order NVOC_RM_ENGINES carries them (gr, fb, nvenc,
/// nvdec, nvjpg, nvofa), matching RUSD_PROC_UTIL's own field names.
const ENGINE_NAMES: [&str; ENGINES] = ["gr", "fb", "nvenc", "nvdec", "nvjpg", "nvofa"];

/// RUSD_BUS_DATA_PCIE_GEN_INFO bit fields, mirroring
/// NV2080_CTRL_BUS_INFO_PCIE_LINK_CAP_* in ctrl2080bus.h.
const PCIE_GEN_INFO_GEN_SHIFT: u32 = 12;
const PCIE_GEN_INFO_CURR_LEVEL_SHIFT: u32 = 16;

/// Decode the PCIe generation fields. `None` means the driver left the whole
/// word at 0, i.e. it does not report the link at all.
fn decode_pci_gen_info(info: u32) -> (Option<u32>, Option<u32>) {
    if info == 0 {
        return (None, None);
    }
    let capable_gen = (info >> PCIE_GEN_INFO_GEN_SHIFT) & 0xf;
    let current = (info >> PCIE_GEN_INFO_CURR_LEVEL_SHIFT) & 0xf;
    (Some(current + 1), Some(capable_gen + 1))
}

/// Turn one raw sample into the published status. `checks[i]` is section i's
/// verdict; the same function serves the one-shot query (verdicts derived
/// directly from the native field statuses) and the continuous monitor
/// (verdicts from per-section advancement tracking).
fn publish_all(
    fields: &mut SampleFields,
    raw: &RawSample,
    checks: &[SectionCheck; RUSD_FIELDS],
    scope: FreshnessScope,
    stale_ms: u64,
    rusd_map_reason: Option<&str>,
) -> DeviceStatus {
    // ---- CLOCK: four public domains, one section -------------------------
    let clock = &checks[SEC_CLOCK];
    fields.note_section(clock);
    let clock_ts = raw.timestamps[SEC_CLOCK];
    let mut actual_khz = [None; CLK_DOMAINS];
    let mut target_mhz = [None; CLK_DOMAINS];
    for (domain, index) in [("graphics", 0usize), ("memory", 1), ("video", 2), ("sm", 3)] {
        let actual = raw.clk_actual_khz[index];
        let target = raw.clk_target_mhz[index];
        // A domain the driver does not implement reports 0/0. Publishing that
        // as "0 MHz" would look like a stopped clock.
        let value = if actual == 0 && target == 0 {
            ValueCheck::NotReported
        } else if actual > MAX_CLOCK_KHZ || target > MAX_TARGET_MHZ {
            ValueCheck::Invalid
        } else {
            ValueCheck::Ok
        };
        let name = format!("{domain}_clock_khz");
        if fields.publish(&name, clock, clock_ts, value, stale_ms, scope) {
            actual_khz[index] = Some(actual);
        }
        let name = format!("{domain}_target_clock_mhz");
        if fields.publish(&name, clock, clock_ts, value, stale_ms, scope) {
            target_mhz[index] = Some(target);
        }
    }

    // ---- temperatures: one section per sensor ----------------------------
    let mut temperature = [None; TEMP_SENSORS];
    for (index, (section, name)) in [
        (SEC_TEMP_GPU, "temperature_c"),
        (SEC_TEMP_MEMORY, "temperature_memory_c"),
        (SEC_TEMP_BOARD, "temperature_board_c"),
        (SEC_TEMP_POWER_SUPPLY, "temperature_power_supply_c"),
    ]
    .into_iter()
    .enumerate()
    {
        let check = &checks[section];
        fields.note_section(check);
        let fx256 = raw.temperature_fx256[index];
        let value = if temperature_sane(fx256) {
            ValueCheck::Ok
        } else {
            ValueCheck::Invalid
        };
        if fields.publish(name, check, raw.timestamps[section], value, stale_ms, scope) {
            temperature[index] = Some(f64::from(fx256) / 256.0);
        }
    }

    // ---- power block ------------------------------------------------------
    let power = &checks[SEC_POWER];
    fields.note_section(power);
    let power_value = |value: u32| {
        if value <= MAX_POWER_MW {
            ValueCheck::Ok
        } else {
            ValueCheck::Invalid
        }
    };
    let power_mw = fields
        .publish(
            "power_mw",
            power,
            raw.timestamps[SEC_POWER],
            power_value(raw.power_mw),
            stale_ms,
            scope,
        )
        .then_some(raw.power_mw);

    let limit = &checks[SEC_POWER_LIMIT];
    fields.note_section(limit);
    let power_limit_mw = fields
        .publish(
            "power_limit_mw",
            limit,
            raw.timestamps[SEC_POWER_LIMIT],
            power_value(raw.power_limit_mw),
            stale_ms,
            scope,
        )
        .then_some(raw.power_limit_mw);
    let power_limit_requested_mw = fields
        .publish(
            "power_limit_requested_mw",
            limit,
            raw.timestamps[SEC_POWER_LIMIT],
            power_value(raw.power_limit_requested_mw),
            stale_ms,
            scope,
        )
        .then_some(raw.power_limit_requested_mw);

    let average = &checks[SEC_AVG_POWER];
    fields.note_section(average);
    let average_power_mw = fields
        .publish(
            "average_power_mw",
            average,
            raw.timestamps[SEC_AVG_POWER],
            power_value(raw.average_power_mw),
            stale_ms,
            scope,
        )
        .then_some(raw.average_power_mw);

    let policy = &checks[SEC_POWER_POLICY];
    fields.note_section(policy);
    let power_policy_tgp_mw = fields
        .publish(
            "power_policy_tgp_mw",
            policy,
            raw.timestamps[SEC_POWER_POLICY],
            power_value(raw.power_policy_tgp_mw),
            stale_ms,
            scope,
        )
        .then_some(raw.power_policy_tgp_mw);

    // ---- clock throttle reasons -----------------------------------------
    let throttle = &checks[SEC_THROTTLE];
    fields.note_section(throttle);
    let throttle_reason_mask = fields
        .publish(
            "throttle_reason_mask",
            throttle,
            raw.timestamps[SEC_THROTTLE],
            ValueCheck::Ok,
            stale_ms,
            scope,
        )
        .then_some(raw.throttle_reason_mask);

    // ---- fans: one section, 16 slots the driver may leave unpopulated ----
    let fan = &checks[SEC_FAN];
    fields.note_section(fan);
    let fan_ts = raw.timestamps[SEC_FAN];
    let mut fan_rpms: Vec<u32> = Vec::new();
    let mut fan_coolers: Vec<FanCoolerReading> = Vec::new();
    let mut implausible = false;
    for (slot, rpm) in raw.fan_rpm.iter().copied().enumerate() {
        if rpm == 0 {
            continue; // an empty cooler slot, or a stopped fan
        }
        if rpm > MAX_FAN_RPM {
            implausible = true;
            continue;
        }
        fan_rpms.push(rpm);
        fan_coolers.push(FanCoolerReading {
            slot: slot as u32,
            rpm,
        });
    }
    // Every non-zero slot was nonsense: the section itself is not usable.
    let fan_value = if implausible && fan_rpms.is_empty() {
        ValueCheck::Invalid
    } else {
        ValueCheck::Ok
    };
    let fan_max = fan_rpms.iter().copied().max().unwrap_or(0);
    let fan_rpm = fields
        .publish("fan_rpm", fan, fan_ts, fan_value, stale_ms, scope)
        .then_some(fan_max);
    let fan_rpms = fields
        .publish("fan_rpms", fan, fan_ts, fan_value, stale_ms, scope)
        .then_some(fan_rpms);
    let fan_coolers = fields
        .publish("fan_coolers", fan, fan_ts, fan_value, stale_ms, scope)
        .then_some(fan_coolers);

    // ---- device utilization ----------------------------------------------
    let util = &checks[SEC_UTIL];
    fields.note_section(util);
    let percent = |value: u32| {
        if value <= 100 {
            ValueCheck::Ok
        } else {
            ValueCheck::Invalid
        }
    };
    let utilization_percent = fields
        .publish(
            "utilization_percent",
            util,
            raw.timestamps[SEC_UTIL],
            percent(raw.utilization_percent),
            stale_ms,
            scope,
        )
        .then_some(raw.utilization_percent);
    let memory_utilization_percent = fields
        .publish(
            "memory_utilization_percent",
            util,
            raw.timestamps[SEC_UTIL],
            percent(raw.memory_utilization_percent),
            stale_ms,
            scope,
        )
        .then_some(raw.memory_utilization_percent);

    // ---- per-engine utilization (procUtil, percent x100) -----------------
    let proc = &checks[SEC_PROC_UTIL];
    fields.note_section(proc);
    let engine_sane = raw
        .engine_utilization_pct100
        .iter()
        .all(|pct100| *pct100 <= 100 * 100);
    let mut engine_utilization_percent = BTreeMap::new();
    if fields.publish(
        "engine_utilization_percent",
        proc,
        raw.timestamps[SEC_PROC_UTIL],
        if engine_sane {
            ValueCheck::Ok
        } else {
            ValueCheck::Invalid
        },
        stale_ms,
        scope,
    ) {
        for (name, pct100) in ENGINE_NAMES.iter().zip(raw.engine_utilization_pct100) {
            engine_utilization_percent.insert((*name).to_owned(), f64::from(pct100) / 100.0);
        }
    }

    // ---- PCIe link --------------------------------------------------------
    let pci = &checks[SEC_PCI];
    fields.note_section(pci);
    let (pcie_link_gen, pcie_max_link_gen) = decode_pci_gen_info(raw.pci_gen_info);
    let pci_value = if pcie_link_gen.is_some() {
        ValueCheck::Ok
    } else {
        ValueCheck::NotReported
    };
    let pcie_link_gen = fields
        .publish(
            "pcie_link_gen",
            pci,
            raw.timestamps[SEC_PCI],
            pci_value,
            stale_ms,
            scope,
        )
        .then_some(pcie_link_gen)
        .flatten();
    let pcie_max_link_gen = fields
        .publish(
            "pcie_max_link_gen",
            pci,
            raw.timestamps[SEC_PCI],
            pci_value,
            stale_ms,
            scope,
        )
        .then_some(pcie_max_link_gen)
        .flatten();

    // ---- live RM controls (not RUSD sections) ----------------------------
    let pstate = fields
        .publish_live("pstate", raw.pstate_status)
        .then_some(raw.pstate);
    // The byte count is a field of the RUSD sample itself, so a snapshot that
    // failed to map invalidates it no matter what its own status word says. That
    // word is the thing that lies when the mapping failed, and the first real
    // wedged `get-status --macos-rm` is what showed it: fourteen neighbours from
    // the same failed mapping printed `N/A` with the page reason, while this one
    // printed `Memory Used: 0.000 MiB` -- a zero-filled sample's status 0 read as
    // "fine". `pstate` is different and stays on its own status: it comes from a
    // live RM query that still answers on the wedged driver.
    let memory_used_bytes = fields
        .publish_snapshot_backed("memory_used_bytes", raw.memory_used_status, rusd_map_reason)
        .then_some(u64::from(raw.memory_used_kb) * 1024);

    DeviceStatus {
        pstate,
        graphics_clock_khz: actual_khz[0],
        memory_clock_khz: actual_khz[1],
        video_clock_khz: actual_khz[2],
        sm_clock_khz: actual_khz[3],
        graphics_target_clock_mhz: target_mhz[0],
        memory_target_clock_mhz: target_mhz[1],
        video_target_clock_mhz: target_mhz[2],
        sm_target_clock_mhz: target_mhz[3],
        temperature_c: temperature[0],
        temperature_memory_c: temperature[1],
        temperature_board_c: temperature[2],
        temperature_power_supply_c: temperature[3],
        power_mw,
        power_limit_mw,
        power_limit_requested_mw,
        average_power_mw,
        power_policy_tgp_mw,
        throttle_reason_mask,
        throttle_reasons: throttle_reason_mask
            .map(decode_throttle_reasons)
            .unwrap_or_default(),
        fan_rpm,
        fan_rpms: fan_rpms.unwrap_or_default(),
        fan_coolers: fan_coolers.unwrap_or_default(),
        utilization_percent,
        memory_utilization_percent,
        engine_utilization_percent,
        pcie_link_gen,
        pcie_max_link_gen,
        memory_used_bytes,
        rusd_timestamps: std::mem::take(&mut fields.timestamps),
        unavailable: std::mem::take(&mut fields.unavailable),
    }
}

/// One-shot interpretation: the native side already resolved freshness while
/// waiting for the initial sample, so its per-section status maps directly to
/// a verdict.
fn interpret_oneshot(raw: &RawSample, map_status: u32) -> DeviceStatus {
    let mut fields = SampleFields::default();
    let override_reason =
        (map_status != 0).then(|| format!("RUSD allocation/mapping failed: 0x{map_status:08x}"));
    let checks = std::array::from_fn(|index| SectionCheck {
        state: match raw.field_status[index] {
            // A section the native layer called readable must carry the stamp
            // it was read under; no stamp means nothing usable was captured.
            FIELD_OK if map_status == 0 && raw.timestamps[index] != 0 => FieldState::Advancing,
            FIELD_OK => FieldState::Unsupported,
            FIELD_UNAVAILABLE => FieldState::Unsupported,
            FIELD_NOT_FRESH => FieldState::NeverRefreshed,
            _ => FieldState::Unstable,
        },
        silent_ms: 0,
        epoch_reset: false,
        override_reason: override_reason.clone(),
    });
    publish_all(
        &mut fields,
        raw,
        &checks,
        FreshnessScope::InitialQuery,
        0,
        override_reason.as_deref(),
    )
}

/// Interpret one raw native sample for the continuous monitor. Pure and
/// deterministic so the freshness, staleness, zero-value, epoch-reset,
/// plausibility and partial-missing rules are CPU testable.
fn interpret_sample(
    trackers: &mut [FieldTracker; RUSD_FIELDS],
    raw: &RawSample,
    now_ns: u64,
    stale_after: Duration,
) -> DeviceSample {
    let stale_ns = u64::try_from(stale_after.as_nanos()).unwrap_or(u64::MAX);
    let stale_ms = stale_ns / 1_000_000;
    let mut fields = SampleFields::default();
    let checks = std::array::from_fn(|index| {
        track_section(
            &mut trackers[index],
            raw.field_status[index],
            raw.timestamps[index],
            now_ns,
            stale_ns,
        )
    });
    // A monitor session has its mapping by construction; there is no page status
    // to carry here, so the field's own word is the only gate available.
    let status = publish_all(
        &mut fields,
        raw,
        &checks,
        FreshnessScope::Session,
        stale_ms,
        None,
    );
    DeviceSample {
        status,
        updated: fields.updated,
        freshness: fields.freshness,
        epoch_reset: fields.epoch_resets,
        advancing_sections: fields.advancing_sections,
    }
}

/// RUSD sections in the order reported by [`MacosRmSession::initial_settled`]
/// and by the native field indices.
pub const MONITOR_SECTIONS: [&str; RUSD_FIELDS] = [
    "clock",
    "temperature_gpu",
    "temperature_memory",
    "temperature_board",
    "temperature_power_supply",
    "power",
    "power_limit",
    "average_power",
    "power_policy",
    "throttle",
    "fan",
    "utilization",
    "pci",
    "proc_util",
];

/// A persistent read-only RM session: the connection chain and the mapped RUSD
/// page are created once and reused for every [`sample`](Self::sample).
///
/// [`close`](Self::close) releases everything and reports a release error;
/// `Drop` is the fallback for the paths that forget to call it (it cannot
/// report anything, and a panic-abort build never reaches it at all).
pub struct MacosRmSession {
    #[cfg(all(target_os = "macos", target_arch = "x86_64"))]
    handle: *mut SessionOpaque,
    info: DeviceInfo,
    trackers: [FieldTracker; RUSD_FIELDS],
    start: Instant,
    stale_after: Duration,
    /// Per section: did the open wait reach a final verdict? True means the
    /// section published a new value inside the budget, or the driver reports
    /// it as unpublished (there is nothing to wait for). A `false` entry is a
    /// polled section that had not produced a new value yet, so the first
    /// samples report it as `never_refreshed` instead of showing old data.
    initial_settled: [bool; RUSD_FIELDS],
    /// Consecutive samples in which no RUSD section advanced.
    silent_samples: u32,
}

/// Which block of the private clock-offset channel to read. The numbers are the
/// `kind` argument the native side maps onto its command table, and the sizes
/// are the exact param sizes those commands require.
#[cfg(all(target_os = "macos", target_arch = "x86_64"))]
#[derive(Debug, Clone, Copy)]
enum OffsetBlock {
    Directory,
    Range,
    Control,
    ClockDocument,
}

#[cfg(all(target_os = "macos", target_arch = "x86_64"))]
impl OffsetBlock {
    fn kind(self) -> u32 {
        match self {
            Self::Directory => 0,
            Self::Range => 1,
            Self::Control => 2,
            Self::ClockDocument => 3,
        }
    }

    fn bytes(self) -> usize {
        match self {
            Self::Directory => macos_offset::OFFSET_DIRECTORY_BYTES,
            Self::Range => macos_offset::OFFSET_RANGE_BYTES,
            Self::Control => macos_offset::OFFSET_CONTROL_BYTES,
            Self::ClockDocument => macos_offset::CLK_DOCUMENT_BYTES,
        }
    }
}

/// Read one block and parse it, keeping the driver's refusal distinct from a
/// payload that could not be made sense of. Must be called with the transport
/// lock held, like every other entry point.
#[cfg(all(target_os = "macos", target_arch = "x86_64"))]
fn read_offset_block<T>(
    handle: *mut SessionOpaque,
    block: OffsetBlock,
    parse: fn(&[u8]) -> Result<T, macos_offset::OffsetParseError>,
) -> macos_offset::OffsetBlockRead<T> {
    let mut payload = vec![0u8; block.bytes()];
    let mut error = [0u8; 256];
    // SAFETY: the handle is live, `payload` is exactly the size its command
    // declares, the buffers outlive the call, and C writes the payload only on
    // a zero return.
    let status = unsafe {
        nvoc_macos_rm_session_offset(
            handle,
            block.kind(),
            payload.as_mut_ptr(),
            payload.len(),
            error.as_mut_ptr(),
            error.len(),
        )
    };
    if status != 0 {
        return macos_offset::OffsetBlockRead::Refused(status);
    }
    match parse(&payload) {
        Ok(parsed) => macos_offset::OffsetBlockRead::Parsed(parsed),
        Err(err) => macos_offset::OffsetBlockRead::Malformed(err),
    }
}

impl MacosRmSession {
    /// Open with the default staleness window and no cancellation hook.
    pub fn open() -> Result<Self, Error> {
        Self::open_with(MONITOR_STALE_AFTER, INITIAL_FRESH_BUDGET, &|| false)
    }

    /// Open a session, waiting at most `initial_budget` for every polled
    /// section to publish its first new value, and bailing out early when
    /// `cancelled()` returns true so a stop request is never stuck behind the
    /// freshness wait.
    ///
    /// A timed-out wait is not an open failure: the affected sections are
    /// reported as [`FieldState::NeverRefreshed`] until they publish, and
    /// [`initial_settled`](Self::initial_settled) says which ones are still
    /// waiting.
    #[cfg(all(target_os = "macos", target_arch = "x86_64"))]
    pub fn open_with(
        stale_after: Duration,
        initial_budget: Duration,
        cancelled: &dyn Fn() -> bool,
    ) -> Result<Self, Error> {
        let mut info = std::mem::MaybeUninit::<Snapshot>::uninit();
        let mut handle: *mut SessionOpaque = std::ptr::null_mut();
        // The native side hands back each polled section's mapping-time
        // watermark: the value a reading must move past to count as new.
        let mut baseline = [0u64; RUSD_FIELDS];
        let mut error = [0u8; 256];
        let status = {
            let _guard = lock_transport()?;
            // SAFETY: C receives correctly sized writable buffers and either
            // fills `info`/`baseline` fully or returns an error with `handle`
            // left null.
            unsafe {
                nvoc_macos_rm_session_open(
                    &mut handle,
                    info.as_mut_ptr(),
                    std::mem::size_of::<Snapshot>(),
                    baseline.as_mut_ptr(),
                    RUSD_FIELDS,
                    error.as_mut_ptr(),
                    error.len(),
                )
            }
        };
        if status != 0 {
            return Err(Error::Custom(format!("macOS RM: {}", text(&error))));
        }
        // SAFETY: a zero status means C initialised every field.
        let info = unsafe { info.assume_init() };
        if info.abi_version != ABI_VERSION {
            let mut err = [0u8; 256];
            let _guard = lock_transport()?;
            // SAFETY: the handle came from a successful open and is closed once.
            let _ = unsafe { nvoc_macos_rm_session_close(handle, err.as_mut_ptr(), err.len()) };
            return Err(Error::Custom(format!(
                "macOS RM session ABI version {} is not {ABI_VERSION}",
                info.abi_version
            )));
        }
        let device = device_from_snapshot(&info);
        let mut session = MacosRmSession {
            handle,
            info: device.info,
            trackers: baseline.map(|ts| FieldTracker {
                ts,
                changed_ns: 0,
                seen_change: false,
            }),
            start: Instant::now(),
            stale_after,
            initial_settled: [false; RUSD_FIELDS],
            silent_samples: 0,
        };
        session.initial_settled = session.wait_initial_settled(initial_budget, cancelled);
        Ok(session)
    }

    #[cfg(not(all(target_os = "macos", target_arch = "x86_64")))]
    pub fn open_with(
        _stale_after: Duration,
        _initial_budget: Duration,
        _cancelled: &dyn Fn() -> bool,
    ) -> Result<Self, Error> {
        Err(Error::Custom(
            "macOS RM backend requires x86_64 macOS with the NullMoth driver".into(),
        ))
    }

    pub fn info(&self) -> &DeviceInfo {
        &self.info
    }

    /// Identity triple used to detect that the backend now reports a different
    /// device than the one originally discovered.
    pub fn identity(&self) -> (String, u32, u32) {
        (
            self.info.pci_address.clone(),
            self.info.device_id,
            self.info.rm_gpu_id,
        )
    }

    /// Per section (see [`MONITOR_SECTIONS`]): did the open wait reach a final
    /// verdict for it? `false` means the section is polled but had not
    /// published a new value inside the budget, so the first samples report it
    /// as `never_refreshed` instead of showing old data as fresh. Sections the
    /// driver does not publish settle immediately: there is nothing to wait
    /// for, and they are reported as `unsupported`, not as a pending wait.
    pub fn initial_settled(&self) -> [bool; RUSD_FIELDS] {
        self.initial_settled
    }

    /// One coherent, read-only sample. Session-level failures (dead fds, a
    /// page that stopped publishing, lost device) return `Err` and require
    /// reopening; per-field problems are reported inside the sample.
    pub fn sample(&mut self) -> Result<DeviceSample, Error> {
        let raw = self.raw_sample()?;
        let now = self.mono_ns();
        let sample = interpret_sample(&mut self.trackers, &raw, now, self.stale_after);
        self.track_health(&sample)?;
        Ok(sample)
    }

    /// Ask the caller to reconnect when the whole page goes quiet: a single
    /// section without an update is normal, but every section silent for
    /// several consecutive samples means this session no longer publishes.
    fn track_health(&mut self, sample: &DeviceSample) -> Result<(), Error> {
        if sample.advancing_sections() > 0 {
            self.silent_samples = 0;
            return Ok(());
        }
        self.silent_samples += 1;
        if self.silent_samples >= FROZEN_SAMPLES_BEFORE_RECONNECT {
            return Err(Error::Custom(format!(
                "macOS RM session stopped publishing RUSD data (no section advanced for {} consecutive samples)",
                self.silent_samples
            )));
        }
        Ok(())
    }

    fn mono_ns(&self) -> u64 {
        self.start.elapsed().as_nanos().min(u128::from(u64::MAX)) as u64
    }

    /// Release the mapping, the RM objects and both fds, reporting a release
    /// failure instead of swallowing it. `Drop` still covers callers that do
    /// not call this.
    pub fn close(mut self) -> Result<(), Error> {
        self.close_native()
    }

    #[cfg(all(target_os = "macos", target_arch = "x86_64"))]
    fn close_native(&mut self) -> Result<(), Error> {
        {
            if self.handle.is_null() {
                return Ok(());
            }
            let mut error = [0u8; 256];
            let _guard = transport().lock().unwrap_or_else(|err| err.into_inner());
            // SAFETY: the handle came from a successful open and is closed once.
            let status = unsafe {
                nvoc_macos_rm_session_close(self.handle, error.as_mut_ptr(), error.len())
            };
            self.handle = std::ptr::null_mut();
            if status != 0 {
                return Err(Error::Custom(format!(
                    "macOS RM session close: {}",
                    text(&error)
                )));
            }
        }
        Ok(())
    }

    /// Nothing to release where the backend does not exist.
    #[cfg(not(all(target_os = "macos", target_arch = "x86_64")))]
    fn close_native(&mut self) -> Result<(), Error> {
        Ok(())
    }

    #[cfg(all(target_os = "macos", target_arch = "x86_64"))]
    fn raw_sample(&mut self) -> Result<RawSample, Error> {
        let _guard = lock_transport()?;
        let mut raw = std::mem::MaybeUninit::<RawSample>::uninit();
        let mut error = [0u8; 256];
        // SAFETY: the handle is live and `raw` is a correctly sized output.
        let status = unsafe {
            nvoc_macos_rm_session_sample(
                self.handle,
                raw.as_mut_ptr(),
                std::mem::size_of::<RawSample>(),
                error.as_mut_ptr(),
                error.len(),
            )
        };
        if status != 0 {
            return Err(Error::Custom(format!("macOS RM sample: {}", text(&error))));
        }
        // SAFETY: a zero status means C initialised every field.
        let raw = unsafe { raw.assume_init() };
        if raw.abi_version != ABI_VERSION {
            return Err(Error::Custom(format!(
                "macOS RM sample ABI version {} is not {ABI_VERSION}",
                raw.abi_version
            )));
        }
        Ok(raw)
    }

    #[cfg(not(all(target_os = "macos", target_arch = "x86_64")))]
    fn raw_sample(&mut self) -> Result<RawSample, Error> {
        Err(Error::Custom(
            "macOS RM backend requires x86_64 macOS with the NullMoth driver".into(),
        ))
    }

    /// Read the private clock-offset channel: the domain directory, the
    /// per-domain offset range, and the block that carries the current values.
    ///
    /// The public thermal read: what sensors this driver exposes, their reading
    /// range, and each one's target type. Read-only, and a plain refusal (a
    /// wedged GSP answers 0x62 here like everywhere else) is reported as an
    /// error rather than as zeroes.
    #[cfg(all(target_os = "macos", target_arch = "x86_64"))]
    pub fn read_thermal(&mut self) -> Result<ThermalReadout, Error> {
        let _guard = lock_transport()?;
        let mut out = std::mem::MaybeUninit::<ThermalReadout>::uninit();
        let mut error = [0u8; 256];
        // SAFETY: C receives a correctly-sized writable buffer and either fills
        // all of it (return 0) or leaves nothing usable (non-zero), and it does
        // not retain the pointer.
        let status = unsafe {
            nvoc_macos_rm_session_thermal(
                self.handle,
                out.as_mut_ptr(),
                std::mem::size_of::<ThermalReadout>(),
                error.as_mut_ptr(),
                error.len(),
            )
        };
        if status != 0 {
            return Err(Error::Custom(format!(
                "macOS RM thermal read: {} (0x{status:08x})",
                text(&error)
            )));
        }
        // SAFETY: a zero return means C initialized the whole struct.
        Ok(unsafe { out.assume_init() })
    }

    #[cfg(not(all(target_os = "macos", target_arch = "x86_64")))]
    pub fn read_thermal(&mut self) -> Result<ThermalReadout, Error> {
        Err(Error::Custom(
            "macOS RM backend requires x86_64 macOS with the NullMoth driver".into(),
        ))
    }

    /// Read-only, so it is safe to call while the write gate is closed. Every
    /// block reports its own outcome, and the call needs a healthy GSP: while
    /// the GSP is in its reset-required state all four come back
    /// [`macos_offset::OffsetBlockRead::Refused`] with `0x62`, which must not
    /// be rendered as a value.
    #[cfg(all(target_os = "macos", target_arch = "x86_64"))]
    pub fn read_offset_channel(&mut self) -> Result<macos_offset::OffsetChannelSnapshot, Error> {
        let _guard = lock_transport()?;
        Ok(macos_offset::OffsetChannelSnapshot {
            directory: read_offset_block(
                self.handle,
                OffsetBlock::Directory,
                macos_offset::parse_directory,
            ),
            range: read_offset_block(self.handle, OffsetBlock::Range, macos_offset::parse_range),
            control: read_offset_block(
                self.handle,
                OffsetBlock::Control,
                macos_offset::parse_control,
            ),
            clock_document: read_offset_block(
                self.handle,
                OffsetBlock::ClockDocument,
                macos_offset::parse_clock_document,
            ),
        })
    }

    #[cfg(not(all(target_os = "macos", target_arch = "x86_64")))]
    pub fn read_offset_channel(&mut self) -> Result<macos_offset::OffsetChannelSnapshot, Error> {
        Err(Error::Custom(
            "macOS RM backend requires x86_64 macOS with the NullMoth driver".into(),
        ))
    }

    /// Best-effort wait until every polled section advanced past the
    /// mapping-time baseline, so the first emitted sample is known new. A
    /// section the driver reports as unavailable settles at once: it is not
    /// "about to publish", it simply has no data on this platform. The
    /// trackers are deliberately left untouched: the first real `sample()`
    /// re-observes the same advancement and reports it as `advancing`.
    #[cfg(all(target_os = "macos", target_arch = "x86_64"))]
    fn wait_initial_settled(
        &mut self,
        budget: Duration,
        cancelled: &dyn Fn() -> bool,
    ) -> [bool; RUSD_FIELDS] {
        let mut settled = [false; RUSD_FIELDS];
        let deadline = Instant::now() + budget;
        loop {
            if cancelled() {
                return settled;
            }
            match self.raw_sample() {
                Ok(raw) => {
                    for (index, seen) in settled.iter_mut().enumerate() {
                        // Either the driver has no data for this section at all,
                        // or it published a timestamp this session has not seen
                        // yet. Both mean the section is settled for this open.
                        let unavailable = raw.field_status[index] == FIELD_UNAVAILABLE;
                        let advanced = raw.field_status[index] == FIELD_OK
                            && raw.timestamps[index] != 0
                            && raw.timestamps[index] != self.trackers[index].ts;
                        if unavailable || advanced {
                            *seen = true;
                        }
                    }
                }
                // A session-level failure is reported by the first real sample
                // and handled by the caller's reconnect path; do not mask it.
                Err(_) => return settled,
            }
            if settled.iter().all(|seen| *seen) || Instant::now() >= deadline {
                return settled;
            }
            std::thread::sleep(Duration::from_millis(50));
        }
    }
}

impl Drop for MacosRmSession {
    #[cfg(all(target_os = "macos", target_arch = "x86_64"))]
    fn drop(&mut self) {
        if self.handle.is_null() {
            return;
        }
        let mut error = [0u8; 256];
        // Recover from a poisoned lock so the native resources are still
        // released; leaking the mapping is worse than ignoring poisoning.
        let _guard = transport().lock().unwrap_or_else(|err| err.into_inner());
        // SAFETY: the handle came from a successful open and is closed once.
        unsafe { nvoc_macos_rm_session_close(self.handle, error.as_mut_ptr(), error.len()) };
        self.handle = std::ptr::null_mut();
    }

    #[cfg(not(all(target_os = "macos", target_arch = "x86_64")))]
    fn drop(&mut self) {}
}

#[cfg(test)]
mod tests {
    use super::*;

    fn snapshot() -> Snapshot {
        // SAFETY: private ABI consists exclusively of integers and byte arrays.
        unsafe { std::mem::zeroed() }
    }

    /// The settings projection may only carry what the RM session reports, and
    /// every family it cannot read must be named in `unavailable` with writes
    /// and restore reported as unavailable.
    #[test]
    fn settings_projection_reports_reads_and_gaps() {
        let info = DeviceInfo {
            name: Some("NVIDIA GeForce GTX 1650 SUPER".into()),
            driver_version: "610.57.04".into(),
            rm_gpu_id: 0x100,
            pci_address: "0000:01:00.0".into(),
            vendor_id: 0x10DE,
            device_id: 0x2187,
            subsystem_id: Some(0x4018_0062),
            revision: Some(0xA1),
            board_id: Some(376),
            chip_sku: Some("250".into()),
            project: Some("G166".into()),
            memory_total_bytes: Some(4_294_967_296),
            memory_usable_bytes: Some(4_075_880_448),
            bar1_bytes: Some(268_435_456),
            l2_cache_bytes: Some(1_048_576),
            supported_class_count: Some(95),
            unavailable: BTreeMap::new(),
        };
        let mut status = DeviceStatus {
            pstate: Some(8),
            graphics_clock_khz: Some(1_530_000),
            memory_clock_khz: Some(6_000_984),
            video_clock_khz: Some(1_410_000),
            sm_clock_khz: None,
            graphics_target_clock_mhz: Some(1530),
            memory_target_clock_mhz: Some(6000),
            video_target_clock_mhz: Some(1410),
            sm_target_clock_mhz: None,
            temperature_c: Some(44.125),
            temperature_memory_c: None,
            temperature_board_c: None,
            temperature_power_supply_c: None,
            power_mw: Some(11_559),
            power_limit_mw: Some(100_000),
            power_limit_requested_mw: Some(100_000),
            average_power_mw: None,
            power_policy_tgp_mw: Some(11_426),
            throttle_reason_mask: Some(0),
            throttle_reasons: Vec::new(),
            fan_rpm: Some(0),
            fan_rpms: Vec::new(),
            fan_coolers: Vec::new(),
            utilization_percent: Some(0),
            memory_utilization_percent: Some(0),
            engine_utilization_percent: BTreeMap::new(),
            pcie_link_gen: Some(3),
            pcie_max_link_gen: Some(3),
            memory_used_bytes: Some(209_649_664),
            rusd_timestamps: BTreeMap::new(),
            unavailable: BTreeMap::new(),
        };
        status.pstate = Some(1);

        let settings = settings_from(&info, &status);
        assert_eq!(settings.pstate, Some(1));
        assert_eq!(settings.power_limit_mw, Some(100_000));
        assert_eq!(settings.memory_total_bytes, Some(4_294_967_296));
        assert_eq!(settings.memory_used_bytes, Some(209_649_664));
        assert_eq!(settings.board_id, Some(376));
        assert_eq!(settings.chip_sku.as_deref(), Some("250"));
        assert!(settings.capabilities.settings_read);
        assert!(!settings.capabilities.settings_write);
        assert!(!settings.capabilities.settings_restore);
        for family in [
            "core_clock_offset",
            "memory_clock_offset",
            "voltage_boost",
            "thermal_limit",
            "fan_control",
            "pstate_lock",
            "vf_curve",
            "power_limit_write",
            "settings_restore",
        ] {
            assert!(
                settings.unavailable.contains_key(family),
                "missing gap entry for {family}"
            );
        }
        // The gap reason must point at the recorded probe evidence.
        assert!(
            settings
                .notes
                .iter()
                .any(|note| note.contains("0x2080B201"))
        );
        assert!(settings.notes.iter().any(|note| note.contains("0x1f")));
    }

    /// An unread or unauthorised family must not be reported as an absent one.
    ///
    /// The first real `get-settings --macos-rm` run printed one shared sentence,
    /// "no readable RM control for this family on this driver", for every
    /// family -- including ones whose control the driver demonstrably
    /// dispatches. This test is the reason that cannot come back: each family
    /// names its own cause, the write families share the privilege one, and none
    /// of them is the retired blanket claim.
    #[test]
    fn every_gap_names_its_own_cause_rather_than_a_missing_control() {
        let info = DeviceInfo {
            pci_address: "0000:01:00.0".to_string(),
            driver_version: "610.57.04".to_string(),
            ..Default::default()
        };
        let settings = settings_from(&info, &DeviceStatus::default());

        let retired = "no readable RM control for this family on this driver";
        for (family, reason) in &settings.unavailable {
            assert!(!reason.is_empty(), "{family} has an empty reason");
            assert!(
                reason.ends_with("(see notes)"),
                "{family} must point at the recorded evidence: {reason}"
            );
            assert!(
                !reason.contains(retired),
                "{family} still claims the control is missing: {reason}"
            );
        }

        // The families that only need a *write* share the privilege cause.
        for family in ["pstate_lock", "settings_restore"] {
            assert_eq!(
                settings.unavailable[family], WRITE_NEEDS_PRIVILEGE_REASON,
                "{family} should give the privilege cause"
            );
        }
        // The power-limit write has the privilege cause plus a precondition of
        // its own, so it must not simply borrow the shared string.
        assert_eq!(
            settings.unavailable["power_limit_write"], POWER_WRITE_NEEDS_PRIVILEGE_REASON,
            "power_limit_write should state its own documented precondition"
        );
        assert_ne!(
            POWER_WRITE_NEEDS_PRIVILEGE_REASON, WRITE_NEEDS_PRIVILEGE_REASON,
            "the power write must not collapse into the shared cause"
        );
        // And the ones with a real read gap do not borrow it.
        for family in [
            "core_clock_offset",
            "memory_clock_offset",
            "fan_control",
            "vf_curve",
            "thermal_limit",
            "voltage_boost",
        ] {
            assert_ne!(
                settings.unavailable[family], WRITE_NEEDS_PRIVILEGE_REASON,
                "{family} needs a write? then say so; otherwise name its gap"
            );
        }
        assert_eq!(
            settings.unavailable["fan_control"],
            FAN_CONTROL_UNREADABLE_REASON
        );
        assert_eq!(settings.unavailable["vf_curve"], VF_CURVE_UNREADABLE_REASON);
    }

    /// The driver's own bytes, as captured by the probe: the same fixtures the
    /// parser test uses, so the numbers below are measured rather than restated.
    const OFFSET_RANGE_FIXTURE: &[u8] = include_bytes!("../tests/data/macos-offset-range.bin");

    /// The directory bytes from the same capture.
    const OFFSET_DIRECTORY_FIXTURE: &[u8] =
        include_bytes!("../tests/data/macos-offset-directory.bin");

    /// The clock document from the same capture: the one block that publishes an
    /// offset rather than a window or a set of unlabelled dwords.
    const CLK_DOCUMENT_FIXTURE: &[u8] = include_bytes!("../tests/data/macos-clkdoc-776.bin");

    /// A snapshot that varies only the directory block; the other two keep the
    /// status the live wedge read returned for them.
    fn directory_snapshot(
        directory: macos_offset::OffsetBlockRead<macos_offset::OffsetDirectory>,
    ) -> macos_offset::OffsetChannelSnapshot {
        macos_offset::OffsetChannelSnapshot {
            directory,
            range: macos_offset::OffsetBlockRead::Refused(0x62),
            control: macos_offset::OffsetBlockRead::Refused(0x62),
            clock_document: macos_offset::OffsetBlockRead::Refused(0x62),
        }
    }

    fn offset_snapshot(
        range: macos_offset::OffsetBlockRead<macos_offset::OffsetRange>,
    ) -> macos_offset::OffsetChannelSnapshot {
        // Only the range block is varied: the other two are carried so the note
        // can report their statuses, which is the point of reporting them.
        macos_offset::OffsetChannelSnapshot {
            directory: macos_offset::OffsetBlockRead::Refused(0x62),
            range,
            control: macos_offset::OffsetBlockRead::Parsed(
                macos_offset::parse_control(&[0u8; macos_offset::OFFSET_CONTROL_BYTES])
                    .expect("a zeroed block is a well-formed control block"),
            ),
            clock_document: macos_offset::OffsetBlockRead::Refused(0x62),
        }
    }

    /// A projection with no window, as `settings_from` leaves it before any
    /// channel read, so the tests start from the state a caller sees when the
    /// block was never requested.
    /// The clock document is the one block that publishes an offset, so it is the
    /// one that fills the structured field, and it does so from the captured
    /// payload rather than from one built to match the parser.
    #[test]
    fn the_clock_document_fills_the_offsets_and_a_refusal_leaves_them_absent() {
        use macos_offset::OffsetBlockRead::{Parsed, Refused};

        let document = macos_offset::parse_clock_document(CLK_DOCUMENT_FIXTURE)
            .expect("the captured document parses at its declared geometry");
        let mut settings = bare_settings();
        apply_offset_channel(
            &mut settings,
            &macos_offset::OffsetChannelSnapshot {
                directory: Refused(0x62),
                range: Refused(0x62),
                control: Refused(0x62),
                clock_document: Parsed(document),
            },
        );

        let offsets = settings
            .clock_offsets
            .expect("the document carried offsets");
        assert_eq!(offsets.len(), 11);
        // The two that can be checked against something else: the graphics clock
        // RUSD reported in the same session, and the bit the in-tree NVAPI copy
        // names MCLK, whose frequency is that session's live memory clock.
        assert_eq!(
            offsets
                .iter()
                .find(|offset| offset.domain_bit == 1)
                .map(|offset| (offset.frequency_khz, offset.offset_khz)),
            Some((1_530_000, -1))
        );
        assert_eq!(
            offsets
                .iter()
                .find(|offset| offset.domain_bit == 0x10)
                .map(|offset| (offset.frequency_khz, offset.offset_khz)),
            Some((6_000_984, -126))
        );

        // A refused document must leave the field absent rather than empty, or
        // "the offset was not read" would look like "no offset is applied" --
        // the distinction the whole channel exists to keep.
        let mut wedged = bare_settings();
        apply_offset_channel(
            &mut wedged,
            &macos_offset::OffsetChannelSnapshot {
                directory: Refused(0x62),
                range: Refused(0x62),
                control: Refused(0x62),
                clock_document: Refused(0x62),
            },
        );
        assert_eq!(wedged.clock_offsets, None);
        assert!(
            wedged
                .notes
                .iter()
                .any(|note| note.contains("0x20809061") && note.contains("0x00000062")),
            "the note names the block and its refusal: {:?}",
            wedged.notes
        );
    }

    fn offsets(entries: &[(u32, u32, i32)]) -> Option<Vec<ClockOffsetReading>> {
        Some(
            entries
                .iter()
                .map(
                    |(domain_bit, frequency_khz, offset_khz)| ClockOffsetReading {
                        domain_bit: *domain_bit,
                        frequency_khz: *frequency_khz,
                        offset_khz: *offset_khz,
                    },
                )
                .collect(),
        )
    }

    #[test]
    fn a_domain_is_named_only_by_an_exact_unique_live_clock_match() {
        let mut settings = bare_settings();
        settings.clock_offsets = offsets(&[
            (1, 1_575_000, -1000),
            (0x10, 7_500_000, 250),
            (4, 999_000, 0),
        ]);
        let status = DeviceStatus {
            graphics_clock_khz: Some(1_575_000),
            memory_clock_khz: Some(7_500_000),
            // A live clock no record published: the domain must stay unnamed, and
            // the note has to say which comparison failed rather than go quiet.
            video_clock_khz: Some(1_111_000),
            ..Default::default()
        };
        corroborate_named_offsets(&mut settings, &status);

        let named = settings.clock_offsets_named.as_ref().expect("channel read");
        assert_eq!(
            named
                .iter()
                .map(|entry| (entry.name.as_str(), entry.domain_bit, entry.offset_khz))
                .collect::<Vec<_>>(),
            vec![("graphics", 1, -1000), ("memory", 0x10, 250)]
        );
        // The offset is the document's own field; the live clock only decided
        // which record the name refers to, and the evidence says so.
        let graphics = settings.named_clock_offset("graphics").unwrap();
        assert_eq!(graphics.frequency_khz, 1_575_000);
        assert!(
            graphics.corroboration.contains("1575000 kHz")
                && graphics.corroboration.contains("only one of the 3 records"),
            "{}",
            graphics.corroboration
        );
        // A domain no record matched keeps its bit and gets no name.
        assert!(settings.named_clock_offset("video").is_err());
        assert!(
            settings
                .notes
                .iter()
                .any(|note| note.contains("video stays unnamed")),
            "{:?}",
            settings.notes
        );
    }

    #[test]
    fn a_tie_leaves_the_domain_unnamed_rather_than_picking_one() {
        let mut settings = bare_settings();
        settings.clock_offsets = offsets(&[(1, 1_575_000, -1000), (2, 1_575_000, 500)]);
        let status = DeviceStatus {
            graphics_clock_khz: Some(1_575_000),
            memory_clock_khz: Some(7_500_000),
            ..Default::default()
        };
        corroborate_named_offsets(&mut settings, &status);

        let named = settings.clock_offsets_named.as_ref().expect("channel read");
        assert!(
            named.iter().all(|entry| entry.name != "graphics"),
            "a tie must name neither record: {named:?}"
        );
        assert!(
            settings
                .notes
                .iter()
                .any(|note| note.contains("would be a choice rather than a measurement")),
            "{:?}",
            settings.notes
        );
        let reason = settings.named_clock_offset("graphics").unwrap_err();
        assert!(
            reason.contains("not named") && reason.contains("clock_offsets"),
            "{reason}"
        );
    }

    #[test]
    fn an_unread_channel_reports_the_missing_read_not_a_zero() {
        let settings = bare_settings();
        assert_eq!(settings.clock_offsets_named, None);
        let reason = settings.named_clock_offset("memory").unwrap_err();
        assert!(
            reason.contains("was not read") && reason.contains("memory"),
            "{reason}"
        );
    }

    #[test]
    fn two_names_may_match_one_record_and_both_are_reported() {
        // Idle Turing parts commonly report the same number for the graphics and
        // SM clocks. The rule must not silently pick one of the two names: both
        // comparisons did hold, so both names are reported with the reason.
        let mut settings = bare_settings();
        settings.clock_offsets = offsets(&[(1, 300_000, -500)]);
        let status = DeviceStatus {
            graphics_clock_khz: Some(300_000),
            sm_clock_khz: Some(300_000),
            ..Default::default()
        };
        corroborate_named_offsets(&mut settings, &status);

        let named = settings.clock_offsets_named.as_ref().expect("channel read");
        assert_eq!(
            named
                .iter()
                .map(|entry| entry.name.as_str())
                .collect::<Vec<_>>(),
            vec!["graphics", "processor"]
        );
        assert!(
            settings
                .notes
                .iter()
                .any(|note| note.contains("answers to both graphics and processor")),
            "{:?}",
            settings.notes
        );
    }

    /// The settings a plan is computed from: two records, one of them named by
    /// the same-snapshot comparison, plus the window the range block reported.
    fn planned_settings() -> DeviceSettings {
        let mut settings = bare_settings();
        settings.clock_offsets = offsets(&[(1, 1_575_000, -126), (0x10, 6_000_984, -126)]);
        settings.clock_offset_limits_khz = Some(ClockOffsetLimits {
            min_khz: -18_750,
            max_khz: 18_750,
            records: 3,
        });
        corroborate_named_offsets(
            &mut settings,
            &DeviceStatus {
                graphics_clock_khz: Some(1_575_000),
                memory_clock_khz: Some(6_000_984),
                ..Default::default()
            },
        );
        settings
    }

    #[test]
    fn a_planned_step_is_ready_only_inside_the_window_the_driver_reported() {
        let settings = planned_settings();
        let step = plan_offset_step(&settings, &OffsetStepTarget::Domain("memory".into()), 8_000);
        assert_eq!(step.verdict, OffsetStepVerdict::Ready);
        assert_eq!(step.domain_bit, Some(0x10));
        assert_eq!(step.domain.as_deref(), Some("memory"));
        assert_eq!(step.current_offset_khz, Some(-126));
        assert_eq!(step.target_offset_khz, Some(7_874));
        assert!(
            step.identity.contains("same-snapshot comparison"),
            "{}",
            step.identity
        );
        assert!(
            step.detail.contains("inside the reported window")
                && step.detail.contains("Nothing has been written"),
            "{}",
            step.detail
        );

        // The same request, big enough to leave the window, is refused with the
        // numbers that decided it.
        let out = plan_offset_step(
            &settings,
            &OffsetStepTarget::Domain("memory".into()),
            -20_000,
        );
        assert_eq!(out.verdict, OffsetStepVerdict::OutsideWindow);
        assert!(
            out.detail.contains("-20126 kHz") && out.detail.contains("-18750"),
            "{}",
            out.detail
        );
    }

    #[test]
    fn a_planned_step_names_the_layer_it_could_not_resolve() {
        let settings = planned_settings();
        // A device bit the document published no record for.
        let absent = plan_offset_step(&settings, &OffsetStepTarget::DomainBit(9), 1_000);
        assert_eq!(absent.verdict, OffsetStepVerdict::RecordAbsent);
        assert!(absent.detail.contains("2 records"), "{}", absent.detail);
        // A public name no live clock corroborated.
        let unnamed = plan_offset_step(&settings, &OffsetStepTarget::Domain("video".into()), 1_000);
        assert_eq!(unnamed.verdict, OffsetStepVerdict::DomainUnnamed);
        assert!(unnamed.detail.contains("not named"), "{}", unnamed.detail);
        // A channel that was never read.
        assert_eq!(
            plan_offset_step(&bare_settings(), &OffsetStepTarget::DomainBit(1), 1_000).verdict,
            OffsetStepVerdict::ChannelUnread
        );
        // A record that exists, but no reported window to bound a step with.
        let mut windowless = planned_settings();
        windowless.clock_offset_limits_khz = None;
        let unbounded = plan_offset_step(&windowless, &OffsetStepTarget::DomainBit(1), 1_000);
        assert_eq!(unbounded.verdict, OffsetStepVerdict::WindowUnknown);
        assert!(
            unbounded.detail.contains("0x20809025"),
            "{}",
            unbounded.detail
        );
    }

    #[test]
    fn a_device_bit_needs_no_name_and_reports_that_it_had_none() {
        // The bit form is the one that works before any naming, so it must not
        // borrow a name from elsewhere -- and it must say it had none.
        let mut settings = bare_settings();
        settings.clock_offsets = offsets(&[(4, 999_000, 300)]);
        settings.clock_offset_limits_khz = Some(ClockOffsetLimits {
            min_khz: -18_750,
            max_khz: 18_750,
            records: 3,
        });
        corroborate_named_offsets(&mut settings, &DeviceStatus::default());
        let step = plan_offset_step(&settings, &OffsetStepTarget::DomainBit(4), -1_000);
        assert_eq!(step.verdict, OffsetStepVerdict::Ready);
        assert_eq!(step.domain, None);
        assert_eq!(step.target_offset_khz, Some(-700));
        assert!(
            step.identity.contains("no public name was corroborated"),
            "{}",
            step.identity
        );
    }

    #[test]
    fn the_request_grammar_is_shared_and_rejects_typos() {
        assert_eq!(
            parse_offset_target("graphics"),
            Ok(OffsetStepTarget::Domain("graphics".into()))
        );
        assert_eq!(
            parse_offset_target(" BIT:0X10 "),
            Ok(OffsetStepTarget::DomainBit(0x10))
        );
        assert_eq!(
            parse_offset_target("bit:16"),
            Ok(OffsetStepTarget::DomainBit(16))
        );
        // A typo must not arrive as "unnamed": it is refused, and the message
        // says both accepted forms.
        let typo = parse_offset_target("grphics").unwrap_err();
        assert!(typo.contains("graphics") && typo.contains("bit:"), "{typo}");
        assert!(parse_offset_target("bit:").is_err());

        assert_eq!(parse_offset_delta("+8mhz"), Ok(8_000));
        assert_eq!(parse_offset_delta("-2000"), Ok(-2_000));
        assert_eq!(parse_offset_delta("1500khz"), Ok(1_500));
        let bad = parse_offset_delta("fast").unwrap_err();
        assert!(bad.contains("kHz"), "{bad}");
        assert!(parse_offset_delta("99999999999").is_err());
    }

    fn bare_settings() -> DeviceSettings {
        DeviceSettings {
            pstate: None,
            power_limit_mw: None,
            power_limit_requested_mw: None,
            pcie_link_gen: None,
            memory_total_bytes: None,
            memory_used_bytes: None,
            board_id: None,
            chip_sku: None,
            project: None,
            clock_offset_limits_khz: None,
            clock_offset_domains: None,
            clock_offsets: None,
            clock_offsets_named: None,
            unavailable: BTreeMap::new(),
            capabilities: SettingsCapabilities {
                settings_read: true,
                settings_write: false,
                settings_restore: false,
            },
            notes: Vec::new(),
        }
    }

    #[test]
    fn the_domain_directory_is_the_devices_own_list() {
        let mut settings = bare_settings();
        let directory =
            macos_offset::parse_directory(OFFSET_DIRECTORY_FIXTURE).expect("committed fixture");
        apply_offset_channel(
            &mut settings,
            &directory_snapshot(macos_offset::OffsetBlockRead::Parsed(directory)),
        );
        let domains = settings
            .clock_offset_domains
            .expect("the directory was read, so the domains are listed");
        // The recorded directory: ten entries, and the stamped field is a bit
        // *position*, not an index -- entry 2 carries bit 4, which is exactly
        // why no domain name is claimed for any of them.
        assert_eq!(domains.len(), 10);
        assert_eq!(
            domains.iter().map(|d| d.index).collect::<Vec<_>>(),
            (0..10).collect::<Vec<_>>()
        );
        assert_eq!(
            domains.iter().map(|d| d.domain_bit).collect::<Vec<_>>(),
            vec![1, 2, 16, 4, 8, 1_048_576, 524_288, 64, 2_097_152, 32]
        );
        // Three entries carry 0x0808, six 0x0909 and one the stamp-only 0x0303
        // at index 6 -- the entry the mask sweep identified by behaviour.
        assert_eq!(domains.iter().filter(|d| d.type_stamp == 0x0808).count(), 3);
        assert_eq!(domains[6].type_stamp, 0x0303);
        assert_eq!(domains[6].index, 6);
        // A directory is not a range: reading it must not invent a window.
        assert_eq!(settings.clock_offset_limits_khz, None);
    }

    #[test]
    fn a_refused_directory_is_not_an_empty_domain_list() {
        let mut settings = bare_settings();
        apply_offset_channel(
            &mut settings,
            &directory_snapshot(macos_offset::OffsetBlockRead::Refused(0x62)),
        );
        assert_eq!(settings.clock_offset_domains, None);
        let note = settings.notes.join("\n");
        assert!(note.contains("directory 0x20809019: refused"), "{note}");
        assert!(note.contains("no domain is listed"), "{note}");
    }

    #[test]
    fn the_offset_window_is_the_devices_not_a_constant() {
        let mut settings = bare_settings();
        let range = macos_offset::parse_range(OFFSET_RANGE_FIXTURE).expect("committed fixture");
        apply_offset_channel(
            &mut settings,
            &offset_snapshot(macos_offset::OffsetBlockRead::Parsed(range)),
        );
        // The recorded block: three records, all -18750..+18750 kHz.
        assert_eq!(
            settings.clock_offset_limits_khz,
            Some(ClockOffsetLimits {
                min_khz: -18_750,
                max_khz: 18_750,
                records: 3,
            })
        );
        let note = settings.notes.join("\n");
        assert!(note.contains("range 0x20809025: NV_OK"), "{note}");
        assert!(note.contains("control 0x2080901B: NV_OK"), "{note}");
        // The same reading states why the *offset* is still not a number: the
        // window is a limit, and reporting it must not quietly promote the
        // offset families out of `unavailable`.
        assert!(!settings.notes.is_empty());
    }

    #[test]
    fn a_refused_range_block_is_never_a_zero_window() {
        let mut settings = bare_settings();
        apply_offset_channel(
            &mut settings,
            &offset_snapshot(macos_offset::OffsetBlockRead::Refused(0x62)),
        );
        assert_eq!(settings.clock_offset_limits_khz, None);
        let note = settings.notes.join("\n");
        assert!(
            note.contains("refused 0x0000_0062") || note.contains("refused 0x00000062"),
            "{note}"
        );
        assert!(note.contains("no window is reported"), "{note}");
    }

    /// The entry's numbers are one read: current value, the span the driver
    /// declares, and the value a restore would have to write back.
    ///
    /// The span here comes through the production path (`apply_offset_channel`
    /// parsing a three-record range block), not from a hand-set field, so a
    /// regression in that parsing shows up as an entry that lost its window. The
    /// name still comes from the same-snapshot comparison, because no name is
    /// invented for a bit the comparison did not corroborate.
    #[test]
    fn the_entry_view_carries_the_window_and_the_restore_target() {
        let range = macos_offset::OffsetRange {
            head: Vec::new(),
            records: (0..3)
                .map(|index| macos_offset::OffsetRangeRecord {
                    index,
                    tag: 0x0100_0101 + index as u32,
                    max_vf_khz: 1_638_400,
                    min_khz: -18_750,
                    max_khz: 18_750,
                })
                .collect(),
        };
        let mut settings = bare_settings();
        apply_offset_channel(
            &mut settings,
            &offset_snapshot(macos_offset::OffsetBlockRead::Parsed(range)),
        );
        settings.clock_offsets = offsets(&[(1, 1_575_000, -1)]);
        corroborate_named_offsets(
            &mut settings,
            &DeviceStatus {
                graphics_clock_khz: Some(1_575_000),
                ..Default::default()
            },
        );

        let view = settings.clock_offset_view("graphics").expect("named");
        assert_eq!(view.domain_bit, 1);
        assert_eq!((view.frequency_khz, view.offset_khz), (1_575_000, -1));
        assert_eq!(
            (view.window_min_khz, view.window_max_khz),
            (Some(-18_750), Some(18_750))
        );
        // The accessor a caller without a domain name uses agrees with the view.
        assert_eq!(settings.window_absence_reason(), None);
        assert_eq!(view.window_records, Some(3));
        assert_eq!(view.window_reason, None);
        // The restore target is the value read before any write, so it is the
        // current offset and not a caller-supplied number.
        assert_eq!(view.restore_khz, view.offset_khz);
        assert!(
            view.corroboration.contains("1575000 kHz"),
            "{}",
            view.corroboration
        );
    }

    /// No span read is not a span of zero, and an unnamed domain is refused at
    /// the entry for the same reason the backend refuses it.
    #[test]
    fn an_unread_window_is_absent_with_the_blocks_own_reason() {
        let mut settings = bare_settings();
        apply_offset_channel(
            &mut settings,
            &offset_snapshot(macos_offset::OffsetBlockRead::Refused(0x62)),
        );
        settings.clock_offsets = offsets(&[(1, 1_575_000, -1)]);
        corroborate_named_offsets(
            &mut settings,
            &DeviceStatus {
                graphics_clock_khz: Some(1_575_000),
                ..Default::default()
            },
        );

        let view = settings.clock_offset_view("graphics").expect("named");
        assert_eq!((view.window_min_khz, view.window_max_khz), (None, None));
        assert_eq!(view.window_records, None);
        // A caller reporting the whole projection has no domain to ask with, so
        // the reason must also be reachable without one -- and it is the same
        // text, not a paraphrase.
        assert_eq!(
            settings.window_absence_reason().as_deref(),
            view.window_reason.as_deref(),
            "the projection-wide reason and the view's must be the same words"
        );
        let reason = view.window_reason.expect("an absent span states its cause");
        assert!(reason.contains("no window is reported"), "{reason}");
        assert!(reason.contains("refused"), "{reason}");

        // The domain itself is still reported: losing the window must not lose
        // the offset, or the two reads would collapse into one failure.
        assert_eq!(view.offset_khz, -1);

        // A domain the comparison did not corroborate is an error here, with the
        // reason that says which comparison failed -- never a zero.
        let error = settings.clock_offset_view("video").unwrap_err();
        assert!(error.contains("video"), "{error}");
    }

    #[test]
    fn records_that_disagree_do_not_become_one_window() {
        let mut settings = bare_settings();
        // Two records with different ceilings: reporting either as "the" window
        // would attribute it to a domain the backend has not identified.
        let range = macos_offset::OffsetRange {
            head: Vec::new(),
            records: vec![
                macos_offset::OffsetRangeRecord {
                    index: 0,
                    tag: 0x0103_0101,
                    max_vf_khz: 1_638_400,
                    min_khz: -18_750,
                    max_khz: 18_750,
                },
                macos_offset::OffsetRangeRecord {
                    index: 1,
                    tag: 0x0104_0101,
                    max_vf_khz: 1_638_400,
                    min_khz: -18_750,
                    max_khz: 12_000,
                },
            ],
        };
        apply_offset_channel(
            &mut settings,
            &offset_snapshot(macos_offset::OffsetBlockRead::Parsed(range)),
        );
        assert_eq!(settings.clock_offset_limits_khz, None);
        let note = settings.notes.join("\n");
        assert!(note.contains("disagree"), "{note}");

        // And an empty selection is its own reason, not a zero-width window.
        let empty = macos_offset::OffsetRange {
            head: Vec::new(),
            records: Vec::new(),
        };
        let mut settings = bare_settings();
        apply_offset_channel(
            &mut settings,
            &offset_snapshot(macos_offset::OffsetBlockRead::Parsed(empty)),
        );
        assert_eq!(settings.clock_offset_limits_khz, None);
        assert!(
            settings
                .notes
                .join("\n")
                .contains("no range record was selected"),
            "{:?}",
            settings.notes
        );
    }

    /// A raw sample in which every section is readable and carries the same
    /// stamps. Tests move the stamps to model advancing, quiet and resetting
    /// sections, and overwrite values to model missing or implausible readings.
    fn raw_sample() -> RawSample {
        RawSample {
            abi_version: ABI_VERSION,
            timestamps: [1_000; RUSD_FIELDS],
            clk_target_mhz: [1_530, 6_000, 1_410, 1_800],
            clk_actual_khz: [1_530_000, 6_000_984, 1_410_000, 1_800_000],
            temperature_fx256: [12_800, 12_544, 11_776, 11_520], // 50/49/46/45 C
            power_mw: 11_412,
            power_limit_requested_mw: 100_000,
            power_limit_mw: 100_000,
            average_power_mw: 11_000,
            power_policy_tgp_mw: 11_500,
            throttle_reason_mask: 0,
            fan_rpm: [1_200, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0],
            utilization_percent: 7,
            memory_utilization_percent: 3,
            engine_utilization_pct100: [700, 300, 0, 0, 0, 0],
            pci_gen_info: 0x0022_2000, // ASPM L0S+L1, gen/curr/gpu_gen = Gen3
            pstate: 1,
            memory_used_kb: 200_000,
            ..RawSample::default()
        }
    }

    /// Fields this sample marked as new, in BTreeMap (sorted) order.
    fn updated_fields(sample: &DeviceSample) -> Vec<&str> {
        sample
            .updated
            .iter()
            .filter(|(_, updated)| **updated)
            .map(|(name, _)| name.as_str())
            .collect()
    }

    /// Trackers seeded the way session_open() seeds them: with the mapping-time
    /// watermark the first sample must advance past.
    fn trackers_at(baseline: u64) -> [FieldTracker; RUSD_FIELDS] {
        [FieldTracker {
            ts: baseline,
            changed_ns: 0,
            seen_change: false,
        }; RUSD_FIELDS]
    }

    #[test]
    fn typed_queries_reject_targets_without_rm_before_native_access() {
        let target = crate::GpuTarget::without_backends(GpuId(0x100), 0);
        assert!(!target.has_macos_rm());
        let error = crate::run(&target, QueryMacosRmStatus).unwrap_err();
        assert!(error.to_string().contains("has no macOS RM backend"));
        let error = crate::run(&target, QueryMacosRmInfo).unwrap_err();
        assert!(error.to_string().contains("has no macOS RM backend"));
    }

    #[test]
    fn missing_data_serializes_as_null_while_valid_p0_is_preserved() {
        let mut s = snapshot();
        let value = serde_json::to_value(status_from_snapshot(&s)).unwrap();
        // A zeroed section table means "nothing was read", never "0 MHz".
        assert_eq!(value["pstate"], 0);
        assert!(value["graphics_clock_khz"].is_null());
        assert!(value["temperature_c"].is_null());
        assert!(value["unavailable"]["power_mw"].is_string());
        s.telemetry.sample.pstate_status = 0x56;
        let value = serde_json::to_value(status_from_snapshot(&s)).unwrap();
        assert!(value["pstate"].is_null());
        assert!(
            value["unavailable"]["pstate"]
                .as_str()
                .unwrap()
                .contains("00000056")
        );
    }

    #[test]
    fn telemetry_keeps_zero_readings_and_converts_signed_fixed_point_temperature() {
        let mut s = snapshot();
        s.telemetry.sample = raw_sample();
        s.telemetry.sample.temperature_fx256[0] = -128; // -0.5 C
        let status = status_from_snapshot(&s);
        assert_eq!(status.graphics_clock_khz, Some(1_530_000));
        assert_eq!(status.memory_clock_khz, Some(6_000_984));
        assert_eq!(status.video_clock_khz, Some(1_410_000));
        assert_eq!(status.sm_clock_khz, Some(1_800_000));
        assert_eq!(status.temperature_c, Some(-0.5));
        assert_eq!(status.temperature_memory_c, Some(49.0));
        assert_eq!(status.power_mw, Some(11_412));
        assert_eq!(status.fan_rpm, Some(1_200));
        assert_eq!(status.utilization_percent, Some(7));
        assert_eq!(status.memory_used_bytes, Some(200_000 * 1024));
        assert!(status.unavailable.is_empty());
        assert_eq!(status.rusd_timestamps["temperature_c"], 1_000);
    }

    #[test]
    fn zero_valued_and_implausible_fields_are_reported_not_measured() {
        let mut s = snapshot();
        s.telemetry.sample = raw_sample();
        // SM/board clocks a driver does not implement read back as 0/0.
        s.telemetry.sample.clk_target_mhz[3] = 0;
        s.telemetry.sample.clk_actual_khz[3] = 0;
        // A fan slot the driver never populated reads as an absurd rpm count.
        s.telemetry.sample.fan_rpm = [294_118, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0];
        // An impossible temperature and an over-100 utilization are rejected.
        s.telemetry.sample.temperature_fx256[1] = 1_000 * 256;
        s.telemetry.sample.utilization_percent = 101;
        let status = status_from_snapshot(&s);
        assert!(status.sm_clock_khz.is_none());
        assert!(status.sm_target_clock_mhz.is_none());
        assert!(
            status.unavailable["sm_clock_khz"].contains("reports no value"),
            "{:?}",
            status.unavailable
        );
        assert!(status.fan_rpm.is_none());
        assert!(status.fan_rpms.is_empty());
        assert!(status.unavailable["fan_rpm"].contains("implausible"));
        assert!(status.temperature_memory_c.is_none());
        assert!(status.unavailable["temperature_memory_c"].contains("implausible"));
        assert!(status.utilization_percent.is_none());
        assert!(status.unavailable["utilization_percent"].contains("implausible"));
        // Fields that were fine are untouched.
        assert_eq!(status.graphics_clock_khz, Some(1_530_000));
        assert_eq!(status.temperature_c, Some(50.0));
    }

    #[test]
    fn fan_readings_keep_the_cooler_slot_and_the_report_names_the_missing_fields() {
        let mut s = snapshot();
        s.telemetry.sample = raw_sample();
        // Slots 0 and 2 are turning, slot 1 is stopped or empty -- this layer
        // cannot tell those apart, so it must not renumber what is left.
        s.telemetry.sample.fan_rpm = [1_200, 0, 2_500, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0];
        let status = status_from_snapshot(&s);
        assert_eq!(status.fan_rpms, vec![1_200, 2_500]);
        assert_eq!(
            status.fan_coolers,
            vec![
                FanCoolerReading {
                    slot: 0,
                    rpm: 1_200
                },
                FanCoolerReading {
                    slot: 2,
                    rpm: 2_500
                },
            ]
        );
        let report = status.fan_report();
        assert_eq!(report.count, 2);
        assert_eq!(report.coolers, status.fan_coolers);
        assert_eq!(report.min_percent, None);
        assert_eq!(report.max_percent, None);
        assert_eq!(report.current_percent, None);
        assert_eq!(report.unavailable.len(), 3);
        for field in ["min_percent", "max_percent", "current_percent"] {
            assert_eq!(report.unavailable[field], FAN_PERCENT_UNREADABLE_REASON);
        }
        assert!(
            report.notes.iter().any(|note| note.contains("stopped fan")),
            "{:?}",
            report.notes
        );

        // An unusable fan section is "not read", which must not come out as
        // "no cooler turned".
        let mut s = snapshot();
        s.telemetry.sample = raw_sample();
        s.telemetry.sample.fan_rpm = [294_118, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0];
        let unread = status_from_snapshot(&s).fan_report();
        assert_eq!(unread.count, 0);
        assert!(
            unread.notes.iter().any(|note| note.contains("not read")),
            "{:?}",
            unread.notes
        );
        assert!(
            unread.notes.iter().all(|note| !note.contains("stopped")),
            "{:?}",
            unread.notes
        );
    }

    #[test]
    fn the_power_limit_report_gives_the_wall_and_names_the_ends_it_has_no_source_for() {
        // The healthy fixture: 100 W requested and enforced, and a policy field
        // at instantaneous scale. The two ends have no source and say so; the
        // wall is reported as the wall.
        let mut s = snapshot();
        s.telemetry.sample = raw_sample();
        let report = status_from_snapshot(&s).power_limit_report();
        assert_eq!(report.source, "macos-rm");
        assert_eq!(report.current_watt, Some(100.0));
        assert_eq!(report.requested_watt, Some(100.0));
        assert_eq!(report.policy_tgp_watt, Some(11.5));
        assert_eq!(report.min_watt, None);
        assert_eq!(report.max_watt, None);
        assert_eq!(report.unavailable.len(), 2);
        for end in ["min_watt", "max_watt"] {
            assert_eq!(report.unavailable[end], POWER_RANGE_UNREAD_REASON);
        }
        assert!(
            report
                .notes
                .iter()
                .any(|note| note.contains("尚未定位") && note.contains("policy")),
            "{:?}",
            report.notes
        );

        // A wattage the checks rejected is null with that field's own reason,
        // while its neighbours keep their numbers: one unread field must not
        // blank the family, and it must never come out as 0 W.
        let mut s = snapshot();
        s.telemetry.sample = raw_sample();
        s.telemetry.sample.power_limit_mw = MAX_POWER_MW + 1;
        let status = status_from_snapshot(&s);
        let report = status.power_limit_report();
        assert_eq!(report.current_watt, None);
        assert_ne!(report.current_watt, Some(0.0));
        assert_eq!(report.requested_watt, Some(100.0));
        assert_eq!(
            report.unavailable["current_watt"],
            status.unavailable["power_limit_mw"]
        );
        assert_eq!(report.unavailable.len(), 3);

        // A plausible zero is data, not an unread field: 0 mW comes back as
        // 0.0 W and is absent from `unavailable`.
        let mut s = snapshot();
        s.telemetry.sample = raw_sample();
        s.telemetry.sample.power_limit_mw = 0;
        let report = status_from_snapshot(&s).power_limit_report();
        assert_eq!(report.current_watt, Some(0.0));
        assert!(!report.unavailable.contains_key("current_watt"));
    }

    #[test]
    fn the_legacy_temp_view_reports_each_sensor_and_the_fields_it_cannot_fill() {
        // Healthy fixture: four plausible sensors. The readings are reported
        // with the driver's own target names and field provenance, and the
        // range/controller ends say why they are null instead of being filled
        // with a made-up bound.
        let mut s = snapshot();
        s.telemetry.sample = raw_sample();
        let views =
            status_from_snapshot(&s).legacy_temp_sensor_report(ThermalReadOutcome::NotAttempted);
        assert_eq!(views.len(), 4);
        assert_eq!(views[0].target, "gpu");
        assert_eq!(views[0].field, "temperature_c");
        assert_eq!(views[0].current_c, Some(50.0));
        assert_eq!(views[1].current_c, Some(49.0));
        assert_eq!(views[2].current_c, Some(46.0));
        assert_eq!(views[3].target, "power_supply");
        assert_eq!(views[3].current_c, Some(45.0));
        for view in &views {
            assert_eq!(view.min_c, None);
            assert_eq!(view.max_c, None);
            assert_eq!(view.controller, None);
            assert_eq!(view.unavailable["min_c"], THERMAL_RANGE_UNREAD_REASON);
            assert_eq!(view.unavailable["max_c"], THERMAL_RANGE_UNREAD_REASON);
            assert_eq!(
                view.unavailable["controller"],
                THERMAL_CONTROLLER_UNREAD_REASON
            );
            // The measured candidate is named, so "not wired in" cannot be
            // misread as "the driver publishes no bound".
            assert!(view.unavailable["min_c"].contains("0x20800513"));
            assert!(!view.unavailable.contains_key("current_c"));
        }

        // An implausible sensor is null with that field's own reason -- never
        // 0 C, which would read as a real freezing reading -- while its
        // neighbours keep their numbers.
        let mut s = snapshot();
        s.telemetry.sample = raw_sample();
        s.telemetry.sample.temperature_fx256[1] = 200 * 256;
        let status = status_from_snapshot(&s);
        let views = status.legacy_temp_sensor_report(ThermalReadOutcome::NotAttempted);
        assert_eq!(views[1].current_c, None);
        assert_ne!(views[1].current_c, Some(0.0));
        assert_eq!(
            views[1].unavailable["current_c"],
            status.unavailable["temperature_memory_c"]
        );
        assert!(views[1].unavailable["current_c"].contains("implausible"));
        assert_eq!(views[0].current_c, Some(50.0));
        assert_eq!(views[3].current_c, Some(45.0));
    }

    /// A thermal read as the device measured it: one GPU sensor, range [0,127],
    /// reading 47, target 0 of type 1.
    fn measured_thermal_read() -> ThermalReadout {
        let mut read = ThermalReadout {
            target_count: 1,
            sensor_count: 1,
            reported_sensors: 1,
            target_types_known: 1,
            ..ThermalReadout::default()
        };
        read.target_types[0] = THERMAL_TARGET_GPU;
        read.sensors[0] = ThermalSensorReadout {
            executed: 1,
            result: 0,
            target_index: 0,
            provider_index: 0,
            target_type: THERMAL_TARGET_GPU,
            target_known: 1,
            reading: 47,
            range_min: 0,
            range_max: 127,
            range_known: 1,
        };
        read
    }

    #[test]
    fn the_thermal_read_fills_the_range_of_its_own_target_and_nothing_else() {
        let mut s = snapshot();
        s.telemetry.sample = raw_sample();
        let status = status_from_snapshot(&s);

        // The read answers for the GPU sensor: its range is the driver's own,
        // the live value stays this snapshot's, and the source says which is
        // which. The other three sensors keep a reason -- and that reason says
        // the read answered for another target, not that the range is unread.
        let read = measured_thermal_read();
        let views = status.legacy_temp_sensor_report(ThermalReadOutcome::Answered(&read));
        assert_eq!(views[0].min_c, Some(0.0));
        assert_eq!(views[0].max_c, Some(127.0));
        assert_eq!(views[0].current_c, Some(50.0));
        assert_eq!(
            views[0].current_source.as_deref(),
            Some("rusd:temperature_c")
        );
        assert!(!views[0].unavailable.contains_key("min_c"));
        assert!(views[0].unavailable["controller"].contains("provider index 0"));
        for view in &views[1..] {
            assert_eq!(view.min_c, None);
            assert_eq!(view.max_c, None);
            assert!(view.unavailable["min_c"].contains("named no sensor"));
            assert!(!view.unavailable["min_c"].contains("does not wire"));
        }

        // A range the instruction never wrote must not become a range of zero:
        // the flag, not the value, decides.
        let mut unwritten = measured_thermal_read();
        unwritten.sensors[0].range_known = 0;
        let views = status.legacy_temp_sensor_report(ThermalReadOutcome::Answered(&unwritten));
        assert_eq!(views[0].min_c, None);
        assert_eq!(views[0].max_c, None);

        // A sensor whose target type the driver did not name cannot be placed,
        // so it fills no entry: the type is what says which sensor this is.
        let mut untyped = measured_thermal_read();
        untyped.sensors[0].target_known = 0;
        let views = status.legacy_temp_sensor_report(ThermalReadOutcome::Answered(&untyped));
        for view in &views {
            assert_eq!(view.min_c, None);
            assert!(view.unavailable["min_c"].contains("named no sensor"));
        }

        // When this snapshot's own section has nothing, the read's reading is
        // reported with its own source instead of a null.
        let mut s = snapshot();
        s.telemetry.sample = raw_sample();
        s.telemetry.sample.temperature_fx256[0] = 200 * 256; // implausible
        let status = status_from_snapshot(&s);
        let views = status.legacy_temp_sensor_report(ThermalReadOutcome::Answered(&read));
        assert_eq!(views[0].current_c, Some(47.0));
        assert_eq!(
            views[0].current_source.as_deref(),
            Some("0x20800513:sensor0")
        );
    }

    #[test]
    fn a_refused_thermal_read_is_not_reported_as_an_unwired_one() {
        // The three states must stay apart: a caller that never asked, a driver
        // that refused (a wedged GSP answers 0x62 here too), and a read that
        // answered. Only the first may claim the read is not wired in.
        let mut s = snapshot();
        s.telemetry.sample = raw_sample();
        let status = status_from_snapshot(&s);

        let views = status.legacy_temp_sensor_report(ThermalReadOutcome::NotAttempted);
        assert!(views[0].unavailable["min_c"].contains("does not wire that read in yet"));

        let views = status.legacy_temp_sensor_report(ThermalReadOutcome::Refused(
            "macOS RM thermal read: RUSD allocation/mapping failed: 0x00000062 (0x00000062)",
        ));
        assert_eq!(views[0].min_c, None);
        let reason = &views[0].unavailable["min_c"];
        assert!(reason.contains("0x00000062"), "{reason}");
        assert!(reason.contains("did not complete"), "{reason}");
        assert!(
            !reason.contains("does not wire that read in yet"),
            "{reason}"
        );
        // A refused read also must not turn the live reading into a null: those
        // come from a different read.
        assert_eq!(views[0].current_c, Some(50.0));
    }

    #[test]
    fn throttle_reasons_and_pcie_generation_are_decoded() {
        let mut s = snapshot();
        s.telemetry.sample = raw_sample();
        s.telemetry.sample.throttle_reason_mask = (1 << 0) | (1 << 2) | (1 << 6);
        let status = status_from_snapshot(&s);
        assert_eq!(status.throttle_reason_mask, Some(0b1000101));
        assert_eq!(
            status.throttle_reasons,
            ["gpu_idle", "sw_power_cap", "hw_thermal_slowdown"]
        );
        assert_eq!(status.pcie_link_gen, Some(3));
        assert_eq!(status.pcie_max_link_gen, Some(3));

        // A zero reason mask is "nothing throttling", not "no data".
        s.telemetry.sample.throttle_reason_mask = 0;
        assert!(status_from_snapshot(&s).throttle_reasons.is_empty());

        // The whole gen-info word at 0 means the driver does not report a link.
        assert_eq!(decode_pci_gen_info(0), (None, None));
        assert_eq!(decode_pci_gen_info(0x0000_1000), (Some(1), Some(2)));

        // The NVML-facing view keeps the full nine-reason list in NVML order,
        // with the active flags decoded from the same mask.
        let states = throttle_reason_states(Some(0b1000101));
        assert_eq!(states.len(), 9);
        assert_eq!(states[0], ("GPU Idle", true));
        assert_eq!(states[1], ("App Clock Setting", false));
        assert_eq!(states[2], ("SW Power Cap", true));
        assert_eq!(states[6], ("HW Thermal", true));
        assert_eq!(states[8], ("Display Clock", false));
        // An unavailable mask never fabricates an active reason.
        assert!(
            throttle_reason_states(None)
                .iter()
                .all(|(_, active)| !active)
        );
    }

    /// A zero-filled RUSD sample must not be read as a measurement of zero.
    ///
    /// Measured on the device: with the mapping refused (`0x62`), fourteen
    /// RUSD-backed fields printed `N/A` with the page reason while
    /// `memory_used_bytes` printed `0.000 MiB`, because a zero-filled sample
    /// carries status 0 for that field, which reads as "fine". The byte count
    /// lives in that same sample, so the page reason has to win over it.
    #[test]
    fn a_failed_rusd_page_does_not_publish_a_zero_memory_measurement() {
        let mut s = snapshot();
        s.telemetry.sample = raw_sample();
        // The page failed, and the native layer handed back a zero-filled sample:
        // the section stamps are gone and the field word still says "fine".
        s.telemetry.status = 0x62;
        for status in s.telemetry.sample.field_status.iter_mut() {
            *status = FIELD_UNAVAILABLE;
        }
        s.telemetry.sample.memory_used_status = 0;
        s.telemetry.sample.memory_used_kb = 0;

        let status = status_from_snapshot(&s);
        assert!(
            status.memory_used_bytes.is_none(),
            "a zero-filled page must not report 0 bytes used"
        );
        assert!(
            status.unavailable["memory_used_bytes"].contains("00000062"),
            "{:?}",
            status.unavailable["memory_used_bytes"]
        );
        // The same page reason the sections carry, not a second wording.
        assert_eq!(
            status.unavailable["memory_used_bytes"],
            status.unavailable["temperature_c"]
        );
        // And the live query that still answers is untouched by the page.
        assert!(status.pstate.is_some());
    }

    /// A page that failed to map may not yield a single number.
    ///
    /// The per-field guards each look right on their own, and one of them still
    /// let a zero through -- the byte count whose status word rides inside the
    /// sample. This one checks the whole payload at once instead: with the page
    /// refused, nothing the backend publishes may be a number, except the live
    /// RM query that is measured to answer on the wedged driver. A new field
    /// that forgets the page gate fails here without anyone having to remember
    /// to list it.
    #[test]
    fn a_failed_page_publishes_no_numbers_at_all() {
        let mut s = snapshot();
        s.telemetry.sample = raw_sample();
        // The page refused, a zero-filled sample came back, and the native layer
        // still called the sections readable -- the case where a "fine" status
        // word and a 0 payload are both lies.
        s.telemetry.status = 0x62;
        for status in s.telemetry.sample.field_status.iter_mut() {
            *status = FIELD_OK;
        }
        s.telemetry.sample.pstate_status = 0;
        s.telemetry.sample.memory_used_status = 0;
        s.telemetry.sample.memory_used_kb = 0;

        let status = status_from_snapshot(&s);
        let json = serde_json::to_value(&status).expect("the status serialises");
        let mut numbers: Vec<&String> = json
            .as_object()
            .expect("an object")
            .iter()
            .filter(|(_, value)| value.is_number())
            .map(|(name, _)| name)
            .collect();
        numbers.sort();
        // `pstate` is the exception, and it is a measured one rather than a
        // guess: the pstate query is answered by the GSP itself, and the wedged
        // driver still returns P0 for it.
        assert_eq!(
            numbers,
            vec!["pstate"],
            "a failed page produced a number: {numbers:?}"
        );
        assert!(status.pstate.is_some());

        // Every suppressed field says why, in the page's own words.
        let page = "RUSD allocation/mapping failed: 0x00000062";
        for name in [
            "memory_used_bytes",
            "power_mw",
            "fan_rpm",
            "temperature_c",
            "memory_clock_khz",
            "pcie_link_gen",
            "average_power_mw",
        ] {
            assert_eq!(status.unavailable[name], page, "{name}");
        }
        // The two containers that carry numbers are emptied, not half-filled.
        assert!(status.rusd_timestamps.is_empty());
        assert!(status.engine_utilization_percent.is_empty());
        // The wedged device reported 26 unavailable names for the same page
        // failure on the field set as it stood then, and 27 once `fan_coolers`
        // -- the same readings with the driver's cooler slot kept -- joined it
        // (measured on that device, 2026-10-18, the same 0x00000062 page
        // failure), so this fixture is the device case rather than an
        // approximation of it.
        assert_eq!(status.unavailable.len(), 27);
    }

    #[test]
    fn engine_utilization_is_normalized_from_percent_x100() {
        let mut s = snapshot();
        s.telemetry.sample = raw_sample();
        s.telemetry.sample.engine_utilization_pct100 = [9_148, 25, 0, 0, 0, 0];
        let status = status_from_snapshot(&s);
        assert_eq!(status.engine_utilization_percent["gr"], 91.48);
        assert_eq!(status.engine_utilization_percent["fb"], 0.25);
        assert_eq!(status.engine_utilization_percent["nvdec"], 0.0);

        s.telemetry.sample.engine_utilization_pct100[0] = 10_001;
        let status = status_from_snapshot(&s);
        assert!(status.engine_utilization_percent.is_empty());
        assert!(status.unavailable["engine_utilization_percent"].contains("implausible"));
    }

    #[test]
    fn stale_failed_and_invalid_fields_never_become_measurements() {
        let mut s = snapshot();
        s.telemetry.sample = raw_sample();
        // The whole RUSD page failed to allocate/map: every section says so.
        s.telemetry.status = 0x1f;
        for status in s.telemetry.sample.field_status.iter_mut() {
            *status = FIELD_UNAVAILABLE;
        }
        let status = status_from_snapshot(&s);
        assert!(status.temperature_c.is_none());
        assert!(status.rusd_timestamps.is_empty());
        assert!(status.unavailable["temperature_c"].contains("0000001f"));
        // The memory byte count is a field *of* the page rather than a live
        // control, so the failed page suppresses it too -- its own status word
        // rides in the same sample and cannot outvote the mapping that failed.
        // (This is what the wedged `get-status --macos-rm` proved: the count
        // printed as 0.000 MiB while its neighbours printed N/A.)
        assert!(status.memory_used_bytes.is_none());
        assert!(status.unavailable["memory_used_bytes"].contains("0000001f"));
        // A live control really does stand on its own: the pstate query answered.
        assert!(status.pstate.is_some());

        // With the page mapped, the field's own status word is the gate.
        s.telemetry.status = 0;
        s.telemetry.sample.memory_used_status = 0x57;
        let status = status_from_snapshot(&s);
        assert!(status.memory_used_bytes.is_none());
        assert!(status.unavailable["memory_used_bytes"].contains("00000057"));

        // A section that stayed on its mapping-time watermark is called out
        // with the one-shot wording, not the monitor's "session opened".
        s.telemetry.status = 0;
        s.telemetry.sample.field_status[SEC_POWER] = FIELD_NOT_FRESH;
        s.telemetry.sample.timestamps[SEC_POWER] = 0;
        let status = status_from_snapshot(&s);
        assert!(status.power_mw.is_none());
        assert!(
            status.unavailable["power_mw"].contains("initial freshness budget"),
            "{:?}",
            status.unavailable
        );
    }

    #[test]
    fn pci_bus_identity_is_distinct_from_rm_gpu_id_and_failed_queries_are_absent() {
        let mut s = snapshot();
        s.rm_gpu_id = 0x100;
        s.bus = 3;
        s.slot = 2;
        s.memory_status = 0x1f;
        s.total_bytes = 4 * 1024 * 1024 * 1024;
        let device = device_from_snapshot(&s);
        assert_eq!(device.id().0, 3 * 256);
        assert_eq!(device.info().pci_address, "0000:03:02.0");
        assert_eq!(device.info().rm_gpu_id, 0x100);
        assert_eq!(device.info().memory_total_bytes, None);
        assert!(device.info().unavailable["memory"].contains("0000001f"));
    }

    #[test]
    fn monitor_keeps_valid_zero_readings_and_marks_the_first_sample_updated() {
        let mut trackers = trackers_at(0);
        let sample = interpret_sample(&mut trackers, &raw_sample(), 0, MONITOR_STALE_AFTER);
        assert_eq!(sample.status.temperature_c, Some(50.0));
        assert_eq!(sample.status.pstate, Some(1));
        assert!(sample.status.unavailable.is_empty());
        assert_eq!(sample.advancing_sections(), RUSD_FIELDS);
        // 8 clock + 4 temperature + 2 limit + power + avg + policy + throttle
        // + fan_rpm + fan_rpms + fan_coolers + 2 utilization + engines + 2 pcie
        // + the two live RM controls (pstate, memory_used_bytes).
        assert_eq!(sample.updated.len(), 28);
        assert!(sample.updated.values().all(|value| *value));
        assert_eq!(sample.status.rusd_timestamps["temperature_c"], 1_000);
    }

    #[test]
    fn monitor_flags_repeat_then_stale_sections_without_fabricating_values() {
        let mut trackers = [FieldTracker::default(); RUSD_FIELDS];
        let raw = raw_sample();
        interpret_sample(&mut trackers, &raw, 0, MONITOR_STALE_AFTER);

        // Same timestamp one second later: still a coherent reading, just not
        // refreshed.
        let sample = interpret_sample(&mut trackers, &raw, 1_000_000_000, MONITOR_STALE_AFTER);
        assert!(sample.status.unavailable.is_empty());
        assert_eq!(sample.status.temperature_c, Some(50.0));
        assert_eq!(sample.advancing_sections(), 0);
        assert_eq!(updated_fields(&sample), ["memory_used_bytes", "pstate"]);

        // Unchanged past the staleness window: reported unavailable, never as
        // a repeated or zero value.
        let sample = interpret_sample(&mut trackers, &raw, 6_000_000_000, MONITOR_STALE_AFTER);
        assert!(sample.status.temperature_c.is_none());
        assert!(sample.status.power_mw.is_none());
        assert!(
            sample.status.unavailable["temperature_c"].contains("no RUSD update"),
            "{:?}",
            sample.status.unavailable
        );
        assert!(!sample.status.rusd_timestamps.contains_key("temperature_c"));

        // A later advance clears the stale state and values return.
        let mut newer = raw;
        newer.timestamps = [2_000; RUSD_FIELDS];
        let sample = interpret_sample(&mut trackers, &newer, 6_100_000_000, MONITOR_STALE_AFTER);
        assert_eq!(sample.status.temperature_c, Some(50.0));
        assert!(sample.updated.values().all(|value| *value));
        assert!(sample.status.unavailable.is_empty());
    }

    #[test]
    fn monitor_treats_unstable_or_invalid_fields_as_unavailable() {
        let mut trackers = [FieldTracker::default(); RUSD_FIELDS];
        let mut raw = raw_sample();
        raw.field_status[SEC_TEMP_GPU] = FIELD_UNSTABLE;
        raw.timestamps[SEC_TEMP_GPU] = 0;
        raw.utilization_percent = 101;
        raw.clk_actual_khz[0] = u32::MAX;
        let sample = interpret_sample(&mut trackers, &raw, 0, MONITOR_STALE_AFTER);
        assert!(sample.status.temperature_c.is_none());
        assert!(sample.status.utilization_percent.is_none());
        assert!(sample.status.graphics_clock_khz.is_none());
        assert_eq!(sample.status.memory_clock_khz, Some(6_000_984));
        assert!(
            sample.status.unavailable["temperature_c"].contains("unstable sequence"),
            "{:?}",
            sample.status.unavailable
        );
        assert!(
            sample.status.unavailable["utilization_percent"].contains("implausible"),
            "{:?}",
            sample.status.unavailable
        );
        assert!(!sample.updated.contains_key("graphics_clock_khz"));
        assert!(sample.updated["memory_clock_khz"]);

        // A section the driver does not publish at all is a different
        // condition from one that raced, and neither is a measurement.
        let mut trackers = trackers_at(0);
        let mut raw = raw_sample();
        raw.field_status[SEC_POWER] = FIELD_UNAVAILABLE;
        raw.timestamps[SEC_POWER] = 0;
        let sample = interpret_sample(&mut trackers, &raw, 0, MONITOR_STALE_AFTER);
        assert!(sample.status.power_mw.is_none());
        assert_eq!(sample.freshness["power_mw"], FieldState::Unsupported);
        assert!(
            sample.status.unavailable["power_mw"].contains("no valid timestamp"),
            "{:?}",
            sample.status.unavailable
        );
        assert!(sample.epoch_reset.is_empty());
    }

    #[test]
    fn monitor_distinguishes_never_refreshed_from_not_updated_this_sample() {
        // Opening on an already-old page: the watermark is the baseline, so an
        // unchanged stamp means "nothing new yet", not "fresh zero".
        let mut trackers = trackers_at(1_000);
        let raw = raw_sample();
        let sample = interpret_sample(&mut trackers, &raw, 100_000_000, MONITOR_STALE_AFTER);
        assert!(sample.status.temperature_c.is_none());
        assert_eq!(sample.advancing_sections(), 0);
        for field in [
            "graphics_clock_khz",
            "temperature_c",
            "power_mw",
            "power_limit_mw",
            "fan_rpm",
            "utilization_percent",
            "pcie_link_gen",
        ] {
            assert_eq!(
                sample.freshness[field],
                FieldState::NeverRefreshed,
                "{field}"
            );
            assert!(!sample.updated.contains_key(field));
            assert!(
                sample.status.unavailable[field].contains("since this session opened"),
                "{field}: {:?}",
                sample.status.unavailable
            );
        }
        // Live controls are RM queries, not RUSD sections, so they are new on
        // every successful sample.
        assert_eq!(sample.freshness["pstate"], FieldState::Advancing);
        assert_eq!(sample.status.pstate, Some(1));
        assert_eq!(sample.status.memory_used_bytes, Some(200_000 * 1024));

        // The first advance publishes values and marks them new.
        let mut newer = raw;
        newer.timestamps = [2_000; RUSD_FIELDS];
        let sample = interpret_sample(&mut trackers, &newer, 600_000_000, MONITOR_STALE_AFTER);
        assert_eq!(sample.status.temperature_c, Some(50.0));
        assert_eq!(sample.advancing_sections(), RUSD_FIELDS);
        assert!(sample.status.unavailable.is_empty());
        assert!(sample.updated.values().all(|updated| *updated));

        // Now the same stamps are "unchanged", not "never refreshed".
        let sample = interpret_sample(&mut trackers, &newer, 700_000_000, MONITOR_STALE_AFTER);
        assert_eq!(sample.freshness["temperature_c"], FieldState::Unchanged);
        assert_eq!(sample.status.temperature_c, Some(50.0));
        assert_eq!(updated_fields(&sample), ["memory_used_bytes", "pstate"]);
        assert!(sample.status.unavailable.is_empty());
    }

    #[test]
    fn monitor_treats_a_backwards_stamp_as_new_data_from_a_reset_epoch() {
        let mut trackers = trackers_at(9_000);
        trackers[SEC_TEMP_GPU].seen_change = true;
        trackers[SEC_TEMP_GPU].changed_ns = 0;
        let mut raw = raw_sample();
        raw.timestamps = [1_000; RUSD_FIELDS];
        let sample = interpret_sample(&mut trackers, &raw, 500_000_000, MONITOR_STALE_AFTER);
        // A restarted clock still carries freshly written data, but the sample
        // says so instead of inventing an age from the stamp.
        assert_eq!(sample.status.temperature_c, Some(50.0));
        assert_eq!(sample.freshness["temperature_c"], FieldState::Advancing);
        assert!(sample.updated.values().all(|updated| *updated));
        assert!(sample.epoch_reset.contains(&"temperature_c".to_string()));
        assert!(
            sample
                .epoch_reset
                .contains(&"graphics_clock_khz".to_string())
        );
    }

    #[test]
    fn sample_json_carries_freshness_epoch_resets_and_serializes_field_states() {
        let mut trackers = trackers_at(9_000);
        let mut raw = raw_sample();
        // Temperature moved backwards (a reset epoch), power is unpublished and
        // the fan still sits on the mapping-time watermark.
        raw.timestamps[SEC_TEMP_GPU] = 1_000;
        raw.field_status[SEC_POWER] = FIELD_UNAVAILABLE;
        raw.timestamps[SEC_POWER] = 0;
        raw.timestamps[SEC_FAN] = 9_000;
        let sample = interpret_sample(&mut trackers, &raw, 10_000_000, MONITOR_STALE_AFTER);
        let value = serde_json::to_value(&sample).unwrap();
        assert_eq!(value["freshness"]["temperature_c"], "advancing");
        assert_eq!(value["freshness"]["power_mw"], "unsupported");
        assert_eq!(value["freshness"]["fan_rpm"], "never_refreshed");
        assert!(
            sample.epoch_reset.contains(&"temperature_c".to_string()),
            "{:?}",
            sample.epoch_reset
        );
        assert!(value["advancing_sections"].is_null());
        assert!(value["temperature_c"].is_number());
        assert!(value["power_mw"].is_null());
        assert_eq!(value["engine_utilization_percent"]["gr"], 7.0);
        // The fan section still sits on the open watermark, so it has no
        // reading to publish.
        assert!(value["fan_rpms"].as_array().unwrap().is_empty());
    }

    #[test]
    fn rust_abi_layout_matches_the_native_contract() {
        // Mirrors the _Static_asserts in core/native/macos_rm_abi.h and the
        // assertions in core/tests/macos_rm_abi.c; the ignored hardware test
        // additionally compares this array with the native fingerprint.
        assert_eq!(
            abi_layout(),
            [
                648,
                368,
                376,
                272,
                108,
                8,
                64,
                208,
                176,
                192,
                224,
                248,
                320,
                344,
                348,
                356, // sizes and offsets
                u64::from(ABI_VERSION),
                RUSD_FIELDS as u64,
                FIELD_OK as u64,
                FIELD_UNSTABLE as u64,
                FIELD_UNAVAILABLE as u64,
                FIELD_NOT_FRESH as u64,
                CLK_DOMAINS as u64,
                TEMP_SENSORS as u64,
                FAN_MAX_COOLERS as u64,
                ENGINES as u64,
                POLL_MASK as u64,
                0,
                1,
                2,
                3,
                4,
                5,
                6,
                7,
                8,
                9,
                10,
                11,
                12,
                13,
                std::mem::size_of::<ThermalReadout>() as u64,
                std::mem::size_of::<ThermalSensorReadout>() as u64,
                std::mem::offset_of!(ThermalReadout, sensors) as u64,
            ]
        );
    }

    #[test]
    fn monitor_reports_pstate_failure_and_flattens_the_sample_json() {
        let mut trackers = [FieldTracker::default(); RUSD_FIELDS];
        let mut raw = raw_sample();
        raw.pstate_status = 0x56;
        let sample = interpret_sample(&mut trackers, &raw, 0, MONITOR_STALE_AFTER);
        assert!(sample.status.pstate.is_none());
        assert!(sample.status.unavailable["pstate"].contains("00000056"));
        let value = serde_json::to_value(&sample).unwrap();
        assert_eq!(value["temperature_c"], 50.0);
        assert!(value["updated"]["temperature_c"].as_bool().unwrap());
        assert!(value["unavailable"]["pstate"].is_string());
    }

    #[cfg(not(all(target_os = "macos", target_arch = "x86_64")))]
    #[test]
    fn session_open_reports_the_platform_requirement_off_macos() {
        match MacosRmSession::open() {
            Ok(_) => panic!("macOS RM session must not open off x86_64 macOS"),
            Err(error) => assert!(error.to_string().contains("requires x86_64 macOS")),
        }
    }
}
