//! macOS stock evaluation — the read-only, write-free half of the stage-10
//! closed loop.
//!
//! Runs the original Vulkan render stressor (`cli-stressor-cuda-rs
//! --vulkan-only`, reused verbatim from stage 9) for a bounded time while
//! sampling the native NullMoth RM telemetry, then runs the `stress-macos`
//! bit-exact Metal verifier. It performs **no writes**: there is no verified
//! macOS offset/VF write path yet, so no candidate is applied and
//! `settings_written` is always false. This is the preflight + bounded load +
//! verify step a future macOS closed loop can call unchanged.

use clap::ArgMatches;
use nvoc_core::{
    DeviceStatus, Error, GpuTarget, QueryMacosRmInfo, QueryMacosRmSettings, QueryMacosRmStatus, run,
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
    Ok(StockEvalOptions {
        duration,
        interval,
        stressor,
        metal_check,
        out,
        vulkan,
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

    // Correctness: the rendering load only proves excitation; this verifier is
    // the only bit-exact evidence (see stress-macos/README.md).
    let verifier = match &opts.metal_check {
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
    };

    let report = json!({
        "schema": "nvoc/macos-stock-eval/1",
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
        },
        "settings_written": false,
        "write_gate": "read-only: no verified macOS offset/VF write path on this \
            driver (see get-settings notes); no candidate was applied",
        "pre": pre,
        "under_load": under_load_report,
        "post": post,
        "stressor": {
            "exit_code": output.status.code(),
            "elapsed_s": elapsed,
            "fps_tail": fps_tail(&output.stdout),
        },
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

    if !output.status.success() {
        return Err(error(format!(
            "the Vulkan stressor exited with {:?}",
            output.status.code()
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
}
