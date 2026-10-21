use nvapi::hi::{
    Celsius, ClockDomain, CoolerPolicy, KilohertzDelta, MicrovoltsDelta, PState, Percentage,
};
use nvml_wrapper::enum_wrappers::device::{Api, PerformanceState};
use nvoc_core::{
    BackendSet, CheckVoltageFrequency, ClearEdid, ClkVfDomainClass, ConvertEnum, DeviceInfo,
    DeviceStatus, DisableNvapiThermalSim, FabricTree, FanCurvePointReadout, GetFanCurves,
    GpuTarget, GpuType, NvapiPStateNativeLock, NvapiPerfFreqCap, PmgrArbiterProbe,
    QueryApiRestriction, QueryAutoBoost, QueryDisplays, QueryDomainVfpPoints, QueryEdid,
    QueryFanInfo, QueryGpuInfo, QueryGpuSettings, QueryGpuStatus, QueryLegacyCoreOvervoltRanges,
    QueryNvapiClkDomainFreq, QueryNvapiClkDomainFreqDirect, QueryNvapiClkDomainFreqsBatch,
    QueryNvapiClkDomains, QueryNvapiClkVfPoints, QueryNvapiCoolerInfo,
    QueryNvapiCoreVoltageControl, QueryNvapiDNotifier, QueryNvapiOcScannerIncomplete,
    QueryNvapiPmgrVoltageArbiter, QueryNvapiPowerCeiling, QueryNvapiRatedTdp,
    QueryNvapiTargetTempPolicies, QueryNvapiTgpWattRange, QueryNvapiThermalSettings,
    QueryNvapiThermalSim, QueryNvapiVoltRails, QueryPowerLimits, QueryPstateBaseVoltage,
    QueryPstates, QuerySupportedApplicationsClocks, QueryTdpTempLimits, QueryTemperatureThresholds,
    QueryThrottleReasons, QueryVbiosImage, QueryVfpPointVoltage, QueryVoltageBoost,
    ResetAutoboostStatus, ResetCoolerLevels, ResetFanCurve, ResetFanSpeed, ResetFreqLock,
    ResetLegacyApplicationFreqLock, ResetLegacyGpcRailOvervoltLimit, ResetNvapiFanControl,
    ResetNvapiPowerLimits, ResetNvapiSensorLimits, ResetNvapiTgpWatt, ResetNvapiVfpPrivate,
    ResetPstateGlobalFreqOffset, ResetPublicVftableGpcLock, ResetPublicVftableOffset,
    ResetVfpFrequencyLock, SetApplicationsClocks, SetAutoboostStatus, SetAutoboostSupport,
    SetClockOffset, SetCoolerLevels, SetDomainVfpDeltas, SetEdid, SetFanCurve, SetFanPercent,
    SetFanRpm, SetFanSpeed, SetFanStop, SetGpcVoltLock, SetLegacyClocks, SetLockedClocks,
    SetNvapiBackgroundOcScanner, SetNvapiClkDomainOffset, SetNvapiCoreVoltageControl,
    SetNvapiDNotifier, SetNvapiDynamicBoost, SetNvapiPStateNative, SetNvapiPerfFreqCap,
    SetNvapiPerfLevelLock, SetNvapiPmgrVoltageArbiter, SetNvapiPowerLimits, SetNvapiPstateLock,
    SetNvapiSensorLimits, SetNvapiTargetTemp, SetNvapiTgpWatt, SetNvapiThermalSim,
    SetNvapiVfpPointPrivate, SetNvapiVfpRangePerPointPrivate, SetNvapiVoltRailOffset,
    SetNvapiVoltRailTarget, SetNvmlPstateLock, SetPowerLimit, SetPstateBaseVoltage,
    SetPstateClockOffset, SetPublicVftablePointOffset, SetPublicVftableRangeOffset,
    SetTemperatureLimit, SetVfpFrequencyLock, SetVoltageBoost, VfPointType, VfpResetDomain,
    clk_vf_delta_for_target, detect_gpu_type, discover_targets, fetch_gpu_type, nvapi_status_name,
    nvml_pstate_to_str, parse_nvml_fan_control_policy, run, try_parse_nvml_pstate,
};
use pyo3::exceptions::{PyRuntimeError, PyValueError};
use pyo3::prelude::*;
use pyo3::types::{PyBool, PyDict, PyFloat, PyInt, PyList, PyString};
use serde_json::{Map, Number, Value};
use std::collections::HashMap;
use std::sync::Arc;

type PyResultValue = PyResult<Value>;

fn to_py_err(err: impl std::fmt::Display) -> PyErr {
    PyRuntimeError::new_err(err.to_string())
}

fn invalid_value(err: impl std::fmt::Display) -> PyErr {
    PyValueError::new_err(err.to_string())
}

/// The platform default backend set, mirroring the CLI's `Auto` choice: the
/// native NullMoth RM on macOS (NVAPI and NVML do not exist there) and
/// NVAPI+NVML everywhere else. Resolved from the compile target, not from a
/// provider probe, so it is stable and identical across GUI/TUI/CLI.
fn default_backend_set() -> BackendSet {
    if cfg!(target_os = "macos") {
        BackendSet::MacosRm
    } else {
        BackendSet::Both
    }
}

fn parse_backends(raw: &str) -> PyResult<BackendSet> {
    match raw.trim().to_ascii_lowercase().as_str() {
        // Platform default: the string a GUI/TUI/CLI can share without knowing
        // the host OS. On macOS this is the RM path, elsewhere NVAPI+NVML.
        "auto" | "default" => Ok(default_backend_set()),
        "both" | "all" => Ok(BackendSet::Both),
        "nvapi" => Ok(BackendSet::Nvapi),
        "nvml" => Ok(BackendSet::Nvml),
        // Native NullMoth RM: the macOS backend. Kept as an explicit opt-in so
        // a GUI/TUI on macOS can drive the RM path directly through pynvoc and
        // never loads NVAPI/NVML (which do not exist on macOS).
        "macos-rm" | "macos_rm" | "macosrm" => Ok(BackendSet::MacosRm),
        other => Err(invalid_value(format!(
            "invalid backend set {other:?}; expected 'auto'/'default', 'both'/'all', 'nvapi', 'nvml', or 'macos-rm'"
        ))),
    }
}

/// macOS has no NVAPI/NVML runtime: the native NullMoth RM backend
/// ([`BackendSet::MacosRm`], which is also what `auto`/`default` resolves to
/// here) is the only backend, and it is read-only. Any request that resolves to
/// NVAPI or NVML — every write entry point, plus explicit
/// `backend="nvapi"/"nvml"/"both"` reads — is refused here with one actionable
/// message instead of the cascade of library-load / dlopen failures (including
/// a literal `dlopen("nvml.dll")` on macOS) that would otherwise surface.
fn ensure_backend_available(backends: BackendSet) -> PyResult<()> {
    if cfg!(target_os = "macos") && backends != BackendSet::MacosRm {
        return Err(PyRuntimeError::new_err(
            "NVAPI/NVML are not available on macOS; the native NullMoth RM backend \
             (backend 'macos-rm', the default here) is read-only and does not implement \
             performance writes.",
        ));
    }
    Ok(())
}

/// Three read entries ask for a *shape* the RM snapshot does not have: NVML's
/// severity/threshold table, NVML's per-P-state clock table, and the cooler
/// count/type/percent triple. On macOS they would otherwise be refused by
/// [`ensure_backend_available`] with the platform sentence, which is true of
/// the platform but not of the entry: it reads as "nothing of this family
/// exists here", while the same backend demonstrably answers the *reading* of
/// each family (sensors through `query_legacy_temp_sensor`, the current P-state
/// through `query_status`, per-cooler rpm through `query_fan_info`). So the
/// reason names the missing layer, and it is the same core constant the CLI
/// quotes, so both entries cannot drift apart.
fn refuse_without_source(entry: &str, reason: &str) -> PyResult<()> {
    if cfg!(target_os = "macos") {
        return Err(PyRuntimeError::new_err(source_less_message(entry, reason)));
    }
    Ok(())
}

/// The refusal text, kept separate from raising it so a unit test can assert on
/// it without starting an interpreter (the same reason the other message tests
/// here only check `is_err`). The wrapper adds nothing but the entry name: the
/// first version said "query_pstates reads NVML's per-P-state clock table: the
/// per-pstate clock table has no source here: ...", naming the shape twice
/// because the reason already names it.
fn source_less_message(entry: &str, reason: &str) -> String {
    format!("{entry} is not implemented on macOS: {reason}")
}

fn parse_backend(raw: &str) -> PyResult<&'static str> {
    match raw.trim().to_ascii_lowercase().as_str() {
        "nvapi" => Ok("nvapi"),
        "nvml" => Ok("nvml"),
        "nvapi-cooler" => Ok("nvapi-cooler"),
        "nvml-cooler" => Ok("nvml-cooler"),
        other => Err(invalid_value(format!(
            "invalid backend {other:?}; expected 'nvapi', 'nvml', 'nvapi-cooler', or 'nvml-cooler'"
        ))),
    }
}

fn parse_domain(raw: &str) -> PyResult<ClockDomain> {
    match raw.trim().to_ascii_lowercase().as_str() {
        "core" | "gpu" | "graphics" => Ok(ClockDomain::Graphics),
        "mem" | "memory" => Ok(ClockDomain::Memory),
        other => Err(invalid_value(format!(
            "invalid clock domain {other:?}; expected 'graphics'/'core'/'gpu' or 'memory'/'mem'"
        ))),
    }
}

fn parse_pstate(raw: &str) -> PyResult<PState> {
    let normalized = raw.trim().to_ascii_uppercase();
    PState::from_str(normalized.as_str()).map_err(invalid_value)
}

fn parse_nvml_pstate(raw: &str) -> PyResult<PerformanceState> {
    try_parse_nvml_pstate(raw).map_err(invalid_value)
}

/// Parse a P-State label ("P0", "p12", "12") into its numeric id (u8).
/// Mirrors the CLI `set-pstate-lock` parse logic.
fn parse_pstate_number(raw: &str) -> Result<u8, String> {
    let normalized = raw.trim().trim_start_matches('P').trim_start_matches('p');
    normalized.parse::<u8>().map_err(|e| e.to_string())
}

fn parse_api_restriction_api(raw: &str) -> PyResult<Api> {
    match raw.trim().to_ascii_lowercase().as_str() {
        "app-clocks" | "application-clocks" => Ok(Api::ApplicationClocks),
        "auto-boost" | "autoboost" => Ok(Api::AutoBoostedClocks),
        other => Err(invalid_value(format!(
            "invalid API {other:?}; expected app-clocks, application-clocks, auto-boost, or autoboost"
        ))),
    }
}

fn api_restriction_api_label(api_type: Api) -> &'static str {
    match api_type {
        Api::ApplicationClocks => "app-clocks",
        Api::AutoBoostedClocks => "auto-boost",
    }
}

fn voltage_domain_label(domain: nvoc_core::VoltageDomain) -> &'static str {
    match domain {
        nvoc_core::VoltageDomain::Core => "core",
        nvoc_core::VoltageDomain::Undefined => "undefined",
        _ => "unknown",
    }
}

fn parse_display_id(raw: &str) -> PyResult<u32> {
    let trimmed = raw.trim();
    let digits = trimmed
        .strip_prefix("0x")
        .or_else(|| trimmed.strip_prefix("0X"))
        .unwrap_or(trimmed);
    u32::from_str_radix(digits, 16)
        .map_err(|_| invalid_value(format!("invalid display ID {raw:?}; expected hex")))
}

fn hex_nibble(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

fn parse_edid_hex(raw: &str) -> PyResult<Vec<u8>> {
    let hex = raw.trim();
    let bytes = hex.as_bytes();
    if !bytes.len().is_multiple_of(2) {
        return Err(invalid_value(
            "EDID hex must contain an even number of digits",
        ));
    }

    bytes
        .chunks_exact(2)
        .enumerate()
        .map(|(pair_index, pair)| {
            let high_index = pair_index * 2;
            let high = hex_nibble(pair[0]).ok_or_else(|| {
                invalid_value(format!("invalid EDID hex digit at offset {high_index}"))
            })?;
            let low_index = high_index + 1;
            let low = hex_nibble(pair[1]).ok_or_else(|| {
                invalid_value(format!("invalid EDID hex digit at offset {low_index}"))
            })?;
            Ok((high << 4) | low)
        })
        .collect()
}

fn bytes_to_upper_hex(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789ABCDEF";
    let mut out = String::with_capacity(bytes.len() * 2);
    for &byte in bytes {
        out.push(HEX[(byte >> 4) as usize] as char);
        out.push(HEX[(byte & 0x0F) as usize] as char);
    }
    out
}

fn selected_target<'a>(
    inventory: &'a nvoc_core::TargetInventory,
    gpu: &str,
) -> PyResult<GpuTarget<'a>> {
    for target in inventory.targets() {
        if gpu_id_matches(target.id.0, gpu)? {
            return Ok(target);
        }
    }
    Err(to_py_err("no GPU selected"))
}

fn gpu_id_matches(gpu_id: u32, raw: &str) -> PyResult<bool> {
    let raw = raw.trim();
    if raw.is_empty() {
        return Ok(false);
    }
    if let Some(rest) = raw.strip_prefix("0x").or_else(|| raw.strip_prefix("0X")) {
        let parsed = u32::from_str_radix(rest, 16).map_err(invalid_value)?;
        return Ok(parsed == gpu_id);
    }
    if let Ok(parsed) = raw.parse::<u32>() {
        return Ok(parsed == gpu_id || parsed < 256 && gpu_id == parsed.saturating_mul(256));
    }
    Ok(raw.eq_ignore_ascii_case(&format!("gpu {gpu_id}"))
        || raw.eq_ignore_ascii_case(&format!("gpu{gpu_id}"))
        || raw.eq_ignore_ascii_case(&format!("0x{gpu_id:X}")))
}

/// NVAPI/NVML 句柄是驱动侧不透明值；NVML 官方线程安全，NVAPI 查询同样
/// 被前端多线程并发调用，跨线程共享 inventory 安全。
struct SyncInventory(nvoc_core::TargetInventory);
unsafe impl Send for SyncInventory {}
unsafe impl Sync for SyncInventory {}

/// 进程级 inventory 缓存（按 BackendSet 各一份）。
///
/// GUI/TUI 以 0.5-1 Hz 轮询 pynvoc，若每次调用都 `discover_targets`，
/// 会反复 NVML init + NVAPI 重枚举：Linux 上 libnvidia-api 每次 NVAPI
/// 调用泄漏 fd（约 1 fd/tick，30-60 分钟耗尽 ulimit 1024 → "too many
/// open files"），Windows 上驱动/内核句柄累积一整天，退出时逐个拆除
/// 导致卡死数分钟。缓存后仅在首次使用或 `discover_gpus` 显式刷新时发现。
struct InventoryCache {
    both: Option<Arc<SyncInventory>>,
    nvapi: Option<Arc<SyncInventory>>,
    nvml: Option<Arc<SyncInventory>>,
    macos_rm: Option<Arc<SyncInventory>>,
}

static INVENTORY_CACHE: std::sync::Mutex<InventoryCache> = std::sync::Mutex::new(InventoryCache {
    both: None,
    nvapi: None,
    nvml: None,
    macos_rm: None,
});

fn lock_inventory_cache() -> std::sync::MutexGuard<'static, InventoryCache> {
    INVENTORY_CACHE
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// 是否还允许配置库路径覆盖：NVAPI 显式初始化未发生且 inventory 缓存全空。
/// 所有 pynvoc 调用都经 `with_target` → `InventoryCache::entry` → 发现路径
/// （NVAPI 的 `prepare_nvapi` + 显式 init、NVML 的 `init_nvml` 都在其中），
/// 所以"缓存全空"就等于"进程内还没有任何 GPU 调用触碰过库加载"。
fn library_paths_configurable() -> bool {
    if nvoc_core::nvapi_init_attempted() {
        return false;
    }
    let cache = lock_inventory_cache();
    cache.both.is_none() && cache.nvapi.is_none() && cache.nvml.is_none()
}

fn non_empty_trimmed(value: Option<&str>) -> Option<&str> {
    value.map(str::trim).filter(|p| !p.is_empty())
}

/// 写入库路径覆盖 env（`NVOC_NVML_PATH` / `NVOC_NVAPI_PATH`），与 CLI
/// `--nvml-path`/`--nvapi-path` 的启动期注入同一机制。None 跳过对应变量
/// （不清理已有值）；空串/纯空白视为未传（与 core `override_path` 的
/// 空串语义一致）。
fn set_library_path_env(nvapi_path: Option<&str>, nvml_path: Option<&str>) -> PyResult<()> {
    if !library_paths_configurable() {
        return Err(PyRuntimeError::new_err(
            "library paths must be configured before the first GPU call: NVAPI \
             initialization is one-shot and inventory discovery is cached, so \
             later overrides never take effect",
        ));
    }
    if let Some(path) = non_empty_trimmed(nvapi_path) {
        // SAFETY: 契约是"首次 GPU 调用前、持 GIL 时调用"——此刻尚无任何
        // nvoc-core 路径并发读这两个 env（GPU 调用都发生在之后的 Python
        // worker 线程里），与 CLI 启动期单线程注入同一约定。
        unsafe { std::env::set_var(nvoc_core::dll_path::NVAPI_PATH_ENV, path) };
    }
    if let Some(path) = non_empty_trimmed(nvml_path) {
        // SAFETY: 同上，见 NVAPI 分支注释。
        unsafe { std::env::set_var(nvoc_core::dll_path::NVML_PATH_ENV, path) };
    }
    Ok(())
}

/// 设置 NVML/NVAPI 用户态库路径覆盖，与 CLI `--nvml-path`/`--nvapi-path` 同一
/// 机制（写 env `NVOC_NVML_PATH` / `NVOC_NVAPI_PATH`，nvoc-core 在首次 GPU
/// 调用时读取）。必须在第一次 GPU 调用（discover_gpus / query_* / set_* ...）
/// 之前调用，之后设置不再生效。
///
/// 值的语义（见 nvoc-core dll_path 模块文档）：
/// - NVML：nvml.dll / libnvidia-ml.so.1 的文件路径（Linux 也接受目录，自动
///   拼 SONAME）。指错时 NVML init 直接报错，不静默回落。
/// - NVAPI：Windows 传 nvapi64.dll 所在目录或完整文件路径（文件取父目录）；
///   Linux 传目录（自动拼 `libnvidia-api.so.1`）或文件路径，副本不带预期
///   SONAME 时打印 warning 并回落系统库。
#[pyfunction]
fn configure_library_paths(nvapi_path: Option<&str>, nvml_path: Option<&str>) -> PyResult<()> {
    set_library_path_env(nvapi_path, nvml_path)
}

/// Process-level cache of the NVML-enforced power limit (TGP wall, watts),
/// keyed by GPU id. Populated by `normalize_info` (which runs at GPU-switch /
/// Refresh-Info time — a low-frequency, user-initiated path where the dGPU is
/// already D0 and the NVML device handle is valid). Consumed by
/// `normalize_status`'s 1Hz dashboard poll.
///
/// Why this split: `nvmlDeviceGetEnforcedPowerLimit` (like every NVML
/// device-level query) segfaults inside nvml.dll on a stale device handle when
/// the dGPU is mid-transition (D3cold↔D0) — verified crash at
/// `nvmlDeviceGetEnforcedPowerLimit+0x276` when the user re-enables a powered-
/// off dGPU. NVAPI recovers BEFORE NVML's handle state during that window, so
/// an earlier attempt to gate NVML with an NVAPI-derived liveness proof was
/// true while the NVML handle was still stale — it crashed on re-enable. There
/// is NO pre-call way to detect this (the `GpuLost` error code is never
/// returned — the fault is a native segfault). The only safe course is to
/// never call NVML device queries from the high-frequency poll.
/// The enforced limit is quasi-static (TGP wall, rarely changes), so caching it
/// from the info path loses no real-time accuracy that matters.
static POWER_LIMIT_CACHE: std::sync::Mutex<Option<(u32, std::time::Instant, f64)>> =
    std::sync::Mutex::new(None);

/// Cache TTL: the enforced power limit is the TGP wall, which only moves on a
/// deliberate user action (set-tgp-watt / D-Notifier change). 5 minutes is far
/// shorter than any real change cadence while bounding how stale a cached value
/// can get after a dGPU round-trip.
const POWER_LIMIT_CACHE_TTL: std::time::Duration = std::time::Duration::from_secs(300);

fn cache_power_limit(gpu_id: u32, watts: f64) {
    if let Ok(mut cache) = POWER_LIMIT_CACHE.lock() {
        *cache = Some((gpu_id, std::time::Instant::now(), watts));
    }
}

/// Return the cached enforced power limit for `gpu_id` if fresh, else `None`.
fn cached_power_limit(gpu_id: u32) -> Option<f64> {
    let cache = POWER_LIMIT_CACHE.lock().ok()?;
    let (cached_id, ts, watts) = cache.as_ref()?;
    if *cached_id != gpu_id {
        return None;
    }
    if ts.elapsed() > POWER_LIMIT_CACHE_TTL {
        return None;
    }
    Some(*watts)
}

impl InventoryCache {
    /// 获取指定 backend 集的 inventory；首次访问时发现并缓存。
    /// 返回 Arc 快照 —— 调用方在锁外持有并查询，并发查询互不阻塞。
    fn entry(&mut self, backends: BackendSet) -> PyResult<Arc<SyncInventory>> {
        ensure_backend_available(backends)?;
        let slot = match backends {
            BackendSet::Both => &mut self.both,
            BackendSet::Nvapi => &mut self.nvapi,
            BackendSet::Nvml => &mut self.nvml,
            BackendSet::MacosRm => &mut self.macos_rm,
        };
        if slot.is_none() {
            *slot = Some(Arc::new(SyncInventory(
                discover_targets(backends).map_err(to_py_err)?,
            )));
        }
        Ok(slot.as_ref().expect("slot just filled").clone())
    }

    /// 强制重新发现指定集并更新缓存（discover_gpus 的显式刷新入口）。
    /// 刷新会替换槽位；仍在途的旧快照查询由 Arc 引用计数保活，安全。
    fn refresh(&mut self, backends: BackendSet) -> PyResult<Arc<SyncInventory>> {
        ensure_backend_available(backends)?;
        let slot = match backends {
            BackendSet::Both => &mut self.both,
            BackendSet::Nvapi => &mut self.nvapi,
            BackendSet::Nvml => &mut self.nvml,
            BackendSet::MacosRm => &mut self.macos_rm,
        };
        let fresh = discover_targets(backends).map_err(to_py_err)?;
        *slot = Some(Arc::new(SyncInventory(fresh)));
        Ok(slot.as_ref().expect("slot just filled").clone())
    }
}

fn with_target<F>(gpu: &str, backends: &str, f: F) -> PyResultValue
where
    F: FnOnce(&GpuTarget<'_>) -> PyResultValue,
{
    let backends = parse_backends(backends)?;
    // Lock ONLY for the cache lookup (or first discovery); the query itself
    // runs against the Arc'd snapshot with no lock held — concurrent
    // NVAPI/NVML queries from multiple GUI/TUI worker threads no longer
    // serialize on this mutex (SyncInventory is driver-side thread-safe).
    let inventory = {
        let mut cache = lock_inventory_cache();
        cache.entry(backends)?
    };
    // Resolve the requested GPU against the cached inventory. If it isn't
    // listed (the dGPU was switched off / removed from the bus between polls),
    // refresh the inventory ONCE and retry against the freshly re-enumerated
    // set. `refresh` re-runs `discover_targets` (NVAPI re-enumerate +
    // `Nvml::init`), rebuilding the stale handle table so subsequent polls
    // either find the GPU again (it came back / was just GCOFF) or cleanly
    // omit it. Without this, a dGPU that disappears mid-session leaves
    // `selected_target` permanently erroring and the dashboard never recovers
    // even after the dGPU returns.
    match selected_target(&inventory.0, gpu) {
        Ok(target) => f(&target),
        Err(first_err) => {
            // Cache miss: re-enumerate once. The refreshed Arc<SyncInventory>
            // lives in this frame for the duration of the query (the borrow
            // into `target` is valid while `inventory` is alive). If the GPU is
            // still absent, surface the original error.
            let inventory = {
                let mut cache = lock_inventory_cache();
                cache.refresh(backends)?
            };
            match selected_target(&inventory.0, gpu) {
                Ok(target) => f(&target),
                Err(_) => Err(first_err),
            }
        }
    }
}

fn value_object(entries: impl IntoIterator<Item = (impl Into<String>, Value)>) -> Value {
    let mut map = Map::new();
    for (key, value) in entries {
        if !value.is_null() {
            map.insert(key.into(), value);
        }
    }
    Value::Object(map)
}

fn py_value(py: Python, value: &Value) -> PyResult<Py<PyAny>> {
    match value {
        Value::Null => Ok(py.None()),
        Value::Bool(v) => Ok(PyBool::new(py, *v).to_owned().into_any().unbind()),
        Value::Number(v) => {
            if let Some(i) = v.as_i64() {
                Ok(PyInt::new(py, i).into_any().unbind())
            } else if let Some(u) = v.as_u64() {
                Ok(PyInt::new(py, u).into_any().unbind())
            } else if let Some(f) = v.as_f64() {
                Ok(PyFloat::new(py, f).into_any().unbind())
            } else {
                Ok(py.None())
            }
        }
        Value::String(v) => Ok(PyString::new(py, v).into_any().unbind()),
        Value::Array(items) => {
            let list = PyList::empty(py);
            for item in items {
                list.append(py_value(py, item)?)?;
            }
            Ok(list.into_any().unbind())
        }
        Value::Object(items) => {
            let dict = PyDict::new(py);
            for (key, item) in items {
                dict.set_item(key, py_value(py, item)?)?;
            }
            Ok(dict.into_any().unbind())
        }
    }
}

fn text<T: std::fmt::Display>(value: T) -> Value {
    Value::String(value.to_string())
}

fn i64_value(value: i64) -> Value {
    Value::Number(Number::from(value))
}

fn u64_value(value: u64) -> Value {
    Value::Number(Number::from(value))
}

fn f64_value(value: f64) -> Value {
    Number::from_f64(value)
        .map(Value::Number)
        .unwrap_or(Value::Null)
}

fn option_u32(value: Option<u32>) -> Value {
    value.map(|v| u64_value(v as u64)).unwrap_or(Value::Null)
}

fn khz_to_mhz_i64(value: i32) -> i64 {
    (value / 1000) as i64
}

fn uv_to_mv_i64(value: i32) -> i64 {
    (value / 1000) as i64
}

fn bool_value(value: bool) -> Value {
    Value::Bool(value)
}

/// NVAPI PerfFlags bit -> reason name. Bit semantics mirror nvapi-rs
/// (`sys/src/gpu/power.rs`, NV_GPU_PERF_FLAGS + its display table). Ascending
/// bit order so the decoded list reads consistently regardless of active set.
const PERF_LIMIT_BITS: &[(u32, &str)] = &[
    (1, "Power"),
    (2, "Temperature"),
    (4, "Reliability Voltage"),
    (8, "Operating Voltage"),
    (16, "No Load"),
    (32, "Unknown32"),
];

/// Decode a PerfFlags bitmask into reason names; an empty mask yields `["None"]`.
fn decode_perf_flags(bits: u32) -> Vec<&'static str> {
    let reasons: Vec<&'static str> = PERF_LIMIT_BITS
        .iter()
        .filter(|(bit, _)| bits & bit != 0)
        .map(|(_, name)| *name)
        .collect();
    if reasons.is_empty() {
        vec!["None"]
    } else {
        reasons
    }
}

/// Build a JSON array of strings from the given reason names.
fn text_array_value(items: Vec<&str>) -> Value {
    Value::Array(items.into_iter().map(text).collect())
}

fn percent_value(value: Percentage) -> Value {
    u64_value(value.0 as u64)
}

fn first_number_in_display<T: std::fmt::Display>(value: T) -> Option<f64> {
    let rendered = value.to_string();
    let mut token = String::new();
    let mut started = false;
    for ch in rendered.chars() {
        if ch.is_ascii_digit() || ch == '-' || ch == '+' || ch == '.' {
            token.push(ch);
            started = true;
        } else if started {
            break;
        }
    }
    if token.is_empty() || token == "-" || token == "+" || token == "." {
        None
    } else {
        token.parse().ok()
    }
}

fn target_nvml_device<'a>(target: &GpuTarget<'a>) -> PyResult<nvml_wrapper::Device<'a>> {
    let nvml = target.nvml().map_err(to_py_err)?;
    let count = nvml
        .device_count()
        .map_err(|err| to_py_err(format!("NVML device_count failed: {err:?}")))?;
    for index in 0..count {
        let device = nvml
            .device_by_index(index)
            .map_err(|err| to_py_err(format!("NVML device_by_index({index}) failed: {err:?}")))?;
        let id = nvoc_core::gpu_id_from_nvml_device(&device).map_err(to_py_err)?;
        if id.0 == target.id.0 {
            return Ok(device);
        }
    }
    Err(to_py_err(format!(
        "NVML device for GPU {} not found",
        target.id.0
    )))
}

fn normalize_info_nvml(target: &GpuTarget<'_>) -> PyResultValue {
    let device = target_nvml_device(target)?;
    let mut map = Map::new();
    map.insert("gpu_id".into(), u64_value(target.id.0 as u64));
    map.insert("gpu_id_hex".into(), text(format!("0x{:04X}", target.id.0)));
    map.insert("index".into(), u64_value(target.index as u64));
    map.insert("vendor".into(), text("NVIDIA"));

    let name = device
        .name()
        .map_err(|err| to_py_err(format!("NVML name failed: {err:?}")))?;
    map.insert("name".into(), text(&name));
    map.insert("gpu_name".into(), text(name));

    if let Ok(pci) = device.pci_info() {
        map.insert(
            "bus".into(),
            text(format!(
                "{:04x}:{:02x}:{:02x}.0",
                pci.domain, pci.bus, pci.device
            )),
        );
    }
    if let Ok(uuid) = device.uuid() {
        map.insert("uuid".into(), text(uuid));
    }
    if let Ok(brand) = device.brand() {
        map.insert("brand".into(), text(format!("{brand:?}")));
    }

    Ok(Value::Object(map))
}

/// Identity keys shared by every backend's `info`/`status` map.
fn macos_rm_id_map(gpu_id: u32, index: usize) -> Map<String, Value> {
    let mut map = Map::new();
    map.insert("backend".into(), text("macos-rm"));
    map.insert("gpu_id".into(), u64_value(gpu_id as u64));
    map.insert("gpu_id_hex".into(), text(format!("0x{gpu_id:04X}")));
    map.insert("index".into(), u64_value(index as u64));
    map
}

/// macOS RM p-state number → the "P<n>" label every other backend emits.
fn macos_rm_pstate_label(pstate: Option<u32>) -> Option<String> {
    pstate.map(|p| format!("P{p}"))
}

/// Normalize the native macOS RM [`DeviceInfo`] into the same dict shape the
/// NVAPI/NVML `normalize_info` produces, so pynvoc's `query_info`/`discover_gpus`
/// and the GUI/TUI read one contract regardless of backend. Values the RM
/// backend genuinely cannot read on this platform (architecture codename,
/// VBios image, ...) are omitted rather than fabricated; the RM-native fields
/// (project, chip_sku, board_id, ...) ride along under their own keys.
fn info_macos_rm_to_map(gpu_id: u32, index: usize, info: &DeviceInfo) -> Value {
    let mut map = macos_rm_id_map(gpu_id, index);
    map.insert("vendor".into(), text("NVIDIA"));
    let name = info
        .name
        .clone()
        .unwrap_or_else(|| "NVIDIA GPU".to_string());
    map.insert("name".into(), text(&name));
    map.insert("gpu_name".into(), text(name));
    map.insert("driver_version".into(), text(&info.driver_version));
    // `bus` mirrors the NVAPI/NVML "domain:bus:device.0" spelling the GUI
    // dropdown and automation key off; `pci_address` keeps the RM form.
    map.insert("bus".into(), text(&info.pci_address));
    map.insert("pci_address".into(), text(&info.pci_address));
    map.insert("vendor_id".into(), u64_value(info.vendor_id as u64));
    map.insert("device_id".into(), u64_value(info.device_id as u64));
    map.insert("rm_gpu_id".into(), u64_value(info.rm_gpu_id as u64));
    if let Some(value) = info.subsystem_id {
        map.insert("subsystem_id".into(), u64_value(value as u64));
    }
    if let Some(value) = info.revision {
        map.insert("revision".into(), u64_value(value as u64));
    }
    if let Some(value) = info.board_id {
        map.insert("board_id".into(), u64_value(value as u64));
    }
    if let Some(value) = &info.chip_sku {
        map.insert("chip_sku".into(), text(value));
    }
    if let Some(value) = &info.project {
        map.insert("project".into(), text(value));
    }
    if let Some(value) = info.supported_class_count {
        map.insert("supported_class_count".into(), u64_value(value as u64));
    }
    // Memory totals are the same quantities NVML's `memory_info`/`bar1` report;
    // the NVAPI path spells the total as MiB, so emit both.
    if let Some(bytes) = info.memory_total_bytes {
        map.insert("memory_total_bytes".into(), u64_value(bytes));
        map.insert("memory_total_mib".into(), u64_value(bytes / (1024 * 1024)));
    }
    if let Some(bytes) = info.memory_usable_bytes {
        map.insert("memory_usable_bytes".into(), u64_value(bytes));
        map.insert("memory_usable_mib".into(), u64_value(bytes / (1024 * 1024)));
    }
    if let Some(bytes) = info.bar1_bytes {
        map.insert("bar1_bytes".into(), u64_value(bytes));
    }
    if let Some(bytes) = info.l2_cache_bytes {
        map.insert("l2_cache_bytes".into(), u64_value(bytes));
    }
    if !info.unavailable.is_empty() {
        map.insert(
            "unavailable".into(),
            value_object(info.unavailable.iter().map(|(k, v)| (k.as_str(), text(v)))),
        );
    }
    Value::Object(map)
}

/// Normalize the native macOS RM [`DeviceStatus`] into the same dict shape the
/// NVAPI/NVML `normalize_status` produces. All unit conversion is centralized
/// here (kHz→MHz, mW→W, bytes→MiB) so a consumer that already renders
/// `gpu_clock_mhz`/`mem_clock_mhz`/`temperature_c`/`power_w` needs no macOS
/// branch. The RM-only extras (per-fan rpm, engine utilization, PCIe gen,
/// decoded throttle reasons, ...) are emitted alongside under their own keys.
fn status_macos_rm_to_map(gpu_id: u32, index: usize, status: &DeviceStatus) -> Value {
    let mut map = macos_rm_id_map(gpu_id, index);
    if let Some(label) = macos_rm_pstate_label(status.pstate) {
        map.insert("pstate".into(), text(label));
    }
    if let Some(value) = status.pstate {
        map.insert("pstate_id".into(), u64_value(value as u64));
    }
    // Actual (effective) clocks. NVAPI/CLI report MHz; the RM backend reads kHz.
    let actual_clocks = [
        ("gpu_clock_mhz", status.graphics_clock_khz),
        ("mem_clock_mhz", status.memory_clock_khz),
        ("video_clock_mhz", status.video_clock_khz),
        ("sm_clock_mhz", status.sm_clock_khz),
    ];
    for (key, khz) in actual_clocks {
        if let Some(khz) = khz {
            map.insert(key.into(), f64_value(khz as f64 / 1000.0));
        }
    }
    // The TUI dashboard prefers the `eff_*` keys (actually-running clocks);
    // on this backend the measured clock IS the effective one.
    if let Some(khz) = status.graphics_clock_khz {
        map.insert("eff_gpu_clock_mhz".into(), f64_value(khz as f64 / 1000.0));
    }
    if let Some(khz) = status.memory_clock_khz {
        map.insert("eff_mem_clock_mhz".into(), f64_value(khz as f64 / 1000.0));
    }
    // Driver-requested targets, kept distinct from the measured clocks.
    let target_clocks = [
        ("gpu_target_clock_mhz", status.graphics_target_clock_mhz),
        ("mem_target_clock_mhz", status.memory_target_clock_mhz),
        ("video_target_clock_mhz", status.video_target_clock_mhz),
        ("sm_target_clock_mhz", status.sm_target_clock_mhz),
    ];
    for (key, mhz) in target_clocks {
        if let Some(mhz) = mhz {
            map.insert(key.into(), u64_value(mhz as u64));
        }
    }
    // Temperatures: `temperature_c`/`temp_core` keep the NVAPI/nvidia-smi
    // meaning (GPU core), the remaining sensors ride along typed.
    if let Some(celsius) = status.temperature_c {
        map.insert("temperature_c".into(), f64_value(celsius));
        map.insert("temp_core".into(), f64_value(celsius));
    }
    if let Some(celsius) = status.temperature_memory_c {
        map.insert("temperature_memory_c".into(), f64_value(celsius));
        map.insert("temp_memory".into(), f64_value(celsius));
    }
    if let Some(celsius) = status.temperature_board_c {
        map.insert("temperature_board_c".into(), f64_value(celsius));
    }
    if let Some(celsius) = status.temperature_power_supply_c {
        map.insert("temperature_power_supply_c".into(), f64_value(celsius));
    }
    // Power: `power_w`/`power_limit_w` match NVML/NVAPI semantics (instant
    // draw / enforced TGP wall); the requested limit and the driver's own
    // average/policy blocks are distinct quantities.
    if let Some(mw) = status.power_mw {
        map.insert("power_w".into(), f64_value(mw as f64 / 1000.0));
    }
    if let Some(mw) = status.power_limit_mw {
        map.insert("power_limit_w".into(), f64_value(mw as f64 / 1000.0));
    }
    if let Some(mw) = status.power_limit_requested_mw {
        map.insert(
            "power_limit_requested_w".into(),
            f64_value(mw as f64 / 1000.0),
        );
    }
    if let Some(mw) = status.average_power_mw {
        map.insert("average_power_w".into(), f64_value(mw as f64 / 1000.0));
    }
    if let Some(mw) = status.power_policy_tgp_mw {
        map.insert("power_policy_tgp_w".into(), f64_value(mw as f64 / 1000.0));
    }
    if let Some(mask) = status.throttle_reason_mask {
        map.insert("throttle_reason_mask".into(), u64_value(mask as u64));
    }
    if !status.throttle_reasons.is_empty() {
        map.insert(
            "throttle_reasons".into(),
            Value::Array(status.throttle_reasons.iter().map(text).collect()),
        );
    }
    // Fans: `fan_rpm` is the highest live reading (NVML-style single value),
    // `fan_rpms` is the per-cooler list, and `fan_coolers` keeps the driver's
    // own cooler slot next to each reading (the list position is a position
    // after the plausibility filter, so it is not the cooler index). A stopped
    // fan reads 0 and stays 0.
    if let Some(rpm) = status.fan_rpm {
        map.insert("fan_rpm".into(), u64_value(rpm as u64));
    }
    if !status.fan_rpms.is_empty() {
        map.insert(
            "fan_rpms".into(),
            Value::Array(
                status
                    .fan_rpms
                    .iter()
                    .map(|rpm| u64_value(*rpm as u64))
                    .collect(),
            ),
        );
    }
    if !status.fan_coolers.is_empty() {
        map.insert(
            "fan_coolers".into(),
            Value::Array(
                status
                    .fan_coolers
                    .iter()
                    .map(|cooler| {
                        value_object([
                            ("slot", u64_value(cooler.slot as u64)),
                            ("rpm", u64_value(cooler.rpm as u64)),
                        ])
                    })
                    .collect(),
            ),
        );
    }
    if let Some(percent) = status.utilization_percent {
        map.insert("utilization_percent".into(), u64_value(percent as u64));
        map.insert("gpu_utilization".into(), u64_value(percent as u64));
    }
    if let Some(percent) = status.memory_utilization_percent {
        map.insert(
            "memory_utilization_percent".into(),
            u64_value(percent as u64),
        );
        map.insert("mem_utilization".into(), u64_value(percent as u64));
    }
    if !status.engine_utilization_percent.is_empty() {
        map.insert(
            "engine_utilization_percent".into(),
            value_object(
                status
                    .engine_utilization_percent
                    .iter()
                    .map(|(k, v)| (k.as_str(), f64_value(*v))),
            ),
        );
    }
    if let Some(link_gen) = status.pcie_link_gen {
        map.insert("pcie_link_gen".into(), u64_value(link_gen as u64));
    }
    if let Some(max_link_gen) = status.pcie_max_link_gen {
        map.insert("pcie_max_link_gen".into(), u64_value(max_link_gen as u64));
    }
    if let Some(bytes) = status.memory_used_bytes {
        map.insert("memory_used_bytes".into(), u64_value(bytes));
        map.insert(
            "memory_used_mib".into(),
            f64_value(bytes as f64 / (1024.0 * 1024.0)),
        );
    }
    if !status.unavailable.is_empty() {
        map.insert(
            "unavailable".into(),
            value_object(
                status
                    .unavailable
                    .iter()
                    .map(|(k, v)| (k.as_str(), text(v))),
            ),
        );
    }
    Value::Object(map)
}

fn normalize_info(target: &GpuTarget<'_>) -> PyResultValue {
    // macOS RM devices have no NVAPI/NVML handle, so `QueryGpuInfo` cannot
    // serve them; normalize the native RM info into the same contract.
    if target.has_macos_rm() {
        let device = target.macos_rm().map_err(to_py_err)?;
        return Ok(info_macos_rm_to_map(
            target.id.0,
            target.index,
            device.info(),
        ));
    }
    let info = match run(target, QueryGpuInfo) {
        Ok(report) => report.output,
        Err(_) if !target.has_nvapi() && target.has_nvml() => return normalize_info_nvml(target),
        Err(error) => return Err(to_py_err(error)),
    };
    let mut map = Map::new();
    map.insert("gpu_id".into(), u64_value(target.id.0 as u64));
    map.insert("gpu_id_hex".into(), text(format!("0x{:04X}", target.id.0)));
    map.insert("index".into(), u64_value(target.index as u64));
    map.insert("name".into(), text(&info.name));
    map.insert("gpu_name".into(), text(&info.name));
    map.insert("codename".into(), text(&info.codename));
    map.insert("arch".into(), text(info.arch));
    map.insert("gpu_architecture".into(), text(info.arch));
    map.insert("gpu_type".into(), text(info.gpu_type));
    // Generation series from core's gpu_type.rs detect_gpu_type (name +
    // codename) — the single source of truth. On Ada the ArchInfo enum has
    // no AD variant and `gpu_architecture` reads 'Unknown:400:7:161', so
    // capability flags must come from here, not the arch string.
    let series = fetch_gpu_type(&info).unwrap_or(GpuType::Unknown);
    map.insert("gpu_series".into(), text(series.to_string()));
    map.insert("is_mobile".into(), bool_value(series.is_mobile()));
    map.insert("is_server".into(), bool_value(series.is_server()));
    map.insert(
        "is_legacy_voltage".into(),
        bool_value(series.is_legacy_voltage()),
    );
    map.insert(
        "xbar_supported".into(),
        bool_value(series.supports_xbar_offset()),
    );
    map.insert("bios_version".into(), text(&info.bios_version));
    map.insert("bus".into(), text(info.bus));
    if let Some(vendor) = info.vendor() {
        map.insert("vendor".into(), text(vendor));
    }

    for (clock, limit) in &info.vfp_limits {
        let key_prefix = match *clock {
            ClockDomain::Graphics => "core_clock",
            ClockDomain::Memory => "mem_clock",
            _ => continue,
        };
        map.insert(format!("{key_prefix}_range"), text(limit.range));
        map.insert(
            format!("{key_prefix}_min"),
            i64_value(khz_to_mhz_i64(limit.range.min.0)),
        );
        map.insert(
            format!("{key_prefix}_max"),
            i64_value(khz_to_mhz_i64(limit.range.max.0)),
        );
    }

    if let Some(limit) = info.power_limits.first() {
        map.insert(
            "power_limit_min".into(),
            u64_value(limit.range.min.0 as u64),
        );
        map.insert(
            "power_limit_max".into(),
            u64_value(limit.range.max.0 as u64),
        );
        map.insert(
            "power_limit_default".into(),
            u64_value(limit.default.0 as u64),
        );
    }
    if let Ok(power) = run(target, QueryPowerLimits).map(|report| report.output) {
        map.insert(
            "power_limit_nvml_min_w".into(),
            f64_value(power.min_watts as f64),
        );
        map.insert(
            "power_limit_nvml_current_w".into(),
            f64_value(power.current_watts as f64),
        );
        map.insert(
            "power_limit_nvml_max_w".into(),
            f64_value(power.max_watts as f64),
        );
        map.insert("power_watt_min".into(), f64_value(power.min_watts as f64));
        map.insert(
            "power_watt_current".into(),
            f64_value(power.current_watts as f64),
        );
        map.insert("power_watt_max".into(), f64_value(power.max_watts as f64));
    }
    // Populate the process-level enforced-power-limit cache for the 1Hz
    // `normalize_status` poll. `nvmlDeviceGetEnforcedPowerLimit` (the live TGP
    // wall) is an NVML device-level query that segfaults during dGPU power-state
    // transitions, so it CANNOT run in the per-poll path. It runs here instead
    // — `normalize_info` executes at GPU-switch / Refresh-Info time, a
    // low-frequency user-initiated path where the dGPU is D0 and the NVML
    // handle is valid (the same path already runs `QueryPowerLimits`, another
    // NVML device query, without incident). On failure the cache is left
    // untouched (a stale but plausible value is better than nothing; the poll
    // path falls back to `---` only if the cache is empty/expired).
    if target.has_nvml()
        && let Ok(device) = target_nvml_device(target)
        && let Ok(mw) = device.enforced_power_limit()
    {
        cache_power_limit(target.id.0, mw as f64 / 1000.0);
    }
    if let Some(limit) = info.sensor_limits.first() {
        map.insert(
            "thermal_limit_min".into(),
            i64_value(limit.range.min.0 as i64),
        );
        map.insert(
            "thermal_limit_max".into(),
            i64_value(limit.range.max.0 as i64),
        );
        map.insert(
            "thermal_limit_default".into(),
            i64_value(limit.default.0 as i64),
        );
    }
    let overvolts = run(target, QueryLegacyCoreOvervoltRanges)
        .map(|report| report.output)
        .unwrap_or_default();
    if let Some((pstate, current, min, max)) = overvolts.first() {
        map.insert("legacy_overvolt_pstate".into(), text(pstate));
        map.insert(
            "legacy_overvolt_current_mv".into(),
            i64_value(uv_to_mv_i64(current.0)),
        );
        map.insert(
            "legacy_overvolt_min_mv".into(),
            i64_value(uv_to_mv_i64(min.0)),
        );
        map.insert(
            "legacy_overvolt_max_mv".into(),
            i64_value(uv_to_mv_i64(max.0)),
        );
    }
    Ok(Value::Object(map))
}

fn normalize_status(target: &GpuTarget<'_>) -> PyResultValue {
    // macOS RM devices bypass the NVAPI/NVML status sweep entirely: one RM
    // session snapshot covers clocks, temperatures, power, fans, utilization,
    // PCIe width and memory-in-use, and never touches NVAPI/NVML.
    if target.has_macos_rm() {
        let device = target.macos_rm().map_err(to_py_err)?;
        let status = device.status().map_err(to_py_err)?;
        return Ok(status_macos_rm_to_map(target.id.0, target.index, &status));
    }
    let status = run(target, QueryGpuStatus).map_err(to_py_err)?.output;
    // NOTE on NVML: this function is the GUI/TUI 1Hz dashboard poll and must
    // NOT call any NVML device-level query (`power_usage`,
    // `enforced_power_limit`, `pcie_throughput`, ...). Those segfault inside
    // nvml.dll on a stale device handle during dGPU power-state transitions
    // (D3cold↔D0), and NVAPI recovers BEFORE NVML's handle state in that window
    // — so there is no pre-call liveness proof that makes NVML safe. The only
    // previously-live NVML reads were `power_usage`, `enforced_power_limit`, and
    // the PCIe block; all three are now either replaced by NVAPI
    // (`power_w` ← NVAPI PowerMonitor Board rail) or served from the
    // process-level `POWER_LIMIT_CACHE` (filled by the info path). NVAPI
    // returns clean error codes on a lost dGPU (no deref), so the NVAPI reads
    // below are all segfault-safe — they simply yield nothing and the dashboard
    // shows `---`.
    let mut map = Map::new();
    map.insert("gpu_id".into(), u64_value(target.id.0 as u64));
    map.insert("gpu_id_hex".into(), text(format!("0x{:04X}", target.id.0)));
    map.insert("index".into(), u64_value(target.index as u64));
    map.insert("pstate".into(), text(status.pstate));
    // Core voltage. Prefer the private core_voltage() reading; on legacy
    // GPUs (≤ Kepler, e.g. GT730/Fermi) that path yields nothing, so fall
    // back to the PUBLIC GetVoltageDomainsStatus value (the same number the
    // CLI renders as "Voltage Domains → Voltage: 880000 uV" in get-status).
    // Both sources are µV; the domains readout is the driver's authoritative
    // core-domain voltage on those generations.
    let core_voltage_uv = status
        .voltage
        .map(|v| v.0)
        .or_else(|| status.voltage_domains.map(|d| d.voltage.0));
    if let Some(voltage_uv) = core_voltage_uv {
        map.insert("voltage_mv".into(), f64_value(voltage_uv as f64 / 1000.0));
    }
    for (clock, freq) in &status.clocks {
        match *clock {
            ClockDomain::Graphics => {
                map.insert("gpu_clock_mhz".into(), f64_value(freq.0 as f64 / 1000.0));
            }
            ClockDomain::Memory => {
                map.insert("mem_clock_mhz".into(), f64_value(freq.0 as f64 / 1000.0));
            }
            // Video decode engine clock — the third public clock the CLI's
            // get-status prints ("Clocks: Graphics/Memory/Video"); surfaced so
            // the dashboard can show it next to GPU/MEM.
            ClockDomain::Video => {
                map.insert("video_clock_mhz".into(), f64_value(freq.0 as f64 / 1000.0));
            }
            _ => {}
        }
    }
    // Effective (actually-running) clocks from GetAllClocks V2. Emitted as
    // parallel keys since the TUI native path reads this dict directly.
    if let Some(eff) = &status.effective_clocks {
        for (clock, freq) in eff {
            match *clock {
                ClockDomain::Graphics => {
                    map.insert(
                        "eff_gpu_clock_mhz".into(),
                        f64_value(freq.0 as f64 / 1000.0),
                    );
                }
                ClockDomain::Memory => {
                    map.insert(
                        "eff_mem_clock_mhz".into(),
                        f64_value(freq.0 as f64 / 1000.0),
                    );
                }
                _ => {}
            }
        }
    }
    // All 32 clock domains from GetAllClocks V2 (superset of effective_clocks):
    // includes the internal fabric clocks (Gpc, Xbar/crossbar, Sys, Hub, ...).
    // Emitted as a {domain_name: mhz} dict so the TUI/CLI can render the full
    // clock breakdown GPU-Z-style.
    //
    // MOBILE FALLBACK: on mobile GPUs the driver returns extended_domain[] all-
    // zero, so all_clocks is empty/None. In that case, supplement via the private
    // ClockClient MEASURE_FREQ batch (get-clk-domain-freq's backend) — it reads
    // every controllable domain's physical clock directly, covering the fabric
    // domains that GetAllClocks V2 omits on mobile.
    let all_clocks_mhz = if let Some(all) = &status.all_clocks
        && !all.is_empty()
    {
        let entries = all
            .iter()
            .map(|(domain, freq)| (domain.to_string(), f64_value(freq.0 as f64 / 1000.0)))
            .collect::<Vec<_>>();
        Some(value_object(
            entries.iter().map(|(k, v)| (k.as_str(), v.clone())),
        ))
    } else {
        // GetAllClocks V2 yielded no fabric clocks — try MEASURE_FREQ batch.
        // Query the controllable domain set first (for the domain bit list),
        // then batch-measure them.
        let domains = run(target, QueryNvapiClkDomains)
            .map(|report| report.output)
            .ok()
            .flatten()
            .map(|c| c.entries.iter().map(|e| e.bit).collect::<Vec<u32>>())
            .unwrap_or_default();
        if domains.is_empty() {
            None
        } else {
            let freqs = run(target, QueryNvapiClkDomainFreqsBatch { domains })
                .map(|report| report.output)
                .ok()
                .flatten();
            freqs.and_then(|fs| {
                if fs.is_empty() {
                    return None;
                }
                let entries = fs
                    .iter()
                    .map(|f| (f.domain.to_string(), f64_value(f.freq_mhz)))
                    .collect::<Vec<_>>();
                Some(value_object(
                    entries.iter().map(|(k, v)| (k.as_str(), v.clone())),
                ))
            })
        }
    };
    if let Some(v) = all_clocks_mhz {
        map.insert("all_clocks_mhz".into(), v);
    }
    if let Some((_sensor, temp)) = status.sensors.first() {
        map.insert("temperature_c".into(), f64_value(*temp as f64));
    }
    // Full thermal-sensor list as `[descriptor, temp_celsius]` pairs (same shape
    // as get-status JSON), plus the three primary typed temperatures pulled out
    // by channel_type (0=GPU_AVG/core, 1=GPU_MAX/hot spot, 3=MEMORY/VRAM). The
    // TUI dashboard reads temp_core/temp_hotspot/temp_memory directly (the
    // native path bypasses normalize_query_output, so it cannot re-parse the
    // sensors array itself); `temperature_c` above stays as the core fallback.
    if !status.sensors.is_empty() {
        if let Ok(v) = serde_json::to_value(&status.sensors)
            && !v.is_null()
        {
            map.insert("sensors".into(), v);
        }
        for (desc, temp) in &status.sensors {
            match desc.channel_type {
                Some(0) if !map.contains_key("temp_core") => {
                    map.insert("temp_core".into(), f64_value(*temp as f64));
                }
                Some(1) if !map.contains_key("temp_hotspot") => {
                    map.insert("temp_hotspot".into(), f64_value(*temp as f64));
                }
                Some(3) if !map.contains_key("temp_memory") => {
                    map.insert("temp_memory".into(), f64_value(*temp as f64));
                }
                _ => {}
            }
        }
    }
    // Thermal policy thresholds from the private ClientThermalTarget table
    // (GET-prime 0xC4554575). Two values the TUI dashboard pairs with the live
    // sensor readings: policy 2 = the target-temperature wall (nvidia-smi "GPU
    // Target Temperature" = NVML's GpsCurr channel), policy 1 = max operating
    // temp (NVML's GpuMax). Emitted as plain keys so the TUI can render
    // `CORE <live> / <target> C` and `HOTSPOT <live> / <max> C`. Best-effort:
    // omitted where the driver doesn't expose the slot (desktop GPUs).
    if target.has_nvapi()
        && let Ok(policies) = run(target, QueryNvapiTargetTempPolicies)
    {
        for p in policies.output {
            match p.policy_index {
                2 => {
                    map.insert("target_temp_c".into(), f64_value(p.celsius as f64));
                }
                1 => {
                    map.insert("max_temp_c".into(), f64_value(p.celsius as f64));
                }
                _ => {}
            }
        }
    }
    // Live board power draw (watts). PREFER the NVAPI PowerMonitor "Board"
    // rail (InputTotalBoard, pwr_rail 245/223) over NVML's
    // `nvmlDeviceGetPowerUsage`:
    //
    //   - SAME semantic (total board power draw ≈ nvidia-smi's power_usage).
    //   - NVAPI returns a clean NvAPI_Status error when the dGPU is
    //     D3cold/GCOFF; NVML's `nvmlDeviceGetPowerUsage` segfaults inside
    //     nvml.dll on the stale handle (verified: INVALID_POINTER_READ at
    //     nvmlDeviceGetPowerUsage+0x268, killing NVOC-GUI when the user
    //     switches to 核显模式 / powers the dGPU off). The NVML call is a
    //     native segfault — uncatchable by `panic="abort"` or any try/except,
    //     so it MUST be prevented, not caught.
    //   - Zero extra cost: `status.power_rails` is already populated by
    //     `Gpu::status()` above (the NVAPI PowerMonitor sweep). No new NVAPI
    //     call, no dGPU wake — the dashboard's 1Hz poll does NOT wake the GPU.
    //
    // NVML's `nvmlDeviceGetPowerUsage` is intentionally NOT used as a fallback.
    // It segfaults inside nvml.dll on a stale handle during dGPU transitions
    // (verified: `nvmlDeviceGetPowerUsage+0x268`), and NVAPI recovers before
    // NVML in that window so no pre-call check can make it safe. If neither
    // NVAPI path yields a reading, `power_w` is simply omitted (`---`).
    let mut power_w_set = false;
    if let Some(rails) = &status.power_rails {
        // InputTotalBoard (245) / InputTotalBoard2 (223) = total board power,
        // the GPU-Z "Board Power" equivalent of NVML power_usage. Prefer 245
        // (primary), then 223 (second board-total on some SKUs).
        let board = rails
            .iter()
            .find(|r| r.pwr_rail == 245 && r.pwr_mw > 0)
            .or_else(|| rails.iter().find(|r| r.pwr_rail == 223 && r.pwr_mw > 0));
        if let Some(r) = board {
            map.insert("power_w".into(), f64_value(r.pwr_mw as f64 / 1000.0));
            power_w_set = true;
        }
    }
    if !power_w_set
        && let Some((_channel, power)) = status.power.iter().next()
        && let Some(watts) = first_number_in_display(power)
    {
        // Legacy NVAPI power-topology fallback (dimensionless % on some GPUs,
        // but real watts where present). Does not touch NVML.
        map.insert("power_w".into(), f64_value(watts));
    }
    // Current enforced power limit (the live TGP cap, watts). NVML-only
    // (`nvmlDeviceGetEnforcedPowerLimit`, same "Current Power Limit" nvidia-smi
    // -q -d POWER reports).
    //
    // NOT read live here: `nvmlDeviceGetEnforcedPowerLimit` is an NVML
    // device-level query that segfaults inside nvml.dll on a stale handle
    // during dGPU power-state transitions (verified crash at
    // `nvmlDeviceGetEnforcedPowerLimit+0x276` when re-enabling a powered-off
    // dGPU — NVAPI recovers before NVML's handle state, so no pre-call liveness
    // proof can make NVML device queries safe). The enforced limit is
    // quasi-static (TGP wall), so it is populated by the info path
    // (`normalize_info`, user-initiated / GPU-switch time, when the dGPU is D0)
    // and served from the process-level `POWER_LIMIT_CACHE` here. On cache
    // miss/expiry (e.g. the dGPU was off when info last ran) the field is
    // simply omitted — the dashboard shows `---` rather than crashing.
    if let Some(watts) = cached_power_limit(target.id.0) {
        map.insert("power_limit_w".into(), f64_value(watts));
    }

    // Per-rail power (watts) from NVAPI PowerMonitor, keyed by the
    // descriptor's rail IDENTITY (correct on every GPU — laptop vs desktop
    // expose different rail sets/orderings). Emits a { "<RailName>": <watts> }
    // object. The key carries a confidence marker: plain (Measured, private
    // GetStatus offset), `~` (Inferred, disambiguated from a shared offset), or
    // `?` (Ambiguous, full-board view). Unavailable rails (no GetStatus data)
    // are omitted entirely.
    if let Some(rails) = &status.power_rails {
        let mut rail_map = Map::new();
        for r in rails {
            if r.pwr_mw == 0 {
                continue;
            }
            let suffix = match r.confidence {
                nvapi::Confidence::Measured => "",
                nvapi::Confidence::Inferred => "~",
                _ => "?", // Ambiguous (or Unavailable, though pwr_mw!=0 filters most)
            };
            let key = if suffix.is_empty() {
                r.rail_name.clone()
            } else {
                format!("{}{}", r.rail_name, suffix)
            };
            rail_map.insert(key, f64_value(r.pwr_mw as f64 / 1000.0));
        }
        if !rail_map.is_empty() {
            map.insert("power_rails_w".into(), Value::Object(rail_map));
        }
    }

    // Bidirectional real-time PCIe bandwidth (MiB/s), nvitop/HWMonitor-style.
    // `nvmlDeviceGetPcieThroughput` reports KB/s averaged over a ~20ms byte-counter
    // interval (i.e. it IS the live rate — no sliding window needed). TX = bytes
    // the GPU sends (GPU->host), RX = bytes the GPU receives (host->GPU), matching
    // the nvidia-smi / nvitop "Tx/Rx" convention. Maxwell+ only; vGPU unsupported.
    //
    // REMOVED from the per-poll path: these are NVML device-level queries on
    // the cached handle and segfault inside nvml.dll during dGPU power-state
    // transitions (same fault class as `nvmlDeviceGetEnforcedPowerLimit` — NVAPI
    // recovers before NVML's handle state, so no pre-call proof can guard them).
    // The fields are simply omitted on the dashboard poll; they are not part of
    // the metric rows (GPU/MEM/VOLT/TEMP/PWR) and only appeared in the
    // `status -a` JSON, so omitting them has no dashboard impact. A future
    // low-frequency info-path read (mirroring the power-limit cache) could
    // restore them if a consumer needs them.
    // (previously: target_nvml_device + pcie_throughput/replay_counter/link_gen)
    map.insert(
        "vfp_locked".into(),
        bool_value(!status.vfp_locks.is_empty()),
    );
    for lock in status.vfp_locks.values() {
        if let Some(mv) = first_number_in_display(lock) {
            map.insert("vfp_lock_mv".into(), f64_value(mv));
            break;
        }
    }

    // Per-domain utilization %: { Graphics, FrameBuffer(=memory controller),
    // VideoEngine, BusInterface }. Serialized verbatim (matches get-status JSON).
    if let Ok(v) = serde_json::to_value(&status.utilization)
        && !v.is_null()
    {
        map.insert("utilization".into(), v);
    }

    // VRAM (KiB): used = dedicated - dedicated_available_current.
    if let Some(mem) = status.memory {
        let total = mem.dedicated.0;
        let free = mem.dedicated_available_current.0;
        let used = total.saturating_sub(free);
        map.insert(
            "vram".into(),
            value_object([
                ("total_kib", u64_value(total as u64)),
                ("used_kib", u64_value(used as u64)),
                ("free_kib", u64_value(free as u64)),
                ("shared_kib", u64_value(mem.shared.0 as u64)),
            ]),
        );
    }

    // Per-cooler fan status (rpm + level % + active), serialized verbatim.
    if let Ok(v) = serde_json::to_value(&status.coolers)
        && !v.is_null()
    {
        map.insert("coolers".into(), v);
    }

    // PCIe link width (downstream lane count).
    if let Some(lanes) = status.pcie_lanes {
        map.insert("pcie_lanes".into(), u64_value(lanes as u64));
    }

    // NVAPI perf / throttle-limit flags (raw bitset; overlaps NVML throttle
    // reasons). `limits_decoded` is the same mask rendered as reason names so
    // consumers (TUI/CLI) don't each have to re-decode the bits.
    let perf_limits_bits = status.perf.limits.bits();
    map.insert(
        "perf".into(),
        value_object([
            ("unknown", u64_value(status.perf.unknown as u64)),
            ("limits", u64_value(perf_limits_bits as u64)),
            (
                "limits_decoded",
                text_array_value(decode_perf_flags(perf_limits_bits)),
            ),
        ]),
    );

    Ok(Value::Object(map))
}

/// Normalize the native macOS RM settings projection into the same dict shape
/// `normalize_settings` produces for NVAPI/NVML: the writable families the
/// backend cannot read are *absent* and named in `unavailable`, and the
/// capability flags say up front that writes/restore are not available, so a
/// UI disables its controls for the real reason instead of guessing.
fn settings_macos_rm_to_map(
    gpu_id: u32,
    index: usize,
    settings: &nvoc_core::DeviceSettings,
) -> Value {
    let mut map = Map::new();
    map.insert("backend".into(), text("macos-rm"));
    map.insert("gpu_id".into(), u64_value(gpu_id as u64));
    map.insert("gpu_id_hex".into(), text(format!("0x{gpu_id:04X}")));
    map.insert("index".into(), u64_value(index as u64));
    if let Some(pstate) = settings.pstate {
        map.insert("pstate".into(), text(format!("P{pstate}")));
        map.insert("pstate_id".into(), u64_value(pstate as u64));
    }
    // Milliwatts are exact; keep whole watts integral like the NVAPI path and
    // fall back to a fractional value when the driver reports a sub-watt limit.
    let watts = |mw: u32| {
        if mw.is_multiple_of(1000) {
            u64_value((mw / 1000) as u64)
        } else {
            f64_value(mw as f64 / 1000.0)
        }
    };
    if let Some(mw) = settings.power_limit_mw {
        map.insert("power_limit_current".into(), watts(mw));
        map.insert("power_limit_w".into(), f64_value(mw as f64 / 1000.0));
    }
    if let Some(mw) = settings.power_limit_requested_mw {
        map.insert(
            "power_limit_requested_w".into(),
            f64_value(mw as f64 / 1000.0),
        );
    }
    if let Some(policy) = settings.power_limit_policy {
        map.insert("power_limit_current".into(), watts(policy.current_mw));
        map.insert("power_limit_min".into(), watts(policy.minimum_mw));
        map.insert("power_limit_default".into(), watts(policy.default_mw));
        map.insert("power_limit_max".into(), watts(policy.maximum_mw));
        map.insert(
            "power_limit_policy".into(),
            serde_json::to_value(policy).unwrap_or(Value::Null),
        );
    }
    if let Some(link_gen) = settings.pcie_link_gen {
        map.insert("pcie_link_gen".into(), u64_value(link_gen as u64));
    }
    if let Some(bytes) = settings.memory_total_bytes {
        map.insert("memory_total_bytes".into(), u64_value(bytes));
        map.insert("memory_total_mib".into(), u64_value(bytes / (1024 * 1024)));
    }
    if let Some(bytes) = settings.memory_used_bytes {
        map.insert("memory_used_bytes".into(), u64_value(bytes));
        map.insert(
            "memory_used_mib".into(),
            f64_value(bytes as f64 / (1024.0 * 1024.0)),
        );
    }
    if let Some(value) = settings.board_id {
        map.insert("board_id".into(), u64_value(value as u64));
    }
    if let Some(value) = &settings.chip_sku {
        map.insert("chip_sku".into(), text(value));
    }
    if let Some(value) = &settings.project {
        map.insert("project".into(), text(value));
    }
    // The measured offset window. Absent rather than zeroed when the driver
    // refused the range block, so a UI can tell "unknown" from "0 kHz".
    match &settings.clock_offset_limits_khz {
        Some(limits) => {
            map.insert(
                "clock_offset_min_khz".into(),
                i64_value(limits.min_khz as i64),
            );
            map.insert(
                "clock_offset_max_khz".into(),
                i64_value(limits.max_khz as i64),
            );
            map.insert(
                "clock_offset_limits_records".into(),
                u64_value(limits.records as u64),
            );
        }
        // Absent is not enough: a caller cannot tell a refused range block from a
        // block the read never reached, and the whole point of this projection is
        // that a missing window is never a window of zero. The reason is the
        // range block's own outcome, taken from the same shared accessor the CLI
        // entry uses, so the two frontends cannot describe it differently.
        None => {
            if let Some(reason) = settings.window_absence_reason() {
                map.insert("clock_offset_window_reason".into(), text(&reason));
            }
        }
    }
    // The offsets the driver currently applies, per domain, from the clock
    // document. Absent rather than empty when that block was refused or
    // malformed, so "no offset was read" never looks like "no offset is
    // applied". The frequency is carried alongside because it is what identifies
    // the domain: only bit 0x10 can be named (as memory) from the in-tree NVAPI
    // copy, and its frequency is the live memory clock.
    if let Some(offsets) = &settings.clock_offsets {
        map.insert(
            "clock_offsets".into(),
            Value::Array(
                offsets
                    .iter()
                    .map(|offset| {
                        value_object([
                            ("domain_bit", u64_value(offset.domain_bit as u64)),
                            ("frequency_khz", u64_value(offset.frequency_khz as u64)),
                            ("offset_khz", i64_value(offset.offset_khz as i64)),
                        ])
                    })
                    .collect(),
            ),
        );
    }
    // The named subset: only the records whose own published frequency equalled
    // the live clock of the same snapshot, and only where a single record did.
    // The name is therefore a measurement this backend made, not a table lookup
    // (the public headers name no clock domain), and the comparison travels with
    // it so a caller can reject it. Absent when the offset channel was not read;
    // an empty list means it was read and named nothing.
    if let Some(named) = &settings.clock_offsets_named {
        map.insert(
            "clock_offsets_named".into(),
            Value::Array(
                named
                    .iter()
                    .map(|entry| {
                        value_object([
                            ("name", Value::String(entry.name.clone())),
                            ("domain_bit", u64_value(entry.domain_bit as u64)),
                            ("frequency_khz", u64_value(entry.frequency_khz as u64)),
                            ("offset_khz", i64_value(entry.offset_khz as i64)),
                            // The value a restore must write back is the value
                            // read now, so it is carried explicitly: the write
                            // gate's target is read, never invented later.
                            ("restore_khz", i64_value(entry.offset_khz as i64)),
                            ("corroboration", Value::String(entry.corroboration.clone())),
                        ])
                    })
                    .collect(),
            ),
        );
    }
    // The domains the driver will offset. Each entry is the device's own index,
    // domain bit and type stamp: the bit is a *position* in a domain namespace
    // this frontend cannot name (see `ClockOffsetDomain`), so no name is
    // invented here either. Absent when the directory was not read, so "no
    // domains" and "not read" stay distinguishable.
    if let Some(domains) = &settings.clock_offset_domains {
        map.insert(
            "clock_offset_domains".into(),
            Value::Array(
                domains
                    .iter()
                    .map(|domain| {
                        value_object([
                            ("index", u64_value(domain.index as u64)),
                            ("domain_bit", u64_value(domain.domain_bit as u64)),
                            ("type_stamp", u64_value(domain.type_stamp as u64)),
                        ])
                    })
                    .collect(),
            ),
        );
    }
    let caps = settings.capabilities;
    map.insert(
        "capabilities".into(),
        value_object([
            ("settings_read", bool_value(caps.settings_read)),
            ("settings_write", bool_value(caps.settings_write)),
            ("settings_restore", bool_value(caps.settings_restore)),
            ("performance_writes", bool_value(caps.settings_write)),
            (
                "graphics_offset_write",
                bool_value(caps.graphics_offset_write),
            ),
            ("memory_offset_write", bool_value(caps.memory_offset_write)),
            ("power_limit_write", bool_value(caps.power_limit_write)),
        ]),
    );
    if !settings.unavailable.is_empty() {
        map.insert(
            "unavailable".into(),
            value_object(
                settings
                    .unavailable
                    .iter()
                    .map(|(k, v)| (k.as_str(), text(v))),
            ),
        );
    }
    if !settings.notes.is_empty() {
        map.insert(
            "notes".into(),
            Value::Array(settings.notes.iter().map(text).collect()),
        );
    }
    Value::Object(map)
}

fn normalize_settings(target: &GpuTarget<'_>) -> PyResultValue {
    // macOS RM: same dispatch rule as info/status. The RM settings query is
    // read-only and never touches NVAPI/NVML.
    if target.has_macos_rm() {
        let device = target.macos_rm().map_err(to_py_err)?;
        let settings = device.settings().map_err(to_py_err)?;
        return Ok(settings_macos_rm_to_map(
            target.id.0,
            target.index,
            &settings,
        ));
    }
    let settings = run(target, QueryGpuSettings).map_err(to_py_err)?.output;
    let mut map = Map::new();
    map.insert("gpu_id".into(), u64_value(target.id.0 as u64));
    map.insert("gpu_id_hex".into(), text(format!("0x{:04X}", target.id.0)));
    map.insert("index".into(), u64_value(target.index as u64));

    if let Some(boost) = settings.voltage_boost {
        map.insert("voltage_boost_current".into(), u64_value(boost.0 as u64));
    }
    if let Some(limit) = settings.power_limits.first() {
        map.insert("power_limit_current".into(), i64_value(limit.0 as i64));
    }
    if let Some(limit) = settings.sensor_limits.first() {
        map.insert(
            "thermal_limit_current".into(),
            i64_value(limit.value.0 as i64),
        );
    }

    for (pstate, clocks) in &settings.pstate_deltas {
        for (clock, delta) in clocks {
            if *pstate != PState::P0 {
                continue;
            }
            match *clock {
                ClockDomain::Graphics => {
                    map.insert(
                        "core_clock_current".into(),
                        i64_value(khz_to_mhz_i64(delta.0)),
                    );
                }
                ClockDomain::Memory => {
                    map.insert(
                        "mem_clock_current".into(),
                        i64_value(khz_to_mhz_i64(delta.0)),
                    );
                }
                _ => {}
            }
        }
    }

    if let Ok(pstates) = run(target, QueryPstates).map(|report| report.output) {
        let mut labels = Vec::new();
        let mut ranges = Vec::new();
        for item in pstates {
            let label = nvml_pstate_to_str(item.pstate).to_string();
            labels.push(Value::String(label.clone()));
            ranges.push(value_object([
                ("pstate", Value::String(label)),
                ("min_core_mhz", u64_value(item.min_core_mhz as u64)),
                ("max_core_mhz", u64_value(item.max_core_mhz as u64)),
                ("min_memory_mhz", u64_value(item.min_memory_mhz as u64)),
                ("max_memory_mhz", u64_value(item.max_memory_mhz as u64)),
            ]));
        }
        map.insert("supported_pstates".into(), Value::Array(labels));
        map.insert("pstate_ranges".into(), Value::Array(ranges));
    } else {
        // NVML-only QueryPstates failed (NVAPI-only backend, e.g. GT730 on
        // R391.35 where Nvml::init() returns an error). Fall back to the
        // `pstates` list the `QueryGpuSettings` call above ALREADY fetched
        // (via NvAPI_GPU_GetPstates20 — the same source get-info renders).
        // This reuses the in-hand `settings.pstates` roster; no extra NVAPI
        // call and no full `QueryGpuInfo` round-trip. `pstate_ranges` (the
        // NVML-shaped clock bounds) is left empty on this path.
        if !settings.pstates.is_empty() {
            let labels: Vec<Value> = settings
                .pstates
                .iter()
                .map(|pstate| Value::String(pstate.to_string()))
                .collect();
            map.insert("supported_pstates".into(), Value::Array(labels));
        }
    }

    if let Ok(power) = run(target, QueryPowerLimits).map(|report| report.output) {
        map.insert(
            "power_limit_nvml_min_w".into(),
            f64_value(power.min_watts as f64),
        );
        map.insert(
            "power_limit_nvml_current_w".into(),
            f64_value(power.current_watts as f64),
        );
        map.insert(
            "power_limit_nvml_max_w".into(),
            f64_value(power.max_watts as f64),
        );
    }
    if let Ok(fan) = run(target, QueryFanInfo).map(|report| report.output) {
        map.insert("fan_count".into(), u64_value(fan.count as u64));
        map.insert("fan_min".into(), option_u32(fan.min_speed));
        map.insert("fan_max".into(), option_u32(fan.max_speed));
    }
    if let Ok(thresholds) = run(target, QueryTemperatureThresholds).map(|report| report.output) {
        map.insert(
            "temperature_thresholds".into(),
            Value::Array(
                thresholds
                    .into_iter()
                    .map(|threshold| {
                        value_object([
                            ("name", Value::String(threshold.name.to_string())),
                            ("celsius", option_u32(threshold.celsius)),
                        ])
                    })
                    .collect(),
            ),
        );
    }
    let overvolts = run(target, QueryLegacyCoreOvervoltRanges)
        .map(|report| report.output)
        .unwrap_or_default();
    if let Some((pstate, current, min, max)) = overvolts.first() {
        map.insert("legacy_overvolt_pstate".into(), text(pstate));
        map.insert(
            "legacy_overvolt_current_mv".into(),
            i64_value(uv_to_mv_i64(current.0)),
        );
        map.insert(
            "legacy_overvolt_min_mv".into(),
            i64_value(uv_to_mv_i64(min.0)),
        );
        map.insert(
            "legacy_overvolt_max_mv".into(),
            i64_value(uv_to_mv_i64(max.0)),
        );
    }

    let mut locks = Map::new();
    for (id, lock) in &settings.vfp_locks {
        if let Some(value) = lock.lock_value {
            locks.insert(id.to_string(), text(value));
        }
    }
    map.insert("vfp_locks".into(), Value::Object(locks));
    Ok(Value::Object(map))
}

fn normalize_supported_app_clocks(target: &GpuTarget<'_>) -> PyResultValue {
    let items = run(target, QuerySupportedApplicationsClocks)
        .map_err(to_py_err)?
        .output
        .into_iter()
        .map(|item| {
            value_object([
                ("memory_mhz", u64_value(item.memory_mhz as u64)),
                (
                    "graphics_mhz",
                    Value::Array(
                        item.graphics_mhz
                            .into_iter()
                            .map(|v| u64_value(v as u64))
                            .collect(),
                    ),
                ),
            ])
        })
        .collect();
    Ok(Value::Array(items))
}

/// The backend set a fan / power-limit read targets when the caller does not
/// name one: `nvml` on NVAPI platforms (its historical default, unchanged), and
/// the native RM backend on macOS.
///
/// This is not a convenience. Both projections carry a macOS branch that answers
/// from the RM snapshot instead of failing on the missing NVML handle, and a
/// hard-coded `nvml` made that branch unreachable: on macOS `with_target` calls
/// `ensure_backend_available` first, which refuses every set but `macos-rm`, so
/// the entry failed with "NVAPI/NVML are not available on macOS" before the
/// projection ever ran -- the backend could answer and the entry still refused.
/// `macos: false` keeps the other platforms byte-for-byte as they were.
fn fan_power_backends(macos: bool) -> &'static str {
    if macos { "macos-rm" } else { "nvml" }
}

fn normalize_power_limits(target: &GpuTarget<'_>) -> PyResultValue {
    // Shared native policy decoder supplies configuration and bounds;
    // the independently sampled RUSD wall keeps its own field.
    if target.has_macos_rm() {
        let device = target.macos_rm().map_err(to_py_err)?;
        let report = device.power_limit_report().map_err(to_py_err)?;
        let value = serde_json::to_value(&report).map_err(to_py_err)?;
        return Ok(value);
    }
    let power = run(target, QueryPowerLimits).map_err(to_py_err)?.output;
    Ok(value_object([
        ("min_watt", f64_value(power.min_watts as f64)),
        ("current_watt", f64_value(power.current_watts as f64)),
        ("max_watt", f64_value(power.max_watts as f64)),
    ]))
}

/// The legacy thermal entry, in the same shape and from the same sources as the
/// CLI's `get-legacy-temp-sensor`.
///
/// macOS answers from the native RM thermal view: the live reading of every
/// sensor this backend knows, the per-sensor physical range when the public
/// thermal read answers for that sensor's target, and a per-field reason for
/// whatever stays null. Other platforms keep the NVAPI thermal-settings path
/// unchanged.
///
/// Without this function the Python entry could only reach the live readings,
/// through `query_status`'s `sensors` list of `(descriptor, temperature)` pairs
/// -- and a pair carries no range, so the range this backend reads was
/// unreachable from Python while core and the CLI could both report it.
fn normalize_legacy_temp_sensor(target: &GpuTarget<'_>) -> PyResultValue {
    if target.has_macos_rm() {
        let device = target.macos_rm().map_err(to_py_err)?;
        let view = device.thermal_view().map_err(to_py_err)?;
        return serde_json::to_value(&view).map_err(to_py_err);
    }
    let sensors = run(target, QueryNvapiThermalSettings)
        .map_err(to_py_err)?
        .output;
    Ok(Value::Array(
        sensors
            .into_iter()
            .map(|s| {
                value_object([
                    ("target", Value::String(format!("{:?}", s.target))),
                    ("controller", Value::String(format!("{:?}", s.controller))),
                    ("current_c", i64_value(s.current_c as i64)),
                    ("min_c", i64_value(s.min_c as i64)),
                    ("max_c", i64_value(s.max_c as i64)),
                ])
            })
            .collect(),
    ))
}

fn normalize_pstates(target: &GpuTarget<'_>) -> PyResultValue {
    let items = run(target, QueryPstates)
        .map_err(to_py_err)?
        .output
        .into_iter()
        .map(|item| {
            value_object([
                (
                    "pstate",
                    Value::String(nvml_pstate_to_str(item.pstate).to_string()),
                ),
                ("min_core_mhz", u64_value(item.min_core_mhz as u64)),
                ("max_core_mhz", u64_value(item.max_core_mhz as u64)),
                ("min_memory_mhz", u64_value(item.min_memory_mhz as u64)),
                ("max_memory_mhz", u64_value(item.max_memory_mhz as u64)),
            ])
        })
        .collect();
    Ok(Value::Array(items))
}

fn normalize_fan_info(target: &GpuTarget<'_>) -> PyResultValue {
    // macOS RM: same dispatch rule as info/status/settings. This backend does
    // publish a tachometer reading per cooler, so the entry answers from it
    // instead of failing on the missing NVML handle; the projection is core's,
    // so this entry cannot state anything the backend does not.
    if target.has_macos_rm() {
        let device = target.macos_rm().map_err(to_py_err)?;
        let report = device.status().map_err(to_py_err)?.fan_report();
        let value = serde_json::to_value(&report).map_err(to_py_err)?;
        return Ok(value);
    }
    let fan = run(target, QueryFanInfo).map_err(to_py_err)?.output;
    Ok(value_object([
        ("count", u64_value(fan.count as u64)),
        ("min_percent", option_u32(fan.min_speed)),
        ("max_percent", option_u32(fan.max_speed)),
        // v1 `nvmlDeviceGetFanSpeed` on legacy NVML — the one live value on
        // legacy GPUs (min/max are v2-only and absent there).
        ("current_percent", option_u32(fan.current_speed)),
    ]))
}

/// NVAPI cooler-family descriptors (private FanCoolerGetInfo 0x65CE5BFC):
/// per-cooler info + count. The frontends pair this with NVML fan info for
/// the legacy verdict — NVML fans ≥1 with an EMPTY private NVAPI cooler
/// family is the ≤Kepler-driver signature (GT730 live); modern cards report
/// their coolers through the family too (1650S/A4000 both count=1 live).
fn normalize_cooler_info(target: &GpuTarget<'_>) -> PyResultValue {
    let coolers = run(target, QueryNvapiCoolerInfo).map_err(to_py_err)?.output;
    let count = coolers.len();
    let entries = Value::Array(
        coolers
            .into_iter()
            .map(|c| {
                value_object([
                    ("index", u64_value(c.index as u64)),
                    ("type", u64_value(c.cooler_type as u64)),
                    ("min", u64_value(c.min as u64)),
                    ("max", u64_value(c.max as u64)),
                    ("current", u64_value(c.current as u64)),
                    (
                        "current_pwm_percent",
                        u64_value(c.current_pwm_percent as u64),
                    ),
                ])
            })
            .collect(),
    );
    Ok(value_object([
        ("count", u64_value(count as u64)),
        ("coolers", entries),
    ]))
}

fn normalize_temperature_thresholds(target: &GpuTarget<'_>) -> PyResultValue {
    let items = run(target, QueryTemperatureThresholds)
        .map_err(to_py_err)?
        .output
        .into_iter()
        .map(|item| {
            value_object([
                ("name", Value::String(item.name.to_string())),
                ("celsius", option_u32(item.celsius)),
            ])
        })
        .collect();
    Ok(Value::Array(items))
}

fn normalize_throttle_reasons(target: &GpuTarget<'_>) -> PyResultValue {
    let items = run(target, QueryThrottleReasons)
        .map_err(to_py_err)?
        .output
        .into_iter()
        .map(|item| {
            value_object([
                ("name", Value::String(item.name)),
                ("active", bool_value(item.active)),
            ])
        })
        .collect();
    Ok(Value::Array(items))
}

fn normalize_legacy_overvolt_ranges(
    target: &GpuTarget<'_>,
    pstate_filter: Option<PState>,
) -> PyResultValue {
    let items = run(target, QueryLegacyCoreOvervoltRanges)
        .map_err(to_py_err)?
        .output
        .into_iter()
        .filter(|(_pstate, _, _, _)| pstate_filter.is_none_or(|_| true))
        .map(|(pstate, current, min, max)| {
            value_object([
                ("pstate", text(pstate)),
                ("min_uv", i64_value(min.0 as i64)),
                ("current_uv", i64_value(current.0 as i64)),
                ("max_uv", i64_value(max.0 as i64)),
            ])
        })
        .collect();
    Ok(Value::Array(items))
}

fn normalize_pstate_base_voltage(target: &GpuTarget<'_>, pstate: PState) -> PyResultValue {
    let voltage = run(target, QueryPstateBaseVoltage { pstate })
        .map_err(to_py_err)?
        .output;
    Ok(value_object([
        ("pstate", text(voltage.pstate)),
        (
            "voltage_domain",
            Value::String(voltage_domain_label(voltage.voltage_domain).to_string()),
        ),
        ("editable", bool_value(voltage.editable)),
        ("voltage_uv", u64_value(voltage.voltage.0 as u64)),
        ("delta_uv", i64_value(voltage.delta.0 as i64)),
        ("min_delta_uv", i64_value(voltage.min_delta.0 as i64)),
        ("max_delta_uv", i64_value(voltage.max_delta.0 as i64)),
    ]))
}

fn normalize_voltage_boost(target: &GpuTarget<'_>) -> PyResultValue {
    let boost = run(target, QueryVoltageBoost).map_err(to_py_err)?.output;
    Ok(value_object([(
        "voltage_boost_percent",
        boost
            .voltage_boost
            .map(percent_value)
            .unwrap_or(Value::Null),
    )]))
}

fn normalize_auto_boost(target: &GpuTarget<'_>) -> PyResultValue {
    let state = run(target, QueryAutoBoost).map_err(to_py_err)?.output;
    Ok(value_object([
        ("enabled", bool_value(state.enabled)),
        ("default_enabled", bool_value(state.default_enabled)),
    ]))
}

fn normalize_api_restriction(target: &GpuTarget<'_>, api_type: Api) -> PyResultValue {
    let state = run(target, QueryApiRestriction { api_type })
        .map_err(to_py_err)?
        .output;
    Ok(value_object([
        (
            "api",
            Value::String(api_restriction_api_label(state.api_type).to_string()),
        ),
        ("restricted", bool_value(state.restricted)),
    ]))
}

fn normalize_displays(target: &GpuTarget<'_>, all: bool) -> PyResultValue {
    let items = run(target, QueryDisplays { all })
        .map_err(to_py_err)?
        .output
        .into_iter()
        .map(|display| {
            value_object([
                (
                    "display_id",
                    Value::String(format!("0x{:08X}", display.display_id)),
                ),
                ("display_id_u32", u64_value(display.display_id as u64)),
                ("connector", Value::String(display.connector)),
                (
                    "flags_hex",
                    Value::String(format!("0x{:08X}", display.flags_bits)),
                ),
                ("connected", bool_value(display.connected)),
                (
                    "physically_connected",
                    bool_value(display.physically_connected),
                ),
                ("active", bool_value(display.active)),
                ("os_visible", bool_value(display.os_visible)),
                ("dynamic", bool_value(display.dynamic)),
                ("mst_root", bool_value(display.mst_root)),
                ("wireless", bool_value(display.wireless)),
            ])
        })
        .collect();
    Ok(Value::Array(items))
}

fn normalize_edid(target: &GpuTarget<'_>, display_id: u32) -> PyResultValue {
    let edid = run(target, QueryEdid { display_id })
        .map_err(to_py_err)?
        .output;
    Ok(value_object([
        (
            "display_id",
            Value::String(format!("0x{:08X}", edid.display_id)),
        ),
        ("bytes", u64_value(edid.bytes.len() as u64)),
        ("edid_hex", Value::String(bytes_to_upper_hex(&edid.bytes))),
    ]))
}

fn normalize_query_vfp_point(target: &GpuTarget<'_>, point: usize) -> PyResultValue {
    let voltage = run(target, QueryVfpPointVoltage { point })
        .map_err(to_py_err)?
        .output;
    Ok(value_object([("microvolts", u64_value(voltage.0 as u64))]))
}

fn normalize_tdp_temp_limits(target: &GpuTarget<'_>) -> PyResultValue {
    let limits = run(target, QueryTdpTempLimits).map_err(to_py_err)?.output;
    // Fields the driver didn't report (e.g. no thermal-policy entries on
    // V100) are omitted entirely — None maps to Value::Null, which
    // value_object drops — rather than fabricated with placeholder values.
    Ok(value_object([
        (
            "min_tdp",
            limits.min_tdp.map(percent_value).unwrap_or(Value::Null),
        ),
        (
            "default_tdp",
            limits.default_tdp.map(percent_value).unwrap_or(Value::Null),
        ),
        (
            "max_tdp",
            limits.max_tdp.map(percent_value).unwrap_or(Value::Null),
        ),
        (
            "min_temp",
            limits
                .min_temp
                .map(|v| u64_value(v.0 as u64))
                .unwrap_or(Value::Null),
        ),
        (
            "default_temp",
            limits
                .default_temp
                .map(|v| u64_value(v.0 as u64))
                .unwrap_or(Value::Null),
        ),
        (
            "max_temp",
            limits
                .max_temp
                .map(|v| u64_value(v.0 as u64))
                .unwrap_or(Value::Null),
        ),
    ]))
}

fn normalize_voltage_limits(target: &GpuTarget<'_>) -> PyResultValue {
    let value = run(target, nvoc_core::ProbeVoltageLimits)
        .map_err(to_py_err)?
        .output;
    Ok(value_object([
        ("lower_point", u64_value(value.lower_point as u64)),
        ("upper_point", u64_value(value.upper_point as u64)),
    ]))
}

fn normalize_voltage_check(target: &GpuTarget<'_>, point: usize) -> PyResultValue {
    let check = run(target, CheckVoltageFrequency { point })
        .map_err(to_py_err)?
        .output;
    let voltage = run(target, QueryVfpPointVoltage { point })
        .map_err(to_py_err)?
        .output;
    Ok(value_object([
        ("precise", bool_value(check.precise)),
        (
            "matched_point",
            check
                .matched_point
                .map(|point| u64_value(point as u64))
                .unwrap_or(Value::Null),
        ),
        ("microvolts", u64_value(voltage.0 as u64)),
    ]))
}

fn normalize_query_clock_offset(
    target: &GpuTarget<'_>,
    domain: ClockDomain,
    pstate: PerformanceState,
) -> PyResultValue {
    let value = run(target, nvoc_core::QueryClockOffset { domain, pstate })
        .map_err(to_py_err)?
        .output;
    Ok(value_object([("mhz", i64_value(value.mhz as i64))]))
}

/// NVAPI clock-offset read: the live per-pstate clock-offset delta (kHz) from
/// `QueryGpuSettings.pstate_deltas` — the same surface `SetPstateClockOffset`
/// writes, and the source the CLI's `get-pstate-global-freq-offset --nvapi`
/// reads. Unlike NVML (integer MHz only), NVAPI carries sub-MHz resolution
/// (2.5 MHz steps on 10/16/20-series), so the offset is emitted as a
/// fractional MHz value. P-states the driver reports as non-editable are
/// absent from `pstate_deltas`; they read as 0 (no offset), matching the CLI.
fn normalize_query_clock_offset_nvapi(
    target: &GpuTarget<'_>,
    domain: ClockDomain,
    pstate: PState,
) -> PyResultValue {
    let settings = run(target, QueryGpuSettings).map_err(to_py_err)?.output;
    let offset_khz = settings
        .pstate_deltas
        .get(&pstate)
        .and_then(|domains| domains.get(&domain))
        .map(|delta| delta.0)
        .unwrap_or(0);
    Ok(value_object([(
        "mhz",
        f64_value(offset_khz as f64 / 1000.0),
    )]))
}

/// The backend a clock-offset entry resolves to.
///
/// Deliberately not [`parse_backend`]: that one also admits the cooler backends
/// (which carry no clock offsets) and does not know `macos-rm` at all, which is
/// why the offset read used to reject a backend name every other family accepts.
/// Absent or `auto`/`default` means the platform default, the same rule the fan,
/// power and temperature reads use: the RM snapshot on macOS, NVML elsewhere
/// (which is what this entry has defaulted to on every other host).
fn clock_offset_backend(raw: Option<&str>) -> PyResult<&'static str> {
    let Some(raw) = raw else {
        return Ok(fan_power_backends(cfg!(target_os = "macos")));
    };
    match raw.trim().to_ascii_lowercase().as_str() {
        "auto" | "default" => Ok(fan_power_backends(cfg!(target_os = "macos"))),
        "macos-rm" | "macos_rm" | "macosrm" => Ok("macos-rm"),
        "nvml" => Ok("nvml"),
        "nvapi" => Ok("nvapi"),
        other => Err(invalid_value(format!(
            "invalid backend {other:?}; expected 'nvapi', 'nvml', 'auto'/'default', or 'macos-rm'"
        ))),
    }
}

/// The gate's message as a plain string, so the decision is testable without a
/// Python interpreter: this crate's tests run with `--no-default-features`
/// (no `extension-module`, hence no interpreter), where even formatting a
/// [`PyErr`] would panic.
fn macos_write_gate_reason(entry: &str, backend: &str) -> Option<String> {
    (backend == "macos-rm").then(|| format!("{entry}: {}", nvoc_core::MACOS_RM_WRITE_GATE_REASON))
}

/// The same gate for the entries that take no backend argument: on macOS every
/// backend resolves to the native RM one, so the gate is what is missing there,
/// and elsewhere these entries keep the sentence their backend has always given
/// (`None`).
fn macos_write_gate_here_reason(entry: &str) -> Option<String> {
    if cfg!(target_os = "macos") {
        macos_write_gate_reason(entry, "macos-rm")
    } else {
        None
    }
}

/// [`macos_write_gate_here_reason`] as a refusal.
fn macos_write_gate_here(entry: &str) -> PyResult<()> {
    match macos_write_gate_here_reason(entry) {
        Some(reason) => Err(PyRuntimeError::new_err(reason)),
        None => Ok(()),
    }
}

fn macos_clock_target_khz(mhz: f64) -> Result<i32, String> {
    let khz = (mhz * 1000.0).round();
    if !mhz.is_finite() || !(-18_750.0..=18_750.0).contains(&khz) {
        return Err(
            "macOS additive clock target must be finite and within -18.75..18.75 MHz".into(),
        );
    }
    Ok(khz as i32)
}

/// The macOS RM *read* faces that have no source in this backend: name the layer
/// the same way the CLI does, from the same core constant.
///
/// The platform sentence these entries used to raise ("NVAPI/NVML are not
/// available on macOS; ... is read-only and does not implement performance
/// writes") is true of the host and wrong about the entry: it ends by
/// explaining a *write* gate, and it hides that the same family's read answers
/// here. The layer text is the constant the CLI's own arm prints, so the two
/// surfaces cannot describe the missing layer differently.
///
/// The decision is a plain function of `(entry, reason, platform)`, so it is
/// testable without a Python interpreter: this crate's tests run with
/// `--no-default-features`, where formatting a `PyErr` would panic.
fn macos_read_layer_reason(entry: &str, reason: &str) -> Option<String> {
    cfg!(target_os = "macos").then(|| format!("{entry}: {reason}"))
}

/// [`macos_read_layer_reason`] as a refusal.
fn macos_read_layer(entry: &str, reason: &str) -> PyResult<()> {
    match macos_read_layer_reason(entry, reason) {
        Some(text) => Err(PyRuntimeError::new_err(text)),
        None => Ok(()),
    }
}

/// The macOS RM offset read: the snapshot's own per-domain record, projected into
/// the same `mhz` field the NVAPI and NVML branches return, so a caller reads one
/// field whatever the backend.
///
/// The name is resolved by core, which attaches one only where the same snapshot
/// corroborated it against a live clock; a domain it cannot name is therefore
/// refused with that reason instead of being reported as an offset of zero.
fn normalize_query_clock_offset_macos(
    settings: &nvoc_core::DeviceSettings,
    domain: ClockDomain,
) -> PyResultValue {
    let mhz = clock_offset_macos_rm_mhz(settings, domain).map_err(to_py_err)?;
    Ok(value_object([("mhz", f64_value(mhz))]))
}

/// The number behind [`normalize_query_clock_offset_macos`], or the reason there
/// is none -- plain, so the projection can be tested without an interpreter.
fn clock_offset_macos_rm_mhz(
    settings: &nvoc_core::DeviceSettings,
    domain: ClockDomain,
) -> Result<f64, String> {
    let name = nvoc_core::public_offset_domain(domain).ok_or_else(|| {
        format!(
            "this clock domain has no public offset name on macOS; the names the snapshot can \
             corroborate are {}",
            nvoc_core::PUBLIC_OFFSET_DOMAINS.join(", ")
        )
    })?;
    let view = settings.clock_offset_view(name)?;
    Ok(f64::from(view.offset_khz) / 1000.0)
}

fn normalize_domain_vfp_points(
    target: &GpuTarget<'_>,
    domain: ClockDomain,
    infer_missing_default: bool,
) -> PyResultValue {
    let points = run(
        target,
        QueryDomainVfpPoints {
            domain,
            infer_missing_default,
            indexed: true,
        },
    )
    .map_err(to_py_err)?
    .output
    .into_iter()
    .map(|(index, point)| {
        // point_type exposes the per-point editability class so the GUI can
        // detect Fixed/Dyn points (traditional public VFP OC cannot move
        // them) and fall back to the private raw/raw-converted path.
        let ptype = match point.point_type {
            VfPointType::Prog => "prog",
            VfPointType::Fixed => "fixed",
            VfPointType::Dyn => "dyn",
            _ => "unknown",
        };
        value_object([
            ("index", u64_value(index as u64)),
            ("voltage_uv", u64_value(point.voltage.0 as u64)),
            ("frequency_khz", u64_value(point.frequency.0 as u64)),
            ("delta_khz", i64_value(point.delta.0 as i64)),
            (
                "default_frequency_khz",
                u64_value(point.default_frequency.0 as u64),
            ),
            ("point_type", Value::from(ptype)),
        ])
    })
    .collect();
    Ok(Value::Array(points))
}

#[pyfunction]
fn discover_gpus(py: Python<'_>, backends: Option<&str>) -> PyResult<Py<PyAny>> {
    // Default to the platform default ("auto"): RM on macOS, NVAPI+NVML
    // elsewhere. A literal "both" would fail on macOS, where neither NVAPI nor
    // NVML exists; on other platforms "auto" resolves to the same Both set.
    let backends_str = backends.unwrap_or("auto").to_string();
    // Release the GIL: discovery loads nvapi64 + inits NVAPI (hundreds of
    // ms) and the GUI calls it at tick zero to overlap UI construction —
    // holding the GIL would stall the main thread's Python widget build.
    let items = py.detach(|| -> PyResult<Vec<Value>> {
        let backends = parse_backends(&backends_str)?;
        let inventory = {
            let mut cache = lock_inventory_cache();
            cache.refresh(backends)?
        };
        let mut items = Vec::new();
        for target in inventory.0.targets() {
            let mut item = Map::new();
            item.insert("index".into(), u64_value(target.index as u64));
            item.insert("gpu_id".into(), u64_value(target.id.0 as u64));
            item.insert("gpu_id_hex".into(), text(format!("0x{:04X}", target.id.0)));
            item.insert("backend_nvapi".into(), bool_value(target.has_nvapi()));
            item.insert("backend_nvml".into(), bool_value(target.has_nvml()));
            item.insert("backend_macos_rm".into(), bool_value(target.has_macos_rm()));
            if target.has_macos_rm() {
                // No NVAPI handle on macOS: name/ids come from the RM device
                // info, and the NVAPI-only capability flags stay absent so the
                // GUI falls back to its name heuristics.
                if let Ok(device) = target.macos_rm() {
                    let info = device.info();
                    item.insert(
                        "name".into(),
                        text(
                            info.name
                                .clone()
                                .unwrap_or_else(|| "NVIDIA GPU".to_string()),
                        ),
                    );
                    item.insert("driver_version".into(), text(&info.driver_version));
                    item.insert("pci_address".into(), text(&info.pci_address));
                    if let Some(value) = &info.chip_sku {
                        item.insert("chip_sku".into(), text(value));
                    }
                    if let Some(value) = &info.project {
                        item.insert("project".into(), text(value));
                    }
                }
            } else if let Ok(info) = run(&target, QueryGpuInfo).map(|report| report.output) {
                // Capability flags from gpu_type.rs detect_gpu_type (name +
                // codename) right at probe time — the GUI builds the correct
                // mobile/desktop layout on FIRST paint instead of drawing the
                // desktop modal and re-packing when the info query lands.
                let series = detect_gpu_type(&info.name, &info.codename);
                item.insert("name".into(), text(info.name));
                item.insert("codename".into(), text(info.codename));
                item.insert("arch".into(), text(info.arch));
                item.insert("is_mobile".into(), bool_value(series.is_mobile()));
                item.insert("is_server".into(), bool_value(series.is_server()));
                item.insert(
                    "is_legacy_voltage".into(),
                    bool_value(series.is_legacy_voltage()),
                );
                item.insert(
                    "xbar_supported".into(),
                    bool_value(series.supports_xbar_offset()),
                );
            }
            items.push(Value::Object(item));
        }
        Ok(items)
    })?;
    py_value(py, &Value::Array(items))
}

#[pyfunction]
fn query_info(py: Python<'_>, gpu: &str, backends: Option<&str>) -> PyResult<Py<PyAny>> {
    let gpu = gpu.to_string();
    let backends = backends.unwrap_or("both").to_string();
    let value = py.detach(|| with_target(&gpu, &backends, normalize_info))?;
    py_value(py, &value)
}

#[pyfunction]
fn query_status(py: Python<'_>, gpu: &str, backends: Option<&str>) -> PyResult<Py<PyAny>> {
    // GIL released: the 1 Hz dashboard poll runs this sweep while the GUI
    // main thread handles events — holding the GIL stalls drag/click streams.
    let gpu = gpu.to_string();
    let backends = backends.unwrap_or("both").to_string();
    let value = py.detach(|| with_target(&gpu, &backends, normalize_status))?;
    py_value(py, &value)
}

#[pyfunction]
fn query_settings(py: Python<'_>, gpu: &str, backends: Option<&str>) -> PyResult<Py<PyAny>> {
    let gpu = gpu.to_string();
    let backends = backends.unwrap_or("both").to_string();
    let value = py.detach(|| with_target(&gpu, &backends, normalize_settings))?;
    py_value(py, &value)
}

#[pyfunction]
fn query_supported_applications_clocks(
    py: Python<'_>,
    gpu: &str,
    backends: Option<&str>,
) -> PyResult<Py<PyAny>> {
    let value = with_target(
        gpu,
        backends.unwrap_or("nvml"),
        normalize_supported_app_clocks,
    )?;
    py_value(py, &value)
}

#[pyfunction]
#[pyo3(signature = (gpu, backends = None))]
fn query_power_limits(py: Python<'_>, gpu: &str, backends: Option<&str>) -> PyResult<Py<PyAny>> {
    let value = with_target(
        gpu,
        backends.unwrap_or_else(|| fan_power_backends(cfg!(target_os = "macos"))),
        normalize_power_limits,
    )?;
    py_value(py, &value)
}

#[pyfunction]
fn query_pstates(py: Python<'_>, gpu: &str) -> PyResult<Py<PyAny>> {
    refuse_without_source(
        "query_pstates",
        nvoc_core::PSTATE_FREQ_TABLE_UNREADABLE_REASON,
    )?;
    let value = with_target(gpu, "nvml", normalize_pstates)?;
    py_value(py, &value)
}

#[pyfunction]
#[pyo3(signature = (gpu, backends = None))]
fn query_fan_info(py: Python<'_>, gpu: &str, backends: Option<&str>) -> PyResult<Py<PyAny>> {
    let value = with_target(
        gpu,
        backends.unwrap_or_else(|| fan_power_backends(cfg!(target_os = "macos"))),
        normalize_fan_info,
    )?;
    py_value(py, &value)
}

#[pyfunction]
fn query_cooler_info(py: Python<'_>, gpu: &str) -> PyResult<Py<PyAny>> {
    refuse_without_source(
        "query_cooler_info",
        nvoc_core::FAN_PERCENT_UNREADABLE_REASON,
    )?;
    let value = with_target(gpu, "nvapi", normalize_cooler_info)?;
    py_value(py, &value)
}

#[pyfunction]
fn query_temperature_thresholds(py: Python<'_>, gpu: &str) -> PyResult<Py<PyAny>> {
    refuse_without_source(
        "query_temperature_thresholds",
        nvoc_core::THERMAL_THRESHOLDS_UNREADABLE_REASON,
    )?;
    let value = with_target(gpu, "nvml", normalize_temperature_thresholds)?;
    py_value(py, &value)
}

#[pyfunction]
#[pyo3(signature = (gpu, backends = None))]
fn query_legacy_temp_sensor(
    py: Python<'_>,
    gpu: &str,
    backends: Option<&str>,
) -> PyResult<Py<PyAny>> {
    let value = with_target(
        gpu,
        backends.unwrap_or_else(|| fan_power_backends(cfg!(target_os = "macos"))),
        normalize_legacy_temp_sensor,
    )?;
    py_value(py, &value)
}

#[pyfunction]
#[pyo3(signature = (gpu, backends = None))]
fn query_throttle_reasons(
    py: Python<'_>,
    gpu: &str,
    backends: Option<&str>,
) -> PyResult<Py<PyAny>> {
    // `both` matches the query_info/status/settings family: on macOS pass
    // `"auto"`/`"macos-rm"` to get the same nine RM-backed reasons the CLI
    // reports; the NVML path is unchanged elsewhere.
    let value = with_target(gpu, backends.unwrap_or("both"), normalize_throttle_reasons)?;
    py_value(py, &value)
}

#[pyfunction]
#[pyo3(signature = (gpu, pstate = None))]
fn query_legacy_gpc_rail_volt_range(
    py: Python<'_>,
    gpu: &str,
    pstate: Option<&str>,
) -> PyResult<Py<PyAny>> {
    let pstate_filter = match pstate {
        Some(s) => Some(parse_pstate(s)?),
        None => None,
    };
    let value = with_target(gpu, "nvapi", |target| {
        normalize_legacy_overvolt_ranges(target, pstate_filter)
    })?;
    py_value(py, &value)
}

#[pyfunction]
#[pyo3(signature = (gpu, pstate = None))]
fn query_pstate_base_voltage(
    py: Python<'_>,
    gpu: &str,
    pstate: Option<&str>,
) -> PyResult<Py<PyAny>> {
    let pstate = parse_pstate(pstate.unwrap_or("P0"))?;
    let value = with_target(gpu, "nvapi", |target| {
        normalize_pstate_base_voltage(target, pstate)
    })?;
    py_value(py, &value)
}

#[pyfunction]
fn query_voltage_boost(py: Python<'_>, gpu: &str) -> PyResult<Py<PyAny>> {
    let value = with_target(gpu, "nvapi", normalize_voltage_boost)?;
    py_value(py, &value)
}

#[pyfunction]
fn query_auto_boost(py: Python<'_>, gpu: &str) -> PyResult<Py<PyAny>> {
    let value = with_target(gpu, "nvml", normalize_auto_boost)?;
    py_value(py, &value)
}

#[pyfunction]
fn query_api_restriction(py: Python<'_>, gpu: &str, api_type: &str) -> PyResult<Py<PyAny>> {
    let api_type = parse_api_restriction_api(api_type)?;
    let value = with_target(gpu, "nvml", |target| {
        normalize_api_restriction(target, api_type)
    })?;
    py_value(py, &value)
}

#[pyfunction]
#[pyo3(signature = (gpu, all = false))]
fn get_display_list(py: Python<'_>, gpu: &str, all: bool) -> PyResult<Py<PyAny>> {
    let value = with_target(gpu, "nvapi", |target| normalize_displays(target, all))?;
    py_value(py, &value)
}

#[pyfunction]
fn query_edid(py: Python<'_>, gpu: &str, display_id: &str) -> PyResult<Py<PyAny>> {
    let display_id = parse_display_id(display_id)?;
    let value = with_target(gpu, "nvapi", |target| normalize_edid(target, display_id))?;
    py_value(py, &value)
}

#[pyfunction]
fn query_clock_offset(
    py: Python<'_>,
    gpu: &str,
    backends: Option<&str>,
    domain: &str,
    pstate: Option<&str>,
) -> PyResult<Py<PyAny>> {
    let backend = clock_offset_backend(backends)?;
    let domain = parse_domain(domain)?;
    // Dispatch per backend like set_clock_offset: NVML reads NVML's clock-offset
    // API (integer MHz, PerformanceState); NVAPI reads the live pstate delta
    // (kHz, sub-MHz, PState) from QueryGpuSettings — the field
    // SetPstateClockOffset writes; the macOS RM backend reads the offset the
    // snapshot already carries for that domain. The NVAPI/NVML pair use different
    // pstate enums, so the pstate string is parsed inside each branch; the macOS
    // branch takes none, because those records are per domain and not per P-state
    // (the P-state it is in is reported by query_status).
    let value = with_target(gpu, backend, |target| match backend {
        "macos-rm" => {
            let device = target.macos_rm().map_err(to_py_err)?;
            let settings = device.settings().map_err(to_py_err)?;
            normalize_query_clock_offset_macos(&settings, domain)
        }
        "nvml" => {
            let pstate = parse_nvml_pstate(pstate.unwrap_or("P0"))?;
            normalize_query_clock_offset(target, domain, pstate)
        }
        _ => {
            let pstate = parse_pstate(pstate.unwrap_or("P0"))?;
            normalize_query_clock_offset_nvapi(target, domain, pstate)
        }
    })?;
    py_value(py, &value)
}

#[pyfunction]
#[pyo3(signature = (gpu, target, delta, backends = None))]
/// Return the shared plan document for one offset request.
///
/// The document is the one the CLI (`--plan-delta`), the macOS stock evaluation
/// and this function all publish: `applied`, `settings_write`, `requests`, the
/// `note` saying what `ready` does and does not mean, and one step per request
/// under `steps`. Nothing is written; `settings_write` reports whether this
/// backend would write at all.
fn plan_clock_offset_step(
    py: Python<'_>,
    gpu: &str,
    target: &str,
    delta: &str,
    backends: Option<&str>,
) -> PyResult<Py<PyAny>> {
    let gpu = gpu.to_string();
    // The request grammar is the core one, so this call accepts exactly the
    // spellings `nvoc-cli ... --plan-delta` and `nvoc-auto-optimizer macos-eval --plan` accept.
    let requested = nvoc_core::parse_offset_target(target).map_err(invalid_value)?;
    let delta_khz = nvoc_core::parse_offset_delta(delta).map_err(invalid_value)?;
    let backends = backends.unwrap_or("macos-rm").to_string();
    let value = py.detach(|| {
        with_target(&gpu, &backends, |target_gpu| {
            if !target_gpu.has_macos_rm() {
                return Err(invalid_value(
                    "the offset plan is measured against the window the macOS RM backend \
                     reports; this target is not a macOS RM device",
                ));
            }
            let device = target_gpu.macos_rm().map_err(to_py_err)?;
            // The production settings path, so the snapshot this plans against
            // is the same one `query_settings` reports -- names included.
            let settings = device.settings().map_err(to_py_err)?;
            let request = nvoc_core::PlanRequest {
                target: requested,
                delta_khz,
            };
            serde_json::to_value(nvoc_core::plan_document(&settings, &[request])).map_err(to_py_err)
        })
    })?;
    py_value(py, &value)
}

#[pyfunction]
#[pyo3(signature = (gpu, domain = None, infer_missing_default = true))]
fn query_public_vftable(
    py: Python<'_>,
    gpu: &str,
    domain: Option<&str>,
    infer_missing_default: bool,
) -> PyResult<Py<PyAny>> {
    let domain = parse_domain(domain.unwrap_or("graphics"))?;
    let gpu = gpu.to_string();
    // GIL released: the VFP table RM escape is the heaviest read (curve
    // refresh / auto-refresh) and must not stall the GUI event stream.
    let value = py.detach(|| {
        with_target(&gpu, "nvapi", |target| {
            normalize_domain_vfp_points(target, domain, infer_missing_default)
        })
    })?;
    py_value(py, &value)
}

/// vBIOS GPU Boost 2.0 阶梯（Maxwell/Kepler 的 "隐藏" V/F 曲线）。
///
/// 读整片 VBIOS 镜像后用 core 的 legacy_vbios_parser 解码 boost-ladder
/// v0x10 表（BIT 'P'+0x34）：79×5B 点（u16 半 MHz + vmap 电压索引）+ pstate
/// 边界标记。`curve_start_index`/`curve_end_index` 从标记推导出实际曲线窗口
/// （最低速 pstate 边界 .. P0 边界，GM200 = 5..74；两端之外的点是低功耗态
/// 和填充点，绘图时应排除）。电压取 vmap 节点 (min,max) µV → mV。只读。
#[pyfunction]
#[pyo3(signature = (gpu))]
fn query_vbios_vf_curve(py: Python<'_>, gpu: &str) -> PyResult<Py<PyAny>> {
    let gpu = gpu.to_string();
    // GIL released: the image read is a multi-escape loop (2 KB chunks).
    let value = py.detach(|| {
        with_target(&gpu, "nvapi", |target| {
            let image = run(target, QueryVbiosImage).map_err(to_py_err)?.output;
            bios_vf_curve_value(&image).map_err(to_py_err)
        })
    })?;
    py_value(py, &value)
}

/// 阶梯表 → 绘图就绪 payload（GUI/TUI 共用）。表缺失（非 Maxwell/Kepler
/// 世代镜像）时返回 `{"available": false}` 而非报错，便于上层静默隐藏。
fn bios_vf_curve_value(image: &[u8]) -> Result<Value, nvoc_core::Error> {
    let vb = nvoc_core::legacy_vbios_parser::parse(image)?;
    let Some(ladder) = &vb.boost_ladder else {
        return Ok(Value::Object(
            [("available".to_string(), Value::Bool(false))]
                .into_iter()
                .collect(),
        ));
    };
    let points: Vec<Value> = ladder
        .entries
        .iter()
        .enumerate()
        .map(|(i, e)| {
            let (vmin, vmax) = vb.ladder_voltage_uv(e.vmap_index).unwrap_or((0, 0));
            Value::Object(
                [
                    ("index".to_string(), u64_value(i as u64)),
                    (
                        "freq_mhz".to_string(),
                        f64_value(f64::from(e.freq_mhz_x2) / 2.0),
                    ),
                    ("v_min_mv".to_string(), f64_value(f64::from(vmin) / 1000.0)),
                    ("v_max_mv".to_string(), f64_value(f64::from(vmax) / 1000.0)),
                    ("vmap_index".to_string(), u64_value(u64::from(e.vmap_index))),
                ]
                .into_iter()
                .collect(),
            )
        })
        .collect();
    // 曲线窗口：最低速 pstate（P 编号最大，如 P8=raw7）边界 .. P0 边界。
    let mark_index = |raw: u8| {
        ladder
            .marks
            .iter()
            .find(|m| m.pstate_raw == raw)
            .map(|m| u64::from(m.ladder_index))
    };
    let last = ladder.entries.len().saturating_sub(1) as u64;
    // 绘图窗口从 0 起（低功耗点 0..P8 边界也画进双线）；P8/P0 边界仍由
    // pstate_marks 携带，终端展示用。
    let start = 0u64;
    // P0 边界 mark 缺位时（Kepler Quadro 的 4B mark 不携带 P0 边界，
    // 如 K4000），窗口钳到最后一个非零频点——0 MHz 填充点（K4000
    // 63 点中 47 个）不进绘图窗口，否则图表被拖到 0 轴。
    let last_nonzero = ladder
        .entries
        .iter()
        .rposition(|e| e.freq_mhz_x2 != 0)
        .map_or(last, |i| i as u64);
    let end = mark_index(15).unwrap_or(last).min(last).min(last_nonzero);
    let marks: Vec<Value> = ladder
        .marks
        .iter()
        .map(|m| {
            Value::Object(
                [
                    (
                        "pstate".to_string(),
                        nvoc_core::legacy_vbios_parser::pstate_display_name(m.pstate_raw)
                            .map(Value::String)
                            .unwrap_or(Value::Null),
                    ),
                    ("code_raw".to_string(), text(format!("{:#06x}", m.code_raw))),
                    ("index".to_string(), u64_value(u64::from(m.ladder_index))),
                ]
                .into_iter()
                .collect(),
            )
        })
        .collect();
    Ok(Value::Object(
        [
            ("available".to_string(), Value::Bool(true)),
            (
                "table_version".to_string(),
                text(format!("{:#04x}", ladder.ver)),
            ),
            ("curve_start_index".to_string(), u64_value(start)),
            ("curve_end_index".to_string(), u64_value(end)),
            ("points".to_string(), Value::Array(points)),
            ("pstate_marks".to_string(), Value::Array(marks)),
            (
                "warnings".to_string(),
                Value::Array(vb.warnings.iter().map(text).collect()),
            ),
        ]
        .into_iter()
        .collect(),
    ))
}

/// 原生 GC6 唤醒（force_gc6_exit）。移动端 dGPU 空闲掉电（GCOFF）后，
/// NVAPI 读操作会失败并被上层误判为"不支持"；长驻 GUI/TUI 在读取
/// 能力/限制类数据前调用本函数把 GPU 拉回 D0。唤醒非持久。
/// 桌面 GPU（无 GC6）返回 Ok(false)，唤醒成功返回 Ok(true)。
#[pyfunction]
fn force_wake(gpu: &str) -> PyResult<bool> {
    let backends = parse_backends("nvapi")?;
    let inventory = {
        let mut cache = lock_inventory_cache();
        cache.entry(backends)?
    };
    let target = selected_target(&inventory.0, gpu)?;
    // best-effort：桌面端（无 GC6）驱动返回 NoImplementation(-104) 等
    // 错误，同样按"无需唤醒"返回 false，绝不向 Python 抛异常。
    Ok(target.force_wake().is_ok())
}

#[pyfunction]
fn query_vfp_point_voltage(py: Python<'_>, gpu: &str, point: usize) -> PyResult<Py<PyAny>> {
    let value = with_target(gpu, "nvapi", |target| {
        normalize_query_vfp_point(target, point)
    })?;
    py_value(py, &value)
}

#[pyfunction]
fn query_tdp_temp_limits(py: Python<'_>, gpu: &str) -> PyResult<Py<PyAny>> {
    macos_read_layer(
        "query_tdp_temp_limits",
        nvoc_core::THERMAL_LIMIT_UNREADABLE_REASON,
    )?;
    let value = with_target(gpu, "nvapi", normalize_tdp_temp_limits)?;
    py_value(py, &value)
}

#[pyfunction]
fn probe_voltage_limits(py: Python<'_>, gpu: &str) -> PyResult<Py<PyAny>> {
    let value = with_target(gpu, "nvapi", normalize_voltage_limits)?;
    py_value(py, &value)
}

#[pyfunction]
fn check_voltage_frequency(py: Python<'_>, gpu: &str, point: usize) -> PyResult<Py<PyAny>> {
    let value = with_target(gpu, "nvapi", |target| {
        normalize_voltage_check(target, point)
    })?;
    py_value(py, &value)
}

#[pyfunction]
fn set_clock_offset(
    py: Python<'_>,
    gpu: &str,
    backend: &str,
    domain: &str,
    value: f64,
    pstate: Option<&str>,
) -> PyResult<()> {
    let gpu_own = gpu.to_string();
    let backend_own = backend.to_string();
    let domain_own = domain.to_string();
    let pstate_own = pstate.map(|s| s.to_string());
    py.detach(|| -> PyResult<()> {
        let gpu: &str = &gpu_own;
        let backend: &str = &backend_own;
        let domain: &str = &domain_own;
        let pstate: Option<&str> = pstate_own.as_deref();

        // One decimal MHz is allowed — the 2.5 MHz GUI grid divides both the
        // 7.5 MHz (30-series+) and 12.5 MHz (10/16/20-series) hardware steps.
        // NVAPI takes kHz (round to nearest); NVML's API is integer MHz only.
        if !value.is_finite() {
            return Err(PyRuntimeError::new_err(format!(
                "clock offset {value} MHz is not a finite number"
            )));
        }
        let backend = clock_offset_backend(Some(backend))?;
        let domain = parse_domain(domain)?;
        if backend == "macos-rm" {
            // Preserve the public MHz target contract, while rejecting per-Pstate
            // semantics the RM domain control does not implement.
            let khz = macos_clock_target_khz(value).map_err(invalid_value)?;
            nvoc_core::validate_clock_offset_request(domain, khz, pstate.is_some())
                .map_err(to_py_err)?;
            with_target(gpu, backend, |target| {
                run(
                    target,
                    nvoc_core::SetMacosRmClockOffset {
                        domain,
                        khz,
                        cancelled: &|| Python::attach(|py| py.check_signals().is_err()),
                    },
                )
                .map_err(to_py_err)?;
                Ok(Value::Null)
            })?;
            return Ok(());
        }
        let inventory = {
            let mut inventory_cache = lock_inventory_cache();
            inventory_cache.entry(if backend == "nvml" {
                BackendSet::Nvml
            } else {
                BackendSet::Nvapi
            })?
        };
        let target = selected_target(&inventory.0, gpu)?;
        match backend {
            "nvml" => {
                let pstate = parse_nvml_pstate(pstate.unwrap_or("P0"))?;
                run(
                    &target,
                    SetClockOffset {
                        domain,
                        pstate,
                        mhz: value.round() as i32,
                    },
                )
                .map_err(to_py_err)?;
            }
            "nvapi" => {
                let pstate = parse_pstate(pstate.unwrap_or("P0"))?;
                let delta_khz = (value * 1000.0).round() as i32;
                run(
                    &target,
                    SetPstateClockOffset {
                        pstate,
                        domain,
                        delta: KilohertzDelta(delta_khz),
                    },
                )
                .map_err(to_py_err)?;
            }
            _ => {
                return Err(invalid_value(
                    "clock offsets require backend 'nvapi' or 'nvml'; on macOS the RM backend \
                     answers the read and the write stays behind the gate",
                ));
            }
        }
        Ok(())
    })
}

#[pyfunction]
fn set_power_limit(py: Python<'_>, gpu: &str, backend: &str, value: u32) -> PyResult<()> {
    let gpu_own = gpu.to_string();
    let backend_own = backend.to_string();
    py.detach(|| -> PyResult<()> {
        let gpu: &str = &gpu_own;
        let backend: &str = &backend_own;

        let backend = match backend.trim().to_ascii_lowercase().as_str() {
            "macos-rm" | "macos_rm" | "macosrm" => "macos-rm",
            "auto" | "default" => fan_power_backends(cfg!(target_os = "macos")),
            _ => parse_backend(backend)?,
        };
        if backend == "macos-rm" {
            let mw = nvoc_core::validate_macos_power_limit_watts(value).map_err(to_py_err)?;
            with_target(gpu, backend, |target| {
                run(
                    target,
                    nvoc_core::SetMacosRmPowerLimit {
                        target: nvoc_core::PowerLimitTarget::Milliwatts(mw),
                        cancelled: &|| Python::attach(|py| py.check_signals().is_err()),
                    },
                )
                .map_err(to_py_err)?;
                Ok(Value::Null)
            })?;
            return Ok(());
        }
        let inventory = {
            let mut inventory_cache = lock_inventory_cache();
            inventory_cache.entry(if backend == "nvml" {
                BackendSet::Nvml
            } else {
                BackendSet::Nvapi
            })?
        };
        let target = selected_target(&inventory.0, gpu)?;
        match backend {
            "nvml" => {
                run(&target, SetPowerLimit { watts: value }).map_err(to_py_err)?;
            }
            "nvapi" => {
                run(
                    &target,
                    SetNvapiPowerLimits {
                        limits: vec![Percentage(value)],
                    },
                )
                .map_err(to_py_err)?;
            }
            _ => {
                return Err(invalid_value(
                    "power limits require backend 'nvapi' or 'nvml'",
                ));
            }
        }
        Ok(())
    })
}

#[pyfunction]
fn set_thermal_limit(py: Python<'_>, gpu: &str, celsius: i32) -> PyResult<()> {
    let gpu_own = gpu.to_string();
    py.detach(|| -> PyResult<()> {
        let gpu: &str = &gpu_own;

        let inventory = {
            let mut inventory_cache = lock_inventory_cache();
            inventory_cache.entry(BackendSet::Both)?
        };
        let target = selected_target(&inventory.0, gpu)?;
        if target.has_nvapi() {
            run(
                &target,
                SetNvapiSensorLimits {
                    limits: vec![Celsius(celsius).into()],
                },
            )
            .map_err(to_py_err)?;
        } else {
            run(&target, SetTemperatureLimit { celsius }).map_err(to_py_err)?;
        }
        Ok(())
    })
}

#[pyfunction]
fn set_ppab_status(gpu: &str, active: bool) -> PyResult<()> {
    let inventory = {
        let mut inventory_cache = lock_inventory_cache();
        inventory_cache.entry(BackendSet::Nvapi)?
    };
    let target = selected_target(&inventory.0, gpu)?;
    run(&target, SetNvapiDynamicBoost { active }).map_err(to_py_err)?;
    Ok(())
}

#[pyfunction]
fn query_tgp_watt_range(py: Python<'_>, gpu: &str) -> PyResult<Py<PyAny>> {
    let value = with_target(gpu, "nvapi", |target| {
        let info = run(target, QueryNvapiTgpWattRange)
            .map_err(to_py_err)?
            .output;
        Ok(match info {
            None => Value::Null,
            Some(r) => value_object([
                ("policy_index", Value::from(r.policy_index)),
                (
                    "min_watt",
                    r.min_watt.map(Value::from).unwrap_or(Value::Null),
                ),
                (
                    "default_watt",
                    r.default_watt.map(Value::from).unwrap_or(Value::Null),
                ),
                (
                    "max_watt",
                    r.max_watt.map(Value::from).unwrap_or(Value::Null),
                ),
            ]),
        })
    })?;
    py_value(py, &value)
}

#[pyfunction]
#[pyo3(signature = (gpu, watts, policy_index = None))]
fn set_tgp_watt(
    py: Python<'_>,
    gpu: &str,
    watts: u32,
    policy_index: Option<usize>,
) -> PyResult<()> {
    let gpu_own = gpu.to_string();
    py.detach(|| -> PyResult<()> {
        let gpu: &str = &gpu_own;

        let inventory = {
            let mut inventory_cache = lock_inventory_cache();
            inventory_cache.entry(BackendSet::Nvapi)?
        };
        let target = selected_target(&inventory.0, gpu)?;
        run(
            &target,
            SetNvapiTgpWatt {
                watts,
                policy_index,
            },
        )
        .map_err(to_py_err)?;
        Ok(())
    })
}

#[pyfunction]
#[pyo3(signature = (gpu, policy_index = None))]
fn reset_tgp_watt(py: Python<'_>, gpu: &str, policy_index: Option<usize>) -> PyResult<()> {
    let gpu_own = gpu.to_string();
    py.detach(|| -> PyResult<()> {
        let gpu: &str = &gpu_own;

        let inventory = {
            let mut inventory_cache = lock_inventory_cache();
            inventory_cache.entry(BackendSet::Nvapi)?
        };
        let target = selected_target(&inventory.0, gpu)?;
        run(
            &target,
            ResetNvapiTgpWatt {
                policy_index: policy_index.or(Some(2)),
            },
        )
        .map_err(to_py_err)?;
        Ok(())
    })
}

#[pyfunction]
fn query_dnotifier(py: Python<'_>, gpu: &str) -> PyResult<Py<PyAny>> {
    macos_read_layer("query_dnotifier", nvoc_core::POWER_POLICY_UNREADABLE_REASON)?;
    let value = with_target(gpu, "nvapi", |target| {
        let info = run(target, QueryNvapiDNotifier).map_err(to_py_err)?.output;
        Ok(match info {
            None => Value::Null,
            Some(r) => value_object([
                (
                    "active",
                    r.active
                        .map(|l| Value::String(format!("D{l}")))
                        .unwrap_or(Value::Null),
                ),
                (
                    "levels",
                    Value::Array(
                        r.levels
                            .iter()
                            .map(|l| {
                                value_object([
                                    ("level", Value::String(format!("D{}", l.level))),
                                    ("watts", l.watts.map(Value::from).unwrap_or(Value::Null)),
                                ])
                            })
                            .collect(),
                    ),
                ),
            ]),
        })
    })?;
    py_value(py, &value)
}

/// Effective power wall on PPAB mobiles (nvidia-smi's Ceiling trio):
/// `ceiling_watt` = min(requested TGP, active D-Notifier cap). This is the
/// "you set 100W — here is what actually applies" value the GUI/TUI power
/// slider anchors to. Returns None where the private family is unavailable.
#[pyfunction]
fn query_power_ceiling(py: Python<'_>, gpu: &str) -> PyResult<Py<PyAny>> {
    macos_read_layer(
        "query_power_ceiling",
        nvoc_core::POWER_POLICY_UNREADABLE_REASON,
    )?;
    let value = with_target(gpu, "nvapi", |target| {
        let info = run(target, QueryNvapiPowerCeiling)
            .map_err(to_py_err)?
            .output;
        Ok(match info {
            None => Value::Null,
            Some(c) => value_object([
                ("policy_index", Value::from(c.policy_index as u64)),
                (
                    "default_watt",
                    c.default_watt.map(Value::from).unwrap_or(Value::Null),
                ),
                (
                    "requested_watt",
                    c.requested_watt.map(Value::from).unwrap_or(Value::Null),
                ),
                (
                    "dnotify_watt",
                    c.dnotify_watt.map(Value::from).unwrap_or(Value::Null),
                ),
                (
                    "ceiling_watt",
                    c.ceiling_watt.map(Value::from).unwrap_or(Value::Null),
                ),
            ]),
        })
    })?;
    py_value(py, &value)
}

#[pyfunction]
fn set_dnotifier(py: Python<'_>, gpu: &str, level: u8) -> PyResult<()> {
    let gpu_own = gpu.to_string();
    py.detach(|| -> PyResult<()> {
        let gpu: &str = &gpu_own;

        if !(1..=5).contains(&level) {
            return Err(invalid_value("D-Notifier level must be 1..5 (D1-D5)"));
        }
        let inventory = {
            let mut inventory_cache = lock_inventory_cache();
            inventory_cache.entry(BackendSet::Nvapi)?
        };
        let target = selected_target(&inventory.0, gpu)?;
        run(&target, SetNvapiDNotifier { level }).map_err(to_py_err)?;
        Ok(())
    })
}

#[pyfunction]
#[pyo3(signature = (gpu, celsius, policy_index = None))]
fn set_target_temp(
    py: Python<'_>,
    gpu: &str,
    celsius: f32,
    policy_index: Option<usize>,
) -> PyResult<()> {
    let gpu_own = gpu.to_string();
    py.detach(|| -> PyResult<()> {
        let gpu: &str = &gpu_own;

        let inventory = {
            let mut inventory_cache = lock_inventory_cache();
            inventory_cache.entry(BackendSet::Nvapi)?
        };
        let target = selected_target(&inventory.0, gpu)?;
        run(
            &target,
            SetNvapiTargetTemp {
                celsius,
                policy_index,
            },
        )
        .map_err(to_py_err)?;
        Ok(())
    })
}

#[pyfunction]
fn query_target_temp_policies(py: Python<'_>, gpu: &str) -> PyResult<Py<PyAny>> {
    let value = with_target(gpu, "nvapi", |target| {
        let policies = run(target, QueryNvapiTargetTempPolicies).map_err(to_py_err)?;
        Ok(Value::Array(
            policies
                .output
                .into_iter()
                .map(|p| {
                    value_object([
                        ("policy_index", Value::from(p.policy_index)),
                        ("celsius", Value::from(p.celsius as f64)),
                        (
                            "min",
                            p.min.map(|v| Value::from(v as f64)).unwrap_or(Value::Null),
                        ),
                        (
                            "default",
                            p.default
                                .map(|v| Value::from(v as f64))
                                .unwrap_or(Value::Null),
                        ),
                        (
                            "max",
                            p.max.map(|v| Value::from(v as f64)).unwrap_or(Value::Null),
                        ),
                    ])
                })
                .collect(),
        ))
    })?;
    py_value(py, &value)
}

#[pyfunction]
fn query_volt_rails(py: Python<'_>, gpu: &str) -> PyResult<Py<PyAny>> {
    let value = with_target(gpu, "nvapi", |target| {
        let rails = run(target, QueryNvapiVoltRails).map_err(to_py_err)?.output;
        Ok(match rails {
            Some(r) => {
                let entries = |list: &[nvapi::VoltRailEntry]| {
                    Value::Array(
                        list.iter()
                            .map(|e| {
                                value_object([
                                    ("rail_bit", Value::from(e.rail_bit)),
                                    ("type", Value::from(e.entry_type)),
                                    (
                                        "values_uV",
                                        Value::Array(
                                            e.values.iter().map(|v| Value::from(*v)).collect(),
                                        ),
                                    ),
                                ])
                            })
                            .collect(),
                    )
                };
                value_object([
                    ("rail_mask", Value::from(format!("0x{:08X}", r.rail_mask))),
                    // Per-rail P0 bounds (multi-rail parts: GB10 / 50-series
                    // core + Xbar). Status entries are matched by rail_bit —
                    // the entry type is a per-rail protocol tag (Xbar = 3),
                    // not a layout marker. Each entry carries rail_bit plus
                    // the same seven fields as `p0`.
                    (
                        "p0_rails",
                        Value::Array({
                            let mut bits: Vec<u32> = r.status.iter().map(|e| e.rail_bit).collect();
                            bits.sort_unstable();
                            bits.dedup();
                            bits.into_iter()
                                .filter_map(|bit| {
                                    let b = r.p0_bounds_for(bit)?;
                                    let mut ceiling = b.vrm_max_wall_uV;
                                    if b.vbios_wall_uV > 0 && b.vbios_wall_uV < ceiling {
                                        ceiling = b.vbios_wall_uV;
                                    }
                                    #[allow(non_snake_case)]
                                    // uV-suffixed local matches the nvapi-rs field naming
                                    let ceiling_uV = (ceiling - b.effective_wall_uV).max(0);
                                    Some(value_object([
                                        ("rail_bit", Value::from(bit)),
                                        ("current_uV", Value::from(b.current_uV)),
                                        ("target_wall_uV", Value::from(b.target_wall_uV)),
                                        ("effective_wall_uV", Value::from(b.effective_wall_uV)),
                                        ("vbios_wall_uV", Value::from(b.vbios_wall_uV)),
                                        ("vrm_max_wall_uV", Value::from(b.vrm_max_wall_uV)),
                                        ("min_hold_uV", Value::from(b.min_hold_uV)),
                                        ("offset_ceiling_uV", Value::from(ceiling_uV)),
                                    ]))
                                })
                                .collect()
                        }),
                    ),
                    (
                        "p0",
                        match r.p0_bounds() {
                            Some(b) => {
                                let mut ceiling = b.vrm_max_wall_uV;
                                if b.vbios_wall_uV > 0 && b.vbios_wall_uV < ceiling {
                                    ceiling = b.vbios_wall_uV;
                                }
                                #[allow(non_snake_case)]
                                // uV-suffixed local matches the nvapi-rs field naming
                                let ceiling_uV = (ceiling - b.effective_wall_uV).max(0);
                                value_object([
                                    ("current_uV", Value::from(b.current_uV)),
                                    ("target_wall_uV", Value::from(b.target_wall_uV)),
                                    ("effective_wall_uV", Value::from(b.effective_wall_uV)),
                                    ("vbios_wall_uV", Value::from(b.vbios_wall_uV)),
                                    ("vrm_max_wall_uV", Value::from(b.vrm_max_wall_uV)),
                                    ("min_hold_uV", Value::from(b.min_hold_uV)),
                                    ("offset_ceiling_uV", Value::from(ceiling_uV)),
                                ])
                            }
                            None => Value::Null,
                        },
                    ),
                    (
                        "rail_descriptors",
                        Value::Array(
                            r.rail_descriptors
                                .iter()
                                .map(|d| {
                                    value_object([
                                        ("rail_bit", Value::from(d.rail_bit)),
                                        ("type", Value::from(d.entry_type())),
                                    ])
                                })
                                .collect(),
                        ),
                    ),
                    ("control", entries(&r.control)),
                    ("status", entries(&r.status)),
                ])
            }
            None => value_object([("supported", Value::from(false))]),
        })
    })?;
    py_value(py, &value)
}

/// Set a volt-rail to an ABSOLUTE target voltage (millivolts, may carry one
/// decimal — e.g. 1082.5 mV for the 2.5 mV rail step on 10/20-series). The
/// required µV offset is derived inside the operation from the live control
/// offset + status target wall — callers think in mV, not offsets. Returns
/// the derived base wall, the offset written, and the effective wall read
/// back (clamped by the driver to min(target, vbios_wall, vrm_max_wall)).
#[pyfunction]
fn set_volt_rail_target(
    py: Python<'_>,
    gpu: &str,
    rail_bit: u32,
    target_mv: f64,
    expect_type: Option<u32>,
) -> PyResult<Py<PyAny>> {
    // mV → µV. One decimal mV (0.1 mV = 100 µV) is well below any hardware
    // rail step, so round to the nearest µV — the driver clamps to its own
    // step grid anyway. Reject NaN/inf before touching the value.
    if !target_mv.is_finite() {
        return Err(PyRuntimeError::new_err(format!(
            "target {target_mv}mV is not a finite number"
        )));
    }
    #[allow(non_snake_case)] // uV-suffixed local matches the nvapi-rs naming
    let target_uV = i32::try_from((target_mv * 1000.0).round() as i64).map_err(|_| {
        PyRuntimeError::new_err(format!("target {target_mv}mV overflows the µV range"))
    })?;
    let value = with_target(gpu, "nvapi", |target| {
        let out = run(
            target,
            SetNvapiVoltRailTarget {
                rail_bit,
                target_uV,
                expected_type: expect_type,
            },
        )
        .map_err(to_py_err)?
        .output;
        Ok(match out {
            Some(a) => value_object([
                ("applied", Value::from(true)),
                ("rail_bit", Value::from(a.rail_bit)),
                ("target_uV", Value::from(a.target_uV)),
                ("base_wall_uV", Value::from(a.base_wall_uV)),
                ("offset_uV", Value::from(a.offset_uV)),
                ("previous_offset_uV", Value::from(a.previous_offset_uV)),
                ("applied_uV", Value::from(a.applied_uV)),
                ("effective_wall_uV", Value::from(a.effective_wall_uV)),
            ]),
            None => value_object([("supported", Value::from(false))]),
        })
    })?;
    py_value(py, &value)
}

/// Set the GPU frequency perf-cap (NVAPI PerfLimitsSetStatus 0x32CA4983, the
/// ref tool `-gpuclk:<MHz>` SETTER). Clamp the perf max/min frequency.
/// `max_mhz`/`min_mhz` are in MHz; pass -1 for both to reset the cap. The
/// GUI/CLI speak MHz; the underlying NVAPI struct takes kHz (×1000). Either
/// bound may be 0 to leave that side unset (ref tool sets both to the same cap).
#[pyfunction]
fn set_perf_freq_cap(py: Python<'_>, gpu: &str, max_mhz: i32, min_mhz: i32) -> PyResult<Py<PyAny>> {
    let cap = if max_mhz < 0 && min_mhz < 0 {
        NvapiPerfFreqCap::Reset
    } else {
        NvapiPerfFreqCap::Cap {
            max_khz: (max_mhz.max(0) as u32).saturating_mul(1000),
            min_khz: (min_mhz.max(0) as u32).saturating_mul(1000),
        }
    };
    let value = py.detach(|| {
        with_target(gpu, "nvapi", |target| {
            run(target, SetNvapiPerfFreqCap { cap }).map_err(to_py_err)?;
            Ok(value_object([("applied", Value::from(true))]))
        })
    })?;
    py_value(py, &value)
}

#[pyfunction]
fn set_volt_rail_offset(
    py: Python<'_>,
    gpu: &str,
    rail_bit: u32,
    offset_uv: i32,
    expect_type: Option<u32>,
) -> PyResult<Py<PyAny>> {
    let value = with_target(gpu, "nvapi", |target| {
        let out = run(
            target,
            SetNvapiVoltRailOffset {
                rail_bit,
                offset_uV: offset_uv,
                expected_type: expect_type,
            },
        )
        .map_err(to_py_err)?
        .output;
        Ok(match out {
            Some(a) => value_object([
                ("applied", Value::from(true)),
                ("rail_bit", Value::from(a.rail_bit)),
                ("previous_uV", Value::from(a.previous_uV)),
                ("applied_uV", Value::from(a.applied_uV)),
                ("effective_wall_uV", Value::from(a.effective_wall_uV)),
            ]),
            None => value_object([("supported", Value::from(false))]),
        })
    })?;
    py_value(py, &value)
}

/// Read the private ClockClient domain-control block: the controllable-domain
/// mask + per-domain offset/range records. This is the XBar physical-clock
/// family (RM 0x2080901b GET_CONTROL). Returns `{"supported": false}` when the
/// driver does not expose the private interface.
#[pyfunction]
fn query_private_freq_domain_info(py: Python<'_>, gpu: &str) -> PyResult<Py<PyAny>> {
    let value = with_target(gpu, "nvapi", |target| {
        let ctrl = run(target, QueryNvapiClkDomains).map_err(to_py_err)?.output;
        Ok(match ctrl {
            Some(c) => value_object([
                (
                    "controllable_mask",
                    Value::from(format!("0x{:08X}", c.mask)),
                ),
                (
                    "entries",
                    Value::Array(
                        c.entries
                            .iter()
                            .map(|e| {
                                value_object([
                                    ("bit", Value::from(e.bit)),
                                    ("type", Value::from(e.entry_type)),
                                    // false = the protocol doesn't marshal this
                                    // record type's value fields (e.g. 0x02) —
                                    // values_kHz below is NOT driver data.
                                    ("value_modifiable", Value::from(e.value_modifiable)),
                                    // 8 value dwords (V2 rec+268..296); slot
                                    // semantics driver-opaque, slot 0 = the
                                    // signed frequency offset per the article
                                    (
                                        "values_kHz",
                                        Value::Array(
                                            e.values_kHz.iter().map(|v| Value::from(*v)).collect(),
                                        ),
                                    ),
                                ])
                            })
                            .collect(),
                    ),
                ),
            ]),
            None => value_object([("supported", Value::from(false))]),
        })
    })?;
    py_value(py, &value)
}

/// `fabric_relations` payload fragment: the driver's master → slave (ext-slot)
/// domain relation the front-ends compensate against — see
/// `nvoc_core::FabricTree`. Derived from the **unfiltered** table, so a
/// consumer can trust the roster regardless of what it asked for;
/// `table_available: false` = no relation (refuse fabric writes). The CLI's
/// `get-private-vftable` builds the same shape; keep the two in lockstep.
fn fabric_relations_json(tree: &FabricTree) -> Value {
    let mut map = Map::new();
    map.insert("source".into(), Value::from("vftable-ext"));
    map.insert(
        "table_available".into(),
        Value::from(tree.table_available()),
    );
    map.insert(
        "roster".into(),
        Value::Array(
            tree.roster()
                .iter()
                .map(|d| Value::from(d.slug()))
                .collect(),
        ),
    );
    map.insert(
        "edges".into(),
        Value::Array(
            tree.edges()
                .iter()
                .map(|(parent, child)| {
                    let mut e = Map::new();
                    e.insert("parent".into(), Value::from(parent.slug()));
                    e.insert("parent_bit".into(), Value::from(parent.bit()));
                    e.insert("child".into(), Value::from(child.slug()));
                    e.insert("child_bit".into(), Value::from(child.bit()));
                    Value::Object(e)
                })
                .collect(),
        ),
    );
    Value::Object(map)
}

/// Read the private ClockClient V/F-points family (GetInfo 0x8895B510 →
/// GetStatus 0x7FEE9032): per-bank point masks + V/F curve records.
/// Records are voltage-indexed; units live-calibrated vs the public GPC VFP
/// curve (voltage µV, default/current MHz). Bank 0 packs multiple domains:
/// type-8 segments are V/F curves (GPC first, then the 127-point XBAR
/// candidate), type-7 segments are per-domain pstate frequency lists.
/// Returns `{"supported": false}` when the driver doesn't expose it.
///
/// Voltage fallback: some drivers leave the private voltage fields unfilled
/// (e.g. GP100/TCC 582.41 — every record reads 0 µV, live-verified). The
/// private records are INDEXED BY VOLTAGE, so the j-th point of ANY V/F
/// segment sits at the same grid level as the j-th point of every other
/// segment — including the public GPC VFP curve, which DOES carry voltage on
/// those same drivers. When a V/F segment's voltage range is degenerate
/// (max==0), we borrow the public GPC voltage grid by index-within-segment
/// (point at offset j ← public point j) so the GUI plots the curve against a
/// real voltage axis instead of collapsing every point to V=0. Cards whose
/// private voltage IS filled (Ada/R610.74) are untouched.
#[pyfunction]
fn query_private_vftable(py: Python<'_>, gpu: &str) -> PyResult<Py<PyAny>> {
    let value = with_target(gpu, "nvapi", |target| {
        let mut vfp = run(target, QueryNvapiClkVfPoints::default())
            .map_err(to_py_err)?
            .output;
        if let Some(v) = vfp.as_mut() {
            // Detect degenerate-voltage V/F segments (driver left voltage at 0).
            let degenerate: Vec<&nvapi::ClkVfSegment> = v
                .segments
                .iter()
                .filter(|s| s.kind == nvapi::ClkVfSegmentKind::VfCurve && s.voltage_uV_max == 0)
                .collect();
            if !degenerate.is_empty() {
                // Borrow the public GPC voltage grid. Best-effort: if the
                // public query fails or is too short, leave voltages at 0
                // (the pre-fallback behavior — no regression).
                if let Ok(pub_report) = run(
                    target,
                    QueryDomainVfpPoints {
                        domain: ClockDomain::Graphics,
                        infer_missing_default: true,
                        indexed: true,
                    },
                ) {
                    let pub_pts = pub_report.output;
                    // public point (index, VfPoint) → voltage at that grid level
                    let mut grid = vec![0u32; pub_pts.len()];
                    let mut max_idx = 0usize;
                    for (idx, p) in &pub_pts {
                        if *idx < grid.len() {
                            grid[*idx] = p.voltage.0;
                            max_idx = max_idx.max(*idx);
                        }
                    }
                    let grid = &grid[..=max_idx.min(grid.len() - 1)];
                    // Patch each point in a degenerate segment by offset.
                    for p in v.points.iter_mut() {
                        let in_deg = degenerate.iter().any(|s| {
                            s.bank == p.bank
                                && s.start_index as usize <= p.index as usize
                                && p.index as usize <= s.end_index as usize
                        });
                        if !in_deg {
                            continue;
                        }
                        let off = p.index as usize - {
                            // find this point's segment start
                            degenerate
                                .iter()
                                .find(|s| {
                                    s.bank == p.bank
                                        && s.start_index as usize <= p.index as usize
                                        && p.index as usize <= s.end_index as usize
                                })
                                .map(|s| s.start_index as usize)
                                .unwrap_or(0)
                        };
                        if off < grid.len() {
                            p.voltage_uV = grid[off];
                        }
                    }
                    // Refresh segment voltage ranges so the JSON mirrors the patch.
                    for s in v.segments.iter_mut() {
                        if s.kind != nvapi::ClkVfSegmentKind::VfCurve || s.voltage_uV_max != 0 {
                            continue;
                        }
                        let pts: Vec<u32> = v
                            .points
                            .iter()
                            .filter(|p| {
                                s.bank == p.bank
                                    && s.start_index as usize <= p.index as usize
                                    && p.index as usize <= s.end_index as usize
                            })
                            .map(|p| p.voltage_uV)
                            .collect();
                        if let Some((&mn, &mx)) = pts.iter().min().zip(pts.iter().max()) {
                            s.voltage_uV_min = mn;
                            s.voltage_uV_max = mx;
                        }
                    }
                }
            }
        }
        let vfp = vfp;
        Ok(match vfp {
            Some(v) => value_object([
                (
                    "fabric_relations",
                    fabric_relations_json(&FabricTree::from_table(Some(&v))),
                ),
                (
                    "masks",
                    Value::Array(
                        v.masks
                            .iter()
                            .map(|m| Value::from(format!("0x{:016X}", m)))
                            .collect(),
                    ),
                ),
                (
                    "segments",
                    Value::Array(
                        v.segments
                            .iter()
                            .map(|s| {
                                value_object([
                                    ("bank", Value::from(s.bank)),
                                    // EMPIRICAL advisory attribution
                                    ("domain", Value::from(s.domain_hint.as_str())),
                                    (
                                        "kind",
                                        Value::from(match s.kind {
                                            nvapi::ClkVfSegmentKind::VfCurve => "vf_curve",
                                            nvapi::ClkVfSegmentKind::PstateBins => "pstate_bins",
                                        }),
                                    ),
                                    ("type", Value::from(s.record_type)),
                                    ("start_index", Value::from(s.start_index)),
                                    ("end_index", Value::from(s.end_index)),
                                    ("count", Value::from(s.count)),
                                    ("voltage_uV_min", Value::from(s.voltage_uV_min)),
                                    ("voltage_uV_max", Value::from(s.voltage_uV_max)),
                                    ("freq_default_mhz_min", Value::from(s.freq_default_mhz_min)),
                                    ("freq_default_mhz_max", Value::from(s.freq_default_mhz_max)),
                                    ("freq_scale_corrected", Value::from(s.freq_scale_corrected)),
                                ])
                            })
                            .collect(),
                    ),
                ),
                (
                    "points",
                    Value::Array(
                        v.points
                            .iter()
                            .map(|p| {
                                value_object([
                                    ("bank", Value::from(p.bank)),
                                    ("index", Value::from(p.index)),
                                    ("type", Value::from(p.record_type)),
                                    // the V/F grid axis (µV): 450000 = 450 mV
                                    ("voltage_uV", Value::from(p.voltage_uV)),
                                    // current/effective voltage (µV; stock +
                                    // applied offset; modern @rec+0x68,
                                    // Ada-verified — legacy/BW not reported, 0)
                                    ("volt_current_uV", Value::from(p.volt_current_uV)),
                                    // per-point curve voltage offset (µV,
                                    // signed; Blackwell records only, 0
                                    // elsewhere — see set_clk_domain_offset's
                                    // generation-dependent slot mapping)
                                    ("volt_offset_uV", Value::from(p.volt_offset_uV)),
                                    // EXTENDED-section per-domain currents,
                                    // POSITIONAL slots 0..7 (record+
                                    // 0x74+8*(pos−1) freq MHz / +4 volt µV;
                                    // nonzero only where the record's
                                    // extension markers are set). Slot
                                    // attribution is layout-dependent —
                                    // Turing single-block: 1=XBAR 3=SYS
                                    // 5=MSD 7=unknown.
                                    (
                                        "domain_freq_mhz",
                                        Value::Array(
                                            p.domain_freqs_mhz
                                                .iter()
                                                .map(|&x| Value::from(x))
                                                .collect(),
                                        ),
                                    ),
                                    (
                                        "domain_volt_uV",
                                        Value::Array(
                                            p.domain_volts_uV
                                                .iter()
                                                .map(|&x| Value::from(x))
                                                .collect(),
                                        ),
                                    ),
                                    // default MHz at this voltage
                                    ("freq_default_mhz", Value::from(p.freq_default_mhz)),
                                    // current MHz = default + applied offset.
                                    // Some drivers don't maintain the
                                    // current field (P100/TCC 582.41: every
                                    // record reads current=0 while default
                                    // is filled — GetControl is the real
                                    // offset readback there). Fall back to
                                    // default so the plotted "current" line
                                    // doesn't collapse to a flat 0 curve.
                                    (
                                        "freq_current_mhz",
                                        Value::from(if p.freq_current_mhz == 0 {
                                            p.freq_default_mhz
                                        } else {
                                            p.freq_current_mhz
                                        }),
                                    ),
                                ])
                            })
                            .collect(),
                    ),
                ),
            ]),
            // No private table (arch without the read, or it failed): the
            // relation is UNKNOWN — the front-ends refuse fabric writes
            // rather than compensate on a guess.
            None => value_object([
                ("supported", Value::from(false)),
                (
                    "fabric_relations",
                    fabric_relations_json(&FabricTree::from_table(None)),
                ),
            ]),
        })
    })?;
    py_value(py, &value)
}

/// Measure one clock domain's physical clock via two-sample MEASURE_FREQ (RM
/// 0x20809006). `domain_bit` is the sequential domain index: GPC=0, XBAR=1,
/// SYS=2, MCLK=4. Returns the frequency in MHz, or `{"supported": false}`.
#[pyfunction]
fn query_clk_domain_freq(py: Python<'_>, gpu: &str, domain_bit: u32) -> PyResult<Py<PyAny>> {
    let value = with_target(gpu, "nvapi", |target| {
        let freq = run(target, QueryNvapiClkDomainFreq { domain_bit })
            .map_err(to_py_err)?
            .output;
        Ok(match freq {
            Some(f) => value_object([
                ("domain_bit", Value::from(domain_bit)),
                ("freq_mhz", Value::from(f.freq_mhz)),
            ]),
            None => value_object([
                ("supported", Value::from(false)),
                ("domain_bit", Value::from(domain_bit)),
            ]),
        })
    })?;
    py_value(py, &value)
}

/// Direct physical clock for one domain via the green-curve MEASURE path (ID
/// 0x527FC458). One call returns `freq_khz` directly — no two-sample Δcounter
/// /Δtimestamp sleep. Preferred over `query_clk_domain_freq` (counter-based)
/// for XBAR/HOST live-point polling: cheaper and no 50 ms sleep. `domain_bit`:
/// GPC=0, XBAR=1, SYS=2, MCLK=4, HOST=5. `freq_khz == 0` ⇒ the driver refused
/// or the domain isn't measurable through this interface → caller should not
/// draw a live point. Returns `{"supported": false}` when the family is absent.
///
/// `freq_khz` is ALREADY DECODED for HBM MEM (÷4 — see
/// `ClockDomainFreqDirect::mem_scale_divisor`); the raw driver counter is
/// `freq_khz × mem_scale_divisor` (also returned, for diagnostics).
#[pyfunction]
fn query_private_freq_domain_status(
    py: Python<'_>,
    gpu: &str,
    domain_bit: u32,
) -> PyResult<Py<PyAny>> {
    let value = with_target(gpu, "nvapi", |target| {
        let freq = run(target, QueryNvapiClkDomainFreqDirect { domain_bit })
            .map_err(to_py_err)?
            .output;
        Ok(match freq {
            Some(f) => value_object([
                ("domain_bit", Value::from(domain_bit)),
                ("freq_khz", Value::from(f.freq_khz)),
                ("mem_scale_divisor", Value::from(f.mem_scale_divisor)),
            ]),
            None => value_object([
                ("supported", Value::from(false)),
                ("domain_bit", Value::from(domain_bit)),
            ]),
        })
    })?;
    py_value(py, &value)
}

/// Write a signed kHz offset into one clock-domain control record (RM
/// 0x2080d01c SET_CONTROL). DANGEROUS GPU clock write: the operation snapshots
/// the full V2 GetControl block, version-gates (magic 0x261A4), patches a copy,
/// SETs, readbacks, and restores on mismatch. When `temporary` is true the
/// snapshot is restored before returning (the article's reversible experiment
/// recipe). `slot` picks which of the record's 8 value dwords to write (0-7,
/// default 0 = the signed frequency offset; other slots are driver-opaque).
/// No magnitude limit is enforced — the caller owns offset/range policy (the
/// article bounds XBAR ±60000 kHz on GB202).
#[pyfunction]
fn set_clk_domain_offset(
    py: Python<'_>,
    gpu: &str,
    domain_bit: u32,
    offset_khz: i32,
    slot: Option<u32>,
    temporary: Option<bool>,
) -> PyResult<Py<PyAny>> {
    let value = with_target(gpu, "nvapi", |target| {
        let out = run(
            target,
            SetNvapiClkDomainOffset {
                domain_bit,
                offset_kHz: offset_khz,
                slot: slot.unwrap_or(0),
                temporary: temporary.unwrap_or(false),
            },
        )
        .map_err(to_py_err)?
        .output;
        Ok(match out {
            Some(a) => {
                #[allow(non_snake_case)]
                // MHz-suffixed locals: the offset input is MHz now; the raw
                // 8-dword record dump stays in kHz (driver-opaque terms).
                let (previous_mHz, applied_mHz) = (
                    a.previous_kHz as f64 / 1000.0,
                    a.applied_kHz as f64 / 1000.0,
                );
                value_object([
                    ("applied", Value::from(true)),
                    ("bit", Value::from(a.bit)),
                    ("type", Value::from(a.entry_type)),
                    ("slot", Value::from(a.slot)),
                    ("previous_mHz", Value::from(previous_mHz)),
                    ("applied_mHz", Value::from(applied_mHz)),
                    (
                        "values_kHz",
                        Value::Array(a.values_kHz.iter().map(|v| Value::from(*v)).collect()),
                    ),
                    ("temporary_restored", Value::from(a.temporary_restored)),
                ])
            }
            None => value_object([("supported", Value::from(false))]),
        })
    })?;
    py_value(py, &value)
}

/// Write one V/F curve point via the private ClockClient V/F-POINTS
/// SetControl (ID 0xFEC00D04). DANGEROUS: snapshots the full control
/// block, patches one record (mode 0 freq-offset / mode 1 delta), SETs,
/// readbacks, restores on mismatch. `bank` 0 = V/F curve, 1 = pstate-class
/// curve points; `idx` 0..2047. `freq_mode` = mode 0 (u32 kHz — the GUI
/// sends its `deltas_khz` here verbatim) vs mode 1 (i16 delta). Returns
/// `{"supported": false}` when the driver refuses.
#[pyfunction]
fn set_vfp_point_private(
    py: Python<'_>,
    gpu: &str,
    bank: usize,
    idx: usize,
    value_khz: i32,
    freq_mode: Option<bool>,
) -> PyResult<Py<PyAny>> {
    let freq_mode = freq_mode.unwrap_or(false);
    let value = with_target(gpu, "nvapi", |target| {
        // Pascal 2× axis (all Pascal — GpuType::is_pascal): a mode-0 kHz
        // offset must be written as raw = real ×2; mode-1 raw deltas stay
        // verbatim (raw semantics are raw semantics). `value_khz` is the
        // REAL kHz the caller intends — the GUI vfcurve editors pass it.
        let pascal_2x = freq_mode && pascal_2x_axis_cached(gpu, target);
        let written = if pascal_2x {
            value_khz.saturating_mul(2)
        } else {
            value_khz
        };
        let out = run(
            target,
            SetNvapiVfpPointPrivate {
                bank,
                idx,
                freq_mode,
                value: written as u32,
            },
        )
        .map_err(to_py_err)?
        .output;
        Ok(match out {
            Some(retained) => value_object([
                ("applied", Value::from(true)),
                ("bank", Value::from(bank as u64)),
                ("index", Value::from(idx as u64)),
                (
                    "mode",
                    Value::from(if freq_mode { "freq" } else { "delta" }),
                ),
                ("value_khz", Value::from(value_khz)),
                ("written_value", Value::from(written)),
                ("pascal_2x_write", Value::from(pascal_2x)),
                ("retained_raw", Value::from(retained)),
            ]),
            None => value_object([("supported", Value::from(false))]),
        })
    })?;
    py_value(py, &value)
}

/// Paired dual-plane V/F offset write (server Pascal only) — OC-AUTHORITATIVE:
/// the OC plane (B) is set to the demanded value and the PRE-OC plane (A)
/// follows ONLY where it falls short (A = max(A₀, demand) per point, never
/// lowered by an OC edit). Writes go in the invariant-safe order (raises:
/// plane A first). `value_khz` is the caller-intended kHz (the Pascal 2×
/// axis is applied here, matching `set_vfp_point_private`). Returns
/// `{"supported": false}` off the gate; refuses with an error when the
/// current table already violates plane A ≥ plane B.
#[pyfunction]
fn set_vfp_pair_offset(
    py: Python<'_>,
    gpu: &str,
    start: usize,
    count: usize,
    value_khz: i32,
) -> PyResult<Py<PyAny>> {
    let value = with_target(gpu, "nvapi", |target| {
        if !nvoc_core::is_server_pascal_dual_plane(target).map_err(to_py_err)? {
            return Ok(value_object([("supported", Value::from(false))]));
        }
        // the paired path is server-Pascal-only, so the 2× axis always
        // applies inside it
        let value_raw = value_khz.saturating_mul(2) as u32;
        let report = nvoc_core::set_private_vf_oc_offset(target, 0, start, count, value_raw)
            .map_err(to_py_err)?;
        let writes = report
            .writes
            .iter()
            .map(|w| {
                value_object([
                    (
                        "plane",
                        Value::from(match w.plane {
                            nvoc_core::PrivateVfPairPlane::PreOc => "pre_oc",
                            nvoc_core::PrivateVfPairPlane::Oc => "oc",
                        }),
                    ),
                    ("bank", Value::from(w.bank as u64)),
                    ("index", Value::from(w.index as u64)),
                    ("retained_raw", Value::from(w.retained_raw.unwrap_or(0))),
                ])
            })
            .collect::<Vec<_>>();
        Ok(value_object([
            ("applied", Value::from(true)),
            ("bank", Value::from(report.bank as u64)),
            ("start", Value::from(report.start as u64)),
            ("count", Value::from(report.count as u64)),
            ("value_khz", Value::from(value_khz)),
            ("value_raw", Value::from(report.value_raw)),
            (
                "plane_a_before",
                Value::from(
                    report
                        .plane_a_before
                        .iter()
                        .map(|&v| Value::from(v))
                        .collect::<Vec<_>>(),
                ),
            ),
            (
                "plane_b_before",
                Value::from(
                    report
                        .plane_b_before
                        .iter()
                        .map(|&v| Value::from(v))
                        .collect::<Vec<_>>(),
                ),
            ),
            ("writes", Value::from(writes)),
        ]))
    })?;
    py_value(py, &value)
}

/// Ordered paired RESET (server Pascal only): converges BOTH planes of the
/// 80+80 GPC curve to 0 — plane B (OC) first, then plane A (PRE-OC) — the
/// only invariant-safe order for a fill-of-0 (zeroing plane A point n while
/// its n+80 counterpart still holds a positive offset violates A ≥ B and
/// soft-hangs the GPU). Deliberately the CONVERGE controller, NOT the
/// OC-authoritative ratchet: a reset must lower plane A too, which the
/// ratchet never does. Clears the mode-0 kHz offsets of both planes
/// (indices 0..79 + 80..159); the pstate bins (160..167) are untouched.
#[pyfunction]
fn reset_vfp_pair_offset(py: Python<'_>, gpu: &str) -> PyResult<Py<PyAny>> {
    let value = with_target(gpu, "nvapi", |target| {
        if !nvoc_core::is_server_pascal_dual_plane(target).map_err(to_py_err)? {
            return Ok(value_object([("supported", Value::from(false))]));
        }
        let report =
            nvoc_core::set_private_vf_pair_offset(target, 0, 0, 80, 0).map_err(to_py_err)?;
        let writes = report
            .writes
            .iter()
            .map(|w| {
                value_object([
                    (
                        "plane",
                        Value::from(match w.plane {
                            nvoc_core::PrivateVfPairPlane::PreOc => "pre_oc",
                            nvoc_core::PrivateVfPairPlane::Oc => "oc",
                        }),
                    ),
                    ("bank", Value::from(w.bank as u64)),
                    ("index", Value::from(w.index as u64)),
                    ("retained_raw", Value::from(w.retained_raw.unwrap_or(0))),
                ])
            })
            .collect::<Vec<_>>();
        Ok(value_object([
            ("applied", Value::from(true)),
            (
                "reset_order",
                Value::from(["plane B 80-159 -> 0", "plane A 0-79 -> 0"]),
            ),
            ("points_reset", Value::from(report.writes.len() as u64)),
            (
                "plane_a_before",
                Value::from(
                    report
                        .plane_a_before
                        .iter()
                        .map(|&v| Value::from(v))
                        .collect::<Vec<_>>(),
                ),
            ),
            (
                "plane_b_before",
                Value::from(
                    report
                        .plane_b_before
                        .iter()
                        .map(|&v| Value::from(v))
                        .collect::<Vec<_>>(),
                ),
            ),
            ("writes", Value::from(writes)),
        ]))
    })?;
    py_value(py, &value)
}

/// Reset present V/F curve points on `bank` to default via the private
/// SetControl (single RMW cycle). `only_mode` 0|1 restricts the clear to
/// points in that mode (0 = absolute kHz, 1 = raw delta); None clears both.
/// The only way to clear raw/converted private offsets — the public
/// `reset_vfp_deltas` routes through the pstate20 / public Client VfPoints
/// families and cannot reach private state. Returns a dict with `applied`
/// and `points_reset` count, or `{"supported": false}` where the family is
/// absent.
#[pyfunction]
fn reset_vfp_private(
    py: Python<'_>,
    gpu: &str,
    bank: usize,
    only_mode: Option<u8>,
) -> PyResult<Py<PyAny>> {
    let value = with_target(gpu, "nvapi", |target| {
        let out = run(target, ResetNvapiVfpPrivate { bank, only_mode })
            .map_err(to_py_err)?
            .output;
        Ok(match out {
            Some(count) => value_object([
                ("applied", Value::from(true)),
                ("bank", Value::from(bank as u64)),
                ("points_reset", Value::from(count as u64)),
            ]),
            None => value_object([("supported", Value::from(false))]),
        })
    })?;
    py_value(py, &value)
}

// ---------------------------------------------------------------------------
// OC-gap wraps (2026-08-26 audit follow-up) — Python bindings
// ---------------------------------------------------------------------------

// NOTE (2026-08-26): get_dynamic_boost withdrawn — 0xC80068A1 reads the PCF
// platform status bytes, NOT the PPAB enable written by set_ppab_status
// (live-probed; see nvapi-rs examples/probe_pcf_dynamic_boost.rs). The
// nvapi-rs layer keeps the wrap; re-expose only when a true readback is found.

/// Read the core-voltage control object (0xA91F88EB). Raw u32.
#[pyfunction]
fn get_core_voltage_control(py: Python<'_>, gpu: &str) -> PyResult<Py<PyAny>> {
    let value = with_target(gpu, "nvapi", |target| {
        let out = run(target, QueryNvapiCoreVoltageControl)
            .map_err(to_py_err)?
            .output;
        Ok(match out {
            Some(raw) => value_object([("value", Value::from(raw))]),
            None => value_object([("supported", Value::from(false))]),
        })
    })?;
    py_value(py, &value)
}

/// Set the core-voltage control object (0xDC2BD4A6, admin). Distinct from
/// the volt-rail paths.
#[pyfunction]
fn set_core_voltage_control(py: Python<'_>, gpu: &str, value: u32) -> PyResult<Py<PyAny>> {
    let v = with_target(gpu, "nvapi", |target| {
        let out = run(target, SetNvapiCoreVoltageControl { value })
            .map_err(to_py_err)?
            .output;
        Ok(match out {
            Some(()) => value_object([
                ("applied", Value::from(true)),
                ("value", Value::from(value)),
            ]),
            None => value_object([("supported", Value::from(false))]),
        })
    })?;
    py_value(py, &v)
}

/// Read the PMGR voltage-request arbiter values (0x717648FD) — 11 dwords.
#[pyfunction]
fn get_pmgr_arbiter(py: Python<'_>, gpu: &str) -> PyResult<Py<PyAny>> {
    let value = with_target(gpu, "nvapi", |target| {
        let out = run(target, QueryNvapiPmgrVoltageArbiter)
            .map_err(to_py_err)?
            .output;
        Ok(match out {
            PmgrArbiterProbe::Values(values) => value_object([(
                "values",
                Value::Array(values.iter().map(|&d| Value::from(d)).collect()),
            )]),
            PmgrArbiterProbe::Unsupported { status_code } => value_object([
                ("supported", Value::from(false)),
                ("status_code", Value::from(status_code)),
                ("status_name", Value::from(nvapi_status_name(status_code))),
            ]),
        })
    })?;
    py_value(py, &value)
}

/// Set the PMGR voltage-request arbiter values (0x9C4BB8D0, admin).
/// `values` must contain exactly 11 dwords (GET-patch-SET recommended).
#[pyfunction]
fn set_pmgr_arbiter(py: Python<'_>, gpu: &str, values: Vec<u32>) -> PyResult<Py<PyAny>> {
    if values.len() != 11 {
        return Err(invalid_value("values must contain exactly 11 dwords"));
    }
    let mut arr = [0u32; 11];
    arr.copy_from_slice(&values);
    let v = with_target(gpu, "nvapi", |target| {
        let out = run(target, SetNvapiPmgrVoltageArbiter { values: arr })
            .map_err(to_py_err)?
            .output;
        Ok(match out {
            Some(()) => value_object([("applied", Value::from(true))]),
            None => value_object([("supported", Value::from(false))]),
        })
    })?;
    py_value(py, &v)
}

/// Rated-TDP readback trio (0xED2BEA09/0x87BD35EF/0xFCBDF642).
#[pyfunction]
fn get_rated_tdp(py: Python<'_>, gpu: &str) -> PyResult<Py<PyAny>> {
    macos_read_layer("get_rated_tdp", nvoc_core::RATED_TDP_UNWIRED_REASON)?;
    let value = with_target(gpu, "nvapi", |target| {
        let out = run(target, QueryNvapiRatedTdp).map_err(to_py_err)?.output;
        Ok(match out {
            Some((control_mode, caps, raw)) => value_object([
                ("control_mode", Value::from(control_mode)),
                ("info_capabilities", Value::from(caps)),
                (
                    "status_raw",
                    Value::Array(raw.iter().map(|&d| Value::from(d)).collect()),
                ),
            ]),
            None => value_object([("supported", Value::from(false))]),
        })
    })?;
    py_value(py, &value)
}

/// Enable/disable the background OC scanner (0x06DC7CE8).
#[pyfunction]
fn set_background_oc_scanner(py: Python<'_>, gpu: &str, enable: bool) -> PyResult<Py<PyAny>> {
    let v = with_target(gpu, "nvapi", |target| {
        let out = run(target, SetNvapiBackgroundOcScanner { enable })
            .map_err(to_py_err)?
            .output;
        Ok(match out {
            Some(()) => value_object([
                ("applied", Value::from(true)),
                ("enable", Value::from(enable)),
            ]),
            None => value_object([("supported", Value::from(false))]),
        })
    })?;
    py_value(py, &v)
}

/// Query the last INCOMPLETE OC-scanner run's partial results (0xBE371D0A).
#[pyfunction]
fn get_oc_scanner_incomplete(py: Python<'_>, gpu: &str) -> PyResult<Py<PyAny>> {
    let value = with_target(gpu, "nvapi", |target| {
        let out = run(target, QueryNvapiOcScannerIncomplete)
            .map_err(to_py_err)?
            .output;
        Ok(match out {
            Some(()) => value_object([("queried", Value::from(true))]),
            None => value_object([("supported", Value::from(false))]),
        })
    })?;
    py_value(py, &value)
}

/// Read the temperature-simulation state (Secured-Overrides gated).
#[pyfunction]
fn get_temp_sim(py: Python<'_>, gpu: &str) -> PyResult<Py<PyAny>> {
    macos_read_layer("get_temp_sim", nvoc_core::THERMAL_SIM_UNREADABLE_REASON)?;
    let value = with_target(gpu, "nvapi", |target| {
        let out = run(target, QueryNvapiThermalSim).map_err(to_py_err)?.output;
        Ok(match out {
            Some((enable, temp)) => value_object([
                ("enabled", Value::from(enable)),
                ("temperature_c", Value::from(temp)),
            ]),
            None => value_object([("supported", Value::from(false))]),
        })
    })?;
    py_value(py, &value)
}

/// Fake the driver-visible GPU temperature (DANGEROUS research tool;
/// Secured-Overrides gated).
#[pyfunction]
fn set_temp_sim(py: Python<'_>, gpu: &str, temperature_c: i32) -> PyResult<Py<PyAny>> {
    let v = with_target(gpu, "nvapi", |target| {
        let out = run(target, SetNvapiThermalSim { temperature_c })
            .map_err(to_py_err)?
            .output;
        Ok(match out {
            Some(()) => value_object([
                ("applied", Value::from(true)),
                ("temperature_c", Value::from(temperature_c)),
            ]),
            None => value_object([("supported", Value::from(false))]),
        })
    })?;
    py_value(py, &v)
}

/// Disable temperature simulation (restore the real sensor reading).
#[pyfunction]
fn reset_temp_sim(py: Python<'_>, gpu: &str) -> PyResult<Py<PyAny>> {
    let v = with_target(gpu, "nvapi", |target| {
        let out = run(target, DisableNvapiThermalSim)
            .map_err(to_py_err)?
            .output;
        Ok(match out {
            Some(()) => value_object([
                ("applied", Value::from(true)),
                ("disabled", Value::from(true)),
            ]),
            None => value_object([("supported", Value::from(false))]),
        })
    })?;
    py_value(py, &v)
}

/// Admin-free pstate lock (SetPerfLevel 0x75DD3E6A, escape 0x7000040):
/// `level` is an INDEX into the GPU's real available P-State list
/// (get-pstate-native) — not a fixed P8..P0 enum and not the NVCP
/// power-mode dropdown. No release value exists (only valid indices
/// accepted); reboot/driver reload clears the lock; re-locking re-targets.
#[pyfunction]
fn set_pstate_lock_level(py: Python<'_>, gpu: &str, level: u32) -> PyResult<Py<PyAny>> {
    let v = with_target(gpu, "nvapi", |target| {
        let out = run(target, SetNvapiPerfLevelLock { level })
            .map_err(to_py_err)?
            .output;
        Ok(value_object([
            ("applied", Value::from(true)),
            ("level", Value::from(out.applied)),
            (
                "note",
                Value::from(
                    "index into this GPU's real P-State list (get-pstate-native); \
                     no release value — reboot/driver reload clears",
                ),
            ),
        ]))
    })?;
    py_value(py, &v)
}

// NOTE (2026-08-26): get_vfe_equ_info/-control and get_vfe_var_info/-control
// were withdrawn — the PerfVfeEqu/Var surface is not calibrated enough for
// users (equ-control records stay raw). The nvapi-rs layer keeps the wrap.

/// SetControl (ID 0xFEC00D04) using mode-1 (raw f-offset) per-point values
/// in a single RMW cycle. DANGEROUS: snapshots the full control block,
/// patches each record in [start, end], SETs, readbacks, restores on
/// mismatch. `bank` 0 = V/F curve, 1 = pstate-class records; `start`/`end`
/// are inclusive point indices within the bank; `deltas` is the per-point
/// raw i16 f-offset control value (one per point in the range). The GUI's
/// raw-converted path translates MHz targets into these i16 values via
/// `clk_vf_delta_for_target` (the universal g(def) prior) before calling
/// this. Returns `{"supported": false}` when the driver refuses the family.
#[pyfunction]
fn set_vfp_range_per_point_private(
    py: Python<'_>,
    gpu: &str,
    bank: usize,
    start: usize,
    end: usize,
    deltas: Vec<i16>,
) -> PyResult<Py<PyAny>> {
    let count = end.saturating_sub(start).saturating_add(1);
    if deltas.len() < count {
        return Err(invalid_value(format!(
            "deltas has {} value(s) but range {start}..{end} needs {count}",
            deltas.len()
        )));
    }
    let value = with_target(gpu, "nvapi", |target| {
        let out = run(
            target,
            SetNvapiVfpRangePerPointPrivate {
                bank,
                start,
                end,
                deltas: deltas.clone(),
            },
        )
        .map_err(to_py_err)?
        .output;
        Ok(match out {
            Some(()) => value_object([
                ("applied", Value::from(true)),
                ("bank", Value::from(bank as u64)),
                ("start", Value::from(start as u64)),
                ("end", Value::from(end as u64)),
                ("mode", Value::from("raw")),
                ("points_written", Value::from(count as u64)),
            ]),
            None => value_object([("supported", Value::from(false))]),
        })
    })?;
    py_value(py, &value)
}

/// Pascal 2× private-axis gate, cached per GPU id. The chip generation is
/// static for the session and the GUI apply loops call per point — the cheap
/// full_name+short_name escapes behind a cache keep an 80-point apply at one
/// detection, not 80 QueryGpuInfo builds.
fn pascal_2x_axis_cached(gpu: &str, target: &GpuTarget<'_>) -> bool {
    static CACHE: std::sync::OnceLock<std::sync::Mutex<HashMap<String, bool>>> =
        std::sync::OnceLock::new();
    let cache = CACHE.get_or_init(|| std::sync::Mutex::new(HashMap::new()));
    let mut map = match cache.lock() {
        Ok(m) => m,
        Err(p) => p.into_inner(),
    };
    *map.entry(gpu.to_string()).or_insert_with(|| {
        // QueryGpuInfo is heavy, but the cache collapses it to once per GPU
        // per session — an 80-point GUI apply then costs zero extra escapes.
        run(target, QueryGpuInfo)
            .ok()
            .and_then(|r| fetch_gpu_type(&r.output).ok())
            .is_some_and(|t| t.is_pascal())
    })
}

/// Translate a MHz target offset into a private mode-1 raw f-offset control
/// value via the universal g(def) prior (`clk_vf_delta_for_target`). `class`
/// is "graphics" (GPC) or "fabric" (XBAR/HOST). Returns `{"delta": <i32>}` on
/// success or `{"delta": null}` when the prior has no C for this def band.
/// Used by the GUI's raw-converted apply path when the traditional public
/// VFP interface is explicitly unsupported.
///
/// NO Pascal 2× axis scaling here: the mode-1 control word and its g(def)
/// prior are 1:1 on Pascal (live A/B on P100 — a doubled-target write lifted
/// a mid-curve point by ~2.15× the requested MHz, so the prior itself is
/// already correct; only the mode-0 kHz field carries the 2× encoding).
#[pyfunction]
fn clk_vf_delta_for_target_mhz(
    py: Python<'_>,
    def_mhz: u32,
    target_mhz: f64,
    class: &str,
) -> PyResult<Py<PyAny>> {
    let class_enum = match class.trim().to_ascii_lowercase().as_str() {
        "graphics" | "gpc" | "core" | "gpu" => ClkVfDomainClass::Graphics,
        "fabric" | "xbar" | "host" | "msd" => ClkVfDomainClass::Fabric,
        other => {
            return Err(invalid_value(format!(
                "invalid class {other:?}; expected 'graphics'/'gpc' or 'fabric'/'xbar'/'msd'"
            )));
        }
    };
    let delta = clk_vf_delta_for_target(def_mhz, target_mhz, class_enum);
    let value = value_object([(
        "delta",
        match delta {
            Some(d) => Value::from(d),
            None => Value::Null,
        },
    )]);
    py_value(py, &value)
}

#[pyfunction]
fn set_applications_clocks(gpu: &str, memory_mhz: u32, graphics_mhz: u32) -> PyResult<()> {
    let inventory = {
        let mut inventory_cache = lock_inventory_cache();
        inventory_cache.entry(BackendSet::Nvml)?
    };
    let target = selected_target(&inventory.0, gpu)?;
    run(
        &target,
        SetApplicationsClocks {
            memory_mhz,
            graphics_mhz,
        },
    )
    .map_err(to_py_err)?;
    Ok(())
}

#[pyfunction]
fn reset_applications_clocks(gpu: &str) -> PyResult<()> {
    let inventory = {
        let mut inventory_cache = lock_inventory_cache();
        inventory_cache.entry(BackendSet::Nvml)?
    };
    let target = selected_target(&inventory.0, gpu)?;
    run(&target, ResetLegacyApplicationFreqLock).map_err(to_py_err)?;
    Ok(())
}

#[pyfunction]
fn set_locked_clocks(
    py: Python<'_>,
    gpu: &str,
    backend: &str,
    domain: &str,
    min_mhz: u32,
    max_mhz: u32,
) -> PyResult<()> {
    let gpu_own = gpu.to_string();
    let backend_own = backend.to_string();
    let domain_own = domain.to_string();
    py.detach(|| -> PyResult<()> {
        let gpu: &str = &gpu_own;
        let backend: &str = &backend_own;
        let domain: &str = &domain_own;

        let inventory = {
            let mut inventory_cache = lock_inventory_cache();
            inventory_cache.entry(BackendSet::Nvml)?
        };
        let target = selected_target(&inventory.0, gpu)?;
        let domain = parse_domain(domain)?;
        if parse_backend(backend)? != "nvml" {
            return Err(invalid_value("locked clocks currently use the NVML path"));
        }
        run(
            &target,
            SetLockedClocks {
                domain,
                min_mhz,
                max_mhz,
            },
        )
        .map_err(to_py_err)?;
        Ok(())
    })
}

#[pyfunction]
fn reset_locked_clocks(py: Python<'_>, gpu: &str, backend: &str, domain: &str) -> PyResult<()> {
    let gpu_own = gpu.to_string();
    let backend_own = backend.to_string();
    py.detach(|| -> PyResult<()> {
        let gpu: &str = &gpu_own;
        let backend: &str = &backend_own;

        let inventory = {
            let mut inventory_cache = lock_inventory_cache();
            inventory_cache.entry(BackendSet::Nvml)?
        };
        let target = selected_target(&inventory.0, gpu)?;
        let domain = parse_domain(domain)?;
        if parse_backend(backend)? != "nvml" {
            return Err(invalid_value("locked clocks currently use the NVML path"));
        }
        run(&target, ResetFreqLock { domain }).map_err(to_py_err)?;
        Ok(())
    })
}

/// NVML fan 写入的 TDR 恢复包装。
///
/// OC 不稳定触发 TDR(驱动重启)后,进程缓存的 [`Nvml`] 实例与内核的
/// 通道已死,SetFanControlPolicy 报 NotFound(前端 surface 成"找不到
/// policy");而 `Nvml::init()` 对已初始化的库返回 AlreadyInitialized,
/// `Nvml` 的 Drop 又不会 shutdown —— 普通 refresh 重建不出活的 NVML
/// 实例,失效会持续到重启前端。恢复 = 先全局 nvmlShutdown 复位(绕开
/// 被并发快照持有的旧实例),再强制重发现(全新 `Nvml::init` 重枚举),
/// 然后重试一次;仍失败则透传恢复后的错误。
fn with_nvml_fan_recovery<T>(op: impl Fn(&SyncInventory) -> PyResult<T>) -> PyResult<T> {
    let inventory = {
        let mut inventory_cache = lock_inventory_cache();
        inventory_cache.entry(BackendSet::Nvml)?
    };
    match op(&inventory) {
        Ok(v) => Ok(v),
        Err(_first_err) => {
            // shutdown best-effort:库不可达时(无 NVML 可复位),随后的
            // refresh 会报真实错误。
            let _ = nvoc_core::nvml::force_nvml_shutdown();
            let inventory = {
                let mut inventory_cache = lock_inventory_cache();
                inventory_cache.refresh(BackendSet::Nvml)?
            };
            op(&inventory)
        }
    }
}

#[pyfunction]
fn reset_fan_speed(py: Python<'_>, gpu: &str, fan_index: u32) -> PyResult<()> {
    let gpu_own = gpu.to_string();
    py.detach(|| {
        with_nvml_fan_recovery(|inventory| {
            let target = selected_target(&inventory.0, &gpu_own)?;
            run(&target, ResetFanSpeed { fan_index }).map_err(to_py_err)?;
            Ok(())
        })
    })
}

#[pyfunction]
fn set_pstate_base_voltage(gpu: &str, pstate: &str, delta_uv: i32) -> PyResult<()> {
    let inventory = {
        let mut inventory_cache = lock_inventory_cache();
        inventory_cache.entry(BackendSet::Nvapi)?
    };
    let target = selected_target(&inventory.0, gpu)?;
    run(
        &target,
        SetPstateBaseVoltage {
            pstate: parse_pstate(pstate)?,
            delta_uv: MicrovoltsDelta(delta_uv),
        },
    )
    .map_err(to_py_err)?;
    Ok(())
}

#[pyfunction]
fn reset_pstate_base_voltages(gpu: &str) -> PyResult<()> {
    let inventory = {
        let mut inventory_cache = lock_inventory_cache();
        inventory_cache.entry(BackendSet::Nvapi)?
    };
    let target = selected_target(&inventory.0, gpu)?;
    run(&target, ResetLegacyGpcRailOvervoltLimit).map_err(to_py_err)?;
    Ok(())
}

#[pyfunction]
fn set_pstate_clock_offset(gpu: &str, pstate: &str, domain: &str, delta: i32) -> PyResult<()> {
    // This entry carries no backend argument, so the platform decides which one
    // it would write through: on macOS that is the native RM backend, whose read
    // twin (`get-pstate-global-freq-offset`) answers, so the refusal names the
    // gate instead of a runtime the host does not have. Note that no macOS
    // record carries a P-state index -- the snapshot's offsets are per domain --
    // so which field this P-state argument would address is itself part of what
    // the gated verification has to settle.
    macos_write_gate_here("set_pstate_clock_offset")?;
    let inventory = {
        let mut inventory_cache = lock_inventory_cache();
        inventory_cache.entry(BackendSet::Nvapi)?
    };
    let target = selected_target(&inventory.0, gpu)?;
    run(
        &target,
        SetPstateClockOffset {
            pstate: parse_pstate(pstate)?,
            domain: parse_domain(domain)?,
            delta: KilohertzDelta(delta),
        },
    )
    .map_err(to_py_err)?;
    Ok(())
}

#[pyfunction]
fn sync_memory_pstate_as_p0(py: Python<'_>, gpu: &str) -> PyResult<()> {
    py.detach(|| -> PyResult<()> {
        let inventory = {
            let mut inventory_cache = lock_inventory_cache();
            inventory_cache.entry(BackendSet::Nvapi)?
        };
        let target = selected_target(&inventory.0, gpu)?;
        nvoc_core::sync_memory_pstate_as_p0(&target).map_err(to_py_err)?;
        Ok(())
    })
}

#[pyfunction]
fn set_cooler_levels(
    gpu: &str,
    policy: &str,
    level: u32,
    target_name: Option<&str>,
) -> PyResult<()> {
    let inventory = {
        let mut inventory_cache = lock_inventory_cache();
        inventory_cache.entry(BackendSet::Nvapi)?
    };
    let target = selected_target(&inventory.0, gpu)?;
    let cooler_target = match target_name.unwrap_or("all") {
        "1" => nvoc_core::CoolerTarget::Cooler1,
        "2" => nvoc_core::CoolerTarget::Cooler2,
        _ => nvoc_core::CoolerTarget::All,
    };
    let policy = CoolerPolicy::from_str(policy).map_err(invalid_value)?;
    run(
        &target,
        SetCoolerLevels {
            policy,
            level,
            cooler_target,
        },
    )
    .map_err(to_py_err)?;
    Ok(())
}

#[pyfunction]
fn set_vfp_frequency_lock(
    py: Python<'_>,
    gpu: &str,
    domain: &str,
    upper_khz: i32,
    lower_khz: Option<i32>,
) -> PyResult<()> {
    let gpu_own = gpu.to_string();
    let domain_own = domain.to_string();
    py.detach(|| -> PyResult<()> {
        let gpu: &str = &gpu_own;
        let domain: &str = &domain_own;

        let inventory = {
            let mut inventory_cache = lock_inventory_cache();
            inventory_cache.entry(BackendSet::Nvapi)?
        };
        let target = selected_target(&inventory.0, gpu)?;
        run(
            &target,
            SetVfpFrequencyLock {
                domain: parse_domain(domain)?,
                upper: nvapi::hi::Kilohertz(upper_khz.max(0) as u32),
                lower: lower_khz.map(|v| nvapi::hi::Kilohertz(v.max(0) as u32)),
            },
        )
        .map_err(to_py_err)?;
        Ok(())
    })
}

#[pyfunction]
fn reset_vfp_frequency_lock(py: Python<'_>, gpu: &str, domain: &str) -> PyResult<()> {
    let gpu_own = gpu.to_string();
    py.detach(|| -> PyResult<()> {
        let gpu: &str = &gpu_own;

        let inventory = {
            let mut inventory_cache = lock_inventory_cache();
            inventory_cache.entry(BackendSet::Nvapi)?
        };
        let target = selected_target(&inventory.0, gpu)?;
        run(
            &target,
            ResetVfpFrequencyLock {
                domain: parse_domain(domain)?,
            },
        )
        .map_err(to_py_err)?;
        Ok(())
    })
}

#[pyfunction]
fn set_vfp_voltage_lock(
    py: Python<'_>,
    gpu: &str,
    point: Option<usize>,
    voltage_uv: Option<i32>,
    feedback: Option<bool>,
) -> PyResult<()> {
    let gpu_own = gpu.to_string();
    py.detach(|| -> PyResult<()> {
        let gpu: &str = &gpu_own;

        let inventory = {
            let mut inventory_cache = lock_inventory_cache();
            inventory_cache.entry(BackendSet::Nvapi)?
        };
        let target = selected_target(&inventory.0, gpu)?;
        let voltage_target = if let Some(point) = point {
            nvoc_core::NvapiLockedVoltageTarget::Point(point)
        } else if let Some(voltage_uv) = voltage_uv {
            nvoc_core::NvapiLockedVoltageTarget::Voltage(nvapi::hi::Microvolts(
                voltage_uv.max(0) as u32
            ))
        } else {
            return Err(invalid_value("expected either point or voltage"));
        };
        run(
            &target,
            SetGpcVoltLock {
                voltage_target,
                feedback: feedback.unwrap_or(false),
            },
        )
        .map_err(to_py_err)?;
        Ok(())
    })
}

#[pyfunction]
fn reset_vfp_deltas(py: Python<'_>, gpu: &str, domain: Option<&str>) -> PyResult<()> {
    let gpu_own = gpu.to_string();
    let domain_own = domain.map(|s| s.to_string());
    py.detach(|| -> PyResult<()> {
        let gpu: &str = &gpu_own;
        let domain: Option<&str> = domain_own.as_deref();

        let inventory = {
            let mut inventory_cache = lock_inventory_cache();
            inventory_cache.entry(BackendSet::Nvapi)?
        };
        let target = selected_target(&inventory.0, gpu)?;
        let domain = match domain.unwrap_or("all") {
            "all" => VfpResetDomain::All,
            "core" => VfpResetDomain::Core,
            "memory" => VfpResetDomain::Memory,
            other => return Err(invalid_value(format!("invalid VFP reset domain {other:?}"))),
        };
        run(&target, ResetPublicVftableOffset { domain }).map_err(to_py_err)?;
        Ok(())
    })
}

#[pyfunction]
fn set_vfp_point_delta(gpu: &str, point: usize, delta: i32) -> PyResult<()> {
    let inventory = {
        let mut inventory_cache = lock_inventory_cache();
        inventory_cache.entry(BackendSet::Nvapi)?
    };
    let target = selected_target(&inventory.0, gpu)?;
    run(
        &target,
        SetPublicVftablePointOffset {
            point,
            delta: KilohertzDelta(delta),
        },
    )
    .map_err(to_py_err)?;
    Ok(())
}

#[pyfunction]
fn set_vfp_range_delta(
    py: Python<'_>,
    gpu: &str,
    start: usize,
    end: usize,
    delta: i32,
) -> PyResult<()> {
    let gpu_own = gpu.to_string();
    py.detach(|| -> PyResult<()> {
        let gpu: &str = &gpu_own;

        let inventory = {
            let mut inventory_cache = lock_inventory_cache();
            inventory_cache.entry(BackendSet::Nvapi)?
        };
        let target = selected_target(&inventory.0, gpu)?;
        run(
            &target,
            SetPublicVftableRangeOffset {
                start,
                end,
                delta: KilohertzDelta(delta),
            },
        )
        .map_err(to_py_err)?;
        Ok(())
    })
}

#[pyfunction]
fn set_domain_vfp_deltas(
    py: Python<'_>,
    gpu: &str,
    domain: &str,
    deltas: Vec<(usize, i32)>,
) -> PyResult<()> {
    let gpu_own = gpu.to_string();
    let domain_own = domain.to_string();
    py.detach(|| -> PyResult<()> {
        let gpu: &str = &gpu_own;
        let domain: &str = &domain_own;

        let inventory = {
            let mut inventory_cache = lock_inventory_cache();
            inventory_cache.entry(BackendSet::Nvapi)?
        };
        let target = selected_target(&inventory.0, gpu)?;
        let deltas = deltas
            .into_iter()
            .map(|(p, d)| (p, KilohertzDelta(d)))
            .collect::<Vec<_>>();
        run(
            &target,
            SetDomainVfpDeltas {
                domain: parse_domain(domain)?,
                deltas,
            },
        )
        .map_err(to_py_err)?;
        Ok(())
    })
}

#[pyfunction]
fn set_nvapi_power_limits(gpu: &str, limits: Vec<u32>) -> PyResult<()> {
    let inventory = {
        let mut inventory_cache = lock_inventory_cache();
        inventory_cache.entry(BackendSet::Nvapi)?
    };
    let target = selected_target(&inventory.0, gpu)?;
    run(
        &target,
        SetNvapiPowerLimits {
            limits: limits.into_iter().map(Percentage).collect(),
        },
    )
    .map_err(to_py_err)?;
    Ok(())
}

#[pyfunction]
fn set_nvapi_sensor_limits(gpu: &str, limits: Vec<i32>) -> PyResult<()> {
    let inventory = {
        let mut inventory_cache = lock_inventory_cache();
        inventory_cache.entry(BackendSet::Nvapi)?
    };
    let target = selected_target(&inventory.0, gpu)?;
    run(
        &target,
        SetNvapiSensorLimits {
            limits: limits.into_iter().map(|v| Celsius(v).into()).collect(),
        },
    )
    .map_err(to_py_err)?;
    Ok(())
}

#[pyfunction]
fn reset_nvapi_power_limits(gpu: &str) -> PyResult<()> {
    let inventory = {
        let mut inventory_cache = lock_inventory_cache();
        inventory_cache.entry(BackendSet::Nvapi)?
    };
    let target = selected_target(&inventory.0, gpu)?;
    run(&target, ResetNvapiPowerLimits).map_err(to_py_err)?;
    Ok(())
}

#[pyfunction]
fn reset_nvapi_sensor_limits(gpu: &str) -> PyResult<()> {
    let inventory = {
        let mut inventory_cache = lock_inventory_cache();
        inventory_cache.entry(BackendSet::Nvapi)?
    };
    let target = selected_target(&inventory.0, gpu)?;
    run(&target, ResetNvapiSensorLimits).map_err(to_py_err)?;
    Ok(())
}

#[pyfunction]
fn reset_cooler_levels(gpu: &str) -> PyResult<()> {
    let inventory = {
        let mut inventory_cache = lock_inventory_cache();
        inventory_cache.entry(BackendSet::Nvapi)?
    };
    let target = selected_target(&inventory.0, gpu)?;
    run(&target, ResetCoolerLevels).map_err(to_py_err)?;
    Ok(())
}

#[pyfunction]
fn reset_pstate_clock_offsets(gpu: &str, offsets: Vec<(String, String)>) -> PyResult<()> {
    // Same shape as `set_pstate_clock_offset`: the reset half of the family
    // whose read answers on macOS, so the macOS refusal is the gate, not a
    // missing runtime.
    macos_write_gate_here("reset_pstate_clock_offsets")?;
    let inventory = {
        let mut inventory_cache = lock_inventory_cache();
        inventory_cache.entry(BackendSet::Nvapi)?
    };
    let target = selected_target(&inventory.0, gpu)?;
    let offsets = offsets
        .into_iter()
        .map(|(pstate, domain)| Ok((parse_pstate(&pstate)?, parse_domain(&domain)?)))
        .collect::<PyResult<Vec<_>>>()?;
    run(&target, ResetPstateGlobalFreqOffset { offsets }).map_err(to_py_err)?;
    Ok(())
}

#[pyfunction]
fn set_legacy_clocks(gpu: &str, core_mhz: u32, memory_mhz: u32) -> PyResult<()> {
    let inventory = {
        let mut inventory_cache = lock_inventory_cache();
        inventory_cache.entry(BackendSet::Nvapi)?
    };
    let target = selected_target(&inventory.0, gpu)?;
    run(
        &target,
        SetLegacyClocks {
            core_mhz,
            memory_mhz,
        },
    )
    .map_err(to_py_err)?;
    Ok(())
}

/// Lock one NVML P-State or a contiguous range via a memory-clock window
/// (NVAPI locked clocks). Returns None on a clean apply, or a warning string
/// when the derived window also overlaps P-States outside the requested
/// range — identical memory clocks across P-States (e.g. a VBIOS edit
/// pinning P2 to P0's clocks) make the ranges inseparable, so the lock is
/// applied anyway and the caveat surfaced for the UI to display.
#[pyfunction]
fn set_nvapi_pstate_lock(
    py: Python<'_>,
    gpu: &str,
    first_pstate: &str,
    second_pstate: Option<&str>,
) -> PyResult<Option<String>> {
    let gpu_own = gpu.to_string();
    let first_pstate_own = first_pstate.to_string();
    let second_pstate_own = second_pstate.map(|s| s.to_string());
    py.detach(|| -> PyResult<Option<String>> {
        let gpu: &str = &gpu_own;
        let first_pstate: &str = &first_pstate_own;
        let second_pstate: Option<&str> = second_pstate_own.as_deref();

        let inventory = {
            let mut inventory_cache = lock_inventory_cache();
            inventory_cache.entry(BackendSet::Both)?
        };
        let target = selected_target(&inventory.0, gpu)?;
        let report = run(
            &target,
            SetNvapiPstateLock {
                first_pstate: parse_nvml_pstate(first_pstate)?,
                second_pstate: parse_nvml_pstate(second_pstate.unwrap_or(first_pstate))?,
            },
        )
        .map_err(to_py_err)?;
        Ok(report.output.3)
    })
}

/// NVML-adapter twin of [`set_nvapi_pstate_lock`] (memory locked clocks).
/// Same warning contract: None on a clean apply, else the overlap caveat.
#[pyfunction]
fn set_nvml_pstate_lock(
    py: Python<'_>,
    gpu: &str,
    first_pstate: &str,
    second_pstate: Option<&str>,
) -> PyResult<Option<String>> {
    let gpu_own = gpu.to_string();
    let first_pstate_own = first_pstate.to_string();
    let second_pstate_own = second_pstate.map(|s| s.to_string());
    py.detach(|| -> PyResult<Option<String>> {
        let gpu: &str = &gpu_own;
        let first_pstate: &str = &first_pstate_own;
        let second_pstate: Option<&str> = second_pstate_own.as_deref();

        let inventory = {
            let mut inventory_cache = lock_inventory_cache();
            inventory_cache.entry(BackendSet::Nvml)?
        };
        let target = selected_target(&inventory.0, gpu)?;
        let report = run(
            &target,
            SetNvmlPstateLock {
                first_pstate: parse_nvml_pstate(first_pstate)?,
                second_pstate: parse_nvml_pstate(second_pstate.unwrap_or(first_pstate))?,
            },
        )
        .map_err(to_py_err)?;
        Ok(report.output.3)
    })
}

/// Pin the active NVAPI P-State to a single state (the `set-pstate-lock` CLI
/// path: `PerfClientLimitsSetStatus` NDA 0x39442CFB, mode-1 PstateSelect).
/// Pure NVAPI — no NVML, no memory-clock-range derivation. This is the
/// fallback for pre-Kepler GPUs (e.g. GT730/391.35) where
/// `set_nvapi_pstate_lock` (which derives a memory-clock window from NVML
/// P-State ranges) cannot run — the NVML query is Not Supported there.
///
/// Unlike the mem-range lock this interface pins a SINGLE P-State — there is
/// no range form. Callers must force point-mode (start == end) in the UI.
#[pyfunction]
fn set_pstate_native_lock(py: Python<'_>, gpu: &str, pstate: &str) -> PyResult<()> {
    let gpu_own = gpu.to_string();
    let pstate_num = parse_pstate_number(pstate)
        .map_err(|e| invalid_value(format!("invalid P-State '{pstate}': {e}")))?;
    py.detach(|| -> PyResult<()> {
        let gpu: &str = &gpu_own;
        let inventory = {
            let mut inventory_cache = lock_inventory_cache();
            inventory_cache.entry(BackendSet::Nvapi)?
        };
        let target = selected_target(&inventory.0, gpu)?;
        run(
            &target,
            SetNvapiPStateNative {
                lock: NvapiPStateNativeLock::PstateOnly { pstate: pstate_num },
            },
        )
        .map_err(to_py_err)?;
        Ok(())
    })
}

/// Clear all native NVAPI P-State locks (the `reset-pstate-lock` CLI path:
/// `PerfClientLimitsSetStatus` with Reset). Pure NVAPI, no NVML.
#[pyfunction]
fn reset_pstate_native_lock(py: Python<'_>, gpu: &str) -> PyResult<()> {
    let gpu_own = gpu.to_string();
    py.detach(|| -> PyResult<()> {
        let gpu: &str = &gpu_own;
        let inventory = {
            let mut inventory_cache = lock_inventory_cache();
            inventory_cache.entry(BackendSet::Nvapi)?
        };
        let target = selected_target(&inventory.0, gpu)?;
        run(
            &target,
            SetNvapiPStateNative {
                lock: NvapiPStateNativeLock::Reset,
            },
        )
        .map_err(to_py_err)?;
        Ok(())
    })
}

#[pyfunction]
fn set_voltage_boost(py: Python<'_>, gpu: &str, value: u32) -> PyResult<()> {
    let gpu_own = gpu.to_string();
    py.detach(|| -> PyResult<()> {
        let gpu: &str = &gpu_own;

        let inventory = {
            let mut inventory_cache = lock_inventory_cache();
            inventory_cache.entry(BackendSet::Nvapi)?
        };
        let target = selected_target(&inventory.0, gpu)?;
        run(
            &target,
            SetVoltageBoost {
                boost: Percentage(value),
            },
        )
        .map_err(to_py_err)?;
        Ok(())
    })
}

#[pyfunction]
fn reset_voltage_boost(gpu: &str) -> PyResult<()> {
    let inventory = {
        let mut inventory_cache = lock_inventory_cache();
        inventory_cache.entry(BackendSet::Nvapi)?
    };
    let target = selected_target(&inventory.0, gpu)?;
    run(
        &target,
        SetVoltageBoost {
            boost: Percentage(0),
        },
    )
    .map_err(to_py_err)?;
    Ok(())
}

#[pyfunction]
fn set_auto_boost(gpu: &str, enabled: bool) -> PyResult<()> {
    let inventory = {
        let mut inventory_cache = lock_inventory_cache();
        inventory_cache.entry(BackendSet::Nvml)?
    };
    let target = selected_target(&inventory.0, gpu)?;
    run(&target, SetAutoboostStatus { enabled }).map_err(to_py_err)?;
    Ok(())
}

#[pyfunction]
fn set_auto_boost_default(gpu: &str, enabled: bool) -> PyResult<()> {
    let inventory = {
        let mut inventory_cache = lock_inventory_cache();
        inventory_cache.entry(BackendSet::Nvml)?
    };
    let target = selected_target(&inventory.0, gpu)?;
    run(&target, ResetAutoboostStatus { enabled }).map_err(to_py_err)?;
    Ok(())
}

#[pyfunction]
fn set_api_restriction(gpu: &str, api_type: &str, restricted: bool) -> PyResult<()> {
    let api_type = parse_api_restriction_api(api_type)?;
    let inventory = {
        let mut inventory_cache = lock_inventory_cache();
        inventory_cache.entry(BackendSet::Nvml)?
    };
    let target = selected_target(&inventory.0, gpu)?;
    run(
        &target,
        SetAutoboostSupport {
            api_type,
            restricted,
        },
    )
    .map_err(to_py_err)?;
    Ok(())
}

#[pyfunction]
fn set_edid(gpu: &str, display_id: &str, edid_hex: &str) -> PyResult<()> {
    let display_id = parse_display_id(display_id)?;
    let bytes = parse_edid_hex(edid_hex)?;
    let inventory = {
        let mut inventory_cache = lock_inventory_cache();
        inventory_cache.entry(BackendSet::Nvapi)?
    };
    let target = selected_target(&inventory.0, gpu)?;
    run(&target, SetEdid { display_id, bytes }).map_err(to_py_err)?;
    Ok(())
}

#[pyfunction]
fn clear_edid(gpu: &str, display_id: &str) -> PyResult<()> {
    let display_id = parse_display_id(display_id)?;
    let inventory = {
        let mut inventory_cache = lock_inventory_cache();
        inventory_cache.entry(BackendSet::Nvapi)?
    };
    let target = selected_target(&inventory.0, gpu)?;
    run(&target, ClearEdid { display_id }).map_err(to_py_err)?;
    Ok(())
}

#[pyfunction]
fn set_legacy_voltage_delta(
    py: Python<'_>,
    gpu: &str,
    uv: i32,
    pstate: Option<&str>,
) -> PyResult<()> {
    let gpu_own = gpu.to_string();
    let pstate_own = pstate.map(|s| s.to_string());
    py.detach(|| -> PyResult<()> {
        let gpu: &str = &gpu_own;
        let pstate: Option<&str> = pstate_own.as_deref();

        let inventory = {
            let mut inventory_cache = lock_inventory_cache();
            inventory_cache.entry(BackendSet::Nvapi)?
        };
        let target = selected_target(&inventory.0, gpu)?;
        run(
            &target,
            SetPstateBaseVoltage {
                pstate: parse_pstate(pstate.unwrap_or("P0"))?,
                delta_uv: MicrovoltsDelta(uv),
            },
        )
        .map_err(to_py_err)?;
        Ok(())
    })
}

/// Pin the fan duty through the private fan-simulation surface (percent →
/// 0..65536 level; `percent = None` disables the simulation → back to auto).
/// The fallback layer under the cooler-level percent set: 472.12-class
/// drivers reject the ClientFanCoolers control-block SET (single-fan card +
/// phantom Cooler2 in `All` → generic NVAPI_ERROR -1; the public cooler
/// family is NOT_SUPPORTED outright) while this surface takes the pin.
fn nvapi_fan_percent_pin(
    gpu: &str,
    cooler_index: Option<u32>,
    percent: Option<u32>,
) -> PyResult<()> {
    let inventory = {
        let mut inventory_cache = lock_inventory_cache();
        inventory_cache.entry(BackendSet::Nvapi)?
    };
    let target = selected_target(&inventory.0, gpu)?;
    run(
        &target,
        SetFanPercent {
            cooler_index,
            percent,
        },
    )
    .map_err(to_py_err)?;
    Ok(())
}

/// NVAPI fan reset: clear the control-block level override (bit0) — the only
/// reset that actually unpins modern cards (live A/B 1650S+A4000) — with the
/// public RestoreCoolerSettings fallback for legacy drivers (R391/Fermi,
/// GT730 live). Then best-effort clear the fan-simulation enable bit: the
/// percent→sim fallback pin lives there and the control-block clear doesn't
/// touch it. The sim clear is a no-op when nothing is simulated, and stays
/// silent on drivers without the surface (nothing could have pinned through
/// it either).
fn nvapi_fan_reset(gpu: &str) -> PyResult<()> {
    let inventory = {
        let mut inventory_cache = lock_inventory_cache();
        inventory_cache.entry(BackendSet::Nvapi)?
    };
    let target = selected_target(&inventory.0, gpu)?;
    // Restore-first: RestoreCoolerSettings is the vendor-intended reset and
    // never touches the control-block policy byte. On GP104/582.66 the
    // control-block write (ResetNvapiFanControl) carries policy
    // TemperatureContinuous, which the driver honors — switching the fan to
    // the SW temperature-curve mode and its unpopulated ClientFanPolicies
    // table (0/2/6 RPM stall). Cards without the public surface (1650S /
    // A4000, surface NOT_SUPPORTED) fall through to the control-block clear,
    // the only unpin there (live A/B).
    if run(&target, ResetCoolerLevels).is_ok() {
        let _ = run(
            &target,
            SetFanPercent {
                cooler_index: None,
                percent: None,
            },
        );
        return Ok(());
    }
    if let Err(nda_err) = run(&target, ResetNvapiFanControl)
        && let Err(public_err) = run(&target, ResetCoolerLevels)
    {
        return Err(to_py_err(format!(
            "fan reset failed on both NVAPI paths: \
             control-block override clear: {nda_err}; \
             public RestoreCoolerSettings: {public_err}"
        )));
    }
    let _ = run(
        &target,
        SetFanPercent {
            cooler_index: None,
            percent: None,
        },
    );
    Ok(())
}

#[pyfunction]
fn set_fan(
    py: Python<'_>,
    gpu: &str,
    backend: &str,
    fan_id: Option<&str>,
    policy: Option<&str>,
    level: Option<u32>,
) -> PyResult<()> {
    let gpu_own = gpu.to_string();
    let backend_own = backend.to_string();
    let fan_id_own = fan_id.map(|s| s.to_string());
    let policy_own = policy.map(|s| s.to_string());
    py.detach(|| -> PyResult<()> {
        let gpu: &str = &gpu_own;
        let backend: &str = &backend_own;
        let fan_id: Option<&str> = fan_id_own.as_deref();
        let policy: Option<&str> = policy_own.as_deref();

        let backend = parse_backend(backend)?;
        let fan_id = fan_id.unwrap_or("all");
        let level = level.unwrap_or(60);
        // "auto" is the reset semantic (the GUI/TUI "reset to auto" button).
        // True reset — NOT the SW temperature-continuous policy: on GPUs
        // whose ClientFanPolicies curve table is empty (default fan control
        // is firmware-side), switching to the curve-following mode parks the
        // fan near 0 RPM until the temp spikes.
        let is_reset = policy.is_some_and(|p| p.eq_ignore_ascii_case("auto"));
        match backend {
            "nvml" | "nvml-cooler" => {
                // TDR recovery wrapper: a dead post-TDR NVML instance surfaces
                // as NotFound ("找不到 policy") — first failure triggers a
                // global nvmlShutdown + forced re-discovery + one retry.
                let nvml_outcome = with_nvml_fan_recovery(|inventory| {
                    let target = selected_target(&inventory.0, gpu)?;
                    let fan_count = run(&target, QueryFanInfo)
                        .map(|report| report.output.count)
                        .unwrap_or(1);
                    let fan_indices = if fan_id == "all" {
                        (0..fan_count).collect::<Vec<_>>()
                    } else {
                        vec![fan_id.parse::<u32>().map_err(invalid_value)?]
                    };
                    if is_reset {
                        // nvmlDeviceSetDefaultFanSpeed_v2 — the documented
                        // "restore default control policy" call. The old path set
                        // the SW curve policy AND wrote 0% duty on top.
                        for fan_index in fan_indices {
                            run(&target, ResetFanSpeed { fan_index }).map_err(to_py_err)?;
                        }
                    } else {
                        let policy = parse_nvml_fan_control_policy(policy.unwrap_or("continuous"))
                            .map_err(invalid_value)?;
                        for fan_index in fan_indices {
                            run(
                                &target,
                                SetFanSpeed {
                                    fan_index,
                                    policy,
                                    level,
                                },
                            )
                            .map_err(to_py_err)?;
                        }
                    }
                    Ok(())
                });
                if let Err(nvml_err) = nvml_outcome {
                    // R470-class drivers: nvml.dll exports NO fan-write symbols
                    // at all (GetFanSpeed / _v2 / UnitGetFanSpeedInfo are the
                    // only fan exports), so both the percent pin and the reset
                    // die at GetProcAddress. Rescue through the NVAPI surfaces
                    // instead of failing the frontend.
                    if is_reset {
                        nvapi_fan_reset(gpu).map_err(|fallback_err| {
                            // Both legs are execution failures (no GPU selected,
                            // driver without fan-write symbols, …), not bad
                            // argument values — surface as RuntimeError so
                            // callers can distinguish validation from hardware
                            // trouble.
                            to_py_err(format!(
                                "NVML fan reset failed ({nvml_err}) and the NVAPI fallback also failed: {fallback_err}"
                            ))
                        })?;
                    } else {
                        // Same 0-based index space for NVML fans and NDA
                        // coolers on the single-fan cards this fallback
                        // serves; "all" → every present cooler.
                        let cooler_index = fan_id.parse::<u32>().ok();
                        if let Err(fallback_err) =
                            nvapi_fan_percent_pin(gpu, cooler_index, Some(level))
                        {
                            // Same distinction as the reset leg above: both
                            // failures are execution errors → RuntimeError.
                            return Err(to_py_err(format!(
                                "NVML fan set failed ({nvml_err}) and the fan-simulation percent fallback also failed: {fallback_err}"
                            )));
                        }
                    }
                }
            }
            "nvapi" | "nvapi-cooler" => {
                let inventory = {
                    let mut inventory_cache = lock_inventory_cache();
                    inventory_cache.entry(BackendSet::Nvapi)?
                };
                let target = selected_target(&inventory.0, gpu)?;
                if is_reset {
                    nvapi_fan_reset(gpu)?;
                } else {
                    let cooler_target = match fan_id {
                        "1" => nvoc_core::CoolerTarget::Cooler1,
                        "2" => nvoc_core::CoolerTarget::Cooler2,
                        _ => nvoc_core::CoolerTarget::All,
                    };
                    let mode = match policy.unwrap_or("continuous").to_ascii_lowercase().as_str() {
                        "continuous" => CoolerPolicy::TemperatureContinuous,
                        "manual" => CoolerPolicy::Manual,
                        other => CoolerPolicy::from_str(other).map_err(invalid_value)?,
                    };
                    if let Err(primary_err) = run(
                        &target,
                        SetCoolerLevels {
                            policy: mode,
                            level,
                            cooler_target,
                        },
                    ) {
                        // Fallback: pin the duty through the private
                        // fan-simulation surface (percent → 0..65536 level).
                        // 472.12 live: the control-block SET rejects `All`
                        // (count=2, phantom Cooler2 → generic -1) and the
                        // public cooler family answers NOT_SUPPORTED (-104),
                        // so nothing else can serve the percent pin.
                        let cooler_index = match fan_id {
                            "1" => Some(0),
                            "2" => Some(1),
                            _ => None,
                        };
                        if let Err(fallback_err) =
                            nvapi_fan_percent_pin(gpu, cooler_index, Some(level))
                        {
                            // Execution failure, not a bad argument value.
                            return Err(to_py_err(format!(
                                "cooler-level set failed ({primary_err}) and the fan-simulation percent fallback also failed: {fallback_err}"
                            )));
                        }
                    }
                }
            }
            _ => {
                return Err(invalid_value(
                    "fan control requires nvapi/nvml cooler backend",
                ));
            }
        }
        Ok(())
    })
}

/// Toggle fan stop / zero-RPM for a fan-curve slot (NVAPI FanArbiterSet NDA
/// 0x44CD3014, struct magic 0x10144). RE'd from ref tool 2's setFanCurve's
/// tail call. `curve_index` is the slot (default 0); `enable` true = allow
/// the fan to stop at idle, false = always spin.
#[pyfunction]
fn set_fanstop_status(
    py: Python<'_>,
    gpu: &str,
    enable: bool,
    curve_index: Option<u8>,
) -> PyResult<Py<PyAny>> {
    let value = py.detach(|| {
        with_target(gpu, "nvapi", |target| {
            run(
                target,
                SetFanStop {
                    curve_index: curve_index.unwrap_or(0),
                    enable,
                },
            )
            .map_err(to_py_err)?;
            Ok(value_object([("applied", Value::from(true))]))
        })
    })?;
    py_value(py, &value)
}

/// Set fan speed by RPM via the private FanCoolerSetControl (NDA 0xEB44E8AA).
/// RE'd from ref tool 2's setFanSim: RMW the control block, patch enable+level.
/// cooler_index=None targets every present cooler. Pass rpm=-1 to disable
/// simulation (return to auto).
#[pyfunction]
fn set_fan_rpm(
    py: Python<'_>,
    gpu: &str,
    rpm: i32,
    cooler_index: Option<u32>,
) -> PyResult<Py<PyAny>> {
    let value = py.detach(|| {
        with_target(gpu, "nvapi", |target| {
            let rs = run(
                target,
                SetFanRpm {
                    cooler_index,
                    rpm: if rpm < 0 { None } else { Some(rpm as u32) },
                },
            )
            .map_err(to_py_err)?
            .output;
            let coolers: Vec<Value> = rs
                .iter()
                .map(|r| {
                    let applied = match r.applied_rpm {
                        Some(rpm) => Value::from(rpm),
                        None => Value::Null,
                    };
                    value_object([
                        ("cooler_index", Value::from(r.cooler_index)),
                        ("cooler_type", Value::from(r.cooler_type)),
                        ("min_rpm", Value::from(r.min_rpm)),
                        ("max_rpm", Value::from(r.max_rpm)),
                        ("applied_rpm", applied),
                    ])
                })
                .collect();
            Ok(value_object([
                ("applied", Value::from(true)),
                ("coolers", Value::Array(coolers)),
            ]))
        })
    })?;
    py_value(py, &value)
}

/// Read the GPU fan-curve table (`ClientFanPoliciesGetControl` NDA
/// 0xE543C540, struct magic 0x200DC): up to 4 slots × 3 strictly-monotonic
/// (temp_c, RPM) points per slot. Desktop boards only — mobile EC-driven
/// fans carry no NVAPI curve surface.
#[pyfunction]
fn query_fan_curve(py: Python<'_>, gpu: &str) -> PyResult<Py<PyAny>> {
    macos_read_layer("query_fan_curve", nvoc_core::FAN_CONTROL_UNREADABLE_REASON)?;
    let value = py.detach(|| {
        with_target(gpu, "nvapi", |target| {
            let curves = run(target, GetFanCurves).map_err(to_py_err)?.output;
            let items: Vec<Value> = curves
                .iter()
                .map(|c| {
                    value_object([
                        ("index", Value::from(c.index as u64)),
                        (
                            "points",
                            Value::Array(
                                c.points
                                    .iter()
                                    .map(|p| {
                                        value_object([
                                            ("temp_c", Value::from(p.temp_c as u64)),
                                            ("rpm", Value::from(p.rpm as u64)),
                                        ])
                                    })
                                    .collect(),
                            ),
                        ),
                    ])
                })
                .collect();
            Ok(value_object([("curves", Value::Array(items))]))
        })
    })?;
    py_value(py, &value)
}

/// Write one fan-curve slot (`ClientFanPoliciesSetControl`, struct 0x200DC;
/// RMW GET → patch slot → SET, the driver's Set handler enforces strict
/// monotonicity on every lane and answers a generic -5 on violation).
/// `points` = exactly 3 `(temp_c, rpm)` pairs.
///
/// `activate=True` additionally switches the cooler policy to
/// TemperatureContinuous in the same transaction (the table only drives the
/// fan under that policy — same-transaction semantics as the CLI's
/// `set-fan-curve --activate`) and best-effort clears the fan-simulation
/// percent pin (a live pin keeps overriding the curve). `fan_id` scopes
/// that policy switch / pin clear ("1"→Cooler1 / "2"→Cooler2 / None→All) —
/// the curve table itself is per-GPU, the surface has no per-fan selector.
#[pyfunction]
#[pyo3(signature = (gpu, curve_index, points, activate=false, fan_id=None))]
fn set_fan_curve(
    py: Python<'_>,
    gpu: &str,
    curve_index: u8,
    points: Vec<(u16, u32)>,
    activate: bool,
    fan_id: Option<&str>,
) -> PyResult<Py<PyAny>> {
    let gpu_own = gpu.to_string();
    let fan_id_own = fan_id.map(|s| s.to_string());
    let value = py.detach(move || -> PyResultValue {
        let gpu: &str = &gpu_own;
        let fan_id: Option<&str> = fan_id_own.as_deref();
        if curve_index >= 4 {
            return Err(invalid_value(format!(
                "invalid curve index {curve_index}: slot range is 0-3"
            )));
        }
        if points.len() != 3 {
            return Err(invalid_value(format!(
                "expected 3 (temp_c, rpm) points, got {}",
                points.len()
            )));
        }
        for w in points.windows(2) {
            if w[1].0 <= w[0].0 || w[1].1 <= w[0].1 {
                return Err(invalid_value(
                    "fan-curve points must be strictly increasing in both temperature and RPM",
                ));
            }
        }
        let points: Vec<FanCurvePointReadout> = points
            .into_iter()
            .map(|(temp_c, rpm)| FanCurvePointReadout { temp_c, rpm })
            .collect();
        with_target(gpu, "nvapi", |target| {
            let applied = run(
                target,
                SetFanCurve {
                    index: curve_index,
                    points,
                },
            )
            .map_err(to_py_err)?
            .output;
            let mut policy_switched = false;
            let mut pin_released = false;
            if activate {
                // The curve table only drives the fan under policy
                // TemperatureContinuous — an explicit same-transaction switch
                // makes "entering curve mode" visible instead of a side
                // effect (mirrors the CLI's --activate tail).
                let (cooler_target, cooler_index) = match fan_id {
                    Some("1") => (nvoc_core::CoolerTarget::Cooler1, Some(0u32)),
                    Some("2") => (nvoc_core::CoolerTarget::Cooler2, Some(1)),
                    _ => (nvoc_core::CoolerTarget::All, None),
                };
                policy_switched = run(
                    target,
                    SetCoolerLevels {
                        policy: CoolerPolicy::TemperatureContinuous,
                        level: 0,
                        cooler_target,
                    },
                )
                .is_ok();
                // Best-effort release of the fan-simulation percent pin: a
                // live pin keeps overriding the curve. Silent no-op when
                // nothing is simulated (same tail as nvapi_fan_reset).
                pin_released = run(
                    target,
                    SetFanPercent {
                        cooler_index,
                        percent: None,
                    },
                )
                .is_ok();
            }
            Ok(value_object([
                ("applied", Value::from(true)),
                ("curve", Value::from(curve_index as u64)),
                (
                    "points",
                    Value::Array(
                        applied
                            .applied
                            .iter()
                            .map(|p| {
                                value_object([
                                    ("temp_c", Value::from(p.temp_c as u64)),
                                    ("rpm", Value::from(p.rpm as u64)),
                                ])
                            })
                            .collect(),
                    ),
                ),
                (
                    "policy_switched_to_continuous",
                    Value::from(policy_switched),
                ),
                ("pin_released", Value::from(pin_released)),
            ]))
        })
    })?;
    py_value(py, &value)
}

/// Reset one fan-curve slot to factory (`FanPolicySetControl` NDA
/// 0x2B2A2A45, struct 0x214AC — OR `1 << curve` into the +0x08 reset
/// bitmask). Works where the public RestoreCoolerSettings is rejected with
/// NOT_SUPPORTED (desktop 3060/2070 without a user-mode cooler table).
#[pyfunction]
fn reset_fan_curve(py: Python<'_>, gpu: &str, curve_index: u8) -> PyResult<Py<PyAny>> {
    if curve_index >= 4 {
        return Err(invalid_value(format!(
            "invalid curve index {curve_index}: slot range is 0-3"
        )));
    }
    let value = py.detach(|| {
        with_target(gpu, "nvapi", |target| {
            run(target, ResetFanCurve { index: curve_index }).map_err(to_py_err)?;
            Ok(value_object([
                ("applied", Value::from(true)),
                ("curve", Value::from(curve_index as u64)),
                ("reset", Value::from(true)),
            ]))
        })
    })?;
    py_value(py, &value)
}

#[pyfunction]
fn reset_core_clocks(py: Python<'_>, gpu: &str, backend: &str) -> PyResult<()> {
    let gpu_own = gpu.to_string();
    py.detach(|| -> PyResult<()> {
        let gpu: &str = &gpu_own;

        let backend = parse_backend(backend)?;
        let inventory = {
            let mut inventory_cache = lock_inventory_cache();
            inventory_cache.entry(if backend == "nvml" {
                BackendSet::Nvml
            } else {
                BackendSet::Nvapi
            })?
        };
        let target = selected_target(&inventory.0, gpu)?;
        match backend {
            "nvml" => {
                run(
                    &target,
                    ResetFreqLock {
                        domain: ClockDomain::Graphics,
                    },
                )
                .map_err(to_py_err)?;
            }
            "nvapi" => {
                run(
                    &target,
                    ResetVfpFrequencyLock {
                        domain: ClockDomain::Graphics,
                    },
                )
                .map_err(to_py_err)?;
                run(
                    &target,
                    ResetPstateGlobalFreqOffset {
                        offsets: vec![(PState::P0, ClockDomain::Graphics)],
                    },
                )
                .map_err(to_py_err)?;
            }
            _ => {
                return Err(invalid_value(
                    "clock reset requires backend 'nvapi' or 'nvml'",
                ));
            }
        }
        Ok(())
    })
}

#[pyfunction]
fn reset_mem_clocks(py: Python<'_>, gpu: &str, backend: &str) -> PyResult<()> {
    let gpu_own = gpu.to_string();
    py.detach(|| -> PyResult<()> {
        let gpu: &str = &gpu_own;

        let backend = parse_backend(backend)?;
        let inventory = {
            let mut inventory_cache = lock_inventory_cache();
            inventory_cache.entry(if backend == "nvml" {
                BackendSet::Nvml
            } else {
                BackendSet::Nvapi
            })?
        };
        let target = selected_target(&inventory.0, gpu)?;
        match backend {
            "nvml" => {
                run(
                    &target,
                    ResetFreqLock {
                        domain: ClockDomain::Memory,
                    },
                )
                .map_err(to_py_err)?;
            }
            "nvapi" => {
                run(
                    &target,
                    ResetVfpFrequencyLock {
                        domain: ClockDomain::Memory,
                    },
                )
                .map_err(to_py_err)?;
                run(
                    &target,
                    ResetPstateGlobalFreqOffset {
                        offsets: vec![(PState::P0, ClockDomain::Memory)],
                    },
                )
                .map_err(to_py_err)?;
            }
            _ => {
                return Err(invalid_value(
                    "clock reset requires backend 'nvapi' or 'nvml'",
                ));
            }
        }
        Ok(())
    })
}

#[pyfunction]
fn reset_vfp_lock(py: Python<'_>, gpu: &str) -> PyResult<()> {
    py.detach(|| -> PyResult<()> {
        let inventory = {
            let mut inventory_cache = lock_inventory_cache();
            inventory_cache.entry(BackendSet::Nvapi)?
        };
        let target = selected_target(&inventory.0, gpu)?;
        run(&target, ResetPublicVftableGpcLock).map_err(to_py_err)?;
        Ok(())
    })
}

#[pyfunction]
fn reset_all(py: Python<'_>, gpu: &str, domain: Option<&str>) -> PyResult<()> {
    let gpu_own = gpu.to_string();
    let domain_own = domain.map(|s| s.to_string());
    py.detach(|| -> PyResult<()> {
        let gpu: &str = &gpu_own;
        let domain: Option<&str> = domain_own.as_deref();

        let inventory = {
            let mut inventory_cache = lock_inventory_cache();
            inventory_cache.entry(BackendSet::Both)?
        };
        let target = selected_target(&inventory.0, gpu)?;
        if target.has_nvapi() {
            let vfp_domain = match domain.unwrap_or("all").to_ascii_lowercase().as_str() {
                "all" => VfpResetDomain::All,
                "core" | "graphics" => VfpResetDomain::Core,
                "memory" | "mem" => VfpResetDomain::Memory,
                other => return Err(invalid_value(format!("invalid reset domain {other:?}"))),
            };
            run(
                &target,
                SetVoltageBoost {
                    boost: Percentage(0),
                },
            )
            .map_err(to_py_err)?;
            run(&target, ResetNvapiSensorLimits).map_err(to_py_err)?;
            run(&target, ResetNvapiPowerLimits).map_err(to_py_err)?;
            run(&target, ResetCoolerLevels).map_err(to_py_err)?;
            run(&target, ResetPublicVftableOffset { domain: vfp_domain }).map_err(to_py_err)?;
            run(&target, ResetPublicVftableGpcLock).map_err(to_py_err)?;
            run(&target, ResetLegacyGpcRailOvervoltLimit).map_err(to_py_err)?;
            run(
                &target,
                ResetPstateGlobalFreqOffset {
                    offsets: vec![
                        (PState::P0, ClockDomain::Graphics),
                        (PState::P0, ClockDomain::Memory),
                    ],
                },
            )
            .map_err(to_py_err)?;
        }
        if target.has_nvml() {
            let _ = run(
                &target,
                ResetFreqLock {
                    domain: ClockDomain::Graphics,
                },
            );
            let _ = run(
                &target,
                ResetFreqLock {
                    domain: ClockDomain::Memory,
                },
            );
        }
        Ok(())
    })
}

#[pymodule]
fn _native(_py: Python<'_>, m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_function(wrap_pyfunction!(configure_library_paths, m)?)?;
    m.add_function(wrap_pyfunction!(discover_gpus, m)?)?;
    m.add_function(wrap_pyfunction!(force_wake, m)?)?;
    m.add_function(wrap_pyfunction!(query_info, m)?)?;
    m.add_function(wrap_pyfunction!(query_status, m)?)?;
    m.add_function(wrap_pyfunction!(query_settings, m)?)?;
    m.add_function(wrap_pyfunction!(query_supported_applications_clocks, m)?)?;
    m.add_function(wrap_pyfunction!(query_power_limits, m)?)?;
    m.add_function(wrap_pyfunction!(query_pstates, m)?)?;
    m.add_function(wrap_pyfunction!(query_fan_info, m)?)?;
    m.add_function(wrap_pyfunction!(query_legacy_temp_sensor, m)?)?;
    m.add_function(wrap_pyfunction!(query_cooler_info, m)?)?;
    m.add_function(wrap_pyfunction!(query_temperature_thresholds, m)?)?;
    m.add_function(wrap_pyfunction!(query_throttle_reasons, m)?)?;
    m.add_function(wrap_pyfunction!(query_legacy_gpc_rail_volt_range, m)?)?;
    m.add_function(wrap_pyfunction!(query_pstate_base_voltage, m)?)?;
    m.add_function(wrap_pyfunction!(query_voltage_boost, m)?)?;
    m.add_function(wrap_pyfunction!(query_auto_boost, m)?)?;
    m.add_function(wrap_pyfunction!(query_api_restriction, m)?)?;
    m.add_function(wrap_pyfunction!(get_display_list, m)?)?;
    m.add_function(wrap_pyfunction!(query_edid, m)?)?;
    m.add_function(wrap_pyfunction!(query_clock_offset, m)?)?;
    m.add_function(wrap_pyfunction!(plan_clock_offset_step, m)?)?;
    m.add_function(wrap_pyfunction!(query_public_vftable, m)?)?;
    m.add_function(wrap_pyfunction!(query_vbios_vf_curve, m)?)?;
    m.add_function(wrap_pyfunction!(query_vfp_point_voltage, m)?)?;
    m.add_function(wrap_pyfunction!(query_tdp_temp_limits, m)?)?;
    m.add_function(wrap_pyfunction!(probe_voltage_limits, m)?)?;
    m.add_function(wrap_pyfunction!(check_voltage_frequency, m)?)?;
    m.add_function(wrap_pyfunction!(set_clock_offset, m)?)?;
    m.add_function(wrap_pyfunction!(set_power_limit, m)?)?;
    m.add_function(wrap_pyfunction!(set_thermal_limit, m)?)?;
    m.add_function(wrap_pyfunction!(set_ppab_status, m)?)?;
    m.add_function(wrap_pyfunction!(query_tgp_watt_range, m)?)?;
    m.add_function(wrap_pyfunction!(set_tgp_watt, m)?)?;
    m.add_function(wrap_pyfunction!(reset_tgp_watt, m)?)?;
    m.add_function(wrap_pyfunction!(query_dnotifier, m)?)?;
    m.add_function(wrap_pyfunction!(query_power_ceiling, m)?)?;
    m.add_function(wrap_pyfunction!(query_target_temp_policies, m)?)?;
    m.add_function(wrap_pyfunction!(set_dnotifier, m)?)?;
    m.add_function(wrap_pyfunction!(query_volt_rails, m)?)?;
    m.add_function(wrap_pyfunction!(set_volt_rail_offset, m)?)?;
    m.add_function(wrap_pyfunction!(set_volt_rail_target, m)?)?;
    m.add_function(wrap_pyfunction!(set_perf_freq_cap, m)?)?;
    m.add_function(wrap_pyfunction!(query_private_freq_domain_info, m)?)?;
    m.add_function(wrap_pyfunction!(query_clk_domain_freq, m)?)?;
    m.add_function(wrap_pyfunction!(query_private_freq_domain_status, m)?)?;
    m.add_function(wrap_pyfunction!(query_private_vftable, m)?)?;
    m.add_function(wrap_pyfunction!(set_clk_domain_offset, m)?)?;
    m.add_function(wrap_pyfunction!(set_vfp_point_private, m)?)?;
    m.add_function(wrap_pyfunction!(set_vfp_pair_offset, m)?)?;
    m.add_function(wrap_pyfunction!(reset_vfp_pair_offset, m)?)?;
    m.add_function(wrap_pyfunction!(set_vfp_range_per_point_private, m)?)?;
    m.add_function(wrap_pyfunction!(clk_vf_delta_for_target_mhz, m)?)?;
    m.add_function(wrap_pyfunction!(reset_vfp_private, m)?)?;
    m.add_function(wrap_pyfunction!(get_core_voltage_control, m)?)?;
    m.add_function(wrap_pyfunction!(set_core_voltage_control, m)?)?;
    m.add_function(wrap_pyfunction!(get_pmgr_arbiter, m)?)?;
    m.add_function(wrap_pyfunction!(set_pmgr_arbiter, m)?)?;
    m.add_function(wrap_pyfunction!(get_rated_tdp, m)?)?;
    m.add_function(wrap_pyfunction!(set_background_oc_scanner, m)?)?;
    m.add_function(wrap_pyfunction!(get_oc_scanner_incomplete, m)?)?;
    m.add_function(wrap_pyfunction!(get_temp_sim, m)?)?;
    m.add_function(wrap_pyfunction!(set_temp_sim, m)?)?;
    m.add_function(wrap_pyfunction!(reset_temp_sim, m)?)?;
    m.add_function(wrap_pyfunction!(set_pstate_lock_level, m)?)?;
    m.add_function(wrap_pyfunction!(set_target_temp, m)?)?;
    m.add_function(wrap_pyfunction!(set_applications_clocks, m)?)?;
    m.add_function(wrap_pyfunction!(reset_applications_clocks, m)?)?;
    m.add_function(wrap_pyfunction!(set_locked_clocks, m)?)?;
    m.add_function(wrap_pyfunction!(reset_locked_clocks, m)?)?;
    m.add_function(wrap_pyfunction!(reset_fan_speed, m)?)?;
    m.add_function(wrap_pyfunction!(set_pstate_base_voltage, m)?)?;
    m.add_function(wrap_pyfunction!(reset_pstate_base_voltages, m)?)?;
    m.add_function(wrap_pyfunction!(set_pstate_clock_offset, m)?)?;
    m.add_function(wrap_pyfunction!(sync_memory_pstate_as_p0, m)?)?;
    m.add_function(wrap_pyfunction!(set_cooler_levels, m)?)?;
    m.add_function(wrap_pyfunction!(set_vfp_frequency_lock, m)?)?;
    m.add_function(wrap_pyfunction!(reset_vfp_frequency_lock, m)?)?;
    m.add_function(wrap_pyfunction!(set_vfp_voltage_lock, m)?)?;
    m.add_function(wrap_pyfunction!(reset_vfp_deltas, m)?)?;
    m.add_function(wrap_pyfunction!(reset_vfp_lock, m)?)?;
    m.add_function(wrap_pyfunction!(set_vfp_point_delta, m)?)?;
    m.add_function(wrap_pyfunction!(set_vfp_range_delta, m)?)?;
    m.add_function(wrap_pyfunction!(set_domain_vfp_deltas, m)?)?;
    m.add_function(wrap_pyfunction!(set_nvapi_power_limits, m)?)?;
    m.add_function(wrap_pyfunction!(set_nvapi_sensor_limits, m)?)?;
    m.add_function(wrap_pyfunction!(reset_nvapi_power_limits, m)?)?;
    m.add_function(wrap_pyfunction!(reset_nvapi_sensor_limits, m)?)?;
    m.add_function(wrap_pyfunction!(reset_cooler_levels, m)?)?;
    m.add_function(wrap_pyfunction!(reset_pstate_clock_offsets, m)?)?;
    m.add_function(wrap_pyfunction!(set_legacy_clocks, m)?)?;
    m.add_function(wrap_pyfunction!(set_nvapi_pstate_lock, m)?)?;
    m.add_function(wrap_pyfunction!(set_nvml_pstate_lock, m)?)?;
    m.add_function(wrap_pyfunction!(set_pstate_native_lock, m)?)?;
    m.add_function(wrap_pyfunction!(reset_pstate_native_lock, m)?)?;
    m.add_function(wrap_pyfunction!(set_voltage_boost, m)?)?;
    m.add_function(wrap_pyfunction!(reset_voltage_boost, m)?)?;
    m.add_function(wrap_pyfunction!(set_auto_boost, m)?)?;
    m.add_function(wrap_pyfunction!(set_auto_boost_default, m)?)?;
    m.add_function(wrap_pyfunction!(set_api_restriction, m)?)?;
    m.add_function(wrap_pyfunction!(set_edid, m)?)?;
    m.add_function(wrap_pyfunction!(clear_edid, m)?)?;
    m.add_function(wrap_pyfunction!(set_legacy_voltage_delta, m)?)?;
    m.add_function(wrap_pyfunction!(set_fan, m)?)?;
    m.add_function(wrap_pyfunction!(set_fanstop_status, m)?)?;
    m.add_function(wrap_pyfunction!(set_fan_rpm, m)?)?;
    m.add_function(wrap_pyfunction!(query_fan_curve, m)?)?;
    m.add_function(wrap_pyfunction!(set_fan_curve, m)?)?;
    m.add_function(wrap_pyfunction!(reset_fan_curve, m)?)?;
    m.add_function(wrap_pyfunction!(reset_core_clocks, m)?)?;
    m.add_function(wrap_pyfunction!(reset_mem_clocks, m)?)?;
    m.add_function(wrap_pyfunction!(reset_vfp_lock, m)?)?;
    m.add_function(wrap_pyfunction!(reset_all, m)?)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn backend_parsing() {
        assert_eq!(parse_backends("both").unwrap(), BackendSet::Both);
        assert_eq!(parse_backends("nvapi").unwrap(), BackendSet::Nvapi);
        assert_eq!(parse_backends("nvml").unwrap(), BackendSet::Nvml);
        assert!(parse_backends("cuda").is_err());
        // 'auto' follows the CLI's Auto choice: RM on macOS, NVAPI+NVML elsewhere.
        let expected = if cfg!(target_os = "macos") {
            BackendSet::MacosRm
        } else {
            BackendSet::Both
        };
        assert_eq!(parse_backends("auto").unwrap(), expected);
        assert_eq!(parse_backends("DEFAULT").unwrap(), expected);
    }

    #[test]
    fn backend_parsing_accepts_macos_rm() {
        assert_eq!(parse_backends("macos-rm").unwrap(), BackendSet::MacosRm);
        assert_eq!(parse_backends("MACOS_RM").unwrap(), BackendSet::MacosRm);
        assert_eq!(parse_backends(" macosrm ").unwrap(), BackendSet::MacosRm);
        // Unknown sets stay a hard error (the PyErr message is only formatted
        // with a live interpreter, which the --no-default-features unit tests
        // do not start).
        assert!(parse_backends("cuda").is_err());
        assert!(parse_backends("").is_err());
    }

    /// macOS has no NVAPI/NVML, so any backend set other than the native RM is
    /// refused up front with one actionable message; other platforms keep
    /// every set available. This guards the write-path error parity (the
    /// cascade of `dlopen("nvml.dll")` failures is gone on macOS).
    #[test]
    fn macos_backend_availability_matches_the_platform() {
        if cfg!(target_os = "macos") {
            assert!(ensure_backend_available(BackendSet::MacosRm).is_ok());
            assert!(ensure_backend_available(BackendSet::Nvapi).is_err());
            assert!(ensure_backend_available(BackendSet::Nvml).is_err());
            assert!(ensure_backend_available(BackendSet::Both).is_err());
        } else {
            assert!(ensure_backend_available(BackendSet::Nvapi).is_ok());
            assert!(ensure_backend_available(BackendSet::Nvml).is_ok());
            assert!(ensure_backend_available(BackendSet::Both).is_ok());
            assert!(ensure_backend_available(BackendSet::MacosRm).is_ok());
        }
    }

    /// The three entries whose *shape* the RM snapshot does not have (NVML's
    /// severity table, NVML's per-P-state clock table, the cooler triple) must
    /// not answer with the platform sentence: it is true of the platform and
    /// false of the entry, because the same backend answers the *reading* of
    /// each of those families through another entry. They carry the core
    /// constant the CLI quotes instead, so the two entries cannot drift apart.
    #[test]
    fn shape_less_entries_name_the_missing_layer_not_the_platform() {
        let cases = [
            (
                "query_temperature_thresholds",
                nvoc_core::THERMAL_THRESHOLDS_UNREADABLE_REASON,
                // Names the layer: the opcode that would carry a threshold is
                // not in the family this host answers, so the field is
                // unlocated rather than absent.
                "no severity/threshold opcode exists in the family this host answers",
            ),
            (
                "query_pstates",
                nvoc_core::PSTATE_FREQ_TABLE_UNREADABLE_REASON,
                // Names the layer: the read that could feed the table exists
                // but is private, and the one measurement came back empty.
                "not implemented here for want of a read source",
            ),
            (
                "query_cooler_info",
                nvoc_core::FAN_PERCENT_UNREADABLE_REASON,
                // Names the layer: the field has no source, and inventing a
                // scale is exactly what this backend refuses to do.
                "would need an invented scale",
            ),
        ];
        for (entry, reason, layer) in cases {
            let text = source_less_message(entry, reason);
            assert!(
                text.contains(reason),
                "{entry} must quote the shared reason"
            );
            assert!(
                !text.contains("NVAPI/NVML are not available"),
                "{entry} must not blame the platform"
            );
            // The wrapper adds the entry name and nothing else: the reason
            // already names the shape, and restating it in the wrapper (the
            // first version did) reads as a stutter.
            assert!(
                text.starts_with(&format!("{entry} is not implemented on macOS: ")),
                "{entry} must add nothing but its own name: {text}"
            );
            // Each reason must name the layer rather than pass a verdict.
            assert!(
                reason.contains(layer),
                "{entry}'s reason must name the layer: {reason}"
            );
            if cfg!(target_os = "macos") {
                assert!(refuse_without_source(entry, reason).is_err());
            } else {
                assert!(refuse_without_source(entry, reason).is_ok());
            }
        }
        // ... and no two families may share one sentence: the earlier
        // get-settings run showed what that costs (an unread family reported as
        // a missing one). These are top-level constants, so this is an identity
        // check on the strings the entries quote.
        let mut reasons: Vec<&str> = cases.iter().map(|(_, reason, _)| *reason).collect();
        let before = reasons.len();
        reasons.sort_unstable();
        reasons.dedup();
        assert_eq!(
            before,
            reasons.len(),
            "the three families must not share a sentence"
        );
    }

    /// The fan and power-limit entries must reach a backend that exists on the
    /// platform they run on. Their projections answer from the RM snapshot on
    /// macOS, but the wrapper used to hard-code `nvml`, and `with_target`
    /// refuses any set but `macos-rm` there -- so the projection was dead code
    /// and the entry failed on the missing NVML handle instead. The default is
    /// per platform, and the new parameter stays as the explicit opt-in.
    #[test]
    fn fan_and_power_entries_default_to_a_backend_this_platform_has() {
        assert_eq!(
            parse_backends(fan_power_backends(true)).unwrap(),
            BackendSet::MacosRm
        );
        assert_eq!(
            parse_backends(fan_power_backends(false)).unwrap(),
            BackendSet::Nvml
        );
        if cfg!(target_os = "macos") {
            // The default survives the platform gate, and the value it replaced
            // does not -- which is the whole defect.
            assert!(
                ensure_backend_available(parse_backends(fan_power_backends(true)).unwrap()).is_ok()
            );
            assert!(ensure_backend_available(BackendSet::Nvml).is_err());
        }
    }

    /// The native macOS status uses kHz/mW/bytes; the normalized dict must carry
    /// the same MHz/W keys the NVAPI/NVML backends emit, and must not invent a
    /// value for a field the RM backend left unavailable.
    #[test]
    fn macos_rm_status_normalizes_units_into_the_common_contract() {
        let status = DeviceStatus {
            pstate: Some(8),
            graphics_clock_khz: Some(1_530_000),
            memory_clock_khz: Some(6_000_984),
            video_clock_khz: Some(1_410_000),
            sm_clock_khz: None,
            graphics_target_clock_mhz: Some(1530),
            memory_target_clock_mhz: Some(6000),
            video_target_clock_mhz: Some(1410),
            sm_target_clock_mhz: None,
            temperature_c: Some(44.125),
            temperature_memory_c: None,
            temperature_board_c: None,
            temperature_power_supply_c: None,
            power_mw: Some(11_559),
            power_limit_mw: Some(100_000),
            power_limit_requested_mw: Some(100_000),
            average_power_mw: None,
            power_policy_tgp_mw: Some(11_426),
            throttle_reason_mask: Some(0),
            throttle_reasons: Vec::new(),
            fan_rpm: Some(1650),
            fan_rpms: vec![1650],
            // The same reading with the driver's cooler slot, which the entry
            // that names a cooler needs.
            fan_coolers: vec![nvoc_core::FanCoolerReading { slot: 0, rpm: 1650 }],
            utilization_percent: Some(92),
            memory_utilization_percent: Some(0),
            engine_utilization_percent: std::collections::BTreeMap::from([
                ("gr".to_string(), 91.49),
                ("fb".to_string(), 0.0),
            ]),
            pcie_link_gen: Some(3),
            pcie_max_link_gen: Some(3),
            memory_used_bytes: Some(209_649_664),
            rusd_timestamps: std::collections::BTreeMap::new(),
            unavailable: std::collections::BTreeMap::from([(
                "temperature_memory_c".to_string(),
                "RUSD section has no valid timestamp (driver does not publish it)".to_string(),
            )]),
        };
        let value = status_macos_rm_to_map(0x100, 2, &status);
        let map = value.as_object().expect("status map is an object");
        assert_eq!(map["gpu_id"], u64_value(0x100));
        assert_eq!(map["gpu_id_hex"].as_str(), Some("0x0100"));
        assert_eq!(map["index"], u64_value(2));
        assert_eq!(map["pstate"].as_str(), Some("P8"));
        assert_eq!(map["pstate_id"], u64_value(8));
        approx(map, "gpu_clock_mhz", 1530.0);
        approx(map, "mem_clock_mhz", 6000.984);
        approx(map, "video_clock_mhz", 1410.0);
        approx(map, "eff_gpu_clock_mhz", 1530.0);
        approx(map, "temperature_c", 44.125);
        approx(map, "temp_core", 44.125);
        approx(map, "power_w", 11.559);
        approx(map, "power_limit_w", 100.0);
        approx(map, "memory_used_mib", 209_649_664.0 / (1024.0 * 1024.0));
        assert_eq!(map["fan_rpm"], u64_value(1650));
        assert_eq!(map["pcie_link_gen"], u64_value(3));
        assert_eq!(map["utilization_percent"], u64_value(92));
        // Unavailable sections stay absent: no 0 MHz / 0 C fabrication.
        assert!(!map.contains_key("sm_clock_mhz"));
        assert!(!map.contains_key("temperature_memory_c"));
        assert!(!map.contains_key("average_power_w"));
        assert!(map["unavailable"].get("temperature_memory_c").is_some());
    }

    #[test]
    fn macos_rm_info_normalizes_identity_into_the_common_contract() {
        let info = DeviceInfo {
            name: Some("NVIDIA GeForce GTX 1650 SUPER".to_string()),
            driver_version: "610.57.04".to_string(),
            rm_gpu_id: 0x100,
            pci_address: "0000:01:00.0".to_string(),
            vendor_id: 0x10DE,
            device_id: 0x2187,
            subsystem_id: Some(0x4018_0062),
            revision: Some(0xA1),
            board_id: Some(376),
            chip_sku: Some("250".to_string()),
            project: Some("G166".to_string()),
            memory_total_bytes: Some(4_294_967_296),
            memory_usable_bytes: Some(4_075_880_448),
            bar1_bytes: Some(268_435_456),
            l2_cache_bytes: Some(1_048_576),
            supported_class_count: Some(95),
            unavailable: std::collections::BTreeMap::new(),
        };
        let value = info_macos_rm_to_map(0x100, 0, &info);
        let map = value.as_object().expect("info map is an object");
        assert_eq!(map["backend"].as_str(), Some("macos-rm"));
        assert_eq!(map["gpu_id"], u64_value(0x100));
        assert_eq!(map["name"].as_str(), Some("NVIDIA GeForce GTX 1650 SUPER"));
        assert_eq!(map["gpu_name"], map["name"]);
        assert_eq!(map["vendor"].as_str(), Some("NVIDIA"));
        assert_eq!(map["bus"].as_str(), Some("0000:01:00.0"));
        assert_eq!(map["driver_version"].as_str(), Some("610.57.04"));
        assert_eq!(map["memory_total_mib"], u64_value(4096));
        assert_eq!(map["project"].as_str(), Some("G166"));
        // No codename is exposed by RM: do not fabricate an architecture.
        assert!(!map.contains_key("codename"));
        assert!(!map.contains_key("arch"));
    }

    /// The macOS offset read: one field, taken from the snapshot's own record,
    /// and a domain that same snapshot could not corroborate refused with core's
    /// reason -- not reported as an offset of zero, and not blamed on the host.
    ///
    /// Asserted on the plain projection, because this crate's tests run with
    /// `--no-default-features` (no `extension-module`, hence no interpreter),
    /// where even formatting a `PyErr` would panic.
    #[test]
    fn macos_rm_clock_offset_answers_from_the_snapshot_and_names_an_unnamed_domain() {
        let settings = nvoc_core::DeviceSettings {
            clock_offsets_named: Some(vec![nvoc_core::NamedClockOffset {
                name: "memory".to_string(),
                domain_bit: 0x10,
                frequency_khz: 6_000_984,
                offset_khz: -126,
                corroboration: "the record published 6000984 kHz, which is this snapshot's live \
                                memory clock, and it is the only one of the 2 records that did"
                    .to_string(),
            }]),
            ..Default::default()
        };
        let mhz = clock_offset_macos_rm_mhz(&settings, ClockDomain::Memory)
            .expect("a domain this snapshot corroborated answers");
        // The NVAPI branch's unit, so one field stays comparable across backends.
        assert_eq!(mhz, -0.126);

        let err = clock_offset_macos_rm_mhz(&settings, ClockDomain::Graphics)
            .expect_err("a domain this snapshot did not corroborate cannot answer");
        assert!(
            err.contains("graphics is not named in this reading"),
            "the refusal is core's own named reason, domain included: {err}"
        );
        assert!(
            !err.contains("NVAPI/NVML"),
            "the refusal must not blame the platform for the entry's gap: {err}"
        );

        // The py-facing wrapper keeps both outcomes, and is built without an
        // interpreter ever being asked for a message: a `PyErr` is never
        // formatted here, not even on the failure path (the literals below say
        // why instead), because `PyErr`'s `Display` needs the interpreter.
        let value = match normalize_query_clock_offset_macos(&settings, ClockDomain::Memory) {
            Ok(value) => value,
            Err(_) => panic!("the corroborated domain must answer"),
        };
        assert_eq!(value["mhz"].as_f64(), Some(-0.126));
        assert_eq!(
            value.as_object().expect("the value is an object").len(),
            1,
            "the macOS branch returns the same single field the NVAPI/NVML branches do"
        );
        assert!(normalize_query_clock_offset_macos(&settings, ClockDomain::Graphics).is_err());
    }

    /// The offset family's backend resolution and the write half: the read
    /// answers on macOS, so the write names the gate rather than a host runtime
    /// that does not exist there.
    #[test]
    fn macos_clock_targets_preserve_fractional_mhz_and_reject_nonfinite_or_outside_window() {
        assert_eq!(macos_clock_target_khz(0.25).unwrap(), 250);
        assert_eq!(macos_clock_target_khz(-15.0).unwrap(), -15_000);
        for value in [f64::NAN, f64::INFINITY, -18.751, 18.751, 1e30] {
            assert!(macos_clock_target_khz(value).is_err());
        }
    }

    #[test]
    fn macos_rm_offset_backends_and_the_write_gate_name_the_right_layer() {
        // Absent or `auto` resolves to the platform's own backend, so a caller
        // that says nothing gets the one that exists on the host it runs on.
        // `unwrap_or` rather than `unwrap`, so a failure never has to format the
        // `PyErr` that none of these spellings should produce.
        assert_eq!(
            clock_offset_backend(None).unwrap_or("?"),
            fan_power_backends(cfg!(target_os = "macos"))
        );
        assert_eq!(
            clock_offset_backend(Some("auto")).unwrap_or("?"),
            fan_power_backends(cfg!(target_os = "macos"))
        );
        assert_eq!(
            clock_offset_backend(Some(" macOS-RM ")).unwrap_or("?"),
            "macos-rm",
            "the macOS spelling is a backend here, as it is everywhere else"
        );
        assert_eq!(clock_offset_backend(Some("nvapi")).unwrap_or("?"), "nvapi");
        assert!(
            clock_offset_backend(Some("nvapi-cooler")).is_err(),
            "the cooler backends carry no clock offsets, and must still be refused"
        );

        let reason = macos_write_gate_reason("set_clock_offset", "macos-rm")
            .expect("the macOS offset write is behind the gate");
        assert!(reason.starts_with("set_clock_offset: "), "{reason}");
        assert!(
            reason.contains(nvoc_core::MACOS_RM_WRITE_GATE_REASON),
            "{reason}"
        );
        assert!(
            !reason.contains("NVAPI/NVML"),
            "the gate is what is missing, not the host runtime: {reason}"
        );
        assert!(
            macos_write_gate_reason("set_clock_offset", "nvml").is_none(),
            "a non-macOS backend passes the gate"
        );

        // The entries with no backend argument resolve the platform the same way
        // the reads do, so on macOS they name the gate too; elsewhere they keep
        // the sentence their backend has always given, which is why this
        // assertion is written per platform.
        let here = macos_write_gate_here_reason("set_pstate_clock_offset");
        if cfg!(target_os = "macos") {
            let reason = here.expect("the offset write is behind the gate on macOS too");
            assert!(reason.starts_with("set_pstate_clock_offset: "), "{reason}");
            assert!(
                reason.contains(nvoc_core::MACOS_RM_WRITE_GATE_REASON),
                "{reason}"
            );
            assert!(
                macos_write_gate_here_reason("reset_pstate_clock_offsets").is_some(),
                "the reset half of the same family answers to the same gate"
            );
        } else {
            assert!(
                here.is_none(),
                "off macOS the backend's own sentence is still the right one"
            );
        }
    }

    /// The Python half of Stage 11 item 5 for the three families' refused reads:
    /// the same layer text the CLI arm prints, from the same core constant, and
    /// never the platform sentence that ends by explaining a write gate.
    #[test]
    fn macos_read_layers_are_named_from_the_same_constants_as_the_cli() {
        // The pairs the entries carry. A table rather than spot checks, because
        // an entry left out of it would keep the platform sentence and no other
        // assertion here would notice.
        let pairs: [(&str, &str); 6] = [
            ("query_fan_curve", nvoc_core::FAN_CONTROL_UNREADABLE_REASON),
            (
                "query_power_ceiling",
                nvoc_core::POWER_POLICY_UNREADABLE_REASON,
            ),
            ("query_dnotifier", nvoc_core::POWER_POLICY_UNREADABLE_REASON),
            ("get_rated_tdp", nvoc_core::RATED_TDP_UNWIRED_REASON),
            ("get_temp_sim", nvoc_core::THERMAL_SIM_UNREADABLE_REASON),
            (
                "query_tdp_temp_limits",
                nvoc_core::THERMAL_LIMIT_UNREADABLE_REASON,
            ),
        ];
        let on_macos = cfg!(target_os = "macos");
        for (entry, constant) in pairs {
            match macos_read_layer_reason(entry, constant) {
                Some(text) => {
                    assert!(on_macos, "{entry} is refused off macOS: {text}");
                    assert!(text.starts_with(entry), "{text}");
                    assert!(text.contains(constant), "{text}");
                    assert!(
                        !text.contains("NVAPI/NVML"),
                        "the layer is what is missing, not the host runtime: {text}"
                    );
                    assert!(
                        !text.contains("does not implement performance writes"),
                        "a read is not refused by a write gate: {text}"
                    );
                }
                None => assert!(
                    !on_macos,
                    "{entry} must name its layer on macOS, not fall back to the platform sentence"
                ),
            }
        }
        // The power-policy text is shared by three entries and must not claim a
        // driver refusal: nothing was issued for them to be refused.
        assert!(
            nvoc_core::POWER_POLICY_UNREADABLE_REASON.contains("尚未定位"),
            "the policy view is unlocated, not refused"
        );
    }

    /// The macOS settings projection must report what it reads, and must name
    /// the families it cannot read instead of emitting default-looking zeros.
    #[test]
    fn macos_rm_settings_report_capabilities_and_gaps() {
        let settings = nvoc_core::DeviceSettings {
            pstate: Some(8),
            power_limit_mw: Some(100_000),
            power_limit_requested_mw: Some(100_000),
            power_limit_policy: None,
            pcie_link_gen: Some(3),
            memory_total_bytes: Some(4_294_967_296),
            memory_used_bytes: Some(209_649_664),
            board_id: Some(376),
            chip_sku: Some("250".to_string()),
            project: Some("G166".to_string()),
            // The measured half: the window the driver reports, alongside the
            // families it does not report at all.
            clock_offset_limits_khz: Some(nvoc_core::ClockOffsetLimits {
                min_khz: -18_750,
                max_khz: 18_750,
                records: 3,
            }),
            // The domains, as the device stamped them: a bit, not a name.
            clock_offset_domains: Some(vec![nvoc_core::ClockOffsetDomain {
                index: 0,
                domain_bit: 1,
                type_stamp: 0x0808,
            }]),
            // The offsets the clock document publishes, from the same capture:
            // the memory bit's record, whose frequency is the live memory clock.
            clock_offsets: Some(vec![nvoc_core::ClockOffsetReading {
                domain_bit: 0x10,
                frequency_khz: 6_000_984,
                offset_khz: -126,
            }]),
            // The named half of the same reading: core attached "memory" because
            // that record's own published frequency is this snapshot's live
            // memory clock and no other record matched.
            clock_offsets_named: Some(vec![nvoc_core::NamedClockOffset {
                name: "memory".to_string(),
                domain_bit: 0x10,
                frequency_khz: 6_000_984,
                offset_khz: -126,
                corroboration: "the record published 6000984 kHz, which is this snapshot's live \
                                memory clock, and it is the only one of the 2 records that did"
                    .to_string(),
            }]),
            unavailable: std::collections::BTreeMap::from([(
                "core_clock_offset".to_string(),
                nvoc_core::macos_rm::CLOCK_OFFSET_UNREADABLE_REASON.to_string(),
            )]),
            capabilities: nvoc_core::SettingsCapabilities {
                settings_read: true,
                settings_write: false,
                settings_restore: false,
                graphics_offset_write: false,
                memory_offset_write: false,
                power_limit_write: false,
            },
            notes: vec!["read: ...".to_string()],
        };
        let value = settings_macos_rm_to_map(0x100, 0, &settings);
        let map = value.as_object().expect("settings map is an object");
        assert_eq!(map["pstate"].as_str(), Some("P8"));
        assert_eq!(map["power_limit_current"], u64_value(100));
        assert_eq!(map["memory_total_mib"], u64_value(4096));
        assert_eq!(map["capabilities"]["settings_write"], bool_value(false));
        assert_eq!(map["capabilities"]["settings_read"], bool_value(true));
        // No fabricated offset/boost values.
        assert!(!map.contains_key("core_clock_current"));
        assert!(!map.contains_key("mem_clock_current"));
        assert!(!map.contains_key("voltage_boost_current"));
        assert!(map["unavailable"].get("core_clock_offset").is_some());
        // The window and the offsets are measurements, so they are numbers. The
        // named NVAPI-style keys stay absent: macOS reports an offset per domain
        // bit, and only one of those bits can be named from the in-tree copy, so
        // the per-bit list is carried instead of two named values.
        assert_eq!(map["clock_offset_min_khz"], i64_value(-18_750));
        assert_eq!(map["clock_offset_max_khz"], i64_value(18_750));
        assert_eq!(map["clock_offset_limits_records"], u64_value(3));
        let offsets = map["clock_offsets"]
            .as_array()
            .expect("the offset list is an array");
        assert_eq!(offsets.len(), 1);
        assert_eq!(offsets[0]["domain_bit"], u64_value(0x10));
        assert_eq!(offsets[0]["frequency_khz"], u64_value(6_000_984));
        assert_eq!(offsets[0]["offset_khz"], i64_value(-126));
        // The named half carries the comparison, not just the conclusion, so a
        // caller can reject the name without re-reading the device.
        let named = map["clock_offsets_named"]
            .as_array()
            .expect("the named offset list is an array");
        assert_eq!(named.len(), 1);
        assert_eq!(named[0]["name"].as_str(), Some("memory"));
        assert_eq!(named[0]["domain_bit"], u64_value(0x10));
        assert_eq!(named[0]["frequency_khz"], u64_value(6_000_984));
        assert_eq!(named[0]["offset_khz"], i64_value(-126));
        assert!(
            named[0]["corroboration"]
                .as_str()
                .is_some_and(|text| text.contains("6000984 kHz")),
            "{:?}",
            named[0]["corroboration"]
        );
        let domains = map["clock_offset_domains"]
            .as_array()
            .expect("the domain list is an array");
        assert_eq!(domains.len(), 1);
        assert_eq!(domains[0]["index"], u64_value(0));
        assert_eq!(domains[0]["domain_bit"], u64_value(1));
        assert_eq!(domains[0]["type_stamp"], u64_value(0x0808));
        assert_eq!(map["notes"].as_array().map(Vec::len), Some(1));
    }

    fn approx(map: &Map<String, Value>, key: &str, expected: f64) {
        let got = map[key]
            .as_f64()
            .unwrap_or_else(|| panic!("{key} is not a number"));
        assert!((got - expected).abs() < 1e-6, "{key}: {got} != {expected}");
    }

    #[test]
    fn domain_parsing_accepts_ui_aliases() {
        assert_eq!(parse_domain("core").unwrap(), ClockDomain::Graphics);
        assert_eq!(parse_domain("graphics").unwrap(), ClockDomain::Graphics);
        assert_eq!(parse_domain("mem").unwrap(), ClockDomain::Memory);
        assert_eq!(parse_domain("memory").unwrap(), ClockDomain::Memory);
        assert!(parse_domain("video").is_err());
    }

    #[test]
    fn pstate_parsing() {
        assert_eq!(parse_pstate("p0").unwrap(), PState::P0);
        assert_eq!(parse_nvml_pstate("P0").unwrap(), PerformanceState::Zero);
        assert!(parse_pstate("P16").is_err());
        assert!(parse_nvml_pstate("P16").is_err());
    }

    #[test]
    fn numeric_display_parser_handles_units() {
        assert_eq!(first_number_in_display("912.5 mV").unwrap(), 912.5);
        assert_eq!(first_number_in_display("N/A"), None);
        assert_eq!(first_number_in_display("-12 MHz").unwrap(), -12.0);
    }

    #[test]
    fn library_paths_configurable_before_first_gpu_call() {
        // 本 crate 的其余测试都是纯解析逻辑，fresh 测试进程里 NVAPI Once
        // 未消费、inventory 缓存全空——覆盖仍可配置。
        assert!(library_paths_configurable());
    }

    #[test]
    fn non_empty_trimmed_skips_blank() {
        assert_eq!(non_empty_trimmed(None), None);
        assert_eq!(non_empty_trimmed(Some("")), None);
        assert_eq!(non_empty_trimmed(Some("   ")), None);
        assert_eq!(non_empty_trimmed(Some(" /opt/nv ")), Some("/opt/nv"));
    }

    #[test]
    fn set_library_path_env_writes_and_skips() {
        set_library_path_env(Some(" /opt/nvapi "), None).unwrap();
        assert_eq!(
            std::env::var(nvoc_core::dll_path::NVAPI_PATH_ENV).unwrap(),
            "/opt/nvapi"
        );
        // 空串等价未传：NVAPI env 保持上一行写入的值，NVML env 新写入。
        set_library_path_env(Some("   "), Some(" /opt/nvml ")).unwrap();
        assert_eq!(
            std::env::var(nvoc_core::dll_path::NVAPI_PATH_ENV).unwrap(),
            "/opt/nvapi"
        );
        assert_eq!(
            std::env::var(nvoc_core::dll_path::NVML_PATH_ENV).unwrap(),
            "/opt/nvml"
        );
    }
}
