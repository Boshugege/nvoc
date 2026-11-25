//! The pure half of the macOS stock evaluation, in `core` because more than one
//! entry publishes it.
//!
//! Nothing here opens a device, runs a subprocess or writes anything: the plan
//! document is decided from one `DeviceSettings` snapshot, and the telemetry
//! summaries are reductions over samples another entry collected. Keeping them
//! here is not tidiness -- it is the only way the optimizer, the CLI and the
//! Python bindings can publish the *same* document for the same question. They
//! used to publish two shapes (a bare step and a document), which is exactly the
//! kind of difference a user cannot see the reason for.
//!
//! Everything is a typed value with `Serialize`; no JSON is built here. `core`
//! carries `serde` but not `serde_json` (that stays a test-only dependency), so
//! the shape is described once, in Rust, and each entry serialises it.

use crate::macos_rm::{
    OffsetStep, OffsetStepTarget, parse_offset_delta, parse_offset_target, plan_offset_step,
};
use crate::{DeviceSettings, DeviceStatus, Error};
use serde::Serialize;
use std::collections::BTreeSet;

fn error(message: impl Into<String>) -> Error {
    Error::Custom(message.into())
}

/// One `--plan <target>=<delta>` request, already parsed with the grammar the
/// core crate shares with the CLI and the Python bindings, so all three accept
/// and refuse exactly the same requests.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlanRequest {
    pub target: OffsetStepTarget,
    pub delta_khz: i32,
}

/// Parse `--plan <target>=<delta>` values. The grammar is the shared one, so a
/// request one entry accepts is accepted by every entry, and a typo is refused
/// with the grammar's own words rather than a silently unnamed domain.
pub fn parse_plan_requests(values: &[String]) -> Result<Vec<PlanRequest>, Error> {
    values
        .iter()
        .map(|value| {
            let (target, delta) = value.split_once('=').ok_or_else(|| {
                error(format!(
                    "--plan {value:?} is not <target>=<delta>: expected e.g. --plan \
                     graphics=+8mhz or --plan bit:0x1=-2000"
                ))
            })?;
            Ok(PlanRequest {
                target: parse_offset_target(target).map_err(error)?,
                delta_khz: parse_offset_delta(delta).map_err(error)?,
            })
        })
        .collect()
}

/// The note that travels with every plan document. It is a constant so the three
/// entries cannot describe the same verdict differently.
pub const PLAN_NOTE: &str = "Dry run: every step below comes from the read-only snapshot and nothing was \
written. `ready` means the step names a record and stays inside the window the \
driver reported -- not that it was applied. `settings_write` reports whether \
this backend would write at all, so a ready step with settings_write = false is \
a step waiting on the backend, not a failed one.";

/// One plan document: what each requested step would do, decided from a
/// read-only snapshot.
///
/// This is the interface, not a local detail: `applied` is always false (nothing
/// in this path writes), `settings_write` reports whether this backend would
/// write at all, and `note` says what `ready` does and does not mean. A `ready`
/// step with `settings_write = false` is a step waiting on the backend, not a
/// failed one.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PlanDocument {
    pub applied: bool,
    pub settings_write: bool,
    pub requests: usize,
    pub note: &'static str,
    pub steps: Vec<OffsetStep>,
}

/// Build the document for one or more requests. Infallible on purpose: a step is
/// a value from [`plan_offset_step`], so there is no failure left to report here,
/// and a reader never has to ask whether a plan is missing or empty.
pub fn plan_document(settings: &DeviceSettings, requests: &[PlanRequest]) -> PlanDocument {
    PlanDocument {
        applied: false,
        settings_write: settings.capabilities.settings_write,
        requests: requests.len(),
        note: PLAN_NOTE,
        steps: requests
            .iter()
            .map(|request| plan_offset_step(settings, &request.target, request.delta_khz))
            .collect(),
    }
}

/// What one applied-then-put-back candidate did, as every entry publishes it.
///
/// The write half of the closed loop has exactly one thing to prove that a
/// read-only plan cannot: that a candidate went in, was read back, and came
/// back out identically. So `settled` is about the *round trip*, not about
/// speed -- a candidate that was put back exactly is settled even if the clocks
/// never moved, and one whose restore disagrees with the original is not
/// settled however fast it ran.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct CandidateOutcome {
    pub domain: &'static str,
    pub requested_khz: i32,
    pub original_khz: i32,
    pub applied_khz: i32,
    pub apply_verified: bool,
    pub other_bytes_preserved: bool,
    /// The offset the restore actually read back, or `null` when no restore was
    /// attempted (the run was cancelled before it, for instance).
    pub restored_khz: Option<i32>,
    pub restore_verified: bool,
    pub settled: bool,
    pub note: &'static str,
}

pub const CANDIDATE_NOTE: &str = "One candidate offset was applied to one clock domain and put \
back. `apply_verified` is the driver's own read-back of the value it stored and \
`other_bytes_preserved` says the rest of the 2108-byte record was untouched; \
`settled` additionally requires a restore that read back the original offset. \
`settled` says the round trip was clean -- it is not a stability claim, and a \
candidate whose restore never ran is reported as unsettled rather than assumed \
good.";

/// Reduce the two write reports into the published document. A restore report
/// that is missing, unverified, or that read back a different offset than the
/// original leaves `settled` false: the record is not known to be where it
/// started.
pub fn candidate_outcome(
    requested_khz: i32,
    applied: &crate::ClockOffsetWriteReport,
    restored: Option<&crate::ClockOffsetWriteReport>,
) -> CandidateOutcome {
    let restored_khz = restored.map(|r| r.applied_khz);
    let restore_verified = restored.is_some_and(|r| r.readback_verified);
    CandidateOutcome {
        domain: applied.domain,
        requested_khz,
        original_khz: applied.original_khz,
        applied_khz: applied.applied_khz,
        apply_verified: applied.readback_verified,
        other_bytes_preserved: applied.other_bytes_preserved,
        restored_khz,
        restore_verified,
        settled: applied.readback_verified
            && applied.other_bytes_preserved
            && restore_verified
            && restored_khz == Some(applied.original_khz),
        note: CANDIDATE_NOTE,
    }
}

/// min/mean/max/n over one telemetry field.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Stats {
    pub min: f64,
    pub mean: f64,
    pub max: f64,
    pub n: usize,
}

/// Reduce one field across the samples. `None` (which serialises as `null`) when
/// no sample carried the field: never fabricated as zero.
pub fn stats(
    samples: &[DeviceStatus],
    pick: impl Fn(&DeviceStatus) -> Option<f64>,
) -> Option<Stats> {
    let xs: Vec<f64> = samples.iter().filter_map(pick).collect();
    if xs.is_empty() {
        return None;
    }
    let n = xs.len() as f64;
    Some(Stats {
        min: xs.iter().copied().fold(f64::INFINITY, f64::min),
        mean: xs.iter().sum::<f64>() / n,
        max: xs.iter().copied().fold(f64::NEG_INFINITY, f64::max),
        n: xs.len(),
    })
}

/// Whether a window that is called a load actually did anything, measured from
/// the telemetry the same window published.
///
/// Measured on the test card: the Metal verifier at the optimizer's default
/// 65536 elements runs 143 dispatches in 60 s while the driver reports the idle
/// clock (1530 MHz), the idle power (11.4 W) and 0 percent utilisation, and the
/// die warms by about 4 degrees. A throughput comparison across such a window
/// compares near-idle compute, so the report has to say whether the window was
/// demonstrably busy instead of letting "under load" mean "a binary was
/// running".
///
/// The three signals are deliberately separate: on this transport the power
/// field barely moves at all, so a rising temperature or a clock lift above the
/// pre-window value is the evidence that survives. `demonstrated` is false both
/// when nothing moved and when the window published nothing to judge -- the two
/// are reported apart (`signals`), because "no telemetry" and "no work" are
/// different problems.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct LoadEvidence {
    pub utilization_percent: Option<Stats>,
    pub graphics_clock_mhz: Option<Stats>,
    pub power_w: Option<Stats>,
    pub temperature_c: Option<Stats>,
    /// Under-window clock max minus the pre-window reading, when both exist.
    pub clock_lift_mhz: Option<f64>,
    /// Under-window temperature max minus the pre-window reading.
    pub temperature_rise_c: Option<f64>,
    /// Which of the signals moved far enough to count as work.
    pub signals: Vec<&'static str>,
    /// True when at least one signal cleared its (deliberately low) bar.
    pub demonstrated: bool,
    pub note: &'static str,
}

/// Utilisation at or above this percent counts as work.
pub const LOAD_UTILIZATION_PERCENT: f64 = 50.0;
/// A die that warms this much did work, whatever the utilisation field says.
pub const LOAD_TEMPERATURE_RISE_C: f64 = 5.0;
/// A clock this far above the pre-window value counts as the card leaving idle.
pub const LOAD_CLOCK_LIFT_MHZ: f64 = 100.0;

pub const LOAD_EVIDENCE_NOTE: &str = "A window only counts as a load when its own telemetry \
shows it: utilisation at or above 50 percent, or a die temperature rise of at least 5 C, or a \
clock lift of at least 100 MHz above the pre-window reading. The card measured here does not \
move its power field under the bounded Metal verifier, so power is reported but never used as \
the deciding signal. `demonstrated` false means the throughput numbers in this report compare \
near-idle compute -- not that no code ran.";

pub fn load_evidence(samples: &[DeviceStatus], pre: Option<&DeviceStatus>) -> LoadEvidence {
    let utilization_percent = stats(samples, |s| s.utilization_percent.map(f64::from));
    let graphics_clock_mhz = stats(samples, |s| s.graphics_clock_khz.map(|k| k as f64 / 1000.0));
    let power_w = stats(samples, |s| s.power_mw.map(|mw| mw as f64 / 1000.0));
    let temperature_c = stats(samples, |s| s.temperature_c);
    let clock_lift_mhz = match (
        pre.and_then(|p| p.graphics_clock_khz)
            .map(|k| k as f64 / 1000.0),
        graphics_clock_mhz.as_ref().map(|s| s.max),
    ) {
        (Some(before), Some(peak)) => Some(peak - before),
        _ => None,
    };
    let temperature_rise_c = match (
        pre.and_then(|p| p.temperature_c),
        temperature_c.as_ref().map(|s| s.max),
    ) {
        (Some(before), Some(peak)) => Some(peak - before),
        _ => None,
    };
    let mut signals: Vec<&'static str> = Vec::new();
    if utilization_percent
        .as_ref()
        .is_some_and(|s| s.mean >= LOAD_UTILIZATION_PERCENT)
    {
        signals.push("utilization");
    }
    if temperature_rise_c.is_some_and(|d| d >= LOAD_TEMPERATURE_RISE_C) {
        signals.push("temperature");
    }
    if clock_lift_mhz.is_some_and(|d| d >= LOAD_CLOCK_LIFT_MHZ) {
        signals.push("clock");
    }
    LoadEvidence {
        utilization_percent,
        graphics_clock_mhz,
        power_w,
        temperature_c,
        clock_lift_mhz,
        temperature_rise_c,
        demonstrated: !signals.is_empty(),
        signals,
        note: LOAD_EVIDENCE_NOTE,
    }
}

/// Throttle evidence for a load: the union of the reason masks the driver
/// published while it ran, and the names of every reason asserted at least once.
///
/// The load only proves excitation, so the report has to say *why* the clocks
/// moved -- or that nothing throttled. An empty `active_reasons` means "the
/// driver asserted no reason", never "the field was not read": the number of
/// samples that actually carried a mask is reported alongside.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ThrottleSummary {
    pub samples_with_mask: usize,
    pub mask_any: Option<u32>,
    pub active_reasons: Vec<String>,
}

pub fn throttle_summary(samples: &[DeviceStatus]) -> ThrottleSummary {
    let mut mask_any: Option<u32> = None;
    let mut active: BTreeSet<&str> = BTreeSet::new();
    let mut samples_with_mask = 0usize;
    for sample in samples {
        let Some(mask) = sample.throttle_reason_mask else {
            continue;
        };
        samples_with_mask += 1;
        mask_any = Some(mask_any.unwrap_or(0) | mask);
        active.extend(sample.throttle_reasons.iter().map(String::as_str));
    }
    ThrottleSummary {
        samples_with_mask,
        mask_any,
        active_reasons: active.into_iter().map(str::to_owned).collect(),
    }
}

/// What a bounded load did: the sampled fields plus the throttle evidence.
///
/// One value, so a report cannot carry the load and forget the reason it was
/// capped -- the two used to be assembled separately and joined by hand.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct LoadSummary {
    pub samples: usize,
    pub graphics_clock_mhz: Option<Stats>,
    pub memory_clock_mhz: Option<Stats>,
    pub temperature_c: Option<Stats>,
    pub power_w: Option<Stats>,
    pub fan_rpm: Option<Stats>,
    pub gpu_utilization_percent: Option<Stats>,
    pub throttle: ThrottleSummary,
}

pub fn load_summary(samples: &[DeviceStatus]) -> LoadSummary {
    LoadSummary {
        samples: samples.len(),
        graphics_clock_mhz: stats(samples, |s| s.graphics_clock_khz.map(|k| k as f64 / 1000.0)),
        memory_clock_mhz: stats(samples, |s| s.memory_clock_khz.map(|k| k as f64 / 1000.0)),
        temperature_c: stats(samples, |s| s.temperature_c),
        power_w: stats(samples, |s| s.power_mw.map(|mw| mw as f64 / 1000.0)),
        fan_rpm: stats(samples, |s| s.fan_rpm.map(f64::from)),
        gpu_utilization_percent: stats(samples, |s| s.utilization_percent.map(f64::from)),
        throttle: throttle_summary(samples),
    }
}

/// What a verified Metal load actually got through, so two windows can be
/// compared on one number instead of on impressions.
///
/// The Vulkan stressor prints rolling `fps` lines; the Metal verifier prints one
/// JSON line with the dispatches it launched, the ones it verified and the
/// elapsed time. Throughput is derived from those, never from a separate clock,
/// so a window cannot look faster merely because it was sampled differently.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct LoadThroughput {
    pub mode: String,
    pub iterations: u64,
    pub verified: u64,
    pub failures: u64,
    pub elapsed_ms: u64,
    pub iterations_per_second: f64,
    pub verified_full_buffer: bool,
    pub stopped_by_signal: bool,
}

/// Read one `"key": <value>` field out of the verifier's single-line report.
/// The emitter is one process writing one line with a fixed key set; a scanner
/// is enough, and it keeps `core` free of a JSON dependency.
fn json_field<'a>(line: &'a str, key: &str) -> Option<&'a str> {
    let needle = format!("\"{key}\":");
    let start = line.find(&needle)? + needle.len();
    let rest = line[start..].trim_start();
    let end = rest.find([',', '}']).unwrap_or(rest.len());
    Some(rest[..end].trim().trim_matches('"'))
}

/// The last verifier report in a load's stdout, or `None` when the process
/// printed none (the Vulkan stressor does not, and a crashed load prints nothing).
pub fn metal_throughput(stdout: &[u8]) -> Option<LoadThroughput> {
    let text = String::from_utf8_lossy(stdout);
    let line = text
        .lines()
        .rev()
        .find(|line| line.contains("\"schema\":\"nvoc/macos-metal-check/1\""))?;
    let elapsed_ms = json_field(line, "elapsed_ms")?.parse::<u64>().ok()?;
    let iterations = json_field(line, "iterations")?.parse::<u64>().ok()?;
    if elapsed_ms == 0 {
        return None;
    }
    Some(LoadThroughput {
        mode: json_field(line, "mode")?.to_owned(),
        iterations,
        verified: json_field(line, "verified")?.parse().ok()?,
        failures: json_field(line, "failures")?.parse().ok()?,
        elapsed_ms,
        iterations_per_second: iterations as f64 * 1000.0 / elapsed_ms as f64,
        verified_full_buffer: json_field(line, "verified_full_buffer")? == "true",
        stopped_by_signal: json_field(line, "stopped_by_signal")? == "true",
    })
}

pub const THROUGHPUT_NOTE: &str = "Throughput is the verifier's own dispatched-and-verified \
count divided by its own elapsed time. A delta between two windows is reported as a \
comparison, not as a verdict: the windows are separate short runs on a power-limited \
card, so a difference inside the run-to-run spread is noise, and matching numbers on a \
power-capped window do not mean a candidate had no effect. A window whose \
verified_full_buffer is false checked a sample of each dispatch, not the whole buffer: \
that window measures load, and the separate verifier run is what proves correctness.";

/// The two windows on one number, or `None` when there is nothing to divide by.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ThroughputDelta {
    pub baseline_per_second: f64,
    pub candidate_per_second: f64,
    pub percent: f64,
    pub note: &'static str,
}

pub fn throughput_delta(
    baseline: &LoadThroughput,
    candidate: &LoadThroughput,
) -> Option<ThroughputDelta> {
    if baseline.iterations_per_second <= 0.0 {
        return None;
    }
    Some(ThroughputDelta {
        baseline_per_second: baseline.iterations_per_second,
        candidate_per_second: candidate.iterations_per_second,
        percent: (candidate.iterations_per_second - baseline.iterations_per_second) * 100.0
            / baseline.iterations_per_second,
        note: THROUGHPUT_NOTE,
    })
}

/// How much the score moves between two identical windows.
///
/// A sweep can only declare a winner if it knows its own noise. Two stock
/// windows back to back on the same card give that directly: the mean says
/// where the baseline sits, the largest gap from the mean says how far one
/// window can wander for no reason at all.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct NoiseFloor {
    pub windows: usize,
    pub mean_per_second: f64,
    /// Largest absolute deviation from the mean, as a percentage of the mean.
    /// One window is not a spread, so this is `None` until there are two.
    pub spread_percent: Option<f64>,
    /// The smallest difference the run is willing to call a difference: the
    /// measured spread, or [`MIN_RESOLVABLE_PERCENT`] when the two windows
    /// happened to land closer together than the report can resolve.
    pub threshold_percent: f64,
}

/// A spread measured from two windows is a *lower* bound on run-to-run noise,
/// not the whole of it, and the score itself is quantised (whole iterations in
/// a bounded window). On the test card a single 20 s window moved by 0.02
/// percent between two identical runs, while a longer A/B comparison moved by
/// 0.15 percent for no reason at all. Anything under half a percent is
/// therefore below what this instrument can resolve, whatever two lucky
/// windows happened to say, and is reported as indistinguishable.
pub const MIN_RESOLVABLE_PERCENT: f64 = 0.5;

pub fn noise_floor(runs: &[LoadThroughput]) -> Option<NoiseFloor> {
    if runs.is_empty() {
        return None;
    }
    let mean = runs.iter().map(|r| r.iterations_per_second).sum::<f64>() / runs.len() as f64;
    if mean <= 0.0 {
        return None;
    }
    let spread_percent = (runs.len() > 1).then(|| {
        runs.iter()
            .map(|r| (r.iterations_per_second - mean).abs() * 100.0 / mean)
            .fold(0.0_f64, f64::max)
    });
    let threshold_percent = spread_percent.map_or(MIN_RESOLVABLE_PERCENT, |spread| {
        spread.max(MIN_RESOLVABLE_PERCENT)
    });
    Some(NoiseFloor {
        windows: runs.len(),
        mean_per_second: mean,
        spread_percent,
        threshold_percent,
    })
}

/// One candidate's window judged against the measured noise.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct SweepVerdict {
    pub target: String,
    pub requested_khz: i32,
    pub percent: f64,
    /// True only when the window beat the baseline by more than the baseline
    /// wandered on its own. A gain inside the spread is reported, never
    /// selected.
    pub beats_noise: bool,
}

fn gaps_as_null<S: serde::Serializer>(values: &[f64], serializer: S) -> Result<S::Ok, S::Error> {
    use serde::ser::SerializeSeq;
    let mut seq = serializer.serialize_seq(Some(values.len()))?;
    for value in values {
        seq.serialize_element(&gap_as_null_value(*value))?;
    }
    seq.end()
}

fn gap_as_null_value(value: f64) -> Option<f64> {
    if value.is_nan() { None } else { Some(value) }
}

fn gap_as_null<S: serde::Serializer>(value: &f64, serializer: S) -> Result<S::Ok, S::Error> {
    match gap_as_null_value(*value) {
        Some(value) => serializer.serialize_f64(value),
        None => serializer.serialize_none(),
    }
}

/// Whether a fold result is allowed to be promoted as a winner.
///
/// Repetition alone answers "did the throughput number come back"; it does not
/// answer "did the clock move". A candidate whose throughput rose while the
/// clock it is supposed to be buying stayed flat is an unexplained gain, and this
/// backend's whole offset window is small enough that such a gain is far more
/// likely to be scheduling luck than a clock effect. So promotion asks both
/// questions, and says which one failed.
///
/// No clock telemetry is not the same as a flat clock: without samples the
/// promotion rests on the throughput fold alone, which the `clock_known` flag
/// keeps visible to the reader.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct PromotionGate {
    pub reproduced: bool,
    pub clock_consistent: bool,
    pub clock_known: bool,
    pub promotable: bool,
    pub reason: &'static str,
}

pub fn promotion_gate(reproduced: bool, clock: Option<&ClockAgreement>) -> PromotionGate {
    match clock {
        Some(agreement) => {
            let promotable = reproduced && agreement.consistent;
            PromotionGate {
                reproduced,
                clock_consistent: agreement.consistent,
                clock_known: true,
                promotable,
                reason: if promotable {
                    "the throughput repeated and the clock moved by about the same amount"
                } else if reproduced {
                    "the throughput repeated but the clock change does not account for it"
                } else {
                    "the throughput did not repeat"
                },
            }
        }
        None => PromotionGate {
            reproduced,
            clock_consistent: false,
            clock_known: false,
            promotable: reproduced,
            reason: if reproduced {
                "the throughput repeated; no clock telemetry, so promotion rests on the fold alone"
            } else {
                "the throughput did not repeat"
            },
        },
    }
}

/// The clock a window actually ran at, taken from the driver's own samples.
///
/// The offset this backend accepts is small -- the device reports a span of about
/// 19 MHz -- and the clock moves in steps, so a report that shows only throughput
/// cannot tell "the offset did nothing" from "the offset moved the clock but the
/// load did not follow". The median is reported with the min and max so a step
/// inside one window is visible.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ClockWindow {
    pub median_mhz: f64,
    pub min_mhz: f64,
    pub max_mhz: f64,
    pub samples: usize,
}

/// Median of the samples the driver actually produced. A window with no clock
/// readings yields nothing rather than a zero that would read as "0 MHz".
pub fn clock_window(samples: &[Option<u32>]) -> Option<ClockWindow> {
    let mut khz: Vec<u32> = samples.iter().flatten().copied().collect();
    if khz.is_empty() {
        return None;
    }
    khz.sort_unstable();
    let middle = khz.len() / 2;
    let median = if khz.len().is_multiple_of(2) {
        (f64::from(khz[middle - 1]) + f64::from(khz[middle])) / 2.0
    } else {
        f64::from(khz[middle])
    };
    Some(ClockWindow {
        median_mhz: median / 1000.0,
        min_mhz: f64::from(khz[0]) / 1000.0,
        max_mhz: f64::from(khz[khz.len() - 1]) / 1000.0,
        samples: khz.len(),
    })
}

/// How much faster the candidate window's clock was, in percent. A baseline of
/// zero is not a baseline: no division, no number.
pub fn clock_delta_percent(baseline_mhz: f64, candidate_mhz: f64) -> Option<f64> {
    if baseline_mhz <= 0.0 || candidate_mhz <= 0.0 {
        return None;
    }
    Some((candidate_mhz - baseline_mhz) / baseline_mhz * 100.0)
}

/// Throughput change next to the clock change that is supposed to explain it.
///
/// `consistent` is the weakest useful claim: the measured throughput moved by
/// about the amount the clock moved, within the same threshold the throughput
/// had to clear. A throughput gain with no clock change, or a clock change the
/// throughput ignored, is worth seeing rather than averaging away.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ClockAgreement {
    pub clock_percent: Option<f64>,
    pub throughput_percent: f64,
    pub difference_percent: Option<f64>,
    pub consistent: bool,
}

pub fn clock_agreement(
    clock_percent: Option<f64>,
    throughput_percent: f64,
    threshold_percent: f64,
) -> ClockAgreement {
    let difference_percent = clock_percent.map(|clock| throughput_percent - clock);
    ClockAgreement {
        clock_percent,
        throughput_percent,
        difference_percent,
        consistent: difference_percent.is_some_and(|d| d.abs() <= threshold_percent),
    }
}

/// Whether a round should wait before it starts.
///
/// Repeated rounds on a card that is still cooling measure the card's thermal
/// drift as much as the candidate: the die was seen 68 C at the end of one
/// window and the clock steps down 1950 -> 1920 MHz on the way there. A round
/// that waits for a target die temperature starts from a comparable state, and
/// the temperature it actually started at is reported so a reader can see
/// whether the control worked.
///
/// No telemetry is not "cool enough": without a temperature reading the wait is
/// pointless, and pretending otherwise would hide that the control was not in
/// effect.
pub fn needs_cooldown(temperature_c: Option<f64>, target_c: f64) -> bool {
    match temperature_c {
        Some(temperature) => temperature > target_c,
        None => false,
    }
}

/// Whether the wait has run out. A bounded wait that reports its failure is
/// better than a round that quietly starts hot.
pub fn cooldown_expired(elapsed_s: f64, timeout_s: f64) -> bool {
    timeout_s > 0.0 && elapsed_s >= timeout_s
}

/// One candidate seen across several independent rounds of the same sweep.
///
/// A single round can promote a candidate on one lucky window. Repeating the
/// whole sweep and requiring each round to clear its own threshold is the
/// cheapest way to ask a second question: does it happen again?
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ReproducedVerdict {
    pub target: String,
    pub requested_khz: i32,
    /// One percentage per round, in the order the rounds ran. A null is a round
    /// that did not measure this candidate -- kept as a null, not as a zero,
    /// so the gap cannot be mistaken for a flat result.
    #[serde(serialize_with = "gaps_as_null")]
    pub percent_per_round: Vec<f64>,
    /// The headline number: the mean of the rounds that ran.
    #[serde(serialize_with = "gap_as_null")]
    pub percent_mean: f64,
    pub rounds_beating_noise: usize,
    pub rounds: usize,
    /// True only when every round carried the candidate *and* cleared its own
    /// threshold. A candidate that needed pooling to clear the floor, or that
    /// was only measured once out of three rounds, is not reproduced.
    pub reproduced: bool,
}

/// Folds per-round verdicts into one reproduced verdict per candidate.
///
/// Candidates are matched by (target, requested offset), because the rounds are
/// separate runs and the order inside a round is a ranking, not an identity.
pub fn reproduce_sweeps(rounds: &[Vec<SweepVerdict>]) -> Vec<ReproducedVerdict> {
    let mut out: Vec<ReproducedVerdict> = Vec::new();
    for (index, round) in rounds.iter().enumerate() {
        for verdict in round {
            let slot = out.iter().position(|v| {
                v.target == verdict.target && v.requested_khz == verdict.requested_khz
            });
            let slot = match slot {
                Some(slot) => slot,
                None => {
                    out.push(ReproducedVerdict {
                        target: verdict.target.clone(),
                        requested_khz: verdict.requested_khz,
                        percent_per_round: Vec::new(),
                        percent_mean: 0.0,
                        rounds_beating_noise: 0,
                        rounds: rounds.len(),
                        reproduced: false,
                    });
                    out.len() - 1
                }
            };
            // Keep the per-round list aligned with the round index so a gap
            // stays a gap: a missing measurement must not shift the columns.
            while out[slot].percent_per_round.len() < index {
                out[slot].percent_per_round.push(f64::NAN);
            }
            out[slot].percent_per_round.push(verdict.percent);
            if verdict.beats_noise {
                out[slot].rounds_beating_noise += 1;
            }
        }
    }
    for verdict in &mut out {
        let measured: Vec<f64> = verdict
            .percent_per_round
            .iter()
            .copied()
            .filter(|p| !p.is_nan())
            .collect();
        verdict.percent_mean = if measured.is_empty() {
            f64::NAN
        } else {
            measured.iter().sum::<f64>() / measured.len() as f64
        };
        verdict.reproduced = verdict.rounds > 0
            && measured.len() == verdict.rounds
            && verdict.rounds_beating_noise == verdict.rounds;
    }
    out.sort_by(|a, b| b.percent_mean.total_cmp(&a.percent_mean));
    out
}

pub const REPEAT_NOTE: &str = "Every round is a complete sweep: its own baseline repeat, its \
own noise floor, and the same candidate list. A candidate is reproduced only when each \
round carried it and each round cleared that round's threshold -- the rounds are not pooled \
into one number to clear the bar, because a result that needs pooling has not repeated. \
The per-round percentages are kept so a reader can see the disagreement.";

pub const SWEEP_NOTE: &str = "Candidates are ranked against a baseline repeat, not against \
a single window. The threshold is the wider of the measured spread and half a \
percent, because two lucky windows understate the real run-to-run noise; \
anything inside it is reported as indistinguishable rather than promoted.";

pub const SWEEP_IDLE_NOTE: &str = "No winner is promoted: at least one window in this sweep \
did not demonstrate that the card did any work (see the load_evidence blocks -- utilisation, \
temperature rise and clock lift all stayed put), so the throughput numbers here compare \
near-idle compute. The measurements are kept for the record; they are not a ranking of \
offset effects.";

/// Rank candidates by their measured window, refusing to promote anything the
/// noise floor cannot separate. Order is best-first, and a tie keeps the order
/// the caller asked for, so a repeat run is comparable.
pub fn rank_sweep(
    floor: &NoiseFloor,
    candidates: &[(String, i32, LoadThroughput)],
) -> Vec<SweepVerdict> {
    let mut verdicts: Vec<SweepVerdict> = candidates
        .iter()
        .map(|(target, requested_khz, run)| {
            let percent =
                (run.iterations_per_second - floor.mean_per_second) * 100.0 / floor.mean_per_second;
            SweepVerdict {
                target: target.clone(),
                requested_khz: *requested_khz,
                percent,
                // Two windows and no floor is the one case with no threshold:
                // never promote on the strength of a single baseline window.
                beats_noise: floor.spread_percent.is_some() && percent > floor.threshold_percent,
            }
        })
        .collect();
    verdicts.sort_by(|a, b| b.percent.total_cmp(&a.percent));
    verdicts
}

/// Keep the stressor's rolling FPS footer for the report without dumping the
/// whole log; the last few `[VKGFX-H]` lines carry the final rates.
pub fn fps_tail(stdout: &[u8]) -> Vec<String> {
    let text = String::from_utf8_lossy(stdout);
    let mut lines: Vec<String> = text
        .lines()
        .filter(|l| l.contains("fps"))
        .map(str::to_owned)
        .collect();
    let keep = lines.len().saturating_sub(3);
    lines.drain(..keep);
    lines
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{ClockOffsetLimits, ClockOffsetReading};
    use serde_json::{Value, json};

    #[test]
    fn load_summary_is_null_when_field_absent() {
        let summary = load_summary(&[DeviceStatus::default()]);
        assert_eq!(summary.samples, 1);
        assert_eq!(summary.graphics_clock_mhz, None);
        // A field the driver did not publish stays absent per field, never 0.
        assert_eq!(summary.power_w, None);
        assert_eq!(summary.fan_rpm, None);
    }

    /// The window the Metal verifier actually produced at the optimizer's
    /// default settings: idle clock, idle power, no utilisation, +4 C.
    fn idle_metal_window() -> Vec<DeviceStatus> {
        vec![
            DeviceStatus {
                graphics_clock_khz: Some(1_530_000),
                power_mw: Some(11_400),
                utilization_percent: Some(0),
                temperature_c: Some(48.2),
                ..Default::default()
            },
            DeviceStatus {
                graphics_clock_khz: Some(1_530_000),
                power_mw: Some(11_500),
                utilization_percent: Some(0),
                temperature_c: Some(49.4),
                ..Default::default()
            },
        ]
    }

    #[test]
    fn a_window_that_never_leaves_idle_is_not_a_load() {
        let pre = DeviceStatus {
            graphics_clock_khz: Some(1_530_000),
            power_mw: Some(11_300),
            utilization_percent: Some(0),
            temperature_c: Some(48.3),
            ..Default::default()
        };
        let evidence = load_evidence(&idle_metal_window(), Some(&pre));
        assert!(!evidence.demonstrated);
        assert!(evidence.signals.is_empty());
        // The numbers are still reported: "nothing moved" is the finding, and a
        // consumer can see what did not move.
        assert_eq!(
            evidence.graphics_clock_mhz.as_ref().map(|s| s.max),
            Some(1530.0)
        );
        assert_eq!(evidence.power_w.as_ref().map(|s| s.max), Some(11.5));
        // Measured pre-window 48.3 C against a 49.4 C window peak: the card is
        // warm because it was warm before, not because this window worked it.
        assert_eq!(evidence.temperature_rise_c, Some(49.4 - 48.3));
        assert_eq!(evidence.clock_lift_mhz, Some(0.0));
    }

    #[test]
    fn a_clock_that_does_not_rise_above_the_pre_window_value_is_not_a_lift() {
        // The card did report 1875 MHz during one window, but the pre-window
        // reading was already there: a max-min comparison would have called
        // that load.
        let pre = DeviceStatus {
            graphics_clock_khz: Some(1_875_000),
            ..Default::default()
        };
        let samples = vec![DeviceStatus {
            graphics_clock_khz: Some(1_875_000),
            temperature_c: Some(50.0),
            ..Default::default()
        }];
        let evidence = load_evidence(&samples, Some(&pre));
        assert_eq!(evidence.clock_lift_mhz, Some(0.0));
        assert!(!evidence.demonstrated);
    }

    #[test]
    fn each_individual_signal_can_establish_a_load() {
        let pre = DeviceStatus {
            graphics_clock_khz: Some(1_530_000),
            temperature_c: Some(45.0),
            ..Default::default()
        };
        let busy = vec![DeviceStatus {
            graphics_clock_khz: Some(1_530_000),
            utilization_percent: Some(91),
            temperature_c: Some(46.0),
            ..Default::default()
        }];
        assert_eq!(
            load_evidence(&busy, Some(&pre)).signals,
            vec!["utilization"]
        );

        let hot = vec![DeviceStatus {
            graphics_clock_khz: Some(1_530_000),
            utilization_percent: Some(0),
            temperature_c: Some(51.0),
            ..Default::default()
        }];
        let evidence = load_evidence(&hot, Some(&pre));
        assert_eq!(evidence.signals, vec!["temperature"]);

        let boosted = vec![DeviceStatus {
            graphics_clock_khz: Some(1_950_000),
            utilization_percent: Some(0),
            temperature_c: Some(46.0),
            ..Default::default()
        }];
        let evidence = load_evidence(&boosted, Some(&pre));
        assert_eq!(evidence.signals, vec!["clock"]);
        assert_eq!(evidence.clock_lift_mhz, Some(420.0));
    }

    #[test]
    fn no_telemetry_is_reported_as_no_judgement_not_as_an_idle_window() {
        let evidence = load_evidence(&[DeviceStatus::default()], None);
        assert!(!evidence.demonstrated);
        assert!(evidence.signals.is_empty());
        assert_eq!(evidence.utilization_percent, None);
        assert_eq!(evidence.clock_lift_mhz, None);
        assert_eq!(evidence.temperature_rise_c, None);
    }

    fn verdict(target: &str, khz: i32, percent: f64, beats_noise: bool) -> SweepVerdict {
        SweepVerdict {
            target: target.to_owned(),
            requested_khz: khz,
            percent,
            beats_noise,
        }
    }

    #[test]
    fn a_candidate_that_clears_every_round_is_reproduced() {
        let rounds = vec![
            vec![
                verdict("graphics", 15000, 0.68, true),
                verdict("graphics", 8000, 0.15, false),
            ],
            vec![
                verdict("graphics", 15000, 0.68, true),
                verdict("graphics", 8000, 0.31, false),
            ],
        ];
        let out = reproduce_sweeps(&rounds);
        assert_eq!(out[0].requested_khz, 15000);
        assert!(out[0].reproduced);
        assert_eq!(out[0].rounds_beating_noise, 2);
        assert!((out[0].percent_mean - 0.68).abs() < 1e-9);
        assert!(!out[1].reproduced);
    }

    #[test]
    fn a_candidate_that_wins_one_round_and_loses_the_next_is_not_reproduced() {
        let rounds = vec![
            vec![verdict("graphics", 15000, 0.68, true)],
            vec![verdict("graphics", 15000, 0.12, false)],
        ];
        let out = reproduce_sweeps(&rounds);
        assert!(!out[0].reproduced);
        assert_eq!(out[0].rounds_beating_noise, 1);
        assert!((out[0].percent_mean - 0.40).abs() < 1e-9);
    }

    #[test]
    fn a_candidate_missing_from_a_round_is_not_reproduced() {
        let rounds = vec![
            vec![verdict("graphics", 15000, 0.68, true)],
            vec![verdict("graphics", 8000, 0.31, false)],
        ];
        let out = reproduce_sweeps(&rounds);
        let missing = out.iter().find(|v| v.requested_khz == 15000).unwrap();
        assert!(!missing.reproduced, "one round is not a reproduction");
        assert_eq!(missing.rounds, 2);
        assert_eq!(missing.rounds_beating_noise, 1);
    }

    #[test]
    fn a_gap_in_the_rounds_keeps_the_columns_aligned() {
        let rounds = vec![
            vec![verdict("graphics", 8000, 0.10, false)],
            vec![verdict("graphics", 15000, 0.68, true)],
            vec![
                verdict("graphics", 8000, 0.20, false),
                verdict("graphics", 15000, 0.70, true),
            ],
        ];
        let out = reproduce_sweeps(&rounds);
        let eight = out.iter().find(|v| v.requested_khz == 8000).unwrap();
        assert_eq!(eight.percent_per_round.len(), 3);
        assert!(
            eight.percent_per_round[1].is_nan(),
            "round 1 has no measurement"
        );
        assert!(!eight.reproduced);
        let fifteen = out.iter().find(|v| v.requested_khz == 15000).unwrap();
        assert_eq!(fifteen.percent_per_round.len(), 3);
        assert!(fifteen.percent_per_round[0].is_nan());
        // Measured in two rounds out of three is not a reproduction either: the
        // rounds that ran, ran without it.
        assert!(!fifteen.reproduced);
        assert_eq!(fifteen.rounds_beating_noise, 2);
    }

    #[test]
    fn a_round_that_did_not_measure_a_candidate_serialises_as_a_gap_not_a_zero() {
        let rounds = vec![
            vec![verdict("graphics", 8000, 0.10, false)],
            vec![verdict("graphics", 15000, 0.68, true)],
        ];
        let out = reproduce_sweeps(&rounds);
        let fifteen = out.iter().find(|v| v.requested_khz == 15000).unwrap();
        let text = serde_json::to_string(fifteen).unwrap();
        assert!(text.contains("\"percent_per_round\":[null,0.68]"), "{text}");
        assert!(!text.contains("NaN"), "{text}");
    }

    #[test]
    fn a_repeated_gain_the_clock_explains_is_promotable() {
        let clock = clock_agreement(Some(0.775), 0.788, 0.5);
        let gate = promotion_gate(true, Some(&clock));
        assert!(gate.promotable);
        assert!(gate.clock_known);
        assert!(gate.reason.contains("moved by about the same amount"));
    }

    #[test]
    fn a_repeated_gain_the_clock_does_not_explain_is_not_promotable() {
        let clock = clock_agreement(Some(0.0), 0.9, 0.5);
        let gate = promotion_gate(true, Some(&clock));
        assert!(!gate.promotable, "a gain with a flat clock is unexplained");
        assert!(gate.reproduced);
        assert!(gate.reason.contains("does not account for it"));
    }

    #[test]
    fn a_gain_that_did_not_repeat_is_not_promotable_even_with_a_moved_clock() {
        let clock = clock_agreement(Some(0.775), 0.7, 0.5);
        assert!(!promotion_gate(false, Some(&clock)).promotable);
    }

    #[test]
    fn no_clock_telemetry_falls_back_to_the_fold_and_says_so() {
        let gate = promotion_gate(true, None);
        assert!(gate.promotable);
        assert!(
            !gate.clock_known,
            "the reader must see the clock was not checked"
        );
        assert!(gate.reason.contains("no clock telemetry"));
    }

    #[test]
    fn a_clock_window_reports_the_state_the_card_ran_in() {
        let window = clock_window(&[Some(1_935_000), Some(1_950_000), None, Some(1_935_000)])
            .expect("samples");
        assert_eq!(window.median_mhz, 1935.0);
        assert_eq!(window.min_mhz, 1935.0);
        assert_eq!(window.max_mhz, 1950.0);
        assert_eq!(window.samples, 3, "a missing sample is not a zero");
    }

    #[test]
    fn a_window_without_clock_readings_has_no_window() {
        assert_eq!(clock_window(&[None, None]), None);
        assert_eq!(clock_window(&[]), None);
    }

    #[test]
    fn an_offset_that_buys_one_step_is_about_three_quarters_of_a_percent() {
        let percent = clock_delta_percent(1935.0, 1950.0).expect("both clocks known");
        assert!((percent - 0.7752).abs() < 0.001, "{percent}");
    }

    #[test]
    fn a_clock_that_did_not_move_cannot_explain_a_throughput_gain() {
        let agreement = clock_agreement(Some(0.0), 0.82, 0.5);
        assert!(!agreement.consistent);
        assert_eq!(agreement.difference_percent, Some(0.82));
    }

    #[test]
    fn a_throughput_that_follows_the_clock_is_consistent() {
        let agreement = clock_agreement(Some(0.775), 0.788, 0.5);
        assert!(agreement.consistent);
    }

    #[test]
    fn no_clock_change_reported_means_no_agreement_claim() {
        let agreement = clock_agreement(None, 0.9, 0.5);
        assert!(!agreement.consistent, "silence is not agreement");
        assert_eq!(agreement.difference_percent, None);
    }

    #[test]
    fn a_hot_die_waits_and_a_cool_one_does_not() {
        assert!(needs_cooldown(Some(60.0), 48.0));
        assert!(!needs_cooldown(Some(47.5), 48.0));
        assert!(
            !needs_cooldown(Some(48.0), 48.0),
            "at the target is cool enough"
        );
    }

    #[test]
    fn no_temperature_reading_does_not_mean_cool_enough_to_start() {
        // Waiting is pointless without a reading, but the report must not be
        // able to claim the control was in effect.
        assert!(!needs_cooldown(None, 48.0));
    }

    #[test]
    fn a_wait_ends_at_its_timeout() {
        assert!(!cooldown_expired(10.0, 240.0));
        assert!(cooldown_expired(240.0, 240.0));
        assert!(!cooldown_expired(1e9, 0.0), "no timeout never expires");
    }

    #[test]
    fn the_repeat_note_says_pooling_is_not_reproduction() {
        assert!(REPEAT_NOTE.contains("not pooled"));
        assert!(REPEAT_NOTE.contains("each round cleared that round's threshold"));
    }

    #[test]
    fn the_idle_sweep_note_keeps_the_numbers_but_refuses_the_ranking() {
        assert!(SWEEP_IDLE_NOTE.contains("No winner is promoted"));
        assert!(SWEEP_IDLE_NOTE.contains("not a ranking of offset effects"));
        assert!(SWEEP_IDLE_NOTE.contains("load_evidence"));
    }

    #[test]
    fn the_load_evidence_note_says_what_false_means() {
        assert!(LOAD_EVIDENCE_NOTE.contains("near-idle compute"));
        assert!(LOAD_EVIDENCE_NOTE.contains("not that no code ran"));
        // The thresholds the note promises are the ones the code uses.
        assert!(LOAD_EVIDENCE_NOTE.contains("50 percent"));
        assert!(LOAD_EVIDENCE_NOTE.contains("5 C"));
        assert!(LOAD_EVIDENCE_NOTE.contains("100 MHz"));
        assert_eq!(LOAD_UTILIZATION_PERCENT, 50.0);
        assert_eq!(LOAD_TEMPERATURE_RISE_C, 5.0);
        assert_eq!(LOAD_CLOCK_LIFT_MHZ, 100.0);
    }

    #[test]
    fn stats_reports_min_mean_max() {
        let a = DeviceStatus {
            temperature_c: Some(40.0),
            ..Default::default()
        };
        let b = DeviceStatus {
            temperature_c: Some(60.0),
            ..Default::default()
        };
        let v = stats(&[a, b], |s| s.temperature_c).expect("two samples carried it");
        assert_eq!(v.min, 40.0);
        assert_eq!(v.max, 60.0);
        assert_eq!(v.mean, 50.0);
        assert_eq!(v.n, 2);
        // No sample carried the field, and no samples at all: both are absent
        // rather than a zero that would read as a measurement.
        assert_eq!(
            stats(&[DeviceStatus::default()], |s| s.power_mw.map(f64::from)),
            None
        );
        let no_samples: [DeviceStatus; 0] = [];
        assert_eq!(stats(&no_samples, |s| s.temperature_c), None);
    }

    #[test]
    fn throttle_summary_unions_masks_and_reasons() {
        let a = DeviceStatus {
            throttle_reason_mask: Some(0b101),
            throttle_reasons: vec!["gpu_idle".into(), "sw_power_cap".into()],
            ..Default::default()
        };
        let b = DeviceStatus {
            throttle_reason_mask: Some(0b10),
            throttle_reasons: vec!["app_clock_setting".into()],
            ..Default::default()
        };
        let summary = throttle_summary(&[a, b]);
        assert_eq!(summary.samples_with_mask, 2);
        assert_eq!(summary.mask_any, Some(0b111));
        assert_eq!(
            summary.active_reasons,
            vec!["app_clock_setting", "gpu_idle", "sw_power_cap"]
        );
    }

    #[test]
    fn throttle_summary_reports_no_mask_as_null_not_empty_union() {
        let summary = throttle_summary(&[DeviceStatus::default()]);
        assert_eq!(summary.samples_with_mask, 0);
        assert_eq!(summary.mask_any, None);
        assert!(summary.active_reasons.is_empty());
    }

    #[test]
    fn fps_tail_keeps_last_lines_only() {
        let log = b"x\n1 fps\n2 fps\n3 fps\n4 fps\n5 fps\n";
        let tail = fps_tail(log);
        assert_eq!(tail, vec!["3 fps", "4 fps", "5 fps"]);
        // A log with no rate line keeps nothing, rather than inventing a rate.
        assert!(fps_tail(b"no rates here\n").is_empty());
    }

    #[test]
    fn plan_requests_use_the_shared_grammar_and_reject_typos() {
        let requests = parse_plan_requests(&["graphics=+8mhz".into(), "bit:0x1=-2000".into()])
            .expect("both forms are accepted");
        assert_eq!(requests.len(), 2);
        assert_eq!(
            requests[0].target,
            OffsetStepTarget::Domain("graphics".into())
        );
        assert_eq!(requests[0].delta_khz, 8_000);
        assert_eq!(requests[1].target, OffsetStepTarget::DomainBit(1));
        assert_eq!(requests[1].delta_khz, -2_000);

        // A missing '=' is the flag's own error, a typo is the shared grammar's,
        // and neither may arrive as a silently unnamed domain.
        let no_delta = parse_plan_requests(&["graphics".into()]).unwrap_err();
        assert!(
            format!("{no_delta}").contains("<target>=<delta>"),
            "{no_delta}"
        );
        let typo = parse_plan_requests(&["grphics=1".into()]).unwrap_err();
        assert!(format!("{typo}").contains("bit:"), "{typo}");
    }

    /// One snapshot with a named record and a reported window: the case where a
    /// step can be `ready`, used by the tests below.
    fn snapshot_with_one_offset() -> DeviceSettings {
        DeviceSettings {
            clock_offset_limits_khz: Some(ClockOffsetLimits {
                min_khz: -18_750,
                max_khz: 18_750,
                records: 1,
            }),
            clock_offsets: Some(vec![ClockOffsetReading {
                domain_bit: 0x10,
                frequency_khz: 6_000_984,
                offset_khz: -126,
            }]),
            ..Default::default()
        }
    }

    #[test]
    fn a_plan_document_is_never_applied_and_says_whether_this_backend_writes() {
        let requests = parse_plan_requests(&["bit:0x10=+8mhz".into(), "bit:0x9=+1mhz".into()])
            .expect("parses");
        let plan = serde_json::to_value(plan_document(&snapshot_with_one_offset(), &requests))
            .expect("serialises");
        assert_eq!(plan["applied"], json!(false));
        assert_eq!(plan["settings_write"], json!(false));
        assert_eq!(plan["requests"], json!(2));
        assert_eq!(plan["steps"][0]["verdict"], json!("ready"));
        assert_eq!(plan["steps"][0]["target_offset_khz"], json!(7_874));
        // The second bit has no record, and the plan says which layer is missing
        // instead of guessing a value for it.
        assert_eq!(plan["steps"][1]["verdict"], json!("record_absent"));
        assert_eq!(plan["steps"][1]["target_offset_khz"], Value::Null);
        assert!(
            plan["note"]
                .as_str()
                .is_some_and(|n| n.contains("nothing was written")),
            "{}",
            plan["note"]
        );
    }

    /// The keys are the interface three entries publish, so they are pinned here:
    /// a rename or a dropped key has to be a deliberate change, not a local edit.
    #[test]
    fn the_plan_document_keys_are_the_published_interface() {
        let plan = serde_json::to_value(plan_document(&snapshot_with_one_offset(), &[]))
            .expect("serialises");
        let keys: Vec<&str> = plan
            .as_object()
            .expect("an object")
            .keys()
            .map(String::as_str)
            .collect();
        assert_eq!(
            keys,
            vec!["applied", "note", "requests", "settings_write", "steps"]
        );
        // No requests is an empty document, not a missing one: an entry that
        // asked nothing still says what it did (nothing) and whether it writes.
        assert_eq!(plan["requests"], json!(0));
        assert_eq!(plan["steps"], json!([]));
        assert_eq!(plan["settings_write"], json!(false));
    }

    /// The backend does not write yet, and the document must say so from the one
    /// switch every other surface reads, not from a second copy of the answer.
    #[test]
    fn the_documents_write_flag_is_the_backends_own_switch() {
        let document = plan_document(&DeviceSettings::default(), &[]);
        assert!(!document.applied);
        assert_eq!(
            document.settings_write,
            crate::macos_rm::SETTINGS_WRITE_SUPPORTED
        );
    }

    /// The load summary's keys are published too, and the throttle evidence is
    /// part of the same object: a report cannot carry the load and forget why it
    /// was capped.
    #[test]
    fn the_load_summary_keys_carry_the_throttle_evidence() {
        let with_mask = DeviceStatus {
            temperature_c: Some(50.0),
            throttle_reason_mask: Some(0b1),
            throttle_reasons: vec!["sw_power_cap".into()],
            ..Default::default()
        };
        let summary = serde_json::to_value(load_summary(&[with_mask])).expect("serialises");
        let keys: Vec<&str> = summary
            .as_object()
            .expect("an object")
            .keys()
            .map(String::as_str)
            .collect();
        assert_eq!(
            keys,
            vec![
                "fan_rpm",
                "gpu_utilization_percent",
                "graphics_clock_mhz",
                "memory_clock_mhz",
                "power_w",
                "samples",
                "temperature_c",
                "throttle",
            ]
        );
        assert_eq!(summary["throttle"]["mask_any"], json!(1));
        assert_eq!(
            summary["throttle"]["active_reasons"],
            json!(["sw_power_cap"])
        );
        assert_eq!(summary["temperature_c"]["n"], json!(1));
        assert_eq!(summary["fan_rpm"], Value::Null);
    }

    fn report(
        domain: &'static str,
        original: i32,
        applied: i32,
        verified: bool,
    ) -> crate::ClockOffsetWriteReport {
        crate::ClockOffsetWriteReport {
            domain,
            original_khz: original,
            applied_khz: applied,
            readback_verified: verified,
            other_bytes_preserved: true,
        }
    }

    #[test]
    fn a_candidate_is_settled_only_when_the_restore_read_back_the_original() {
        let applied = report("graphics", -100_000, -25_000, true);
        let restored = report("graphics", -25_000, -100_000, true);
        let outcome = candidate_outcome(-25_000, &applied, Some(&restored));
        assert_eq!(outcome.original_khz, -100_000);
        assert_eq!(outcome.applied_khz, -25_000);
        assert_eq!(outcome.restored_khz, Some(-100_000));
        assert!(outcome.apply_verified && outcome.restore_verified);
        assert!(outcome.settled);
    }

    #[test]
    fn a_restore_that_lands_elsewhere_or_never_runs_is_not_settled() {
        let applied = report("graphics", -100_000, -25_000, true);
        // Put back to the wrong offset: the record is not known to be where it
        // started, so the round trip did not settle even though both writes were
        // acknowledged.
        let wrong = report("graphics", -25_000, 0, true);
        let outcome = candidate_outcome(-25_000, &applied, Some(&wrong));
        assert!(outcome.restore_verified);
        assert_eq!(outcome.restored_khz, Some(0));
        assert!(!outcome.settled);
        // No restore at all (cancelled before it ran) is unsettled, not assumed
        // good, and it does not claim a restored value it never read.
        let none = candidate_outcome(-25_000, &applied, None);
        assert_eq!(none.restored_khz, None);
        assert!(!none.restore_verified);
        assert!(!none.settled);
    }

    #[test]
    fn an_unverified_apply_keeps_the_round_trip_unsettled() {
        let applied = report("memory", 0, -50_000, false);
        let restored = report("memory", -50_000, 0, true);
        let outcome = candidate_outcome(-50_000, &applied, Some(&restored));
        assert!(!outcome.apply_verified);
        assert!(outcome.restore_verified);
        assert!(!outcome.settled);
        let mut vandalised = report("memory", 0, -50_000, true);
        vandalised.other_bytes_preserved = false;
        let outcome = candidate_outcome(-50_000, &vandalised, Some(&restored));
        assert!(outcome.apply_verified);
        assert!(!outcome.other_bytes_preserved);
        assert!(!outcome.settled);
    }

    #[test]
    fn the_candidate_document_publishes_the_round_trip_not_a_verdict_on_speed() {
        let applied = report("graphics", 0, -25_000, true);
        let restored = report("graphics", -25_000, 0, true);
        let value = serde_json::to_value(candidate_outcome(-25_000, &applied, Some(&restored)))
            .expect("serialise");
        assert_eq!(
            value
                .as_object()
                .expect("object")
                .keys()
                .cloned()
                .collect::<Vec<_>>(),
            vec![
                "applied_khz",
                "apply_verified",
                "domain",
                "note",
                "original_khz",
                "other_bytes_preserved",
                "requested_khz",
                "restore_verified",
                "restored_khz",
                "settled",
            ]
        );
        assert_eq!(value["settled"], json!(true));
        assert_eq!(value["domain"], json!("graphics"));
    }

    const ONE_LINE: &str = r#"{"schema":"nvoc/macos-metal-check/1","device":"NVIDIA GeForce GTX 1650 SUPER","backend":"metal","mode":"stress","elements":65536,"iters_per_thread":20000,"verify_sample":65536,"verified_full_buffer":true,"iterations":11,"verified":11,"failures":0,"checked_elements":720896,"mismatched_elements":0,"first_bad_index":0,"first_bad_got":0,"first_bad_want":0,"elapsed_ms":46839,"stopped_by_signal":false}"#;

    #[test]
    fn a_verified_window_yields_its_own_throughput() {
        let stdout = format!("banner line\n{ONE_LINE}\n");
        let t = metal_throughput(stdout.as_bytes()).expect("parsed");
        assert_eq!(t.mode, "stress");
        assert_eq!(t.iterations, 11);
        assert_eq!(t.verified, 11);
        assert_eq!(t.failures, 0);
        assert!(t.verified_full_buffer);
        assert!(!t.stopped_by_signal);
        assert!((t.iterations_per_second - 11.0 * 1000.0 / 46839.0).abs() < 1e-9);
    }

    #[test]
    fn a_window_without_a_verifier_line_has_no_throughput() {
        // The Vulkan stressor prints fps lines, not this schema, and a crashed
        // load prints nothing: both must report "no throughput", never zero.
        assert_eq!(metal_throughput(b"fps 61.2\nfps 59.8\n"), None);
        assert_eq!(metal_throughput(b""), None);
        // A zero elapsed window is not a rate.
        let zero = ONE_LINE.replace("\"elapsed_ms\":46839", "\"elapsed_ms\":0");
        assert_eq!(metal_throughput(zero.as_bytes()), None);
    }

    #[test]
    fn the_two_windows_are_compared_on_one_number_or_not_at_all() {
        let baseline = metal_throughput(ONE_LINE.as_bytes()).expect("parsed");
        let faster = ONE_LINE.replace("\"iterations\":11", "\"iterations\":22");
        let candidate = metal_throughput(faster.as_bytes()).expect("parsed");
        let delta = throughput_delta(&baseline, &candidate).expect("delta");
        assert!((delta.percent - 100.0).abs() < 1e-9);
        // The note has to say a delta is a comparison, so a reader cannot read
        // the number as a performance verdict.
        assert!(delta.note.contains("not as a verdict"));
        // Nothing to divide by: no comparison rather than an infinite gain.
        let dead = LoadThroughput {
            iterations_per_second: 0.0,
            ..baseline.clone()
        };
        assert_eq!(throughput_delta(&dead, &candidate), None);
    }

    fn window(iterations_per_second: f64) -> LoadThroughput {
        LoadThroughput {
            mode: "stress".into(),
            iterations: 48,
            verified: 48,
            failures: 0,
            elapsed_ms: 20_000,
            iterations_per_second,
            verified_full_buffer: true,
            stopped_by_signal: false,
        }
    }

    #[test]
    fn one_window_is_not_a_noise_floor() {
        // A single stock window says where the baseline sits but not how far it
        // wanders, so the spread stays unknown instead of being called zero.
        let floor = noise_floor(&[window(2.4)]).expect("floor");
        assert_eq!(floor.windows, 1);
        assert_eq!(floor.spread_percent, None);
        assert_eq!(noise_floor(&[]), None);
    }

    #[test]
    fn the_spread_is_the_worst_gap_between_two_identical_windows() {
        let floor = noise_floor(&[window(2.40), window(2.45)]).expect("floor");
        // mean 2.425, gaps 0.025 (1.03%) and 0.025 (1.03%)
        assert!((floor.mean_per_second - 2.425).abs() < 1e-12);
        let spread = floor.spread_percent.expect("spread");
        assert!((spread - 1.0309278350515463).abs() < 1e-9, "{spread}");
        // The measured spread is wider than the resolution floor here, so it is
        // the threshold.
        assert!((floor.threshold_percent - spread).abs() < 1e-12);
    }

    #[test]
    fn a_gain_inside_the_noise_is_reported_and_never_promoted() {
        let floor = noise_floor(&[window(2.40), window(2.45)]).expect("floor");
        let candidates = vec![
            // +0.6%: smaller than the 1.03% spread -> indistinguishable
            ("graphics=-15mhz".to_string(), -15_000, window(2.44)),
            // +4%: outside the spread -> a real winner
            ("graphics=-60mhz".to_string(), -60_000, window(2.52)),
        ];
        let ranked = rank_sweep(&floor, &candidates);
        assert_eq!(ranked[0].target, "graphics=-60mhz");
        assert!(ranked[0].beats_noise);
        assert_eq!(ranked[1].target, "graphics=-15mhz");
        assert!(!ranked[1].beats_noise, "{}", ranked[1].percent);
    }

    #[test]
    fn a_quiet_pair_of_windows_does_not_lower_the_bar_to_its_own_luck() {
        // Two identical windows 0.02 percent apart (the value the test card
        // actually produced) would otherwise let a 0.08 percent candidate pass
        // as a winner. The floor keeps it where it belongs.
        let floor = noise_floor(&[window(2.3810), window(2.3815)]).expect("floor");
        assert!(floor.spread_percent.expect("spread") < MIN_RESOLVABLE_PERCENT);
        assert_eq!(floor.threshold_percent, MIN_RESOLVABLE_PERCENT);
        let ranked = rank_sweep(&floor, &[("graphics".to_string(), -8_000, window(2.3834))]);
        assert!(ranked[0].percent > 0.0);
        assert!(
            !ranked[0].beats_noise,
            "a 0.08 percent gain is below this instrument's resolution"
        );
    }

    #[test]
    fn a_single_baseline_window_can_never_declare_a_winner() {
        // Without a repeat there is no measured spread, so the honest answer is
        // "cannot tell", even for a window that looks much faster.
        let floor = noise_floor(&[window(2.40)]).expect("floor");
        let ranked = rank_sweep(
            &floor,
            &[("graphics=-60mhz".to_string(), -60_000, window(3.0))],
        );
        assert!(!ranked[0].beats_noise);
        assert!(ranked[0].percent > 20.0);
    }
}
