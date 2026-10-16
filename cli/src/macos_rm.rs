use super::*;

pub(super) fn selected(backend: BackendChoice) -> bool {
    backend == BackendChoice::MacosRm
        || (backend == BackendChoice::Auto && cfg!(target_os = "macos"))
}

pub(super) fn validate(command: Command) -> CliResult<()> {
    if matches!(
        command,
        Command::GetGpuList | Command::GetInfo | Command::GetStatus
    ) {
        Ok(())
    } else {
        Err(CliError::new(format!(
            "{} is unavailable on macOS RM; supported commands: get-gpu-list, get-info, get-status. Performance writes are not implemented.",
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
                _ => unreachable!("validated above"),
            };
            if let Some(object) = value.as_object_mut() {
                object.insert("capabilities".into(), json!({
                    "device_info": true, "pstate_query": true,
                    "telemetry": true, "performance_writes": false,
                }));
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

use std::io::Write;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant, SystemTime};

use nvoc_core::{DeviceInfo, DeviceSample, MacosRmSession};

/// SIGINT flag shared with the async-signal-safe handler below.
static INTERRUPTED: AtomicBool = AtomicBool::new(false);
static INTERRUPT_INSTALLED: AtomicBool = AtomicBool::new(false);

#[cfg(unix)]
unsafe extern "C" {
    fn signal(signum: i32, handler: usize) -> usize;
}

#[cfg(unix)]
extern "C" fn request_stop(_signum: i32) {
    INTERRUPTED.store(true, Ordering::SeqCst);
}

/// Install a SIGINT handler so an unbounded run stops after the current sample
/// and still unwinds the session (mapping, RM chain, fds).
#[cfg(unix)]
fn install_interrupt_handler() {
    const SIGINT: i32 = 2;
    if INTERRUPT_INSTALLED.swap(true, Ordering::SeqCst) {
        return;
    }
    let handler: extern "C" fn(i32) = request_stop;
    // SAFETY: the handler only performs an atomic store, which is
    // async-signal-safe, and SIGINT's default disposition is replaced once.
    unsafe {
        signal(SIGINT, handler as usize);
    }
}

#[cfg(not(unix))]
fn install_interrupt_handler() {}

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

pub(super) struct ContinuousOptions {
    interval: Duration,
    /// 0 = run until interrupted.
    count: u64,
}

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
    if !seconds.is_finite() || seconds <= 0.0 {
        return Err(CliError::new(format!(
            "--interval must be a positive number of seconds, got {raw:?}"
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
    Ok(Some(ContinuousOptions {
        interval: Duration::from_secs_f64(seconds),
        count,
    }))
}

/// (PCI address, device id, RM GPU id) — the identity a reopen must reproduce.
type Identity = (String, u32, u32);

const REOPEN_BACKOFF_MS: [u64; 4] = [0, 500, 1_000, 2_000];

fn open_monitored_session(expected: &Identity) -> CliResult<MacosRmSession> {
    let mut last = String::from("no attempt made");
    for delay in REOPEN_BACKOFF_MS {
        if delay > 0 {
            std::thread::sleep(Duration::from_millis(delay));
        }
        match MacosRmSession::open() {
            Ok(session) => {
                let identity = session.identity();
                if &identity == expected {
                    return Ok(session);
                }
                last = format!(
                    "device identity changed: expected {} device {:04x} rm {}, got {} device {:04x} rm {}",
                    expected.0, expected.1, expected.2, identity.0, identity.1, identity.2
                );
            }
            Err(error) => last = error.to_string(),
        }
    }
    Err(CliError::new(format!(
        "could not open a macOS RM session after {} attempts: {last}",
        REOPEN_BACKOFF_MS.len()
    )))
}

fn emit_sample(
    json: bool,
    sequence: u64,
    host_time: SystemTime,
    duration: Duration,
    gpu_id: u32,
    info: &DeviceInfo,
    sample: &DeviceSample,
) -> CliResult<()> {
    if json {
        let line = super::output::macos_rm_sample_json(
            sequence, host_time, duration, gpu_id, info, sample,
        )?;
        println!("{line}");
    } else {
        for line in super::output::format_macos_rm_sample_human(
            sequence, host_time, duration, gpu_id, info, sample,
        ) {
            println!("{line}");
        }
    }
    // Stream line by line so a long run is observable and a Ctrl-C mid-run
    // still leaves every completed sample on disk.
    std::io::stdout().flush().ok();
    Ok(())
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
    install_interrupt_handler();
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

    let mut session = open_monitored_session(&expected)?;
    let mut sequence: u64 = 0;
    loop {
        if interrupted() {
            eprintln!("nvoc-cli: stopped after {sequence} samples (Ctrl-C)");
            break;
        }
        let started = Instant::now();
        let sample = match session.sample() {
            Ok(sample) => sample,
            Err(error) => {
                eprintln!("nvoc-cli: macOS RM session lost ({error}); reconnecting");
                session = open_monitored_session(&expected)?;
                continue;
            }
        };
        let duration = started.elapsed();
        sequence += 1;
        emit_sample(
            json,
            sequence,
            SystemTime::now(),
            duration,
            target.id.0,
            &info,
            &sample,
        )?;
        if options.count != 0 && sequence >= options.count {
            break;
        }
        if sleep_interruptible(options.interval) {
            eprintln!("nvoc-cli: stopped after {sequence} samples (Ctrl-C)");
            break;
        }
    }
    Ok(RunOutput {
        rendered: String::new(),
        streamed: true,
        exit_code: 0,
    })
}
