# macOS 后端功能表（stage 0 交付物）

本文件是 macOS 适配的**权威功能表与阶段索引**：原始 NVOC 操作 → macOS 实现状态 →
消费者接入 → 证据。它按阶段 0→12 持续更新，不能只作为文档存在：表中的每一行都必须
能指向代码、实机命令/日志或一条明确的外部阻塞。

## 状态图例

| 状态 | 含义 |
| --- | --- |
| **已实现且实测** | 代码存在，且已在权威 checkout + 实机验证（含返回码/日志） |
| **已实现待实测** | 代码路径存在，缺少实机证据 |
| **未实现** | macOS 后端在访问驱动前给出明确错误（不静默回退到 NVAPI/NVML） |
| **驱动明确拒绝** | 实机返回 `NOT_SUPPORTED` 或等价状态（记录命令与返回码） |
| **调查未知** | 协议/结构尚未确认，需要受控探测 |
| **不适用** | 当前设备/平台不适用（附依据） |
| **待授权** | 属于写入路径，需要恢复验证与用户授权门槛 |

`disabled`、mock、dry-run **不算**实机支持。

## 证据索引

- 权威源码（开发端）：`/home/lyy/work/nvoc-next`，分支 `macos-driver-validation`；
  测试机 `/Users/mac/Desktop/macOS-nvoc-develop/nvoc-next` 内容与之逐文件一致。
- 环境（2026-10-17 复核）：`mac@10.253.254.125` = `macs-imac.dev.skyw.top`、
  macOS 15.8.1 / 24H32、x86_64、i5-8600K、GTX 1650 SUPER（`10de:2187`，4 GiB，
  PCI `0000:01:00.0`）、NullMoth 2.0.4、RM/OGKM 610.57.04、无 sudo。
- 代码依据：`core/native/macos_rm.c`、`core/native/macos_rm_abi.h`、
  `core/native/rusd_read.h`、`core/src/macos_rm.rs`、`probe-macos-rm/src/*`、
  `docs/design/macos-rm-backend.md`、`docs/reverse-engineering/macos-gtx1650s-access-2026-10-16.md`。
- 实测日志（仓库外 `outputs/`）：`stage1-*`（阶段 1 验收：build/fmt/clippy/tests/5 项硬件/
  ABI/探针/CLI 停止与管道）、`continuous-*`（单次与连续基线）、`continuous-watch-*`
  （3 分钟只读 + 时钟域探针）、`continuous-load-*`（温和 Metal 负载响应）、
  `rusd-*`/`nvoc-access-*`（遥测与属性基线）。

## 一、CLI 命令表（101 条，来源：`cli/src/lib.rs` 的 `CommandSpec`）

表中"原 NVOC 行为"取自命令自身的说明（`nvoc-cli <命令> --help` 是完整参数来源）。

### GPU 与显示信息（`Info`，13 项）

| 命令 | 原 NVOC 行为（摘要） | macOS RM 状态 |
| --- | --- | --- |
| `clear-edid` | Clear display EDID through NVAPI | 未实现：macOS RM 在访问驱动前拒绝 |
| `get-display-list` | List NVAPI display IDs for EDID operations | 未实现：macOS RM 在访问驱动前拒绝 |
| `get-edid` | Read display EDID through NVAPI | 未实现：macOS RM 在访问驱动前拒绝 |
| `get-gpu-list` | List discovered GPUs and available backends | **已实现且实测**：设备发现/身份（`get-gpu-list`） |
| `get-info` | Read NVAPI GPU identity and capability information | **已实现且实测**：PCI/VBIOS/显存/驱动信息 |
| `get-settings` | Read NVAPI overclock settings | 未实现：macOS RM 在访问驱动前拒绝 |
| `get-status` | Read live GPU status (macOS RM: P-state and RUSD telemetry; unavailable fields include reasons; --interval streams JSONL samples) | **已实现且实测**：P-state + 8 项 RUSD 遥测 + `--interval` 连续监控 |
| `get-throttle-reasons` | Read NVML throttle reasons | 未实现：macOS RM 在访问驱动前拒绝 |
| `get-uuid` | Read GPU UUID | 未实现：macOS RM 在访问驱动前拒绝 |
| `get-vbios` | Read and decode a VBIOS image. -i/--input <file> parses a local ROM dump OFFLINE (no GPU/NvAPI needed — works on Linux); without it the live image… | 部分：`-i/--input` 离线解析已实现；实时读取是 NVAPI/Windows 路径，macOS 未实现 |
| `restart-display-driver` | Restart the display driver (0xB4B26B65); legacy apply-OC trigger | 未实现：macOS RM 在访问驱动前拒绝 |
| `set-ecc-configuration` | Set NVAPI ECC memory configuration (NvAPI_GPU_SetECCConfiguration 0x1CF639D9): persists in non-volatile memory — default defers to the next reboot,… | 未实现：macOS RM 在访问驱动前拒绝 |
| `set-edid` | Set display EDID through NVAPI | 未实现：macOS RM 在访问驱动前拒绝 |

### 功耗 / TGP（`Power`，14 项）

| 命令 | 原 NVOC 行为（摘要） | macOS RM 状态 |
| --- | --- | --- |
| `get-dnotifier` | Read NVAPI D-Notifier (D0-notify) level + D1-D5 power-cap table (mobile) | 未实现：macOS RM 在访问驱动前拒绝 |
| `get-power-ceiling` | Read the effective power wall on PPAB mobiles (nvidia-smi's Ceiling trio): requested TGP (ClientTgpWattGetStatus) + active D-Notifier cap, ceiling… | 未实现：macOS RM 在访问驱动前拒绝 |
| `get-power-mode` | Read NVIDIA App power mode (Balanced/Max with support gate) | 未实现：macOS RM 在访问驱动前拒绝 |
| `get-public-power-limit` | Read the power-limit range: --nvml (auto default) reports the NVML power-management limits in watts (min/current/max); --nvapi reports the ClientPo… | 未实现：macOS RM 在访问驱动前拒绝 |
| `get-rated-tdp` | Rated-TDP readback trio (0xED2BEA09/0x87BD35EF/0xFCBDF642) | 未实现：macOS RM 在访问驱动前拒绝 |
| `reset-power-limit` | Reset NVAPI TGP to rated/default (mobile) | 未实现：macOS RM 在访问驱动前拒绝 |
| `reset-public-tgp-percent` | Reset NVAPI power limits | 未实现：macOS RM 在访问驱动前拒绝 |
| `set-batteryboost2-status` | Battery Boost 2.0 enable/disable (0xD27D0629); mobile-only; 1=enable 0=disable | 未实现：macOS RM 在访问驱动前拒绝 |
| `set-dnotifier` | Set NVAPI D-Notifier limit level (D1-D5; shares the TGP power-policy table) | 未实现：macOS RM 在访问驱动前拒绝 |
| `set-power-limit` | Set TGP in watts: NVAPI path writes the mobile TGP slider (ClientPowerPolicies, --policy-index); NVML path writes the power-management limit (nvidi… | 未实现：macOS RM 在访问驱动前拒绝 |
| `set-power-mode` | Set NVIDIA App power mode: max \| balanced (the App's Balanced/Max toggle) | 未实现：macOS RM 在访问驱动前拒绝 |
| `set-ppab-status` | Set NVAPI PPAB / Dynamic-Boost enable (on/off) | 未实现：macOS RM 在访问驱动前拒绝 |
| `set-public-tgp-percent` | Set NVAPI power limit in percent | 未实现：macOS RM 在访问驱动前拒绝 |
| `set-whispermode2-status` | Whisper Mode 2.0 status (0xD27D0629, mobile-only): on/off enable; --mode quieter\|quiet\|balanced also writes the acoustic mode (0xD2561B69) | 未实现：macOS RM 在访问驱动前拒绝 |

### 温度（`Thermal`，9 项）

| 命令 | 原 NVOC 行为（摘要） | macOS RM 状态 |
| --- | --- | --- |
| `get-legacy-temp-sensor` | Read NVAPI legacy 3-sensor thermal view (GPU/Memory/Board, live + physical range) | 未实现：macOS RM 在访问驱动前拒绝 |
| `get-public-temp-limit` | Read the NVAPI public temp-limit range (min/default/max Celsius + throttle curve) | 未实现：macOS RM 在访问驱动前拒绝 |
| `get-temp-sim` | Read the temperature-simulation state (GetThermalSimulationMode; Secured-Overrides 'Temp faking allowed' gated) | 未实现：macOS RM 在访问驱动前拒绝 |
| `get-temp-thresholds` | Read temperature thresholds (NVML by default; --nvapi exposes target-temp policy) | 未实现：macOS RM 在访问驱动前拒绝 |
| `reset-temp-limit` | Reset NVAPI sensor limits | 未实现：macOS RM 在访问驱动前拒绝 |
| `reset-temp-sim` | Disable temperature simulation and restore the real sensor reading | 未实现：macOS RM 在访问驱动前拒绝 |
| `set-private-target-temp-limit` | Set an NVAPI target-temp (temp-limit) policy slot in Celsius for mobile sku | 未实现：macOS RM 在访问驱动前拒绝 |
| `set-temp-limit` | Set thermal limit in Celsius: NVAPI path writes the sensor limit; NVML path writes the GPU max-temp threshold, or the acoustic target temp with --d… | 未实现：macOS RM 在访问驱动前拒绝 |
| `set-temp-sim` | Fake the driver-visible GPU temperature in Celsius (DANGEROUS research tool; Extended->basic fallback; Secured-Overrides gated) | 未实现：macOS RM 在访问驱动前拒绝 |

### 风扇（`Fan`，8 项）

| 命令 | 原 NVOC 行为（摘要） | macOS RM 状态 |
| --- | --- | --- |
| `get-fan-curve` | Read the NVAPI fan-curve table (ClientFanPolicies, struct 0x200DC; desktop-only) | 未实现：macOS RM 在访问驱动前拒绝 |
| `get-fan-info` | Read fan/cooler info (NVML: count + min/max percent; NVAPI: per-cooler info via private FanCoolerGetInfo) | 未实现：macOS RM 在访问驱动前拒绝 |
| `get-legacy-fan-policy` | Read fan-policy capabilities (ClientFanPoliciesGetInfo NDA 0x52B76D12: V2 raw block on modern drivers; legacy V1 decoded on R391-era — policy list… | 未实现：macOS RM 在访问驱动前拒绝 |
| `reset-fan-curve` | Reset one fan-curve slot to factory (Private FanPolicy path 0x2B2A2A45; works where restore-fan/cooler-settings is NOT_SUPPORTED, e.g. desktop 3060… | 未实现：macOS RM 在访问驱动前拒绝 |
| `reset-fan-speed` | Restore fan/cooler control: default clears the NVAPI cooler level override (control-block bit0 — the only reset that unpins modern cards) / NVML fa… | 未实现：macOS RM 在访问驱动前拒绝 |
| `set-fan-curve` | Write one fan-curve slot (RMW: --curve idx --points ...). The written slot only drives the fan while the cooler policy is TemperatureContinuous (8)… | 未实现：macOS RM 在访问驱动前拒绝 |
| `set-fan-speed` | Set fan speed: --percent (default) sets cooler level in percent (NVAPI SetCoolerLevels / NVML set_fan_speed); --rpm sets physical RPM via private F… | 未实现：macOS RM 在访问驱动前拒绝 |
| `set-fanstop-status` | Toggle fan stop / zero-RPM for a curve slot (FanArbiterSet NDA 0x44CD3014): on \| off | 未实现：macOS RM 在访问驱动前拒绝 |

### 时钟、偏移与 P-State 锁（`Clock`，20 项）

| 命令 | 原 NVOC 行为（摘要） | macOS RM 状态 |
| --- | --- | --- |
| `get-private-legacy-pstates20-freq-domain-info` | Read-only dump of the private pstates-2.0 delta table (GetPstates20Private 0xC5DDF56E) — the storage whose deltas move the frequency-request ceilin… | 未实现：macOS RM 在访问驱动前拒绝 |
| `get-pstate-freq-range` | Read NVML P-State clock ranges | 未实现：macOS RM 在访问驱动前拒绝 |
| `get-pstate-global-freq-offset` | Read clock offset in MHz | 未实现：macOS RM 在访问驱动前拒绝 |
| `get-pstate-lock` | Read the native NVAPI P-State level table | 未实现：macOS RM 在访问驱动前拒绝 |
| `get-supported-legacy-application-freq` | Read NVML supported application clocks | 未实现：macOS RM 在访问驱动前拒绝 |
| `reset-freq-lock` | Reset core or memory locked clocks | 未实现：macOS RM 在访问驱动前拒绝 |
| `reset-legacy-application-freq-lock` | Reset NVML application clocks | 未实现：macOS RM 在访问驱动前拒绝 |
| `reset-private-forced-pstate-lock-user` | Release a force-locked pstate via SetForcePstate(pstate=16, set_type=0) — pstate=16 is the bitmask=0 sentinel (GetForcePstate returns 16 when no fo… | 未实现：macOS RM 在访问驱动前拒绝 |
| `reset-pstate-global-freq-offset` | Reset NVAPI P-State clock offsets (all touched pstate/domain pairs by default; --domain filters to one clock domain) | 未实现：macOS RM 在访问驱动前拒绝 |
| `reset-pstate-lock` | Clear all native NVAPI P-State locks | 未实现：macOS RM 在访问驱动前拒绝 |
| `set-freq-lock` | Lock core or memory clocks to a MHz range | 未实现：macOS RM 在访问驱动前拒绝 |
| `set-legacy-application-freq-lock` | Set NVML application clocks in MHz | 未实现：macOS RM 在访问驱动前拒绝 |
| `set-legacy-freq` | Set an absolute clock for legacy (Kepler) GPUs in MHz: --domain core (default) or mem picks which clock the value targets | 未实现：macOS RM 在访问驱动前拒绝 |
| `set-overclocked-pstates` | Toggle the overclocked-pstate unlock (EnableOverclockedPstates 0xB23B70EE): 1 opens the extended/OC pstate range before a set-pstate-global-freq-of… | 未实现：macOS RM 在访问驱动前拒绝 |
| `set-private-forced-pstate-lock-user` | Force a P-State via private SetForcePstate (0x025BFB10); set_type 0/1/2 all force-lock, none release (to unlock use reset-private-forced-pstate-loc… | 未实现：macOS RM 在访问驱动前拒绝 |
| `set-private-legacy-pstates20-freq-domain-global-offset` | DANGEROUS RMW of one delta in the private pstates table (SetPstates20Private 0x4C0B519A); --domain takes a domain name (gpc/xbar/m/gpc2/…) or the r… | 未实现：macOS RM 在访问驱动前拒绝 |
| `set-private-permanent-pstate-lock-user` | Admin-free pstate lock (SetPerfLevel 0x75DD3E6A, escape 0x7000040): level is an INDEX into the GPU's real available P-State list (see get-pstate-lo… | 未实现：macOS RM 在访问驱动前拒绝 |
| `set-pstate-global-freq-offset` | Set clock offset in MHz for any clock domain | 未实现：macOS RM 在访问驱动前拒绝 |
| `set-pstate-lock` | Lock the native NVAPI P-State | 未实现：macOS RM 在访问驱动前拒绝 |
| `set-pstate-lock-via-mem-range` | Lock one NVML P-State or a contiguous range via memory freq range | 未实现：macOS RM 在访问驱动前拒绝 |

### 电压（`Voltage`，13 项）

| 命令 | 原 NVOC 行为（摘要） | macOS RM 状态 |
| --- | --- | --- |
| `get-core-voltage-control` | Read the core-voltage control object (0xA91F88EB, escape 0x07000045) | 未实现：macOS RM 在访问驱动前拒绝 |
| `get-legacy-gpc-rail-overvolt-limit` | Read NVAPI P-State base voltage delta in microvolts | 未实现：macOS RM 在访问驱动前拒绝 |
| `get-legacy-gpc-rail-volt-range` | Read NVAPI legacy core overvolt ranges (per-pstate min/current/max; --pstate filters to one) | 未实现：macOS RM 在访问驱动前拒绝 |
| `get-public-gpc-rail-volt-boost` | Read NVAPI voltage boost percent | 未实现：macOS RM 在访问驱动前拒绝 |
| `get-volt-rail-info` | Read private VoltRails family: rail mask + per-rail offsets + live voltages (melonVolt path) + VRM device voltage windows (0xA38ACF9D). --verbose a… | 未实现：macOS RM 在访问驱动前拒绝 |
| `reset-legacy-gpc-rail-overvolt-limit` | Reset NVAPI P-State base voltages | 未实现：macOS RM 在访问驱动前拒绝 |
| `reset-public-gpc-rail-volt-boost` | Reset NVAPI voltage boost percent | 未实现：macOS RM 在访问驱动前拒绝 |
| `set-core-voltage-control` | Set the core-voltage control (0xDC2BD4A6, escape 0x07000044; admin; distinct from volt-rail paths) | 未实现：macOS RM 在访问驱动前拒绝 |
| `set-gpc-volt-lock` | Lock VFP by point or voltage | 未实现：macOS RM 在访问驱动前拒绝 |
| `set-legacy-gpc-rail-overvolt-limit` | Set NVAPI P-State base voltage delta in microvolts | 未实现：macOS RM 在访问驱动前拒绝 |
| `set-overvolt-uv` | Set global NVAPI over-voltage offset in microvolts (PSTATES20 V2 OV array) | 未实现：macOS RM 在访问驱动前拒绝 |
| `set-public-gpc-rail-volt-boost` | Set NVAPI voltage boost percent | 未实现：macOS RM 在访问驱动前拒绝 |
| `set-volt-rail-limit` | Set a volt-rail limit: --offset (default) writes an mV offset directly (melonVolt write path; 5090 MSVDD = rail 1 type 3); --target takes an absolu… | 未实现：macOS RM 在访问驱动前拒绝 |

### V/F、电压点与高级超频（`Vfp`，14 项）

| 命令 | 原 NVOC 行为（摘要） | macOS RM 状态 |
| --- | --- | --- |
| `get-private-freq-domain-info` | Read the private ClockClient domain-control block: controllable mask + per-domain offset/range records (XBar physical-clock path) | 未实现：macOS RM 在访问驱动前拒绝 |
| `get-private-freq-domain-status` | Measure one clock domain's physical clock via two-sample MEASURE_FREQ — MEASURE-bit universe, RTSS order: GPC=0, XBar=1, Sys=2, Hub=3, M=4, Host=5,… | 未实现：macOS RM 在访问驱动前拒绝 |
| `get-private-vftable` | Read the private ClockClient V/F-points family: per-bank point masks + V/F curve records (voltage-indexed, units calibrated vs the public GPC VFP);… | 未实现：macOS RM 在访问驱动前拒绝 |
| `get-public-vftable` | Read the public V-F curve table: default dumps all domains (graphics points plus the trailing memory entries, e.g. index 127..131 on 30/40 series);… | 未实现：macOS RM 在访问驱动前拒绝 |
| `reset-private-freq-domain-global-offset` | Reset private clock-domain global offsets to stock (offset 0) through the same ClkDomains WRITE path as set-private-freq-domain-global-offset; defa… | 未实现：macOS RM 在访问驱动前拒绝 |
| `reset-private-vftable-offset` | Reset private V/F-POINTS overrides (clear freq/raw offsets the public/pstate20 reset paths cannot reach; --freq/--volt --mode freq\|raw --slot 0/1… | 未实现：macOS RM 在访问驱动前拒绝 |
| `reset-public-vftable-gpc-lock` | Reset NVAPI VFP lock | 未实现：macOS RM 在访问驱动前拒绝 |
| `reset-public-vftable-offset` | Reset NVAPI VFP deltas | 未实现：macOS RM 在访问驱动前拒绝 |
| `set-private-freq-domain-global-offset` | Write a signed offset into one clock-domain control record plane (dangerous XBar clock write; --temporary restores the snapshot; --freq/--volt = th… | 未实现：macOS RM 在访问驱动前拒绝 |
| `set-private-vftable-point-offset` | Write one V/F curve point via the private SetControl (dangerous V/F edit; bank 0=V/F curve, 1=pstate-class; default/--freq-mode = kHz freq offset (… | 未实现：macOS RM 在访问驱动前拒绝 |
| `set-private-vftable-range-offset` | Write a range of V/F curve points via the private SetControl (dangerous batch V/F edit; single RMW cycle; default/--freq-mode = same kHz freq offse… | 未实现：macOS RM 在访问驱动前拒绝 |
| `set-public-vftable-point-offset` | Set one VFP point delta in MHz; --import-csv PATH (with no positionals) applies a whole CSV curve (voltage,frequency,delta,default_frequency) inste… | 未实现：macOS RM 在访问驱动前拒绝 |
| `set-public-vftable-range-offset` | Set a VFP point range delta in MHz | 未实现：macOS RM 在访问驱动前拒绝 |
| `sync-vfp-memory-pstate` | Copy the memory VFP second-stage curve onto P0 (same core path as auto-optimizer's sync-vfp-memory-pstate) | 未实现：macOS RM 在访问驱动前拒绝 |

### 性能模式与 API 策略（`Perf`，9 项）

| 命令 | 原 NVOC 行为（摘要） | macOS RM 状态 |
| --- | --- | --- |
| `get-autoboost-status` | Read NVML auto-boost state | 未实现：macOS RM 在访问驱动前拒绝 |
| `get-autoboost-support` | Read NVML API restriction state | 未实现：macOS RM 在访问驱动前拒绝 |
| `get-pmgr-arbiter` | Read the PMGR voltage-request arbiter values (0x717648FD, escape 0x0700019F) | 未实现：macOS RM 在访问驱动前拒绝 |
| `reset-autoboost-status` | Set NVML default auto-boost state | 未实现：macOS RM 在访问驱动前拒绝 |
| `reset-perf-freq-caps` | Clear the GPU frequency perf-cap (PerfLimitsSetStatus, enable=0 on both entries;) | 未实现：macOS RM 在访问驱动前拒绝 |
| `set-autoboost-status` | Set NVML auto-boost state | 未实现：macOS RM 在访问驱动前拒绝 |
| `set-autoboost-support` | Set NVML API restriction state | 未实现：macOS RM 在访问驱动前拒绝 |
| `set-perf-freq-caps` | Set the GPU frequency perf-cap in MHz (PerfLimitsSetStatus; clamp perf max/min freq; --min for the lower bound, default both bounds equal). Use res… | 未实现：macOS RM 在访问驱动前拒绝 |
| `set-pmgr-arbiter` | Set the PMGR voltage-request arbiter values (0x9C4BB8D0; admin; GET-patch-SET RMW recommended) | 未实现：macOS RM 在访问驱动前拒绝 |

### OC Scanner（`Scanner`，1 项）

| 命令 | 原 NVOC 行为（摘要） | macOS RM 状态 |
| --- | --- | --- |
| `oem-oc-scanner` | Control NVIDIA's driver-side (OEM) OC Scanner: --start (driver scans in background and applies V/F offsets itself), --stop, --revert (restore pre-s… | 未实现：macOS RM 在访问驱动前拒绝 |


（共 101 条 CLI 命令）

## 二、非 CLI 消费者接入状态

| 消费者 | 现状 | 依据 / 缺口 |
| --- | --- | --- |
| `nvoc-core` | 已实现：`BackendSet::MacosRm` 分发 + `discover()`/`QueryMacosRmInfo`/`QueryMacosRmStatus`/`MacosRmSession` | 阶段 1 实机 5 项硬件测试通过 |
| `nvoc-cli` | 部分：见上表（4 命令 + 连续监控） | 其余命令在访问驱动前拒绝 |
| `nvoc-python`（pynvoc） | 未实现：发现/查询仍走 NVAPI/NVML 路径 | 阶段 11；需要 macOS 分发与字段/单位归一化 |
| `gui` | 未实现：未接 macOS 能力查询与面板 | 阶段 11；不得只靠 CLI 宣称界面可用 |
| `tui` | 未实现 | 阶段 11 |
| `srv`（ControlBackend） | 未实现：无 macOS 工厂/实现 | 阶段 11 |
| `auto-optimizer` | 未实现：入口按 `has_nvapi` 筛选，且依赖 CUDA/OpenCL 验证器 | 阶段 10；需要非 CUDA 构建与 macOS 负载/验证器 |
| `cli-stressor-cuda-rs` / `cli-stressor-opencl` | 未接入 macOS | 阶段 9：需先实测本机 Metal/Vulkan/OpenCL 计算能力 |
| `probe-macos-rm` | 已存在（只读探针 + CPU 回归），阶段 1 追加 ABI 契约测试 | 不整组运行 stage 3（含写入） |

## 三、阶段路线图现状（0→12）

| 阶段 | 目标 | 现状（2026-10-17） | 下一步 |
| --- | --- | --- | --- |
| 0 基线/功能表 | 起点、边界、待办、证据索引 | 完成：本文件 + SSH/硬件/分支核对 + 101 条命令表 | 随每阶段更新并复核 |
| 1 正确性与资源生命周期 | 修 ABI/释放顺序/旧数据/重连/输出/停止 | **完成**：`78d299d`、`a687453`、`8fb1aeb`；实机 5 项硬件测试、ABI 指纹、broken-pipe、SIGINT 全通过 | 保持回归 |
| 2 基础信息与遥测 | 设备/信息/状态/连续监控 + 公共字段 | 部分：8 项遥测已实测；缺电压、显存用量、逐风扇/%（本机单风扇）、有效频率、限频原因；1 Hz ≥10 分钟长跑未做；core/Python 分发未接 | 补字段与 `--interval 1` 长跑，再补 core/pynvoc 归一化 |
| 3 当前设置/范围/恢复基础 | `get-settings`、范围/默认值/粒度、快照/回读/恢复 | 未开始（macOS 拒绝写入） | 需要先确认 RM 只读查询结构与门槛 |
| 4 核心/显存频率控制 | 偏移 GET/SET、锁频、reset | 未开始 | 依赖阶段 3 与写入授权 |
| 5 功耗/温度/运行策略 | W/%、温度墙、Boost | 遥测可读；无写入 | 同上 |
| 6 逐风扇/模式/曲线 | 风扇模式、曲线读写 | 遥测只有最大 RPM | 同上 |
| 7 V/F、电压点、高级超频 | 曲线点/偏移/导入导出 | 未开始 | 同上 |
| 8 其余功能族 | VBIOS、EDID、限频原因、策略 | 部分：`get-vbios -i` 离线解析可用 | 逐项给依据 |
| 9 自动压力执行与验证器 | 限时负载、停止、结构化结果、正确性校验 | 未开始（已有临时 Metal 负载用于遥测响应，仅 exploratory） | 先实测计算能力与验证器 |
| 10 自动扫描/优化/结果应用 | 受限闭环、保存/应用/恢复 | 未开始 | 依赖 4-9 |
| 11 共享入口与界面集成 | pynvoc/GUI/TUI/srv 接入 | 未开始 | 契约先行的增量接入 |
| 12 全量回归与交付 | 逐项复核、契约对照、CI 与 review 材料 | 进行中：阶段 1 已交付回归与 review 说明 | 每阶段补证据 |

## 四、已知限制（本表不得当成支持声明）

- 多卡：传输只选择第一个匹配的 NVAccel/NVRM 服务；`--gpu` 仍按现有 CPU 侧索引。
- 其他机器/驱动：仅核对过上述单台 GTX 1650 SUPER + NullMoth 2.0.4 + RM 610.57.04；
  未验证其他 macOS 版本、驱动版本、Apple Silicon（不支持，仅 x86_64）或 mobile/OEM 路径。
- 资源观察只覆盖 `/dev/fd` 与 RSS；未做 Mach port / IOKit 句柄全量审计。
- RUSD 时效为策略实现（本会话推进 + 主机单调 5 s 窗口），不是驱动契约。
- 未运行：完整 workspace `--ignored`、stage 3 探针组、任何性能 SET。
