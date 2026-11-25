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
power-capped window do not mean a candidate had no effect.";

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

pub const SWEEP_NOTE: &str = "Candidates are ranked against a baseline repeat, not against \
a single window. The threshold is the wider of the measured spread and half a \
percent, because two lucky windows understate the real run-to-run noise; \
anything inside it is reported as indistinguishable rather than promoted.";

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
