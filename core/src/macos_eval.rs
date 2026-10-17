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
}
