//! The control loop: one OS thread ticking every `interval_ms`, plus an
//! independent heartbeat watchdog.
//!
//! The legacy service drove this from a compio async runtime; the workload
//! is timer + two channels, which a plain thread with `recv_deadline`
//! expresses without the async machinery — and the synchronous design lets
//! the watchdog share the backend via `Arc<Mutex<_>>` so a hung loop still
//! gets its fans un-pinned.

use crate::backend::{GPU_INDEX_MAX, NvapiBackend};
use crate::config::{ControlMode, RuntimeConfig};
use crate::controller::{ControlBackend, GpuController};
use crate::pid::PidController;
use flume::{Receiver, Sender};
use log::{error, info, warn};
use std::sync::{Arc, Mutex, MutexGuard};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

pub type SharedConfig = Arc<Mutex<RuntimeConfig>>;
pub type SharedStatus = Arc<Mutex<Vec<crate::controller::GpuControlStatus>>>;
pub type SharedHeartbeat = Arc<Mutex<Instant>>;
/// Backend handle shared with the HTTP plane (`None` before discovery /
/// after a failed TDR re-discovery — the API layer answers 503).
///
/// Boxed trait object so the platform backend is chosen at discovery: the
/// NVAPI(+NVML) backend everywhere else, the read-only macOS RM backend on
/// macOS. The HTTP plane only ever touches [`ControlBackend`] methods.
pub type SharedBackend = Arc<Mutex<Option<Box<dyn ControlBackend>>>>;
pub type SharedHistory = Arc<Mutex<crate::history::History>>;

/// Consecutive all-GPU-failure ticks before a TDR recovery is attempted.
const HARD_FAIL_REDISCOVER_TICKS: u32 = 5;
/// Minimum spacing between recovery attempts.
const RECOVERY_COOLDOWN: Duration = Duration::from_secs(30);

/// Imperative commands from the HTTP plane that are not pure config edits.
#[derive(Debug, Clone)]
pub enum ServiceCmd {
    /// Legacy `/oc_global`: one-shot P0 graphics clock delta.
    SetOcGlobal { gpu_index: usize, delta_khz: i32 },
    /// Graceful loop exit (fans restored first).
    Shutdown,
}

/// Everything a service/foreground entrypoint needs to wire the loop up.
pub struct LoopHandles {
    pub config: SharedConfig,
    pub status: SharedStatus,
    pub heartbeat: SharedHeartbeat,
    pub backend: SharedBackend,
    pub history: SharedHistory,
    pub cmd_tx: Sender<ServiceCmd>,
    pub cmd_rx: Receiver<ServiceCmd>,
    pub shutdown_tx: Sender<()>,
    pub shutdown_rx: Receiver<()>,
}

pub fn setup(config: RuntimeConfig) -> LoopHandles {
    let (cmd_tx, cmd_rx) = flume::unbounded();
    let (shutdown_tx, shutdown_rx) = flume::unbounded();
    LoopHandles {
        config: Arc::new(Mutex::new(config)),
        status: Arc::new(Mutex::new(Vec::new())),
        heartbeat: Arc::new(Mutex::new(Instant::now())),
        backend: Arc::new(Mutex::new(None)),
        history: Arc::new(Mutex::new(crate::history::History::new(0))),
        cmd_tx,
        cmd_rx,
        shutdown_tx,
        shutdown_rx,
    }
}

/// Mutex access that survives a panic in any other user of the shared state
/// (a long-lived service must not brick its control plane on one poison).
pub fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

/// Discover the platform backend: the read-only native macOS RM backend on
/// macOS (no NVAPI/NVML runtime exists there), the NVAPI(+NVML) backend
/// everywhere else. Both are returned as the same trait object so the whole
/// loop, watchdog and HTTP plane stay backend-agnostic.
fn discover_backend() -> Result<Box<dyn ControlBackend>, String> {
    #[cfg(target_os = "macos")]
    {
        let backend: Box<dyn ControlBackend> = Box::new(crate::macos::MacosRmBackend::discover()?);
        Ok(backend)
    }
    #[cfg(not(target_os = "macos"))]
    {
        let backend: Box<dyn ControlBackend> = Box::new(NvapiBackend::discover()?);
        Ok(backend)
    }
}

fn spawn_watchdog(
    backend: SharedBackend,
    config: SharedConfig,
    heartbeat: SharedHeartbeat,
) -> JoinHandle<()> {
    std::thread::Builder::new()
        .name("nvoc-watchdog".into())
        .spawn(move || {
            loop {
                std::thread::sleep(Duration::from_secs(5));
                let stale_for = lock(&heartbeat).elapsed();
                let timeout = Duration::from_secs(lock(&config).watchdog_timeout_s);
                if stale_for <= timeout {
                    continue;
                }
                error!(
                    "watchdog: control-loop heartbeat stale for {stale_for:?} (> {timeout:?}); \
                 restoring fan control to driver"
                );
                // Never block behind the control thread: it may be wedged
                // inside a driver call (TDR / hang) while holding the mutex.
                // Bounded try_lock keeps the watchdog alive to report —
                // restoring through a wedged driver is impossible from here
                // anyway, so a process-level restart (SCM failure actions /
                // systemd Restart) is the backstop for that case.
                let deadline = Instant::now() + Duration::from_secs(10);
                let backend_guard = loop {
                    match backend.try_lock() {
                        Ok(g) => break Some(g),
                        Err(std::sync::TryLockError::Poisoned(p)) => break Some(p.into_inner()),
                        Err(std::sync::TryLockError::WouldBlock) => {
                            if Instant::now() >= deadline {
                                break None;
                            }
                            std::thread::sleep(Duration::from_millis(100));
                        }
                    }
                };
                match backend_guard {
                    Some(mut guard) => {
                        let Some(backend) = guard.as_mut() else {
                            return;
                        };
                        // A read-only backend never pinned anything, so the
                        // watchdog has nothing to hand back (and must not
                        // issue a write it knows will be rejected).
                        if !backend.supports_writes() {
                            continue;
                        }
                        for i in 0..backend.gpu_count() {
                            if let Err(e) = backend.restore_fan_auto(i) {
                                error!("watchdog: GPU {i} restore failed: {e}");
                            }
                        }
                    }
                    None => error!(
                        "watchdog: backend lock held for 10 s — control thread wedged in a \
                         driver call; in-process restore impossible, relying on service restart"
                    ),
                }
            }
        })
        .expect("spawn watchdog thread")
}

/// Blocking control loop. Returns when shutdown is signalled (SCM stop,
/// Ctrl-C, `/shutdown`) or discovery fails; fans are restored to driver
/// control on every exit path that reaches the loop.
pub fn run_control_loop(handles: LoopHandles) -> Result<(), String> {
    let discovered_backend = discover_backend()?;
    let discovered = discovered_backend.gpu_count();
    let backend_kind = discovered_backend.backend_kind();
    let write_capable = discovered_backend.supports_writes();
    *lock(&handles.backend) = Some(discovered_backend);
    *lock(&handles.history) = crate::history::History::new(discovered);
    let backend = handles.backend.clone();

    // A backend with no actuator surface (macOS RM) must never be asked to
    // actuate: normalise the shared mode to Auto so neither the startup
    // config nor a later HTTP mutation leaves the service reporting a control
    // mode it cannot honour. The per-tick guard below is the hard backstop.
    let cfg0 = {
        let mut cfg = lock(&handles.config);
        if !write_capable && cfg.mode != ControlMode::Auto {
            warn!("{backend_kind} backend is read-only; forcing control mode Auto (observe only)");
            cfg.mode = ControlMode::Auto;
        }
        cfg.clone()
    };
    let selected = cfg0.selected_indices(discovered);
    let mut controllers: Vec<GpuController> = selected
        .iter()
        .map(|_| GpuController::new(PidController::from_params(&cfg0.pid), cfg0.loop_kind))
        .collect();
    info!(
        "control loop: backend {backend_kind}, {} of {discovered} GPU(s) under control, \
         mode {:?}, interval {} ms",
        selected.len(),
        cfg0.mode,
        cfg0.interval_ms
    );
    *lock(&handles.heartbeat) = Instant::now();
    spawn_watchdog(
        backend.clone(),
        handles.config.clone(),
        handles.heartbeat.clone(),
    );

    let LoopHandles {
        config,
        status,
        heartbeat,
        cmd_rx,
        shutdown_rx,
        ..
    } = handles;

    let mut hard_fail_streak: u32 = 0;
    let mut last_recovery: Option<Instant> = None;

    loop {
        let interval = Duration::from_millis(lock(&config).interval_ms);
        let deadline = Instant::now() + interval;
        let mut stop = false;

        while Instant::now() < deadline {
            match shutdown_rx.try_recv() {
                Ok(()) | Err(flume::TryRecvError::Disconnected) => {
                    stop = true;
                    break;
                }
                Err(flume::TryRecvError::Empty) => {}
            }
            match cmd_rx.recv_deadline(deadline) {
                Ok(ServiceCmd::Shutdown) => {
                    stop = true;
                    break;
                }
                Ok(cmd) => handle_cmd(cmd, &backend),
                // HTTP plane gone: keep controlling; the watchdog owns failsafe.
                Err(flume::RecvTimeoutError::Disconnected) => {}
                Err(flume::RecvTimeoutError::Timeout) => break,
            }
        }
        if stop {
            break;
        }

        let mut cfg = lock(&config).clone();
        // Hard backstop: a read-only backend observes only, whatever the
        // shared config says (a write would be rejected by the backend, but
        // silently reporting a mode it cannot honour is worse).
        if !write_capable {
            cfg.mode = ControlMode::Auto;
        }
        let dt_s = cfg.interval_ms as f32 / 1000.0;
        let mut backend_guard = lock(&backend);
        let Some(backend_ref) = backend_guard.as_mut() else {
            error!("backend disappeared; control tick skipped");
            continue;
        };
        let mut snapshot = Vec::with_capacity(selected.len());
        for (slot, &gpu_index) in selected.iter().enumerate() {
            let name = backend_ref.name(gpu_index).into_owned();
            snapshot.push(controllers[slot].tick(gpu_index, &name, &cfg, &mut **backend_ref, dt_s));
        }
        // Dashboard history sample: monitor readback per controlled GPU
        // (best effort — history gaps are fine, control is not affected).
        {
            let mut history = lock(&handles.history);
            for (slot, &gpu_index) in selected.iter().enumerate() {
                let effort = snapshot[slot].fan_written_percent;
                let temp = snapshot[slot].temp_c;
                let monitor = backend_ref.read_monitor(gpu_index).unwrap_or_default();
                history.push(
                    gpu_index,
                    crate::history::sample_from(effort, temp, &monitor),
                );
            }
        }
        drop(backend_guard);

        // TDR / driver-reload recovery: when every controlled GPU failed to
        // read (or errored) for several consecutive ticks, the cached GPU
        // handles and the process-global NVML state are stale. Reset NVML
        // (a plain re-init returns AlreadyInitialized) and rebuild the
        // backend from scratch. Rate-limited so a dead driver yields one
        // attempt per cooldown, not a spin. The controllers' learned state
        // is preserved; the first healthy tick re-pins the fan.
        if !snapshot.is_empty()
            && snapshot
                .iter()
                .all(|s| s.last_error.is_some() || s.temp_c.is_none())
        {
            hard_fail_streak += 1;
        } else {
            hard_fail_streak = 0;
        }
        if hard_fail_streak >= HARD_FAIL_REDISCOVER_TICKS
            && last_recovery.is_none_or(|t| t.elapsed() >= RECOVERY_COOLDOWN)
        {
            warn!(
                "backend unhealthy for {hard_fail_streak} consecutive ticks; \
                 attempting recovery (backend re-discovery)"
            );
            last_recovery = Some(Instant::now());
            // NVML is a Windows/Linux surface; on macOS the only reset is to
            // reopen the RM chain, which re-discovery does.
            #[cfg(not(target_os = "macos"))]
            if let Err(e) = nvoc_core::nvml::force_nvml_shutdown() {
                warn!("recovery: nvmlShutdown failed ({e}); re-init will report the real error");
            }
            match discover_backend() {
                Ok(fresh) => {
                    info!(
                        "recovery: re-discovered {} GPU(s); control state preserved",
                        fresh.gpu_count()
                    );
                    *lock(&backend) = Some(fresh);
                }
                Err(e) => error!("recovery: re-discovery failed ({e}); retrying after cooldown"),
            }
        }

        *lock(&status) = snapshot;
        *lock(&heartbeat) = Instant::now();
    }

    // Every graceful exit hands the fans back to the driver.
    {
        let mut backend_guard = lock(&backend);
        if let Some(backend) = backend_guard.as_mut() {
            for (slot, &gpu_index) in selected.iter().enumerate() {
                controllers[slot].restore(gpu_index, &mut **backend);
            }
        }
    }
    *lock(&heartbeat) = Instant::now();
    info!("control loop stopped; fans restored to driver control");
    Ok(())
}

fn handle_cmd(cmd: ServiceCmd, backend: &Mutex<Option<Box<dyn ControlBackend>>>) {
    let ServiceCmd::SetOcGlobal {
        gpu_index,
        delta_khz,
    } = cmd
    else {
        return; // Shutdown is handled by the loop before dispatch
    };
    if gpu_index > GPU_INDEX_MAX {
        error!("OC command rejected: gpu index {gpu_index} > {GPU_INDEX_MAX}");
        return;
    }
    let mut backend_guard = lock(backend);
    let Some(backend) = backend_guard.as_mut() else {
        error!("OC command rejected: backend not initialized");
        return;
    };
    if gpu_index >= backend.gpu_count() {
        error!(
            "OC command rejected: GPU {gpu_index} out of range (system has {})",
            backend.gpu_count()
        );
        return;
    }
    // `/oc_global` is a legacy NVAPI-only surface: reach it through the
    // downcast hook so a read-only backend reports a refusal instead of the
    // trait growing a method per legacy entry point.
    if backend
        .as_any_mut()
        .downcast_mut::<NvapiBackend>()
        .is_none()
    {
        error!(
            "OC command rejected: {} backend has no global-OC surface (read-only)",
            backend.backend_kind()
        );
        return;
    }
    let nvapi = backend
        .as_any_mut()
        .downcast_mut::<NvapiBackend>()
        .expect("downcast checked immediately above");
    match nvapi.set_oc_global(gpu_index, delta_khz) {
        Ok(()) => info!("GPU {gpu_index}: OC delta {delta_khz} kHz applied"),
        Err(e) => error!("{e}"),
    }
}
