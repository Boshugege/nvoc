//! macOS stock evaluation — the read-only, write-free half of the stage-10
//! closed loop.
//!
//! The pure half (the plan document and the telemetry reductions) lives in
//! `nvoc_core::macos_eval`, because the CLI and the Python bindings publish the
//! same document and must not be able to describe the same verdict differently.
//! What stays here is what needs a device, a subprocess or the filesystem: the
//! argument surface, the bounded load, the Metal verifier and the report.
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
    DeviceStatus, Error, GpuTarget, PlanRequest, QueryMacosRmInfo, QueryMacosRmSettings,
    QueryMacosRmStatus, fps_tail, load_summary, parse_plan_requests, plan_document, run,
};
use serde_json::{Value, json};
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

    // One core value carries the sampled fields and the throttle evidence, so
    // the report cannot describe the load without saying whether the driver
    // capped it. `load_summary` reports how fast/far the load moved the GPU,
    // its `throttle` field whether a reason was asserted and which.
    let under_load_report = serde_json::to_value(load_summary(&under_load))
        .map_err(|e| error(format!("failed to serialise the load summary: {e}")))?;

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
    // The plan section is the shared document from core, so this report and the
    // CLI's `--plan-delta` section are the same object with the same keys, note
    // and write flag -- not two shapes a reader has to reconcile.
    let plan = if opts.plan.is_empty() && !opts.dry_run {
        Value::Null
    } else {
        serde_json::to_value(plan_document(&settings, &opts.plan))
            .map_err(|e| error(format!("failed to serialise the plan document: {e}")))?
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
