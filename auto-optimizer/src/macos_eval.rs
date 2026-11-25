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
    ClockDomain, DeviceStatus, Error, GpuTarget, LoadThroughput, OffsetStepTarget, PlanRequest,
    QueryMacosRmInfo, QueryMacosRmSettings, QueryMacosRmStatus, SetMacosRmClockOffset,
    candidate_outcome, fps_tail, load_summary, metal_throughput, parse_plan_requests,
    plan_document, run, throughput_delta,
};
use serde_json::{Value, json};

/// Each candidate is a full apply/hold/verify/restore transaction in one boot,
/// so the cap is deliberately small: this is evidence gathering, not a scanner.
const SWEEP_MAX_CANDIDATES: usize = 3;
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
    /// `true` runs the Metal load through `metal_check` instead of the Vulkan
    /// stressor, which is the only ready option on a host without the CUDA
    /// stressor built.
    pub metal_load: bool,
    pub stress_elements: u64,
    pub stress_iters: u64,
    /// Elements per dispatch the load window verifies. Full verification makes
    /// the CPU the bottleneck (see the option's help), so the load samples it;
    /// the correctness window keeps verifying every element.
    pub stress_verify_sample: u64,
    /// How many times the whole candidate sweep is repeated. One round can
    /// promote a candidate on one lucky window; more than one asks whether it
    /// happens again.
    pub candidate_rounds: u32,
    /// Die temperature each candidate round waits for before it starts; 0
    /// disables the wait.
    pub cooldown_to_c: f64,
    pub cooldown_timeout_s: f64,
    pub plan: Vec<PlanRequest>,
    /// One candidate applies a write half and puts it back; two or three make a
    /// sweep, which additionally repeats the baseline window so the run can
    /// measure its own noise; empty keeps the run read-only.
    pub candidates: Vec<PlanRequest>,
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
    let candidate_values: Vec<String> = matches
        .get_many::<String>("apply-candidate")
        .map(|values| values.cloned().collect())
        .unwrap_or_default();
    let candidates = parse_plan_requests(&candidate_values)?;
    // Every candidate is a full apply/measure/restore transaction, so the cap
    // is about how many writes one boot should carry, not about the channel.
    if candidates.len() > SWEEP_MAX_CANDIDATES {
        return Err(error(format!(
            "macos-eval takes at most {SWEEP_MAX_CANDIDATES} candidates per run; each one is an \
             apply/measure/restore transaction and more than that is a scanner's job"
        )));
    }
    let dry_run = matches.get_flag("dry-run");
    if !candidates.is_empty() && dry_run {
        return Err(error(
            "--apply-candidate writes an offset and cannot be combined with --dry-run;              use --plan to evaluate the same candidate without writing it",
        ));
    }
    let metal_load = matches
        .get_one::<String>("load-source")
        .map(|v| v == "metal")
        .unwrap_or(false);
    if metal_load && metal_check.is_none() {
        return Err(error(
            "--load-source metal needs --metal-check to point at stress-macos/metal_check",
        ));
    }
    Ok(StockEvalOptions {
        duration,
        interval,
        stressor,
        metal_check,
        out,
        vulkan,
        dry_run,
        metal_load,
        stress_elements: *matches
            .get_one::<u64>("stress-elements")
            .ok_or_else(|| error("missing --stress-elements"))?,
        stress_iters: *matches
            .get_one::<u64>("stress-iters")
            .ok_or_else(|| error("missing --stress-iters"))?,
        stress_verify_sample: *matches
            .get_one::<u64>("stress-verify-sample")
            .ok_or_else(|| error("missing --stress-verify-sample"))?,
        candidate_rounds: *matches
            .get_one::<u32>("candidate-rounds")
            .ok_or_else(|| error("missing --candidate-rounds"))?,
        cooldown_to_c: *matches
            .get_one::<f64>("cooldown-to-c")
            .ok_or_else(|| error("missing --cooldown-to-c"))?,
        cooldown_timeout_s: *matches
            .get_one::<f64>("cooldown-timeout-s")
            .ok_or_else(|| error("missing --cooldown-timeout-s"))?,
        plan: parse_plan_requests(&plan_values)?,
        candidates,
    })
}

/// The clocks a window's own samples reported.
fn clock_of(samples: &[nvoc_core::macos_rm::DeviceStatus]) -> Option<nvoc_core::ClockWindow> {
    let clocks: Vec<Option<u32>> = samples
        .iter()
        .map(|sample| sample.graphics_clock_khz)
        .collect();
    nvoc_core::clock_window(&clocks)
}

/// What a candidate round started from. Recorded whether or not a cooldown was
/// asked for, so a reader can see the die temperature behind each round's number
/// instead of assuming the rounds were comparable.
#[derive(Clone)]
struct RoundStart {
    temperature_c: Option<f64>,
    waited_s: f64,
    cooled: bool,
    timed_out: bool,
}

/// Waits for the die to reach `--cooldown-to-c` before a round starts.
///
/// Read-only: it polls the status the driver already reports and never writes.
/// A wait that runs out starts the round anyway and says so, rather than either
/// hanging or pretending the round started cool.
fn wait_for_cool(gpu: &GpuTarget<'_>, opts: &StockEvalOptions) -> Result<RoundStart, Error> {
    let target = opts.cooldown_to_c;
    let started = Instant::now();
    let mut temperature = run(gpu, QueryMacosRmStatus)
        .ok()
        .and_then(|status| status.output.temperature_c);
    if target <= 0.0 {
        return Ok(RoundStart {
            temperature_c: temperature,
            waited_s: 0.0,
            cooled: false,
            timed_out: false,
        });
    }
    while nvoc_core::needs_cooldown(temperature, target) {
        let elapsed = started.elapsed().as_secs_f64();
        if nvoc_core::cooldown_expired(elapsed, opts.cooldown_timeout_s) {
            return Ok(RoundStart {
                temperature_c: temperature,
                waited_s: elapsed,
                cooled: false,
                timed_out: true,
            });
        }
        std::thread::sleep(Duration::from_secs(5));
        temperature = run(gpu, QueryMacosRmStatus)
            .ok()
            .and_then(|status| status.output.temperature_c);
    }
    Ok(RoundStart {
        temperature_c: temperature,
        waited_s: started.elapsed().as_secs_f64(),
        cooled: true,
        timed_out: false,
    })
}

/// The load half of the stock evaluation, reused verbatim from stage 9. Split
/// out so the dry-run mode skips it as a whole instead of reporting an empty
/// summary as if a load had happened.
struct LoadRun {
    under_load: Value,
    /// Whether this window's own telemetry shows the card did work. Carried
    /// next to the throughput so a report cannot print a comparison without
    /// saying whether either side left idle.
    evidence: nvoc_core::LoadEvidence,
    stressor: Value,
    post: Value,
    exit_code: Option<i32>,
    /// The verified Metal window's own throughput, when the load printed one.
    throughput: Option<LoadThroughput>,
    /// The clock the window actually ran at, from the driver's own samples.
    clock: Option<nvoc_core::ClockWindow>,
}

fn run_bounded_load(gpu: &GpuTarget<'_>, opts: &StockEvalOptions) -> Result<LoadRun, Error> {
    // The Metal source runs the macOS-native verifier as the load: same bounded
    // window, same sampling loop, but a binary that exists on a host without the
    // CUDA stressor built. The Vulkan path stays the default and is unchanged.
    if opts.metal_load {
        let path = opts
            .metal_check
            .as_ref()
            .ok_or_else(|| error("--load-source metal needs --metal-check"))?;
        let mut command = Command::new(path);
        command
            .arg("--stress")
            .arg(opts.duration.to_string())
            .arg("--elements")
            .arg(opts.stress_elements.to_string())
            .arg("--iters")
            .arg(opts.stress_iters.to_string())
            .arg("--verify-sample")
            .arg(opts.stress_verify_sample.to_string())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        return sample_while_loaded(command, gpu, opts);
    }
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
    sample_while_loaded(command, gpu, opts)
}

/// Run one prepared load command and sample the RM telemetry until it exits.
fn sample_while_loaded(
    mut command: Command,
    gpu: &GpuTarget<'_>,
    opts: &StockEvalOptions,
) -> Result<LoadRun, Error> {
    let started = Instant::now();
    // The baseline for the load evidence: what the card looked like on the way
    // in, so a clock that was already lifted is not mistaken for this window's
    // work, and a die that was already warm is not credited to it either.
    let baseline = run(gpu, QueryMacosRmStatus).ok().map(|s| s.output);
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
    let evidence = nvoc_core::load_evidence(&under_load, baseline.as_ref());

    Ok(LoadRun {
        evidence,
        throughput: metal_throughput(&output.stdout),
        clock: clock_of(&under_load),
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

/// What the write half produced: the held load, its verifier, and the two write
/// reports the round trip is judged on.
#[derive(Clone)]
struct CandidateRun {
    evidence: nvoc_core::LoadEvidence,
    throughput: Option<LoadThroughput>,
    clock: Option<nvoc_core::ClockWindow>,
    applied: nvoc_core::ClockOffsetWriteReport,
    restored: Option<nvoc_core::ClockOffsetWriteReport>,
    under_load: Value,
    stressor: Value,
    post: Value,
    verifier: Value,
    exit_code: Option<i32>,
    post_restore_settings: Value,
}

/// The candidate's target as the offset write addresses it. Only the domains
/// this backend has a validated record for are accepted: naming one it cannot
/// write would turn a refusal into a silently different domain.
/// Human label for a plan target, used only in messages: the report keeps the
/// structured target so nothing is parsed back out of prose.
fn target_label(target: &OffsetStepTarget) -> String {
    match target {
        OffsetStepTarget::Domain(name) => name.clone(),
        OffsetStepTarget::DomainBit(bit) => format!("bit:0x{bit:x}"),
    }
}

fn clock_domain_for(target: &OffsetStepTarget) -> Result<ClockDomain, Error> {
    match target {
        OffsetStepTarget::Domain(name) if name == "graphics" => Ok(ClockDomain::Graphics),
        OffsetStepTarget::Domain(name) if name == "memory" => Ok(ClockDomain::Memory),
        OffsetStepTarget::Domain(name) => Err(error(format!(
            "the macOS offset write serves graphics and memory; {name} has no validated record on this backend"
        ))),
        OffsetStepTarget::DomainBit(_) => Err(error(
            "the macOS offset write is addressed by public domain name, not by device bit",
        )),
    }
}

/// Apply one candidate, hold it for the same bounded load, verify under it, then
/// put the original offset back -- and put it back even when the hold failed,
/// because a run that leaves the clock offset somewhere else is worse than a run
/// that reports a failure.
fn run_candidate(
    gpu: &GpuTarget<'_>,
    opts: &StockEvalOptions,
    request: &PlanRequest,
) -> Result<CandidateRun, Error> {
    let domain = clock_domain_for(&request.target)?;
    let never = || false;
    let applied = run(
        gpu,
        SetMacosRmClockOffset {
            domain,
            khz: request.delta_khz,
            cancelled: &never,
        },
    )?
    .output;
    // Everything between the apply and the restore is fallible, so the restore
    // is attempted either way and its own failure is reported as its absence.
    let held = (|| {
        let load = run_bounded_load(gpu, opts)?;
        let verifier = run_verifier(opts.metal_check.as_ref(), false)?;
        Ok::<_, Error>((load, verifier))
    })();
    let restored = run(
        gpu,
        SetMacosRmClockOffset {
            domain,
            khz: applied.original_khz,
            cancelled: &never,
        },
    )
    .map(|out| out.output)
    .ok();
    let post_restore_settings = run(gpu, QueryMacosRmSettings)
        .and_then(|out| {
            serde_json::to_value(&out.output)
                .map_err(|e| error(format!("failed to serialise the restored settings: {e}")))
        })
        .unwrap_or(Value::Null);
    let (load, verifier) = held?;
    Ok(CandidateRun {
        evidence: load.evidence.clone(),
        throughput: load.throughput.clone(),
        clock: load.clock.clone(),
        applied,
        restored,
        under_load: load.under_load,
        stressor: load.stressor,
        post: load.post,
        verifier,
        exit_code: load.exit_code,
        post_restore_settings,
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
            evidence: nvoc_core::load_evidence(&[], Some(&pre)),
            under_load: Value::Null,
            stressor: Value::Null,
            post: Value::Null,
            exit_code: Some(0),
            throughput: None,
            clock: None,
        }
    } else {
        run_bounded_load(gpu, &opts)?
    };
    let verifier = run_verifier(opts.metal_check.as_ref(), opts.dry_run)?;
    // The write half, when asked for: candidates applied one at a time after the
    // stock measurement, each held for its own bounded load, verified under load
    // and restored before the next one starts.
    let rounds_wanted = opts.candidate_rounds.clamp(1, 4);
    let mut round_runs: Vec<Vec<(PlanRequest, CandidateRun)>> = Vec::new();
    let mut round_repeats: Vec<Option<LoadRun>> = Vec::new();
    let mut round_starts: Vec<RoundStart> = Vec::new();
    for _round in 0..rounds_wanted {
        let start = wait_for_cool(gpu, &opts)?;
        round_starts.push(start);
        let mut this_round: Vec<(PlanRequest, CandidateRun)> = Vec::new();
        for request in &opts.candidates {
            let run = run_candidate(gpu, &opts, request)?;
            // Stop the moment a restore is not read back: the next candidate
            // would otherwise be stacked on top of an offset nobody verified was
            // undone.
            if run.restored.is_none() {
                return Err(error(format!(
                    "candidate {} was applied but its restore was not read back; stopping instead \
                     of applying another candidate on top",
                    target_label(&request.target)
                )));
            }
            if run.exit_code != Some(0) {
                return Err(error(format!(
                    "the load exited with {:?} while candidate {} was held",
                    run.exit_code,
                    target_label(&request.target)
                )));
            }
            this_round.push((request.clone(), run));
        }
        // A sweep also repeats the stock window, because a ranking against a
        // single baseline window cannot tell a gain from the baseline's own
        // wander. Each round takes its own repeat, so each round carries its own
        // floor instead of inheriting one from an earlier round.
        let repeat = if this_round.len() > 1 {
            Some(run_bounded_load(gpu, &opts)?)
        } else {
            None
        };
        round_repeats.push(repeat);
        round_runs.push(this_round);
    }
    let runs: Vec<(PlanRequest, CandidateRun)> = round_runs.first().cloned().unwrap_or_default();
    let candidate = runs.first().map(|(_, run)| run);
    let baseline_repeat = round_repeats.first().and_then(|r| r.as_ref());
    let mut sweep = if runs.len() > 1 {
        let mut windows: Vec<nvoc_core::LoadThroughput> = Vec::new();
        windows.extend(load.throughput.clone());
        windows.extend(
            baseline_repeat
                .as_ref()
                .and_then(|run| run.throughput.clone()),
        );
        match nvoc_core::noise_floor(&windows) {
            Some(floor) => {
                let measured: Vec<(String, i32, nvoc_core::LoadThroughput)> = runs
                    .iter()
                    .filter_map(|(request, run)| {
                        run.throughput.as_ref().map(|throughput| {
                            (
                                target_label(&request.target),
                                request.delta_khz,
                                throughput.clone(),
                            )
                        })
                    })
                    .collect();
                let verdicts = nvoc_core::rank_sweep(&floor, &measured);
                // Throughput numbers only mean something if the windows were
                // busy. When the card never left idle the ranking is a ranking
                // of near-idle compute, so no winner is promoted and the note
                // says why -- the numbers stay in the report.
                let windows_loaded = load.evidence.demonstrated
                    && baseline_repeat
                        .as_ref()
                        .is_none_or(|run| run.evidence.demonstrated)
                    && runs.iter().all(|(_, run)| run.evidence.demonstrated);
                Some(json!({
                    "schema": "nvoc/macos-sweep/1",
                    "baseline_windows": floor,
                    "baseline_clock": load.clock,
                    "repeat_baseline_clock": baseline_repeat.and_then(|run| run.clock.clone()),
                    "candidates_measured": measured.len(),
                    "candidates_requested": runs.len(),
                    "windows_loaded": windows_loaded,
                    "baseline_load_evidence": load.evidence,
                    "candidate_load_evidence": runs
                        .iter()
                        .map(|(request, run)| json!({
                            "target": target_label(&request.target),
                            "delta_khz": request.delta_khz,
                            "evidence": run.evidence,
                            "clock": run.clock,
                        }))
                        .collect::<Vec<_>>(),
                    "verdicts": verdicts,
                    "winner": if windows_loaded {
                        verdicts.iter().find(|v| v.beats_noise)
                    } else {
                        None
                    },
                    "note": if windows_loaded {
                        nvoc_core::SWEEP_NOTE
                    } else {
                        nvoc_core::SWEEP_IDLE_NOTE
                    },
                }))
            }
            None => None,
        }
    } else {
        None
    };
    // Stage 12: more than one round asks the only question a single round cannot
    // answer -- does the winner happen again? Each round is folded with its own
    // baseline repeat, and the fold refuses to pool rounds to clear the bar.
    let repeat = if rounds_wanted > 1 && runs.len() > 1 {
        let mut per_round: Vec<Vec<nvoc_core::SweepVerdict>> = Vec::new();
        // Each round's floor is reported next to its verdicts: when nothing is
        // reproduced the reader needs to see the bar each round was asked to
        // clear, not just that it failed.
        let mut per_round_floors: Vec<nvoc_core::NoiseFloor> = Vec::new();
        for (index, round) in round_runs.iter().enumerate() {
            let repeat_run = round_repeats.get(index).and_then(|run| run.as_ref());
            let mut windows: Vec<nvoc_core::LoadThroughput> = Vec::new();
            windows.extend(load.throughput.clone());
            windows.extend(repeat_run.and_then(|run| run.throughput.clone()));
            let Some(floor) = nvoc_core::noise_floor(&windows) else {
                continue;
            };
            let measured: Vec<(String, i32, nvoc_core::LoadThroughput)> = round
                .iter()
                .filter_map(|(request, run)| {
                    run.throughput.as_ref().map(|throughput| {
                        (
                            target_label(&request.target),
                            request.delta_khz,
                            throughput.clone(),
                        )
                    })
                })
                .collect();
            if measured.is_empty() {
                continue;
            }
            per_round_floors.push(floor.clone());
            per_round.push(nvoc_core::rank_sweep(&floor, &measured));
        }
        let verdicts = nvoc_core::reproduce_sweeps(&per_round);
        // A throughput number whose clock did not move by the same amount is
        // worth seeing next to the number, not instead of it: the offset window
        // is small enough that a report without clocks cannot tell "the offset
        // did nothing" from "the load did not follow the clock".
        let mut round_clocks: Vec<Value> = Vec::new();
        for (index, round) in round_runs.iter().enumerate() {
            let repeat_run = round_repeats.get(index).and_then(|run| run.as_ref());
            round_clocks.push(json!({
                "baseline": repeat_run.and_then(|run| run.clock.clone()),
                "candidates": round
                    .iter()
                    .map(|(request, run)| json!({
                        "target": target_label(&request.target),
                        "delta_khz": request.delta_khz,
                        "clock": run.clock,
                    }))
                    .collect::<Vec<_>>(),
            }));
        }
        let threshold = per_round_floors
            .iter()
            .map(|floor| floor.threshold_percent)
            .fold(0.0_f64, f64::max);
        let clock_checks: Vec<Value> = verdicts
            .iter()
            .map(|verdict| {
                let mut deltas: Vec<f64> = Vec::new();
                for (index, round) in round_runs.iter().enumerate() {
                    let baseline = round_repeats
                        .get(index)
                        .and_then(|run| run.as_ref())
                        .and_then(|run| run.clock.as_ref());
                    let candidate = round
                        .iter()
                        .find(|(request, _)| {
                            request.delta_khz == verdict.requested_khz
                                && target_label(&request.target) == verdict.target
                        })
                        .and_then(|(_, run)| run.clock.as_ref());
                    if let (Some(baseline), Some(candidate)) = (baseline, candidate)
                        && let Some(percent) = nvoc_core::clock_delta_percent(
                            baseline.median_mhz,
                            candidate.median_mhz,
                        )
                    {
                        deltas.push(percent);
                    }
                }
                let mean_clock = if deltas.is_empty() {
                    None
                } else {
                    Some(deltas.iter().sum::<f64>() / deltas.len() as f64)
                };
                let agreement =
                    nvoc_core::clock_agreement(mean_clock, verdict.percent_mean, threshold);
                let gate = nvoc_core::promotion_gate(verdict.reproduced, Some(&agreement));
                json!({
                    "target": verdict.target,
                    "requested_khz": verdict.requested_khz,
                    "clock_percent_per_round": deltas,
                    "agreement": agreement,
                    "promotion": gate,
                })
            })
            .collect::<Vec<_>>();
        // Promotion asks two questions: did the number come back, and did the
        // clock move by about the same amount. A repeated gain the clock does not
        // account for stays in the report and does not become a winner.
        let winner = verdicts
            .iter()
            .find(|verdict| {
                clock_checks
                    .iter()
                    .find(|check| {
                        check["requested_khz"].as_i64() == Some(i64::from(verdict.requested_khz))
                            && check["target"].as_str() == Some(verdict.target.as_str())
                    })
                    .is_some_and(|check| check["promotion"]["promotable"].as_bool() == Some(true))
            })
            .cloned();
        Some(json!({
            "schema": "nvoc/macos-repeat/1",
            "rounds_requested": rounds_wanted,
            "rounds_measured": per_round.len(),
            "round_floors": per_round_floors,
            "round_starts": round_starts
                .iter()
                .map(|start| json!({
                    "temperature_c": start.temperature_c,
                    "waited_s": start.waited_s,
                    "cooled": start.cooled,
                    "timed_out": start.timed_out,
                }))
                .collect::<Vec<_>>(),
            "round_clocks": round_clocks,
            "clock_checks": clock_checks,
            "verdicts": verdicts,
            "winner": winner,
            "note": nvoc_core::REPEAT_NOTE,
            "promotion_note": "a winner has to repeat *and* have its clock move by about the same amount; a repeated gain the clock does not account for stays in the report and is not promoted",
        }))
    } else {
        None
    };
    // In a repeated sweep the promotion comes from the fold, not from round one:
    // a round-one winner that did not repeat must not stand as the winner.
    if let Some(doc) = &repeat
        && let Some(Value::Object(map)) = &mut sweep
    {
        map.insert("winner".to_owned(), doc["winner"].clone());
        map.insert("winner_source".to_owned(), json!("repeat"));
        map.insert("rounds".to_owned(), json!(rounds_wanted));
    }

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
            "load_source": if opts.dry_run {
                Value::Null
            } else if opts.metal_load {
                json!("metal")
            } else {
                json!("vulkan")
            },
            "stress_elements": opts.stress_elements,
            "stress_iters": opts.stress_iters,
            "stress_verify_sample": opts.stress_verify_sample,
            "candidate_rounds": rounds_wanted,
            "cooldown_to_c": opts.cooldown_to_c,
            "cooldown_timeout_s": opts.cooldown_timeout_s,
            "plan_requests": opts.plan.len(),
        },
        "mode": if opts.dry_run {
            "dry-run"
        } else if runs.len() > 1 {
            "sweep"
        } else if runs.len() == 1 {
            "candidate"
        } else {
            "stock-load"
        },
        "settings_written": round_runs
            .iter()
            .flatten()
            .all(|(_, run)| run.applied.readback_verified && run.restored.is_some())
            && round_runs.iter().flatten().next().is_some(),
        "sweep": sweep,
        "repeat": repeat,
        "write_gate": match &candidate {
            Some(run) => if run.restored.is_some() {
                "write: one candidate offset was applied to one clock domain through \
                 its own 2108-byte record, held for a bounded load, and restored to the \
                 original offset with the driver's read-back on both writes. The record \
                 was never rebuilt by hand -- see the candidate section for the two \
                 reports and whether the round trip settled."
            } else {
                "write attempted, restore NOT read back: the applied offset is not known \
                 to be back where it started. Re-read the offset before trusting the board."
            },
            None => "read-only: no candidate applied. The write path exists \
            (0x2080d01c, the same 2108-byte block as the readable 0x2080901b) with \
            pre-record, read-back and restore implemented, and --apply-candidate \
            drives it; without that flag nothing is written. The measured window is \
            reported in settings (see get-settings notes), and a --plan request is \
            evaluated against that same snapshot without writing anything.",
        },
        "plan": plan,
        "pre": pre,
        "under_load": load.under_load,
        "load_evidence": if opts.dry_run {
            Value::Null
        } else {
            serde_json::to_value(&load.evidence)
                .map_err(|e| error(format!("failed to serialise the load evidence: {e}")))?
        },
        "post": load.post,
        "stressor": load.stressor,
        "under_load_throughput": load.throughput,
        "candidate_throughput": candidate.as_ref().and_then(|run| run.throughput.clone()),
        "candidate_comparison": match (&load.throughput, candidate.as_ref()) {
            (Some(baseline), Some(run)) => run
                .throughput
                .as_ref()
                .and_then(|candidate| throughput_delta(baseline, candidate)),
            _ => None,
        },
        "verifier": verifier,
        "candidate": candidate.as_ref().map(|run| candidate_outcome(
            runs.first().map_or(0, |(request, _)| request.delta_khz),
            &run.applied,
            run.restored.as_ref(),
        )),
        "candidate_under_load": candidate.as_ref().map(|run| &run.under_load),
        "candidate_stressor": candidate.as_ref().map(|run| &run.stressor),
        "candidate_verifier": candidate.as_ref().map(|run| &run.verifier),
        "candidate_post": candidate.as_ref().map(|run| &run.post),
        "candidate_restored_settings": candidate.as_ref().map(|run| &run.post_restore_settings),
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
    if let Some((request, run)) = runs.iter().find(|(_, run)| run.exit_code != Some(0)) {
        return Err(error(format!(
            "the load exited with {:?} while candidate {} was held",
            run.exit_code,
            target_label(&request.target)
        )));
    }
    Ok(())
}
