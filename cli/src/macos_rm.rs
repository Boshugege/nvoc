use super::*;

pub(super) fn selected(backend: BackendChoice) -> bool {
    backend == BackendChoice::MacosRm
        || (backend == BackendChoice::Auto && cfg!(target_os = "macos"))
}

pub(super) fn validate(command: Command) -> CliResult<()> {
    if matches!(
        command,
        Command::GetGpuList
            | Command::GetInfo
            | Command::GetStatus
            | Command::GetSettings
            | Command::GetThrottleReasons
    ) {
        Ok(())
    } else if command == Command::GetPstateGlobalFreqOffset {
        // The one refusal that would be wrong to leave generic: this backend
        // *does* report offsets, just not as a single named number, so the
        // error has to say which layer is missing rather than "unsupported".
        Err(CliError::new(
            "get-pstate-global-freq-offset is unavailable on macOS RM for a frontend reason, not a driver \
             one: this backend does report offsets, as the device's own per-domain records -- get-settings \
             carries them as clock_offsets (the 0x20809061 document, one entry per domain bit with the \
             frequency it published) and the window as clock_offset_limits_khz (the 0x20809025 range block). \
             What this command needs and the frontend cannot supply yet is the mapping from its --domain name \
             to one of those device bits: no public OGKM header names a clock domain, the directory only \
             stamps bit positions (0..6 and 19..21 on this device), and the single cross-reference, NVAPI's \
             NV_GPU_CLOCK_DOMAIN_ID on Windows, is a hypothesis whose falsification path is a single-domain \
             write. So the list is reported instead of one named number, and set-clock-offset is refused for \
             that same reason plus the 0x1b privilege gate on its write half.",
        ))
    } else {
        Err(CliError::new(format!(
            "{} is unavailable on macOS RM; supported commands: get-gpu-list, get-info, get-status, get-settings, get-throttle-reasons. Performance writes stay gated twice over: the clock SET twin (0x2080D01C, one category byte over from the read: `0x2080901B` + 0x40 on the legacy-non-privileged byte and +1 on the message id) is a 0xC0-0xFF write id, so this unprivileged client is refused with 0x1b before the GSP sees it, and even behind that gate no write is issued until the field identity, the bounded step and the restore path are verified on the device.",
            command.name()
        )))
    }
}

pub(super) fn execute(invocation: &Invocation, command: Command) -> CliResult<Execution> {
    // Refuse unsupported commands before opening IOKit or allocating RM objects.
    validate(command)?;
    let inventory = discover_targets(BackendSet::MacosRm)?;
    let targets = inventory.targets();
    let selected = select_targets(&targets, &gpu_selector(invocation))?;
    if selected.is_empty() {
        return Err(CliError::new("no GPUs matched the selector"));
    }
    let results: Vec<TargetResult> = selected.into_iter().map(|target| {
        let output = target.macos_rm().map_err(CliError::from).and_then(|device| {
            let mut value = match command {
                Command::GetGpuList => json!({
                    "index": target.index, "gpu_id": target.id.0,
                    "gpu_id_hex": format!("0x{:04X}", target.id.0),
                    "pci_bus": target.id.pci_bus(), "pci_address": device.info().pci_address,
                    "rm_gpu_id": device.info().rm_gpu_id, "name": device.info().name,
                    "backend_nvapi": false, "backend_nvml": false, "backend_macos_rm": true,
                }),
                Command::GetInfo => serde_json::to_value(run(&target, nvoc_core::QueryMacosRmInfo)?.output)?,
                Command::GetStatus => serde_json::to_value(run(&target, nvoc_core::QueryMacosRmStatus)?.output)?,
                Command::GetSettings => {
                    serde_json::to_value(run(&target, nvoc_core::QueryMacosRmSettings)?.output)?
                }
                // Decoded by the shared `QueryThrottleReasons` operation (core
                // serves it from the RM snapshot on macOS), so this surface and
                // pynvoc's `query_throttle_reasons` cannot drift apart. NVML
                // also appends per-policy violation counters; the RM backend
                // has no equivalent, so violation stays null like NVML when the
                // device exposes no counters.
                Command::GetThrottleReasons => {
                    let reasons: Vec<serde_json::Value> =
                        run(&target, nvoc_core::QueryThrottleReasons)?
                            .output
                            .into_iter()
                            .map(|reason| json!({"name": reason.name, "active": reason.active}))
                            .collect();
                    json!({ "reasons": reasons, "violation": serde_json::Value::Null })
                }
                _ => unreachable!("validated above"),
            };
            if let Some(object) = value.as_object_mut() {
                // The settings query carries its own capability block; the
                // status/info payloads keep the read-only summary.
                if matches!(
                    command,
                    Command::GetGpuList | Command::GetInfo | Command::GetStatus
                ) {
                    object.insert("capabilities".into(), json!({
                        "device_info": true, "pstate_query": true,
                        "telemetry": true, "performance_writes": false,
                    }));
                }
            }
            Ok(value)
        });
        match output {
            Ok(output) => TargetResult { gpu_id: Some(target.id.0), backend: "macos-rm",
                ok: true, output: Some(output), error: None },
            Err(error) => TargetResult { gpu_id: Some(target.id.0), backend: "macos-rm",
                ok: false, output: None, error: Some(error.to_string()) },
        }
    }).collect();
    let partial = command == Command::GetStatus
        && results.iter().any(|result: &TargetResult| {
            result
                .output
                .as_ref()
                .and_then(|v| v["unavailable"].as_object())
                .is_some_and(|v| !v.is_empty())
        });
    Ok(Execution {
        function: command.name(),
        command,
        backend: "macos-rm".into(),
        warnings: if partial {
            vec!["Partial macOS RM status: unavailable telemetry is null; see unavailable for reasons.".into()]
        } else {
            Vec::new()
        },
        results,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn backend_flag_is_global_and_conflicts_with_other_backends() {
        for args in [["--macos-rm", "get-info"], ["get-info", "--macos-rm"]] {
            assert_eq!(parse_args(args).unwrap().backend, BackendChoice::MacosRm);
        }
        for other in ["--nvapi", "--nvml"] {
            assert!(parse_args(["get-info", "--macos-rm", other]).is_err());
        }
    }

    #[test]
    fn unsupported_commands_are_rejected_before_driver_access() {
        let invocation = parse_args(["set-power-limit", "100", "--macos-rm"]);
        assert!(
            invocation
                .unwrap_err()
                .to_string()
                .contains("unavailable on macOS RM")
        );
        // Also check callers constructing Invocations without the parser.
        let mut invocation = parse_args(["get-info", "--macos-rm"]).unwrap();
        invocation.command = Some(Command::SetPowerLimit);
        let error = run_invocation(&invocation).unwrap_err().to_string();
        assert!(error.contains("unavailable on macOS RM"));
        assert!(!error.contains("driver loaded"));
    }

    #[test]
    fn settings_is_readable_but_writes_stay_refused() {
        assert!(validate(Command::GetSettings).is_ok());
        // Stage-8 read face: throttle reasons are RM-backed now, so the
        // dedicated command joins the macOS allow-list.
        assert!(validate(Command::GetThrottleReasons).is_ok());
        for command in [
            Command::SetPowerLimit,
            Command::SetPstateGlobalFreqOffset,
            Command::SetTempLimit,
            Command::SetFanSpeed,
        ] {
            let error = validate(command).unwrap_err().to_string();
            assert!(error.contains("unavailable on macOS RM"), "{error}");
        }
    }

    #[test]
    fn the_offset_read_refusal_names_the_missing_layer_and_the_working_route() {
        // Stage 11 item 5: an entry must not read as "the backend cannot do
        // this" when the backend does report the data through another surface.
        // The refusal therefore names the frontend mapping that is missing and
        // the two fields that already carry the numbers.
        let error = validate(Command::GetPstateGlobalFreqOffset)
            .unwrap_err()
            .to_string();
        for expected in [
            "unavailable on macOS RM",
            "frontend reason, not a driver",
            "clock_offsets",
            "clock_offset_limits_khz",
            "NV_GPU_CLOCK_DOMAIN_ID",
        ] {
            assert!(error.contains(expected), "{expected} missing from {error}");
        }
        // The write half of the same pair keeps the terse capability refusal:
        // there the driver really does refuse, so no layer needs naming.
        let write = validate(Command::SetPstateGlobalFreqOffset)
            .unwrap_err()
            .to_string();
        assert!(write.contains("unavailable on macOS RM"), "{write}");
        assert!(!write.contains("frontend reason, not a driver"), "{write}");
    }

    #[test]
    fn auto_uses_platform_backend_without_changing_explicit_choices() {
        assert_eq!(selected(BackendChoice::Auto), cfg!(target_os = "macos"));
        assert!(selected(BackendChoice::MacosRm));
        assert!(!selected(BackendChoice::Nvapi));
        assert!(!selected(BackendChoice::Nvml));
    }

    #[cfg(not(all(target_os = "macos", target_arch = "x86_64")))]
    #[test]
    fn explicit_backend_on_other_platforms_reports_requirement() {
        let invocation = parse_args(["get-info", "--macos-rm"]).unwrap();
        assert!(
            run_invocation(&invocation)
                .unwrap_err()
                .to_string()
                .contains("requires x86_64 macOS")
        );
    }
}

// ---------------------------------------------------------------------------
// Continuous macOS RM monitoring (`get-status --interval [--count]`).
// ---------------------------------------------------------------------------

use nvoc_core::MacosRmSession;
use std::io::{ErrorKind, Write};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant, SystemTime};

/// SIGINT flag shared with the async-signal-safe handler below.
static INTERRUPTED: AtomicBool = AtomicBool::new(false);

#[cfg(unix)]
unsafe extern "C" {
    fn signal(signum: i32, handler: usize) -> usize;
}

#[cfg(unix)]
extern "C" fn request_stop(_signum: i32) {
    INTERRUPTED.store(true, Ordering::SeqCst);
}

/// Installs the SIGINT handler for one continuous run and restores the previous
/// disposition on drop, so a library caller (or a second run in the same
/// process) is not left with a modified signal table. The handler only stores
/// an atomic flag, which keeps it async-signal-safe. Nothing here (or anywhere
/// in the Rust layer) can run for SIGKILL or a hard crash.
struct InterruptGuard {
    #[cfg(unix)]
    previous: usize,
}

#[cfg(unix)]
impl InterruptGuard {
    fn install() -> Self {
        const SIGINT: i32 = 2;
        // A new run must not inherit a stop request from an earlier one.
        INTERRUPTED.store(false, Ordering::SeqCst);
        let handler: extern "C" fn(i32) = request_stop;
        // SAFETY: signal() swaps one disposition for a handler that performs a
        // single atomic store; the previous disposition is restored on drop.
        let previous = unsafe { signal(SIGINT, handler as usize) };
        Self { previous }
    }
}

#[cfg(unix)]
impl Drop for InterruptGuard {
    fn drop(&mut self) {
        const SIGINT: i32 = 2;
        // SAFETY: restores exactly the disposition signal() reported.
        unsafe {
            signal(SIGINT, self.previous);
        }
    }
}

#[cfg(not(unix))]
impl InterruptGuard {
    fn install() -> Self {
        INTERRUPTED.store(false, Ordering::SeqCst);
        Self {}
    }
}

fn interrupted() -> bool {
    INTERRUPTED.load(Ordering::SeqCst)
}

/// Sleep, returning early when Ctrl-C arrives.
fn sleep_interruptible(duration: Duration) -> bool {
    let deadline = Instant::now() + duration;
    while Instant::now() < deadline {
        if interrupted() {
            return true;
        }
        let remaining = deadline.saturating_duration_since(Instant::now());
        std::thread::sleep(remaining.min(Duration::from_millis(100)));
    }
    interrupted()
}

#[derive(Debug)]
pub(super) struct ContinuousOptions {
    interval: Duration,
    /// 0 = run until interrupted.
    count: u64,
}

/// Sampling cadence: one sample, then `interval` of rest, so the observed
/// period is the interval plus the sampling cost (measured: `--interval 1`
/// gives 1.000-1.011 s gaps, `--interval 2` gives 2.001-2.011 s).
///
/// Intervals below 1 ms are refused: they would busy-loop the driver instead of
/// sampling, and `Duration::from_secs_f64` rounds sub-nanosecond values to
/// zero. Values above a day are refused as typos; the same bound keeps
/// `Duration::from_secs_f64` from panicking on overflow, which in a release
/// build (`panic = "abort"`) would abort the process and skip session cleanup.
const MIN_INTERVAL_SECS: f64 = 0.001;
const MAX_INTERVAL_SECS: f64 = 86_400.0;

/// Parse `--interval`/`--count`. `Ok(None)` keeps the one-shot path.
pub(super) fn continuous_options(invocation: &Invocation) -> CliResult<Option<ContinuousOptions>> {
    let interval = option_one(invocation, "interval");
    let count = option_one(invocation, "count");
    if interval.is_none() && count.is_none() {
        return Ok(None);
    }
    let raw = interval.ok_or_else(|| {
        CliError::new("--count requires --interval (continuous macOS RM monitoring)")
    })?;
    let seconds: f64 = raw
        .parse()
        .map_err(|_| CliError::new(format!("invalid --interval {raw:?}: expected seconds")))?;
    if !seconds.is_finite() || !(MIN_INTERVAL_SECS..=MAX_INTERVAL_SECS).contains(&seconds) {
        return Err(CliError::new(format!(
            "--interval must be between {MIN_INTERVAL_SECS} and {MAX_INTERVAL_SECS} seconds, got {raw:?}"
        )));
    }
    let interval = Duration::from_secs_f64(seconds);
    if interval < Duration::from_millis(1) {
        return Err(CliError::new(format!(
            "--interval {raw:?} rounds to zero; use at least 1 ms"
        )));
    }
    let count = match count {
        None => 0,
        Some(raw) => raw.parse::<u64>().map_err(|_| {
            CliError::new(format!(
                "invalid --count {raw:?}: expected a non-negative integer"
            ))
        })?,
    };
    Ok(Some(ContinuousOptions { interval, count }))
}

/// (PCI address, device id, RM GPU id) — the identity a reopen must reproduce.
type Identity = (String, u32, u32);

/// Bounded reopen schedule: four attempts with 0/0.5/1/2 s of backoff, and a
/// hard cap on how many times in a row a lost session is rebuilt. A device that
/// keeps disappearing ends the run with an error instead of reopening the
/// driver forever.
const REOPEN_BACKOFF_MS: [u64; 4] = [0, 500, 1_000, 2_000];
const MAX_CONSECUTIVE_REOPENS: u32 = 8;

enum OpenOutcome {
    Opened(Box<MacosRmSession>),
    /// A stop request arrived while waiting out the backoff.
    Cancelled,
}

fn open_monitored_session(
    expected: &Identity,
    cancelled: &dyn Fn() -> bool,
) -> CliResult<OpenOutcome> {
    let mut last = String::from("no attempt made");
    for delay in REOPEN_BACKOFF_MS {
        if delay > 0 && sleep_interruptible(Duration::from_millis(delay)) {
            return Ok(OpenOutcome::Cancelled);
        }
        let opened = MacosRmSession::open_with(
            nvoc_core::MONITOR_STALE_AFTER,
            Duration::from_millis(1_500),
            cancelled,
        );
        match opened {
            Ok(session) => {
                let identity = session.identity();
                if &identity == expected {
                    return Ok(OpenOutcome::Opened(Box::new(session)));
                }
                let error = CliError::new(format!(
                    "device identity changed: expected {} device {:04x} rm {}, got {} device {:04x} rm {}",
                    expected.0, expected.1, expected.2, identity.0, identity.1, identity.2
                ));
                // Release the unexpected device before retrying.
                if let Err(close) = session.close() {
                    last = format!("{error}; release of the unexpected session failed: {close}");
                } else {
                    last = error.to_string();
                }
            }
            Err(error) => last = error.to_string(),
        }
        if cancelled() {
            return Ok(OpenOutcome::Cancelled);
        }
    }
    Err(CliError::new(format!(
        "could not open a macOS RM session after {} attempts: {last}",
        REOPEN_BACKOFF_MS.len()
    )))
}

/// Tell the user which sections had not published a new value when the session
/// opened. Those fields are reported as `never_refreshed` until the driver
/// publishes, instead of being presented as fresh data. Sections the driver
/// does not publish at all are not "pending": they settle during the open and
/// are reported per sample as `unsupported` with a reason, so they are not
/// listed here.
fn warn_missing_initial_freshness(session: &MacosRmSession) {
    let settled = session.initial_settled();
    let waiting: Vec<&str> = nvoc_core::MONITOR_SECTIONS
        .iter()
        .enumerate()
        .filter(|(index, _)| !settled[*index])
        .map(|(_, name)| *name)
        .collect();
    if !waiting.is_empty() {
        eprintln!(
            "nvoc-cli: warning: {} RUSD section(s) had not published a new value when the session opened ({}); they are reported as never_refreshed until they do",
            waiting.len(),
            waiting.join(", ")
        );
    }
}

enum SampleOutput {
    Written,
    /// The consumer closed stdout (e.g. `| head`): stop cleanly.
    ConsumerGone,
}

fn emit_sample(
    json: bool,
    sequence: u64,
    host_time: SystemTime,
    duration: Duration,
    gpu_id: u32,
    info: &nvoc_core::DeviceInfo,
    sample: &nvoc_core::DeviceSample,
) -> CliResult<SampleOutput> {
    let lines = if json {
        vec![super::output::macos_rm_sample_json(
            sequence, host_time, duration, gpu_id, info, sample,
        )?]
    } else {
        super::output::format_macos_rm_sample_human(
            sequence, host_time, duration, gpu_id, info, sample,
        )
    };
    // Stream line by line so a long run is observable and a Ctrl-C mid-run
    // still leaves every completed sample on disk. Writing through `writeln!`
    // instead of `println!` keeps a closed pipe an error to handle rather than
    // a panic: a release build aborts on panic and would skip session cleanup.
    let mut stdout = std::io::stdout().lock();
    for line in &lines {
        if let Err(error) = writeln!(stdout, "{line}") {
            if error.kind() == ErrorKind::BrokenPipe {
                return Ok(SampleOutput::ConsumerGone);
            }
            return Err(CliError::new(format!("cannot write sample: {error}")));
        }
    }
    if let Err(error) = stdout.flush() {
        if error.kind() == ErrorKind::BrokenPipe {
            return Ok(SampleOutput::ConsumerGone);
        }
        return Err(CliError::new(format!("cannot flush sample: {error}")));
    }
    Ok(SampleOutput::Written)
}

/// Build the successful "we stopped" result: continuous runs that end on a stop
/// request, a sample count or a closed consumer are not failures.
fn stopped(sequence: u64, reason: &str) -> RunOutput {
    eprintln!("nvoc-cli: stopped after {sequence} samples ({reason})");
    RunOutput {
        rendered: String::new(),
        streamed: true,
        exit_code: 0,
    }
}

pub(super) fn execute_continuous(
    invocation: &Invocation,
    options: ContinuousOptions,
) -> CliResult<RunOutput> {
    if !selected(invocation.backend) {
        return Err(CliError::new(
            "--interval/--count continuous monitoring requires the macOS RM backend",
        ));
    }
    let _interrupt = InterruptGuard::install();
    let json = invocation.output == OutputFormat::Json;
    let inventory = discover_targets(BackendSet::MacosRm)?;
    let targets = inventory.targets();
    if targets.is_empty() {
        return Err(CliError::new("no macOS RM GPU found"));
    }
    let selected_targets = select_targets(&targets, &gpu_selector(invocation))?;
    let target = match selected_targets.as_slice() {
        [target] => *target,
        _ => {
            return Err(CliError::new(
                "continuous monitoring needs exactly one target; narrow it with --gpu",
            ));
        }
    };
    let info = target.macos_rm().map_err(CliError::from)?.info().clone();
    let expected: Identity = (info.pci_address.clone(), info.device_id, info.rm_gpu_id);
    let cancelled = || interrupted();

    let mut session = match open_monitored_session(&expected, &cancelled)? {
        OpenOutcome::Opened(session) => Some(*session),
        OpenOutcome::Cancelled => return Ok(stopped(0, "Ctrl-C")),
    };
    if let Some(session) = session.as_ref() {
        warn_missing_initial_freshness(session);
    }

    let mut sequence: u64 = 0;
    let mut consecutive_reopens: u32 = 0;
    loop {
        if interrupted() {
            break;
        }
        let live = session
            .as_mut()
            .expect("a live session is always present inside the loop");
        let started = Instant::now();
        let sample = match live.sample() {
            Ok(sample) => {
                consecutive_reopens = 0;
                sample
            }
            Err(error) => {
                if consecutive_reopens >= MAX_CONSECUTIVE_REOPENS {
                    return Err(CliError::new(format!(
                        "macOS RM session lost ({error}) and {consecutive_reopens} consecutive reconnects did not help"
                    )));
                }
                consecutive_reopens += 1;
                eprintln!(
                    "nvoc-cli: macOS RM session lost ({error}); reconnecting ({consecutive_reopens}/{MAX_CONSECUTIVE_REOPENS})"
                );
                // Release the old session *before* opening a new one: the RM
                // transport and the driver's poller are process-global, so two
                // live chains would compete for the same resources.
                let old = session
                    .take()
                    .expect("a live session is always present inside the loop");
                let release = old.close().err();
                if cancelled() {
                    break;
                }
                match open_monitored_session(&expected, &cancelled)? {
                    OpenOutcome::Opened(new) => {
                        if let Some(error) = release {
                            eprintln!(
                                "nvoc-cli: warning: the previous session reported a release failure: {error}"
                            );
                        }
                        warn_missing_initial_freshness(&new);
                        session = Some(*new);
                    }
                    OpenOutcome::Cancelled => break,
                }
                continue;
            }
        };
        let duration = started.elapsed();
        sequence += 1;
        match emit_sample(
            json,
            sequence,
            SystemTime::now(),
            duration,
            target.id.0,
            &info,
            &sample,
        )? {
            SampleOutput::Written => {}
            SampleOutput::ConsumerGone => return Ok(stopped(sequence, "stdout closed")),
        }
        if options.count != 0 && sequence >= options.count {
            break;
        }
        if sleep_interruptible(options.interval) {
            break;
        }
    }
    // Explicit release: a mapping that cannot be unmapped is reported instead
    // of being swallowed, and the caller still gets a clean exit afterwards.
    if let Some(session) = session.take()
        && let Err(error) = session.close()
    {
        return Err(CliError::new(format!("macOS RM session release: {error}")));
    }
    let reason = if interrupted() {
        "Ctrl-C"
    } else {
        "sample count"
    };
    Ok(stopped(sequence, reason))
}

#[cfg(test)]
mod continuous_tests {
    use super::*;

    fn options(args: &[&str]) -> CliResult<Option<ContinuousOptions>> {
        let invocation = parse_args(args.iter().copied()).unwrap();
        continuous_options(&invocation)
    }

    #[test]
    fn single_shot_commands_never_enter_the_continuous_path() {
        assert!(options(&["get-status", "--macos-rm"]).unwrap().is_none());
        assert!(
            options(&["get-status", "--macos-rm", "--verbose"])
                .unwrap()
                .is_none()
        );
    }

    #[test]
    fn interval_accepts_fractional_seconds_and_defaults_count_to_unbounded() {
        let parsed = options(&["get-status", "--macos-rm", "--interval", "0.5"])
            .unwrap()
            .unwrap();
        assert_eq!(parsed.interval, Duration::from_millis(500));
        assert_eq!(parsed.count, 0);
        let parsed = options(&[
            "get-status",
            "--macos-rm",
            "--interval",
            "1",
            "--count",
            "3",
        ])
        .unwrap()
        .unwrap();
        assert_eq!(parsed.interval, Duration::from_secs(1));
        assert_eq!(parsed.count, 3);
    }

    #[test]
    fn interval_boundaries_are_refused_instead_of_rounding_or_panicking() {
        for bad in ["0", "0.0001", "nan", "inf", "abc", "1e30", "86401"] {
            let error = options(&["get-status", "--macos-rm", "--interval", bad])
                .unwrap_err()
                .to_string();
            assert!(
                error.contains("--interval must be between")
                    || error.contains("rounds to zero")
                    || error.contains("expected seconds"),
                "{bad}: {error}"
            );
        }
        // A leading "-" is an option to the parser, so negatives never reach
        // the interval logic at all.
        for bad in ["-1", "-inf"] {
            assert!(
                parse_args(["get-status", "--macos-rm", "--interval", bad]).is_err(),
                "{bad} must be rejected as an option"
            );
        }
        // One millisecond and one day are both accepted.
        assert_eq!(
            options(&["get-status", "--macos-rm", "--interval", "0.001"])
                .unwrap()
                .unwrap()
                .interval,
            Duration::from_millis(1)
        );
        assert_eq!(
            options(&["get-status", "--macos-rm", "--interval", "86400"])
                .unwrap()
                .unwrap()
                .interval,
            Duration::from_secs(86_400)
        );
    }

    #[test]
    fn count_requires_interval_and_rejects_non_integers() {
        let error = options(&["get-status", "--macos-rm", "--count", "3"])
            .unwrap_err()
            .to_string();
        assert!(error.contains("--count requires --interval"), "{error}");
        let error = options(&[
            "get-status",
            "--macos-rm",
            "--interval",
            "1",
            "--count",
            "abc",
        ])
        .unwrap_err()
        .to_string();
        assert!(error.contains("invalid --count"), "{error}");
        // A negative count is an option to the parser, never a value we parse.
        assert!(
            parse_args([
                "get-status",
                "--macos-rm",
                "--interval",
                "1",
                "--count",
                "-1"
            ])
            .is_err()
        );
        // Explicit zero means "until stopped".
        assert_eq!(
            options(&[
                "get-status",
                "--macos-rm",
                "--interval",
                "1",
                "--count",
                "0"
            ])
            .unwrap()
            .unwrap()
            .count,
            0
        );
    }
}
