//! Loopback-only HTTP control plane (tiny_http, one supervised thread).
//!
//! CSRF posture unchanged from the legacy service: every mutating endpoint
//! requires POST **and** the non-simple `X-Requested-With: XMLHttpRequest`
//! header, which browsers refuse to attach to cross-origin requests without
//! a successful preflight.

use crate::config::{
    ControlMode, INTERVAL_MS_MAX, INTERVAL_MS_MIN, LoopKind, ManualFan, RuntimeConfig, SensorKind,
};
use crate::controller::GpuControlStatus;
use crate::monitor::{OffsetBackend, OffsetDomain};
use crate::runtime::{ServiceCmd, SharedBackend, SharedConfig, SharedHistory, SharedStatus, lock};
use crate::{audit, auth, web};
use flume::Sender;
use log::{error, info, warn};
use serde::Serialize;
use std::collections::HashMap;
use std::thread::JoinHandle;
use std::time::Duration;
use tiny_http::{Header, Response, Server};

// Accepted OC frequency-delta range (kHz) for /oc_global (legacy semantics).
const OC_DELTA_MIN: i32 = -2_000_000;
const OC_DELTA_MAX: i32 = 2_000_000;

#[derive(Serialize)]
struct StatusView<'a> {
    mode: ControlMode,
    loop_kind: LoopKind,
    /// Control tick period (ms) — the PID dt.
    interval_ms: u64,
    /// Current PID setpoint (°C), for interpreting `pid.error_c`.
    target_c: f32,
    gpus: &'a [GpuControlStatus],
}

/// Decode a percent-encoded byte sequence (application/x-www-form-urlencoded style).
/// `+` → space, `%XX` → byte 0xXX; invalid escapes pass through as-is.
fn percent_decode(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let b = s.as_bytes();
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'+' {
            out.push(' ');
            i += 1;
        } else if b[i] == b'%' && i + 2 < b.len() {
            if let Ok(byte) = u8::from_str_radix(&s[i + 1..i + 3], 16) {
                out.push(byte as char);
                i += 3;
            } else {
                out.push('%');
                i += 1;
            }
        } else {
            out.push(b[i] as char);
            i += 1;
        }
    }
    out
}

/// Parse a `key=value&key=value` query string; duplicate keys keep the last value.
fn parse_query(query: &str) -> HashMap<String, String> {
    query
        .split('&')
        .filter_map(|pair| {
            let mut parts = pair.splitn(2, '=');
            let key = parts.next().filter(|k| !k.is_empty())?;
            let val = parts.next().unwrap_or("");
            Some((percent_decode(key), percent_decode(val)))
        })
        .collect()
}

/// Send a response, logging on failure (client may have disconnected).
fn respond(request: tiny_http::Request, response: Response<std::io::Cursor<Vec<u8>>>) {
    if let Err(e) = request.respond(response) {
        warn!("HTTP: failed to send response: {e}");
    }
}

/// CSRF guard for state-mutating endpoints (POST + non-simple header).
fn is_mutation_request(request: &tiny_http::Request) -> bool {
    if request.method() != &tiny_http::Method::Post {
        return false;
    }
    request.headers().iter().any(|h| {
        h.field.equiv("x-requested-with") && h.value.as_str().eq_ignore_ascii_case("xmlhttprequest")
    })
}

fn reject_mutation(request: tiny_http::Request, path: &str) {
    warn!("Rejected non-POST or missing X-Requested-With on {path}");
    respond(
        request,
        Response::from_string("Method Not Allowed: use POST with X-Requested-With: XMLHttpRequest")
            .with_status_code(405),
    );
}

/// 401 + Basic challenge (browser pops the native login dialog).
fn challenge(request: tiny_http::Request) {
    let header =
        Header::from_bytes("WWW-Authenticate", "Basic realm=\"nvoc-srv\"").expect("valid header");
    respond(
        request,
        Response::from_string("authentication required")
            .with_status_code(401)
            .with_header(header),
    );
}

/// Audit + execute a backend write op from the HTTP plane.
fn audited_write(
    auth_user: &str,
    action: &str,
    detail: String,
    op: impl FnOnce() -> Result<(), String>,
) -> tiny_http::Response<std::io::Cursor<Vec<u8>>> {
    audited_write_reported(auth_user, action, detail, op, || None)
}

/// `audited_write` plus one line the caller reads back *after* the write landed.
///
/// `report` runs only on success and only for what can still be read at that
/// point -- a value that was written is not evidence it was stored, so the note
/// exists to carry an independent read back to the client instead of trusting
/// the write's own return value.
fn audited_write_reported(
    auth_user: &str,
    action: &str,
    detail: String,
    op: impl FnOnce() -> Result<(), String>,
    report: impl FnOnce() -> Option<String>,
) -> tiny_http::Response<std::io::Cursor<Vec<u8>>> {
    match op() {
        Ok(()) => {
            audit::record(auth_user, action, &detail);
            match report() {
                Some(note) => text_response_body(format!("OK: {action} {detail}\n{note}")),
                None => text_response_body(format!("OK: {action} {detail}")),
            }
        }
        Err(e) => text_response_body(format!("ERR: {e}")),
    }
}

/// The locked window a stored table already carries: its newest record.
///
/// `None` when there is no table, or when the newest record cannot be read --
/// never a window of zero, which is a real value the driver refuses.
fn locked_window_from_history(
    history: Option<&crate::controller::LockedHistory>,
) -> Option<(u32, u32)> {
    let history = history?;
    let (min_mhz, max_mhz) = *history.windows.last()?;
    (min_mhz != 0 && max_mhz != 0).then_some((min_mhz, max_mhz))
}

/// The locked window as `/api/oc` publishes it: `null`, or both ends in MHz.
fn locked_window_json(window: Option<(u32, u32)>) -> serde_json::Value {
    match window {
        Some((min_mhz, max_mhz)) => {
            serde_json::json!({"min_mhz": min_mhz, "max_mhz": max_mhz})
        }
        None => serde_json::Value::Null,
    }
}

/// The stored table as `/api/oc` publishes it: `null` when there is no table.
fn locked_history_json(history: Option<&crate::controller::LockedHistory>) -> serde_json::Value {
    let Some(history) = history else {
        return serde_json::Value::Null;
    };
    serde_json::json!({
        "count_at_4": history.count_at_4,
        // Null when the table and its counter agree; see LockedHistory::anomaly.
        "anomaly": history.anomaly,
        "windows": history
            .windows
            .iter()
            .map(|(min_mhz, max_mhz)| serde_json::json!({"min_mhz": min_mhz, "max_mhz": max_mhz}))
            .collect::<Vec<_>>(),
    })
}

/// What the driver stores after a locked-clock write, as one line for the client.
///
/// `Ok(None)` and `Err` both mean "this backend cannot say", and say so: a
/// backend that has no stored-state read must not be reported as agreeing with
/// the write. Only a stored window that equals the request reads as a match.
fn stored_window_note(
    stored: Result<Option<(u32, u32)>, String>,
    min_mhz: u32,
    max_mhz: u32,
) -> Option<String> {
    match stored {
        Ok(Some((stored_min, stored_max))) if (stored_min, stored_max) == (min_mhz, max_mhz) => {
            Some(format!(
                "stored window: {stored_min}..{stored_max} MHz (matches the request)"
            ))
        }
        Ok(Some((stored_min, stored_max))) => Some(format!(
            "stored window: {stored_min}..{stored_max} MHz (differs from the {}..{} MHz request)",
            min_mhz, max_mhz
        )),
        Ok(None) => Some(
            "stored window: none readable (this backend cannot report stored state)".to_string(),
        ),
        Err(e) => Some(format!("stored window: read failed ({e})")),
    }
}

fn text_response_body(body: String) -> tiny_http::Response<std::io::Cursor<Vec<u8>>> {
    Response::from_string(body)
}

/// `/api/*` — JSON plane: monitoring, history, info, OC writes, audit log.
#[allow(clippy::too_many_arguments)]
fn handle_api(
    request: tiny_http::Request,
    api_path: &str,
    params: &HashMap<String, String>,
    config: &SharedConfig,
    status: &SharedStatus,
    backend: &SharedBackend,
    history: &SharedHistory,
    cmd_tx: &Sender<ServiceCmd>,
    auth_user: &str,
) {
    // cmd_tx is plumbed for future API commands (shutdown/restore live on the
    // legacy routes; new API mutations talk to the backend directly).
    let _ = cmd_tx;
    let gpu_index = || -> Result<usize, String> {
        params
            .get("gpu")
            .ok_or_else(|| "missing 'gpu'".to_string())?
            .parse::<usize>()
            .map_err(|_| "invalid 'gpu'".to_string())
    };

    match (request.method(), api_path) {
        (_, "log") => json_response(request, &audit::snapshot()),

        // ---- reads ----
        (&tiny_http::Method::Get, "status") => {
            let cfg = lock(config);
            let guard = lock(status);
            json_response(
                request,
                &serde_json::json!({
                    "mode": cfg.mode,
                    "loop_kind": cfg.loop_kind,
                    "interval_ms": cfg.interval_ms,
                    "target_c": cfg.pid.target_c,
                    "gpus": &*guard,
                }),
            );
        }
        (&tiny_http::Method::Get, "gpus") => {
            let guard = lock(status);
            let list: Vec<serde_json::Value> = guard
                .iter()
                .map(|g| {
                    serde_json::json!({
                        "index": g.index,
                        "name": g.name,
                        "mode": g.mode,
                        "failsafe": g.failsafe,
                    })
                })
                .collect();
            json_response(request, &serde_json::json!({ "gpus": list }));
        }
        (&tiny_http::Method::Get, "history") => {
            let index = match gpu_index() {
                Ok(v) => v,
                Err(e) => return text_response(request, 400, format!("Bad request: {e}")),
            };
            let seconds = params
                .get("seconds")
                .and_then(|s| s.parse::<u64>().ok())
                .unwrap_or(300)
                .min(3600);
            let guard = lock(history);
            json_response(request, &guard.last(index, seconds));
        }
        (&tiny_http::Method::Get, "info") | (&tiny_http::Method::Get, "vfcurve") => {
            let index = match gpu_index() {
                Ok(v) => v,
                Err(e) => return text_response(request, 400, format!("Bad request: {e}")),
            };
            let mut guard = lock(backend);
            let Some(backend) = guard.as_mut() else {
                return text_response(request, 503, "backend not ready");
            };
            if api_path == "info" {
                match backend.read_gpu_info_json(index) {
                    Ok(v) => json_response(request, &v),
                    Err(e) => text_response(request, 502, e),
                }
            } else {
                match backend.read_vf_curve(index) {
                    Ok(pts) => json_response(
                        request,
                        &serde_json::json!({
                            "points": pts.iter()
                                .map(|(mv, mhz)| serde_json::json!({"mv": mv, "mhz": mhz}))
                                .collect::<Vec<_>>()
                        }),
                    ),
                    Err(e) => text_response(request, 502, e),
                }
            }
        }

        // ---- OC reads for the form ----
        (&tiny_http::Method::Get, "oc") => {
            let index = match gpu_index() {
                Ok(v) => v,
                Err(e) => return text_response(request, 400, format!("Bad request: {e}")),
            };
            let mut guard = lock(backend);
            let Some(backend) = guard.as_mut() else {
                return text_response(request, 503, "backend not ready");
            };
            let core_off = backend
                .read_offset_mhz(index, OffsetDomain::Core, OffsetBackend::Nvml)
                .ok();
            let mem_off = backend
                .read_offset_mhz(index, OffsetDomain::Mem, OffsetBackend::Nvml)
                .ok();
            let power = backend.read_power_limit_w(index).ok().flatten();
            let temp = backend.read_temp_limit_c(index).ok().flatten();
            let window = backend.read_offset_window_mhz(index);
            let clock = backend.read_core_clock_mhz(index);
            let fan = backend.read_fan_window(index);
            // The locked window the transport can report, if any. Null is "this
            // plane publishes none", not a window of zero.
            // One stored-state read per request, not two: a transport that
            // keeps a table answers both questions out of it (the newest record
            // *is* the locked window), and a transport that keeps none is asked
            // the older question. On this backend each read opens an RM session,
            // so asking twice would double the cost of polling this endpoint.
            let locked_history = backend.read_locked_history(index).ok().flatten();
            let locked = match &locked_history {
                Some(history) => locked_window_from_history(Some(history)),
                None => backend.read_locked_window(index).ok().flatten(),
            };
            json_response(
                request,
                &serde_json::json!({
                    "core_offset_mhz": core_off,
                    "mem_offset_mhz": mem_off,
                    // The span the driver itself accepts, and what it is worth at
                    // the clock the card is running now. Both null when the read
                    // did not happen: a missing window is not a window of zero.
                    "offset_window": window.map(|(min_mhz, max_mhz)| {
                        serde_json::json!({
                            "min_mhz": min_mhz,
                            "max_mhz": max_mhz,
                            "potential": nvoc_core::offset_potential(
                                (min_mhz * 1000.0).round() as i32,
                                (max_mhz * 1000.0).round() as i32,
                                clock.map(|mhz| (mhz * 1000.0).round().max(0.0) as u32),
                            ),
                        })
                    }),
                    "power": power.map(|(min, cur, max)| {
                        serde_json::json!({"min_w": min, "current_w": cur, "max_w": max})
                    }),
                    "temp": temp.map(|(min, cur, max)| {
                        serde_json::json!({"min_c": min, "current_c": cur, "max_c": max})
                    }),
                    // The window the transport says is locked now (`locked_window`)
                    // and every window it stores (`locked_history`, oldest first).
                    // Null (not a list, not a window of zero) when the transport
                    // publishes none or keeps no table.
                    "locked_window": locked_window_json(locked),
                    "locked_history": locked_history_json(locked_history.as_ref()),
                    "fan": fan.map(|f| {
                        serde_json::json!({
                            "min_percent": f.min_percent,
                            "max_percent": f.max_percent,
                            "max_rpm": f.max_rpm,
                            "mode": f.mode,
                            "current_percent": f.current_percent,
                            "target_percent": f.target_percent,
                            "rpm": f.rpm,
                        })
                    }),
                }),
            );
        }

        // ---- writes (audited) ----
        (&tiny_http::Method::Post, "oc/offset") => {
            let (index, domain, backend_kind, value) = match oc_offset_params(params) {
                Ok(v) => v,
                Err(e) => return text_response(request, 400, format!("Bad request: {e}")),
            };
            let detail = format!(
                "gpu {index} {domain} {value} MHz via {}",
                backend_kind.as_str()
            );
            let mut guard = lock(backend);
            let Some(backend) = guard.as_mut() else {
                return text_response(request, 503, "backend not ready");
            };
            // Admission before the write: a transport that publishes the span
            // it accepts (macOS RM does) is asked to admit the value first. A
            // value past the span is refused by the driver rather than clamped,
            // so sending it would spend a write to learn what /api/oc already
            // publishes. A transport that publishes no span admits everything:
            // "not told" is not "no span".
            if let Some(refusal) =
                offset_window_refusal(value, backend.read_offset_window_mhz(index))
            {
                log::warn!("oc.offset refused: {refusal}");
                return text_response(request, 409, format!("Refused: {refusal}"));
            }
            let response = audited_write(auth_user, "oc.offset", detail, || {
                backend.write_offset_mhz(index, domain, backend_kind, value)
            });
            respond(request, response);
        }
        (&tiny_http::Method::Post, "oc/power") => {
            let index = match gpu_index() {
                Ok(v) => v,
                Err(e) => return text_response(request, 400, format!("Bad request: {e}")),
            };
            let Some(watts) = params.get("watt").and_then(|s| s.parse::<u32>().ok()) else {
                return text_response(request, 400, "Bad request: 'watt' required");
            };
            let mut guard = lock(backend);
            let Some(backend) = guard.as_mut() else {
                return text_response(request, 503, "backend not ready");
            };
            let detail = format!("gpu {index} {watts} W");
            // The board policy publishes its own span; a value past it is
            // refused here, before the device is touched, exactly as the
            // frequency offset above is.
            if let Some(refusal) = limit_window_refusal(
                watts as i32,
                backend
                    .read_power_limit_w(index)
                    .ok()
                    .flatten()
                    .map(|r| (r.0 as i32, r.2 as i32)),
                "W",
            ) {
                log::warn!("oc.power refused: {refusal}");
                return text_response(request, 409, format!("Refused: {refusal}"));
            }
            let response = audited_write(auth_user, "oc.power", detail, || {
                backend.write_power_limit_w(index, watts)
            });
            respond(request, response);
        }
        (&tiny_http::Method::Post, "oc/temp") => {
            let index = match gpu_index() {
                Ok(v) => v,
                Err(e) => return text_response(request, 400, format!("Bad request: {e}")),
            };
            let Some(c) = params.get("c").and_then(|s| s.parse::<i32>().ok()) else {
                return text_response(request, 400, "Bad request: 'c' required");
            };
            let mut guard = lock(backend);
            let Some(backend) = guard.as_mut() else {
                return text_response(request, 503, "backend not ready");
            };
            let detail = format!("gpu {index} {c} °C");
            if let Some(refusal) = limit_window_refusal(
                c,
                backend
                    .read_temp_limit_c(index)
                    .ok()
                    .flatten()
                    .map(|r| (r.0, r.2)),
                "C",
            ) {
                log::warn!("oc.temp refused: {refusal}");
                return text_response(request, 409, format!("Refused: {refusal}"));
            }
            let response = audited_write(auth_user, "oc.temp", detail, || {
                backend.write_temp_limit_c(index, c)
            });
            respond(request, response);
        }
        (&tiny_http::Method::Post, "oc/locked-clocks") => {
            let index = match gpu_index() {
                Ok(v) => v,
                Err(e) => return text_response(request, 400, format!("Bad request: {e}")),
            };
            let parse = |key: &str| params.get(key).and_then(|s| s.parse::<u32>().ok());
            let (Some(min_mhz), Some(max_mhz)) = (parse("min_mhz"), parse("max_mhz")) else {
                return text_response(
                    request,
                    400,
                    "Bad request: 'min_mhz' and 'max_mhz' required",
                );
            };
            // The window is not clamped: the macOS write path refuses a span the
            // driver does not hold and names it, and the flag pair has one
            // ordering rule the request itself can state.
            if let Some(refusal) = locked_window_refusal(min_mhz, max_mhz) {
                log::warn!("oc.locked_clocks refused: {refusal}");
                return text_response(request, 400, format!("Bad request: {refusal}"));
            }
            let mut guard = lock(backend);
            let Some(backend) = guard.as_mut() else {
                return text_response(request, 503, "backend not ready");
            };
            let detail = format!("gpu {index} {min_mhz}..{max_mhz} MHz");
            // The write happens first so the read-back below borrows the backend
            // only after the write's borrow is over; the report runs on success.
            let write = backend.write_locked_window(index, min_mhz, max_mhz);
            let response = audited_write_reported(
                auth_user,
                "oc.locked_clocks",
                detail,
                || write,
                || stored_window_note(backend.read_locked_window(index), min_mhz, max_mhz),
            );
            respond(request, response);
        }
        (&tiny_http::Method::Post, "reset") => {
            let index = match gpu_index() {
                Ok(v) => v,
                Err(e) => return text_response(request, 400, format!("Bad request: {e}")),
            };
            let Some(kind) = params.get("kind").map(|s| s.as_str()) else {
                return text_response(request, 400, "Bad request: 'kind' required");
            };
            let mut guard = lock(backend);
            let Some(backend) = guard.as_mut() else {
                return text_response(request, 503, "backend not ready");
            };
            let result = match kind {
                "offset_core" | "offset_core_nvml" => {
                    let backend_kind = if kind == "offset_core_nvml" {
                        OffsetBackend::Nvml
                    } else {
                        OffsetBackend::Nvapi
                    };
                    // NVML reset semantics = write 0 on P0.
                    if backend_kind == OffsetBackend::Nvml {
                        backend.write_offset_mhz(index, OffsetDomain::Core, backend_kind, 0)
                    } else {
                        backend.reset_offset(index, OffsetDomain::Core)
                    }
                }
                "offset_mem" => backend.reset_offset(index, OffsetDomain::Mem),
                "power" => backend.reset_power_limit(index),
                "temp" => backend.reset_temp_limit(index),
                other => {
                    return text_response(
                        request,
                        400,
                        format!("Bad request: unknown kind {other:?}"),
                    );
                }
            };
            let detail = format!("gpu {index} {kind}");
            let response = audited_write(auth_user, "reset", detail, || result);
            respond(request, response);
        }

        _ => text_response(request, 404, "Not found"),
    }
}

/// Parse + validate `oc/offset` params: `gpu`, `domain=core|mem`,
/// `backend=nvml|nvapi`, `value=±MHz`.
/// The pre-write admission verdict for an offset write, or `None` to let it run.
///
/// `window` is the span the backend published in MHz (min, max); `None` means the
/// transport publishes none, and an unpublished span refuses nothing.
fn offset_window_refusal(value_mhz: i32, window: Option<(f32, f32)>) -> Option<String> {
    let (min_mhz, max_mhz) = window?;
    let verdict = nvoc_core::offset_verdict(
        value_mhz.saturating_mul(1000),
        (min_mhz * 1000.0).round() as i32,
        (max_mhz * 1000.0).round() as i32,
    );
    verdict.reason
}

/// Refuse a power or thermal value the driver's own span does not hold.
///
/// Same rule as [`offset_window_refusal`] and the same words, because these are
/// the same question about a different unit: `window` is the span the transport
/// publishes (min, max) in `unit`, and `None` (nothing published) refuses
/// nothing.
fn limit_window_refusal(value: i32, window: Option<(i32, i32)>, unit: &str) -> Option<String> {
    let (min, max) = window?;
    nvoc_core::limit_verdict(value, min, max, unit).reason
}

fn oc_offset_params(
    params: &HashMap<String, String>,
) -> Result<(usize, OffsetDomain, OffsetBackend, i32), String> {
    let index = params
        .get("gpu")
        .ok_or("missing 'gpu'")?
        .parse::<usize>()
        .map_err(|_| "invalid 'gpu'")?;
    let domain = params
        .get("domain")
        .and_then(|s| OffsetDomain::parse(s))
        .ok_or("'domain' must be core|mem")?;
    let backend_kind = params
        .get("backend")
        .and_then(|s| OffsetBackend::parse(s))
        .unwrap_or(OffsetBackend::Nvml);
    let raw = params.get("value").ok_or("missing 'value'")?;
    let value = raw
        .trim_end_matches(['m', 'H', 'z'])
        .parse::<i32>()
        .map_err(|_| "invalid 'value'")?;
    if !(-1000..=1000).contains(&value) {
        return Err(format!("'value' must be -1000..1000 MHz, got {value}"));
    }
    Ok((index, domain, backend_kind, value))
}

fn json_content_type() -> Header {
    Header::from_bytes("Content-Type", "application/json").expect("static header is valid ASCII")
}

/// Serve an embedded asset with its content type.
fn serve_static(request: tiny_http::Request, body: &'static str, content_type: &str) {
    let header =
        Header::from_bytes("Content-Type", content_type).expect("static header is valid ASCII");
    respond(request, Response::from_string(body).with_header(header));
}

fn json_response<T: Serialize>(request: tiny_http::Request, value: &T) {
    match serde_json::to_string(value) {
        Ok(json) => respond(
            request,
            Response::from_string(json)
                .with_status_code(200)
                .with_header(json_content_type()),
        ),
        Err(e) => {
            error!("HTTP: serialize failed: {e}");
            respond(
                request,
                Response::from_string("Internal error").with_status_code(500),
            );
        }
    }
}

fn text_response(request: tiny_http::Request, code: u16, msg: impl Into<String>) {
    respond(
        request,
        Response::from_string(msg.into()).with_status_code(code),
    );
}

/// Supervised HTTP thread: restart the accept loop if it ever exits, so a
/// control-plane failure doesn't leave the service running but unresponsive
/// (#67). (Release builds abort on panic — SCM failure actions are the last
/// resort there; this loop covers clean exits and debug-build panics.)
/// Shared plane state threaded through every request.
pub struct ServerState {
    pub config: SharedConfig,
    pub status: SharedStatus,
    pub backend: SharedBackend,
    pub history: SharedHistory,
    pub cmd_tx: Sender<ServiceCmd>,
}

pub fn spawn_supervised(state: std::sync::Arc<ServerState>) -> JoinHandle<()> {
    std::thread::Builder::new()
        .name("nvoc-http".into())
        .spawn(move || {
            loop {
                start_http_server(&state);
                warn!("HTTP server exited; restarting in 1 s");
                std::thread::sleep(Duration::from_secs(1));
            }
        })
        .expect("spawn HTTP thread")
}

pub fn start_http_server(state: &std::sync::Arc<ServerState>) {
    let port = lock(&state.config).port;
    let bind = format!("127.0.0.1:{port}");
    let server = match Server::http(bind.as_str()) {
        Ok(s) => s,
        Err(e) => {
            error!("HTTP server failed to bind on {bind}: {e}");
            return;
        }
    };
    info!("HTTP control plane listening on {bind}");

    for request in server.incoming_requests() {
        let full_url = request.url().to_owned();
        let (path, query_str) = match full_url.split_once('?') {
            Some((p, q)) => (p, q),
            None => (full_url.as_str(), ""),
        };
        let params = parse_query(query_str);
        handle_request(request, path, &params, state);
    }
}

/// Does the active backend accept actuation commands? Before discovery there
/// is no backend and no running loop, so the config plane stays permissive;
/// `run_control_loop` normalises a read-only backend to `Auto` before its
/// first tick, so nothing can be actuated in the gap.
fn writes_supported(backend: &SharedBackend) -> bool {
    match lock(backend).as_ref() {
        Some(active) => active.supports_writes(),
        None => true,
    }
}

fn handle_request(
    request: tiny_http::Request,
    path: &str,
    params: &HashMap<String, String>,
    state: &std::sync::Arc<ServerState>,
) {
    let ServerState {
        config,
        status,
        backend,
        history,
        cmd_tx,
    } = state.as_ref();

    // OS-account authentication gate (whole site — one gate, one audit story).
    let auth_enabled = lock(config).auth.resolves_enabled();
    let mut auth_user = "anonymous".to_string();
    if auth_enabled {
        let header = request
            .headers()
            .iter()
            .find(|h| h.field.equiv("Authorization"))
            .map(|h| h.value.as_str().to_string());
        match header.as_deref() {
            Some(header_value) => {
                let reason = {
                    let cfg = lock(config);
                    auth::authenticate(header_value, &cfg).err()
                };
                match reason {
                    None => auth_user = auth::session_user(header_value).unwrap_or_default(),
                    Some(reason) => {
                        audit::record("-", "login.failed", &reason);
                        warn!("auth: failed login: {reason}");
                        challenge(request);
                        return;
                    }
                }
            }
            None => {
                challenge(request);
                return;
            }
        }
    }

    // API plane (JSON, audited).
    if let Some(api) = path.strip_prefix("/api/") {
        handle_api(
            request, api, params, config, status, backend, history, cmd_tx, &auth_user,
        );
        return;
    }

    match path {
        "/" => {
            // The embedded console page (GET is same-origin safe).
            serve_static(request, web::INDEX_HTML, "text/html; charset=utf-8");
        }
        "/ui.css" => {
            serve_static(request, web::STYLE_CSS, "text/css; charset=utf-8");
        }
        "/ui.js" => {
            serve_static(request, web::APP_JS, "text/javascript; charset=utf-8");
        }

        "/help" => {
            let help = "nvoc-srv control plane\n\
                        GET  /status   — per-GPU temps, fan duty, PID terms, failsafe state\n\
                        GET  /config   — effective runtime configuration\n\
                        POST /pid?target_c=&kp=&ki=&kd=&base_percent=&min_percent=&max_percent=&emergency_delta_c=&idle_delta_c=&temp_guard_c=&min_mhz=&max_mhz=&interval_ms=&sensor=\n\
                        POST /mode?value=auto|pid|manual\n\
                        POST /loop?value=fan_temp|freq_temp|freq_power\n\
                        POST /fan?percent=0-100        (switches to manual)
                        POST /fan?rpm=N | ?mode=stop   (macOS RM: physical rpm target,
                                                        firmware zero-RPM mode)\n\
                        POST /restore                  (alias of /mode?value=auto)\n\
                        POST /oc_global?oc=<kHz>&gpu=<index>\n\
                        POST /shutdown\n\
                        Mutations require POST + X-Requested-With: XMLHttpRequest.\n";
            text_response(request, 200, help);
        }
        "/loop" => {
            if !is_mutation_request(&request) {
                reject_mutation(request, path);
                return;
            }
            match params.get("value").and_then(|v| LoopKind::parse(v)) {
                Some(kind) => {
                    lock(config).loop_kind = kind;
                    info!("control loop set to {kind:?} via HTTP");
                    text_response(request, 200, format!("OK: loop={kind:?}"));
                }
                None => text_response(
                    request,
                    400,
                    "Bad request: 'value' must be fan_temp|freq_temp|freq_power",
                ),
            }
        }
        "/status" => {
            let (mode, loop_kind, interval_ms, target_c) = {
                let cfg = lock(config);
                (cfg.mode, cfg.loop_kind, cfg.interval_ms, cfg.pid.target_c)
            };
            let guard = lock(status);
            json_response(
                request,
                &StatusView {
                    mode,
                    loop_kind,
                    interval_ms,
                    target_c,
                    gpus: &guard,
                },
            );
        }
        "/config" => {
            let cfg = lock(config);
            json_response(request, &*cfg);
        }
        "/pid" => {
            if !is_mutation_request(&request) {
                reject_mutation(request, path);
                return;
            }
            handle_pid_update(request, params, config);
        }
        "/mode" => {
            if !is_mutation_request(&request) {
                reject_mutation(request, path);
                return;
            }
            match params.get("value").map(|s| s.to_ascii_lowercase()) {
                Some(v) if v == "auto" || v == "pid" || v == "manual" => {
                    let mode = match v.as_str() {
                        "auto" => ControlMode::Auto,
                        "pid" => ControlMode::Pid,
                        _ => ControlMode::Manual,
                    };
                    if mode != ControlMode::Auto && !writes_supported(backend) {
                        return text_response(
                            request,
                            409,
                            "Conflict: active backend is read-only (macOS RM); only value=auto is accepted",
                        );
                    }
                    lock(config).mode = mode;
                    info!("control mode set to {mode:?} via HTTP");
                    text_response(request, 200, format!("OK: mode={mode:?}"));
                }
                _ => text_response(request, 400, "Bad request: 'value' must be auto|pid|manual"),
            }
        }
        "/fan" => {
            if !is_mutation_request(&request) {
                reject_mutation(request, path);
                return;
            }
            if !writes_supported(backend) {
                return text_response(
                    request,
                    409,
                    "Conflict: active backend is read-only (macOS RM); manual fan control is unavailable",
                );
            }
            // Three forms, one at a time: a duty percentage, a physical rpm
            // target, or the firmware's zero-RPM stop mode. Refusing an
            // ambiguous request is the point — two forms in one call would
            // leave the configuration saying two things.
            let percent = params.get("percent").is_some();
            let rpm = params.get("rpm").is_some();
            let stop = params
                .get("mode")
                .map(|m| m.eq_ignore_ascii_case("stop"))
                .unwrap_or(false);
            if usize::from(percent) + usize::from(rpm) + usize::from(stop) > 1 {
                return text_response(
                    request,
                    400,
                    "Bad request: give one of 'percent', 'rpm' or 'mode=stop'",
                );
            }
            if stop {
                let mut cfg = lock(config);
                cfg.mode = ControlMode::Manual;
                cfg.manual_fan = ManualFan::Stop;
                info!("manual fan stop (firmware zero-rpm mode) via HTTP");
                return text_response(
                    request,
                    200,
                    "Recorded: manual fanstop; the control loop applies it on its next tick \
                     (the device is not written by this request)",
                );
            }
            if rpm {
                match params.get("rpm").and_then(|s| s.parse::<u32>().ok()) {
                    Some(value) if value > 0 => {
                        let mut cfg = lock(config);
                        cfg.mode = ControlMode::Manual;
                        cfg.manual_fan = ManualFan::Rpm;
                        cfg.manual_rpm = value;
                        info!("manual fan target {value} rpm via HTTP");
                        text_response(
                            request,
                            200,
                            format!(
                                "Recorded: manual {value} rpm; the control loop applies it on its \
                                 next tick (the device is not written by this request)"
                            ),
                        );
                    }
                    _ => text_response(request, 400, "Bad request: 'rpm' must be non-zero"),
                }
            } else {
                match params.get("percent").and_then(|s| s.parse::<u32>().ok()) {
                    Some(p) if p <= 100 => {
                        let mut cfg = lock(config);
                        cfg.mode = ControlMode::Manual;
                        cfg.manual_fan = ManualFan::Percent;
                        cfg.manual_percent = p;
                        info!("manual fan duty {p}% via HTTP");
                        text_response(
                            request,
                            200,
                            format!(
                                "Recorded: manual {p}%; the control loop applies it on its next \
                                 tick (the device is not written by this request)"
                            ),
                        );
                    }
                    _ => text_response(request, 400, "Bad request: 'percent' must be 0–100"),
                }
            }
        }
        "/restore" => {
            if !is_mutation_request(&request) {
                reject_mutation(request, path);
                return;
            }
            lock(config).mode = ControlMode::Auto;
            info!("fan control restored to driver via HTTP /restore");
            text_response(request, 200, "OK: mode=Auto, driver control resumes");
        }
        "/shutdown" => {
            if !is_mutation_request(&request) {
                reject_mutation(request, path);
                return;
            }
            match cmd_tx.send(ServiceCmd::Shutdown) {
                Ok(()) => {
                    info!("shutdown requested via HTTP");
                    text_response(request, 200, "OK: shutting down");
                }
                Err(e) => {
                    error!("HTTP: cannot deliver shutdown: {e}");
                    text_response(request, 500, "Internal error");
                }
            }
        }
        "/oc_global" => {
            if !is_mutation_request(&request) {
                reject_mutation(request, path);
                return;
            }
            handle_oc_global(request, params, cmd_tx);
        }
        _ => text_response(request, 404, "Not found"),
    }
}

/// Assign `params[key]` into `dst` when present; a present-but-invalid value
/// records an error so the whole update is rejected atomically.
fn assign_f32(
    params: &HashMap<String, String>,
    key: &str,
    dst: &mut f32,
    errors: &mut Vec<String>,
) {
    if let Some(raw) = params.get(key) {
        match raw.parse::<f32>() {
            Ok(v) if v.is_finite() => *dst = v,
            _ => errors.push(format!("invalid '{key}'")),
        }
    }
}

fn assign_u32(
    params: &HashMap<String, String>,
    key: &str,
    dst: &mut u32,
    errors: &mut Vec<String>,
) {
    if let Some(raw) = params.get(key) {
        match raw.parse::<u32>() {
            Ok(v) => *dst = v,
            _ => errors.push(format!("invalid '{key}'")),
        }
    }
}

fn assign_bool(
    params: &HashMap<String, String>,
    key: &str,
    dst: &mut bool,
    errors: &mut Vec<String>,
) {
    if let Some(raw) = params.get(key) {
        match raw.to_ascii_lowercase().as_str() {
            "true" | "1" | "on" => *dst = true,
            "false" | "0" | "off" => *dst = false,
            _ => errors.push(format!("invalid '{key}'")),
        }
    }
}

/// Partial PID update; absent parameters keep their current value. Full
/// param-set validation runs after the merge so a bad combination (min>max)
/// is rejected atomically.
fn handle_pid_update(
    request: tiny_http::Request,
    params: &HashMap<String, String>,
    config: &SharedConfig,
) {
    let mut cfg = lock(config);
    let mut errors: Vec<String> = Vec::new();

    // Route the shared parameter names to the ACTIVE loop: `[pid]` for the
    // fan loop, `[freq]` for the frequency loops (units per loop kind).
    match cfg.loop_kind {
        LoopKind::FanTemp => {
            let mut p = cfg.pid.clone();
            assign_f32(params, "target_c", &mut p.target_c, &mut errors);
            assign_f32(params, "kp", &mut p.kp, &mut errors);
            assign_f32(params, "ki", &mut p.ki, &mut errors);
            assign_f32(params, "kd", &mut p.kd, &mut errors);
            assign_f32(params, "base_percent", &mut p.base_percent, &mut errors);
            assign_f32(params, "min_percent", &mut p.min_percent, &mut errors);
            assign_f32(params, "max_percent", &mut p.max_percent, &mut errors);
            assign_f32(
                params,
                "emergency_delta_c",
                &mut p.emergency_delta_c,
                &mut errors,
            );
            assign_f32(params, "idle_delta_c", &mut p.idle_delta_c, &mut errors);
            assign_f32(
                params,
                "write_deadband_percent",
                &mut p.write_deadband_percent,
                &mut errors,
            );
            assign_bool(params, "adaptive_base", &mut p.adaptive_base, &mut errors);
            if errors.is_empty() {
                if let Err(e) = p.validate() {
                    errors.push(e);
                } else {
                    cfg.pid = p;
                }
            }
        }
        LoopKind::FreqTemp | LoopKind::FreqPower => {
            let mut f = cfg.freq.clone();
            // `target_c` and `target` both address the setpoint (unit per
            // loop kind: degC for freq_temp, W for freq_power).
            let target_raw = params
                .get("target_c")
                .or_else(|| params.get("target"))
                .cloned();
            if let Some(raw) = target_raw {
                match raw.parse::<f32>() {
                    Ok(v) if v.is_finite() => f.target = v,
                    _ => errors.push("invalid 'target_c'".to_string()),
                }
            }
            assign_f32(params, "kp", &mut f.kp, &mut errors);
            assign_f32(params, "ki", &mut f.ki, &mut errors);
            assign_f32(params, "kd", &mut f.kd, &mut errors);
            assign_f32(params, "base_percent", &mut f.base_percent, &mut errors);
            assign_f32(params, "min_percent", &mut f.min_percent, &mut errors);
            assign_f32(params, "max_percent", &mut f.max_percent, &mut errors);
            assign_f32(
                params,
                "emergency_delta_c",
                &mut f.emergency_delta,
                &mut errors,
            );
            assign_f32(params, "idle_delta_c", &mut f.idle_delta, &mut errors);
            assign_f32(params, "temp_guard_c", &mut f.temp_guard_c, &mut errors);
            assign_f32(
                params,
                "write_deadband_percent",
                &mut f.write_deadband_percent,
                &mut errors,
            );
            assign_bool(params, "adaptive_base", &mut f.adaptive_base, &mut errors);
            assign_u32(params, "min_mhz", &mut f.min_mhz, &mut errors);
            assign_u32(params, "max_mhz", &mut f.max_mhz, &mut errors);
            if errors.is_empty() {
                if let Err(e) = f.validate() {
                    errors.push(e);
                } else {
                    cfg.freq = f;
                }
            }
        }
    }
    if let Some(v) = params
        .get("interval_ms")
        .and_then(|s| s.parse::<u64>().ok())
    {
        if (INTERVAL_MS_MIN..=INTERVAL_MS_MAX).contains(&v) {
            cfg.interval_ms = v;
        } else {
            errors.push(format!(
                "'interval_ms' must be {INTERVAL_MS_MIN}–{INTERVAL_MS_MAX}"
            ));
        }
    }
    if let Some(s) = params.get("sensor") {
        match SensorKind::parse(s) {
            Some(kind) => cfg.sensor = kind,
            None => errors.push("'sensor' must be core|hotspot|memory|board|max".to_string()),
        }
    }

    if !errors.is_empty() {
        text_response(request, 400, format!("Bad request: {}", errors.join("; ")));
        return;
    }
    let (loop_name, target) = match cfg.loop_kind {
        LoopKind::FanTemp => ("fan_temp", cfg.pid.target_c),
        LoopKind::FreqTemp => ("freq_temp", cfg.freq.target),
        LoopKind::FreqPower => ("freq_power", cfg.freq.target),
    };
    info!(
        "PID updated via HTTP: loop={loop_name} target={target} gains={}",
        cfg_active_gains(&cfg)
    );
    text_response(request, 200, "OK: pid updated");
}

/// Gains of the active loop, for the audit log line.
fn cfg_active_gains(cfg: &RuntimeConfig) -> String {
    match cfg.loop_kind {
        LoopKind::FanTemp => format!(
            "{}/{}/{}/base={}",
            cfg.pid.kp, cfg.pid.ki, cfg.pid.kd, cfg.pid.base_percent
        ),
        LoopKind::FreqTemp | LoopKind::FreqPower => format!(
            "{}/{}/{}/base={}",
            cfg.freq.kp, cfg.freq.ki, cfg.freq.kd, cfg.freq.base_percent
        ),
    }
}

fn handle_oc_global(
    request: tiny_http::Request,
    params: &HashMap<String, String>,
    cmd_tx: &Sender<ServiceCmd>,
) {
    let gpu_index: usize = match params.get("gpu") {
        None => 0,
        Some(s) => match s.parse::<usize>() {
            Ok(n) => n,
            Err(_) => {
                text_response(
                    request,
                    400,
                    format!("Bad request: 'gpu' must be a non-negative integer, got {s:?}"),
                );
                return;
            }
        },
    };
    match params.get("oc").and_then(|s| s.parse::<i32>().ok()) {
        Some(delta) if (OC_DELTA_MIN..=OC_DELTA_MAX).contains(&delta) => {
            match cmd_tx.send(ServiceCmd::SetOcGlobal {
                gpu_index,
                delta_khz: delta,
            }) {
                Ok(()) => {
                    info!("GPU {gpu_index}: queued OC delta {delta} kHz");
                    text_response(request, 200, "OK");
                }
                Err(e) => {
                    error!("HTTP: cannot enqueue OC command: {e}");
                    text_response(request, 500, "Internal error");
                }
            }
        }
        Some(delta) => text_response(
            request,
            400,
            format!("Bad request: oc must be {OC_DELTA_MIN}–{OC_DELTA_MAX} kHz, got {delta}"),
        ),
        None => text_response(request, 400, "Bad request: missing or invalid 'oc'"),
    }
}

/// Refuse a locked window the request itself can rule out.
///
/// The span is not clamped here: the macOS write path refuses a window the
/// driver does not hold and names the span it does. What this checks is only
/// what can be decided without asking the driver.
fn locked_window_refusal(min_mhz: u32, max_mhz: u32) -> Option<String> {
    if min_mhz == 0 || max_mhz == 0 {
        return Some(format!(
            "a locked window needs non-zero bounds, got {min_mhz}..{max_mhz} MHz; nothing was written"
        ));
    }
    if min_mhz > max_mhz {
        return Some(format!(
            "min {min_mhz} MHz is above max {max_mhz} MHz; nothing was written"
        ));
    }
    None
}

#[cfg(test)]
mod tests {
    use super::{
        limit_window_refusal, locked_history_json, locked_window_from_history, locked_window_json,
        locked_window_refusal, offset_window_refusal, stored_window_note,
    };
    use crate::controller::LockedHistory;

    #[test]
    fn the_window_comes_out_of_the_table_that_was_already_read() {
        // The newest record is the locked window, so a table read answers both
        // questions and the older single-window read is not issued again.
        let history = LockedHistory {
            count_at_4: Some(96),
            windows: vec![(1_700, 1_700), (1_620, 1_800)],
            anomaly: None,
        };
        assert_eq!(
            locked_window_from_history(Some(&history)),
            Some((1_620, 1_800))
        );
        assert_eq!(locked_window_from_history(None), None);
        // A table with no windows says nothing about the window.
        assert_eq!(
            locked_window_from_history(Some(&LockedHistory {
                count_at_4: Some(0),
                windows: Vec::new(),
                anomaly: None,
            })),
            None
        );
        // A zero end is the driver's "absent", not a window of zero.
        assert_eq!(
            locked_window_from_history(Some(&LockedHistory {
                count_at_4: Some(48),
                windows: vec![(0, 1_800)],
                anomaly: None,
            })),
            None
        );
    }

    #[test]
    fn the_stored_table_is_published_whole_or_as_null() {
        assert_eq!(locked_history_json(None), serde_json::Value::Null);

        let history = LockedHistory {
            count_at_4: Some(96),
            windows: vec![(1_700, 1_700), (1_620, 1_800)],
            anomaly: None,
        };
        let value = locked_history_json(Some(&history));
        assert_eq!(value["count_at_4"], serde_json::json!(96));
        assert_eq!(value["anomaly"], serde_json::Value::Null);
        assert_eq!(
            value["windows"],
            serde_json::json!([
                {"min_mhz": 1700, "max_mhz": 1700},
                {"min_mhz": 1620, "max_mhz": 1800},
            ])
        );
        // The dword is optional on the wire too: a transport that keeps a table
        // without a counter publishes the windows and a null counter.
        let no_counter = locked_history_json(Some(&LockedHistory {
            count_at_4: None,
            windows: vec![(1_620, 1_800)],
            anomaly: None,
        }));
        assert_eq!(no_counter["count_at_4"], serde_json::Value::Null);

        // A table that disagrees with its counter is published with the rows and
        // with the complaint -- never silently.
        let suspicious = locked_history_json(Some(&LockedHistory {
            count_at_4: Some(288),
            windows: vec![(1_620, 1_800)],
            anomaly: Some("+4 reads 288, which is 6 writes of 48, but the block holds 1".into()),
        }));
        assert_eq!(suspicious["windows"].as_array().map(Vec::len), Some(1));
        assert!(
            suspicious["anomaly"]
                .as_str()
                .is_some_and(|text| text.contains("288")),
            "{suspicious}"
        );
    }

    #[test]
    fn a_locked_window_the_transport_publishes_is_both_ends_or_null() {
        assert_eq!(locked_window_json(None), serde_json::Value::Null);
        assert_eq!(
            locked_window_json(Some((1620, 1800))),
            serde_json::json!({"min_mhz": 1620, "max_mhz": 1800})
        );
    }

    #[test]
    fn a_post_write_note_reports_what_the_driver_stored() {
        // Match, mismatch and "cannot say" must read differently: a client that
        // sees only "OK" would take a write's own return value as evidence.
        let same = stored_window_note(Ok(Some((1620, 1800))), 1620, 1800).expect("note");
        assert!(same.contains("1620..1800 MHz"), "{same}");
        assert!(same.contains("matches the request"), "{same}");

        let other = stored_window_note(Ok(Some((1620, 1700))), 1620, 1800).expect("note");
        assert!(other.contains("differs"), "{other}");
        assert!(other.contains("1620..1800 MHz request"), "{other}");

        // No readable stored state is said out loud, never reported as a match.
        let unreadable = stored_window_note(Ok(None), 1620, 1800).expect("note");
        assert!(
            unreadable.contains("cannot report stored state"),
            "{unreadable}"
        );
        assert!(!unreadable.contains("matches"), "{unreadable}");

        let failed =
            stored_window_note(Err("session refused".to_string()), 1620, 1800).expect("note");
        assert!(failed.contains("read failed"), "{failed}");
        assert!(failed.contains("session refused"), "{failed}");
    }

    #[test]
    fn a_locked_window_the_request_can_rule_out_is_refused_before_the_driver() {
        assert!(locked_window_refusal(0, 1700).unwrap().contains("non-zero"));
        assert!(locked_window_refusal(1700, 0).unwrap().contains("non-zero"));
        assert!(
            locked_window_refusal(1700, 1620)
                .unwrap()
                .contains("above max")
        );
        assert!(locked_window_refusal(1620, 1700).is_none());
        assert!(locked_window_refusal(1620, 1620).is_none());
    }

    #[test]
    fn a_published_span_refuses_an_offset_past_it() {
        let refusal = offset_window_refusal(40, Some((-18.75, 18.75))).expect("refused");
        assert!(refusal.contains("+40 MHz"), "{refusal}");
        assert!(refusal.contains("-18.75 .. +18.75 MHz"), "{refusal}");
        assert!(refusal.contains("nothing was written"), "{refusal}");
    }

    #[test]
    fn a_published_span_admits_what_it_holds() {
        assert_eq!(offset_window_refusal(18, Some((-18.75, 18.75))), None);
        assert_eq!(offset_window_refusal(-18, Some((-18.75, 18.75))), None);
        assert_eq!(offset_window_refusal(0, Some((-18.75, 18.75))), None);
    }

    #[test]
    fn no_published_span_refuses_nothing() {
        // NVAPI/NVML publish no window; "not told" is not "no span".
        assert_eq!(offset_window_refusal(500, None), None);
    }

    #[test]
    fn a_published_board_policy_refuses_a_power_past_it() {
        let refusal = limit_window_refusal(120, Some((70, 100)), "W").expect("refused");
        assert!(refusal.contains("120 W"), "{refusal}");
        assert!(refusal.contains("70 .. 100 W"), "{refusal}");
        assert!(refusal.contains("nothing was written"), "{refusal}");
        // Inside the span — including the floor, which is a legal value.
        assert_eq!(limit_window_refusal(70, Some((70, 100)), "W"), None);
        assert_eq!(limit_window_refusal(100, Some((70, 100)), "W"), None);
    }

    #[test]
    fn a_published_acoustic_policy_refuses_a_target_past_it() {
        let refusal = limit_window_refusal(95, Some((65, 87)), "C").expect("refused");
        assert!(refusal.contains("95 C"), "{refusal}");
        assert!(refusal.contains("65 .. 87 C"), "{refusal}");
        assert_eq!(limit_window_refusal(87, Some((65, 87)), "C"), None);
    }

    #[test]
    fn an_unpublished_window_refuses_nothing() {
        // "Not told" is not "no span": a transport that publishes no policy
        // window must behave exactly as it did before this gate existed.
        assert_eq!(limit_window_refusal(500, None, "W"), None);
        assert_eq!(limit_window_refusal(-5, None, "C"), None);
    }
}
