//! macOS stock evaluation — the read-only, write-free half of the stage-10
//! closed loop.
//!
//! Runs the original Vulkan render stressor (`cli-stressor-cuda-rs
//! --vulkan-only`, reused verbatim from stage 9) for a bounded time while
//! sampling the native NullMoth RM telemetry, then runs the `stress-macos`
//! bit-exact Metal verifier. It performs **no writes**: no candidate is
//! applied and `settings_written` is always false. The write path itself does
//! exist -- 0x2080d01c carries the same 2108-byte block as the readable
//! 0x2080901b, and the single-domain step with pre-record, read-back and
//! restore is implemented and compiles (probe-macos-rm, stage 7.5) -- but a
//! write still waits on authorisation, a healthy post-reboot session, and the
//! field identity that only the write itself can settle. This is the preflight
//! + bounded load + verify step the closed loop can call unchanged.

use clap::ArgMatches;
use nvoc_core::{
    DeviceSettings, DeviceStatus, Error, GpuTarget, OffsetStepTarget, QueryMacosRmInfo,
    QueryMacosRmSettings, QueryMacosRmStatus, parse_offset_delta, parse_offset_target,
    plan_offset_step, run,
};
use serde_json::{Value, json};
use std::collections::BTreeSet;
use std::io::Write;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

fn error(message: impl Into<String>) -> Error {
    Error::Custom(message.into())
}

/// Bounded-load parameters handed to the reused Vulkan stressor. `None` keeps
/// the tool's own default, so the evaluation never silently changes strength.
#[derive(Default, Clone)]
pub struct VulkanArgs {
    pub width: Option<u32>,
    pub height: Option<u32>,
    pub shells: Option<u32>,
    pub iters: Option<u32>,
}

pub struct StockEvalOptions {
    pub duration: f64,
    pub interval: f64,
    pub stressor: PathBuf,
    pub metal_check: Option<PathBuf>,
    pub out: Option<PathBuf>,
    pub vulkan: VulkanArgs,
    /// Preflight and plan only: no load, no verifier, no writes.
    pub dry_run: bool,
    pub plan: Vec<PlanRequest>,
}

/// One `--plan <target>=<delta>` request, already parsed with the grammar the
/// core crate shares with the CLI and the Python bindings, so all three accept
/// and refuse exactly the same requests.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlanRequest {
    pub target: OffsetStepTarget,
    pub delta_khz: i32,
}

fn parse_plan_requests(values: &[String]) -> Result<Vec<PlanRequest>, Error> {
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

/// The dry-run plan: what each requested step would do, decided from the
/// read-only snapshot. Pure, so the decision path is exercised without a device
/// and without a write.
fn dry_run_plan(settings: &DeviceSettings, requests: &[PlanRequest]) -> Result<Value, Error> {
    let mut steps = Vec::with_capacity(requests.len());
    for request in requests {
        let step = plan_offset_step(settings, &request.target, request.delta_khz);
        steps.push(
            serde_json::to_value(&step)
                .map_err(|e| error(format!("failed to serialise a planned step: {e}")))?,
        );
    }
    Ok(json!({
        "applied": false,
        "settings_write": settings.capabilities.settings_write,
        "requests": requests.len(),
        "note": "Dry run: every step below comes from the read-only snapshot and nothing was \
                 written. `ready` means the step names a record and stays inside the window the \
                 driver reported -- not that it was applied. `settings_write` reports whether \
                 this backend would write at all, so a ready step with settings_write = false is \
                 a step waiting on the backend, not a failed one.",
        "steps": steps,
    }))
}

fn parse_options(matches: &ArgMatches) -> Result<StockEvalOptions, Error> {
    let duration = *matches
        .get_one::<f64>("duration")
        .ok_or_else(|| error("missing --duration"))?;
    if !(duration.is_finite() && duration > 0.0) {
        return Err(error("--duration must be a positive number of seconds"));
    }
    let interval = *matches
        .get_one::<f64>("interval")
        .ok_or_else(|| error("missing --interval"))?;
    if !(interval.is_finite() && interval > 0.0) {
        return Err(error("--interval must be a positive number of seconds"));
    }
    let stressor = matches
        .get_one::<String>("vulkan-stressor")
        .map(PathBuf::from)
        .ok_or_else(|| error("missing --vulkan-stressor"))?;
    let metal_check = matches
        .get_one::<String>("metal-check")
        .map(PathBuf::from)
        .filter(|p| !p.as_os_str().is_empty());
    let out = matches
        .get_one::<String>("out")
        .map(PathBuf::from)
        .filter(|p| !p.as_os_str().is_empty());
    let vulkan = VulkanArgs {
        width: matches.get_one::<u32>("vulkan-width").copied(),
        height: matches.get_one::<u32>("vulkan-height").copied(),
        shells: matches.get_one::<u32>("vulkan-shells").copied(),
        iters: matches.get_one::<u32>("vulkan-iters").copied(),
    };
    let plan_values: Vec<String> = matches
        .get_many::<String>("plan")
        .map(|values| values.cloned().collect())
        .unwrap_or_default();
    Ok(StockEvalOptions {
        duration,
        interval,
        stressor,
        metal_check,
        out,
        vulkan,
        dry_run: matches.get_flag("dry-run"),
        plan: parse_plan_requests(&plan_values)?,
    })
}

/// min/mean/max/n over a telemetry field, `null` when the driver published no
/// value for it (never fabricated as zero).
fn stats(samples: &[DeviceStatus], pick: impl Fn(&DeviceStatus) -> Option<f64>) -> Value {
    let xs: Vec<f64> = samples.iter().filter_map(pick).collect();
    if xs.is_empty() {
        return Value::Null;
    }
    let n = xs.len() as f64;
    let mean = xs.iter().sum::<f64>() / n;
    let min = xs.iter().copied().fold(f64::INFINITY, f64::min);
    let max = xs.iter().copied().fold(f64::NEG_INFINITY, f64::max);
    json!({"min": min, "mean": mean, "max": max, "n": xs.len()})
}

fn load_summary(samples: &[DeviceStatus]) -> Value {
    json!({
        "samples": samples.len(),
        "graphics_clock_mhz": stats(samples, |s| s.graphics_clock_khz.map(|k| k as f64 / 1000.0)),
        "memory_clock_mhz": stats(samples, |s| s.memory_clock_khz.map(|k| k as f64 / 1000.0)),
        "temperature_c": stats(samples, |s| s.temperature_c),
        "power_w": stats(samples, |s| s.power_mw.map(|mw| mw as f64 / 1000.0)),
        "fan_rpm": stats(samples, |s| s.fan_rpm.map(f64::from)),
        "gpu_utilization_percent": stats(samples, |s| s.utilization_percent.map(f64::from)),
    })
}

/// Throttle evidence for the stock load: the union of the reason masks the
/// driver published while the stressor ran, and the names of every reason that
/// was asserted at least once.
///
/// The Vulkan load only proves excitation, so the report has to say *why* the
/// clocks moved — or that nothing throttled. An empty `active_reasons` means
/// "the driver asserted no reason", never "the field was not read": the number
/// of samples that actually carried a mask is reported alongside.
fn throttle_summary(samples: &[DeviceStatus]) -> Value {
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
    json!({
        "samples_with_mask": samples_with_mask,
        "mask_any": mask_any,
        "active_reasons": active.into_iter().collect::<Vec<_>>(),
    })
}

/// Keep the stressor's rolling FPS footer for the report without dumping the
/// whole log; the last few `[VKGFX-H]` lines carry the final rates.
fn fps_tail(stdout: &[u8]) -> Vec<String> {
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

/// The load half of the stock evaluation, reused verbatim from stage 9. Split
/// out so the dry-run mode skips it as a whole instead of reporting an empty
/// summary as if a load had happened.
struct LoadRun {
    under_load: Value,
    stressor: Value,
    post: Value,
    exit_code: Option<i32>,
}

fn run_bounded_load(gpu: &GpuTarget<'_>, opts: &StockEvalOptions) -> Result<LoadRun, Error> {
    // Bounded stock load: the original stage-9 Vulkan path, unmodified.
    let mut command = Command::new(&opts.stressor);
    command
        .arg("--vulkan-only")
        .arg("--duration")
        .arg(opts.duration.to_string())
        .arg("--gpu-index")
        .arg(gpu.index.to_string());
    if let Some(v) = opts.vulkan.width {
        command.args(["--vulkan-width", &v.to_string()]);
    }
    if let Some(v) = opts.vulkan.height {
        command.args(["--vulkan-height", &v.to_string()]);
    }
    if let Some(v) = opts.vulkan.shells {
        command.args(["--vulkan-shells", &v.to_string()]);
    }
    if let Some(v) = opts.vulkan.iters {
        command.args(["--vulkan-iters", &v.to_string()]);
    }
    command.stdout(Stdio::piped()).stderr(Stdio::piped());

    let started = Instant::now();
    let mut child = command.spawn().map_err(|e| {
        error(format!(
            "failed to start the Vulkan stressor {}: {e}",
            opts.stressor.display()
        ))
    })?;

    // Sample the RM telemetry until the child exits. Per-sample read errors
    // are dropped rather than aborting (a GCOFF blip must not kill the run).
    let mut under_load: Vec<DeviceStatus> = Vec::new();
    loop {
        match child.try_wait() {
            Ok(Some(_)) => break,
            Ok(None) => {}
            Err(e) => return Err(error(format!("waiting for the stressor failed: {e}"))),
        }
        if let Ok(sample) = run(gpu, QueryMacosRmStatus) {
            under_load.push(sample.output);
        }
        std::thread::sleep(Duration::from_secs_f64(opts.interval));
    }
    let output = child
        .wait_with_output()
        .map_err(|e| error(format!("collecting the stressor output failed: {e}")))?;
    let elapsed = started.elapsed().as_secs_f64();
    let post = run(gpu, QueryMacosRmStatus)?.output;

    // Attach the throttle evidence to the under-load block: `load_summary`
    // reports how fast/far the load moved the GPU, `throttle_summary` reports
    // whether the driver capped it and why.
    let mut under_load_report = load_summary(&under_load);
    if let Some(object) = under_load_report.as_object_mut() {
        object.insert("throttle".into(), throttle_summary(&under_load));
    }

    Ok(LoadRun {
        under_load: under_load_report,
        stressor: json!({
            "exit_code": output.status.code(),
            "elapsed_s": elapsed,
            "fps_tail": fps_tail(&output.stdout),
        }),
        post: serde_json::to_value(&post)
            .map_err(|e| error(format!("failed to serialise the post-load status: {e}")))?,
        exit_code: output.status.code(),
    })
}

/// Correctness: the rendering load only proves excitation; this verifier is
/// the only bit-exact evidence (see stress-macos/README.md). A dry run has no
/// load to verify and says so, rather than reporting a pass it never ran.
fn run_verifier(path: Option<&PathBuf>, dry_run: bool) -> Result<Value, Error> {
    if dry_run {
        return Ok(json!({
            "ran": false,
            "reason": "dry-run: no load was run, so there is nothing to verify",
        }));
    }
    Ok(match path {
        None => json!({"ran": false, "reason": "no --metal-check path supplied"}),
        Some(path) if !path.exists() => {
            json!({"ran": false, "reason": format!("verifier not found at {}", path.display())})
        }
        Some(path) => {
            let out = Command::new(path).arg("--verify").output().map_err(|e| {
                error(format!(
                    "failed to run the verifier {}: {e}",
                    path.display()
                ))
            })?;
            json!({
                "ran": true,
                "path": path.display().to_string(),
                "exit_code": out.status.code(),
                "passed": out.status.success(),
                "stdout": String::from_utf8_lossy(&out.stdout).trim().to_owned(),
            })
        }
    })
}

pub fn run_macos_eval(targets: &[GpuTarget<'_>], matches: &ArgMatches) -> Result<(), Error> {
    if !cfg!(target_os = "macos") {
        return Err(error(
            "macos-eval drives the native NullMoth RM backend and is macOS-only",
        ));
    }
    let gpu = match targets {
        [gpu] => gpu,
        [] => return Err(error("no macOS RM GPU is available for evaluation")),
        _ => {
            return Err(error(
                "macos-eval evaluates exactly one GPU; pass a single --gpu selector",
            ));
        }
    };
    let opts = parse_options(matches)?;

    let info = run(gpu, QueryMacosRmInfo)?.output;
    let settings = run(gpu, QueryMacosRmSettings)?.output;
    let pre = run(gpu, QueryMacosRmStatus)?.output;

    // The preflight + plan mode stops here: no stressor, no verifier, no write,
    // and the load sections stay null rather than becoming an empty summary.
    let load = if opts.dry_run {
        LoadRun {
            under_load: Value::Null,
            stressor: Value::Null,
            post: Value::Null,
            exit_code: Some(0),
        }
    } else {
        run_bounded_load(gpu, &opts)?
    };
    let verifier = run_verifier(opts.metal_check.as_ref(), opts.dry_run)?;
    // A `--plan` request is evaluated against the snapshot read above, whether
    // or not a load follows it: the plan is about the state, not the load. In a
    // dry run the section is always present, so an empty request list reads as
    // "nothing was asked" rather than as a missing feature.
    let plan = if opts.plan.is_empty() && !opts.dry_run {
        Value::Null
    } else {
        dry_run_plan(&settings, &opts.plan)?
    };

    let report = json!({
        "schema": "nvoc/macos-stock-eval/1",
        "mode": if opts.dry_run { "dry-run" } else { "stock-load" },
        "gpu": {
            "index": gpu.index,
            "gpu_id": gpu.id.0,
            "name": info.name,
            "pci_address": info.pci_address,
            "driver_version": info.driver_version,
        },
        "options": {
            "duration_s": opts.duration,
            "sample_interval_s": opts.interval,
            "stressor": opts.stressor.display().to_string(),
            "vulkan": {
                "width": opts.vulkan.width,
                "height": opts.vulkan.height,
                "shells": opts.vulkan.shells,
                "iters": opts.vulkan.iters,
            },
            "dry_run": opts.dry_run,
            "plan_requests": opts.plan.len(),
        },
        "settings_written": false,
        "write_gate": "read-only: no candidate applied. The write path exists \
            (0x2080d01c, the same 2108-byte block as the readable 0x2080901b) with \
            pre-record, read-back and restore implemented; a write waits on \
            authorisation, a healthy post-reboot session, and the field identity \
            only the write itself can give. The measured window is reported in \
            settings (see get-settings notes), and a --plan request is evaluated \
            against that same snapshot without writing anything.",
        "plan": plan,
        "pre": pre,
        "under_load": load.under_load,
        "post": load.post,
        "stressor": load.stressor,
        "verifier": verifier,
        "settings": settings,
    });

    let pretty = serde_json::to_string_pretty(&report)
        .map_err(|e| error(format!("failed to serialise the evaluation report: {e}")))?;
    if let Some(path) = &opts.out {
        let mut file = std::fs::File::create(path).map_err(Error::from)?;
        file.write_all(pretty.as_bytes()).map_err(Error::from)?;
        file.write_all(b"\n").map_err(Error::from)?;
        eprintln!("wrote {}", path.display());
    }
    println!("{pretty}");

    if !opts.dry_run && load.exit_code != Some(0) {
        return Err(error(format!(
            "the Vulkan stressor exited with {:?}",
            load.exit_code
        )));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn load_summary_is_null_when_field_absent() {
        let s = DeviceStatus::default();
        let summary = load_summary(&[s]);
        assert_eq!(summary["graphics_clock_mhz"], Value::Null);
        assert_eq!(summary["samples"], json!(1));
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
        let v = stats(&[a, b], |s| s.temperature_c);
        assert_eq!(v["min"], json!(40.0));
        assert_eq!(v["max"], json!(60.0));
        assert_eq!(v["mean"], json!(50.0));
        assert_eq!(v["n"], json!(2));
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
        assert_eq!(summary["samples_with_mask"], json!(2));
        assert_eq!(summary["mask_any"], json!(0b111));
        assert_eq!(
            summary["active_reasons"],
            json!(["app_clock_setting", "gpu_idle", "sw_power_cap"])
        );
    }

    #[test]
    fn throttle_summary_reports_no_mask_as_null_not_empty_union() {
        let summary = throttle_summary(&[DeviceStatus::default()]);
        assert_eq!(summary["samples_with_mask"], json!(0));
        assert_eq!(summary["mask_any"], Value::Null);
        assert_eq!(summary["active_reasons"], json!([]));
    }

    #[test]
    fn fps_tail_keeps_last_lines_only() {
        let log = b"x\n1 fps\n2 fps\n3 fps\n4 fps\n5 fps\n";
        let tail = fps_tail(log);
        assert_eq!(tail, vec!["3 fps", "4 fps", "5 fps"]);
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

    #[test]
    fn a_dry_run_plan_is_never_applied_and_says_whether_this_backend_writes() {
        let settings = DeviceSettings {
            clock_offset_limits_khz: Some(nvoc_core::ClockOffsetLimits {
                min_khz: -18_750,
                max_khz: 18_750,
                records: 1,
            }),
            clock_offsets: Some(vec![nvoc_core::ClockOffsetReading {
                domain_bit: 0x10,
                frequency_khz: 6_000_984,
                offset_khz: -126,
            }]),
            ..Default::default()
        };
        let requests = parse_plan_requests(&["bit:0x10=+8mhz".into(), "bit:0x9=+1mhz".into()])
            .expect("parses");
        let plan = dry_run_plan(&settings, &requests).expect("serialises");
        assert_eq!(plan["applied"], json!(false));
        assert_eq!(plan["settings_write"], json!(false));
        assert_eq!(plan["requests"], json!(2));
        assert_eq!(plan["steps"][0]["verdict"], json!("ready"));
        assert_eq!(plan["steps"][0]["target_offset_khz"], json!(7_874));
        // The second bit has no record, and the plan says which layer is missing
        // instead of guessing a value for it.
        assert_eq!(plan["steps"][1]["verdict"], json!("record_absent"));
        assert_eq!(plan["steps"][1]["target_offset_khz"], json!(Value::Null));
        assert!(
            plan["note"]
                .as_str()
                .is_some_and(|n| n.contains("nothing was written")),
            "{}",
            plan["note"]
        );
    }
}
