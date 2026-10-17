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

- **交付索引**：测试机 `~/Desktop/macOS-nvoc-develop/outputs/INDEX.md`（按阶段列出权威 bundle、
  review 文档与早期产物；`stage12-regression-*` 的 `citation-check.txt` 核验本表引用的 bundle
  是否真实存在）。
- 权威源码（开发端）：`/home/lyy/work/nvoc-next`，分支 `macos-driver-validation`；
  测试机 `/Users/mac/Desktop/macOS-nvoc-develop/nvoc-next` 内容与之逐文件一致。
- 环境（2026-10-17 复核）：`mac@10.253.254.125` = `macs-imac.dev.skyw.top`、
  macOS 15.8.1 / 24H32、x86_64、i5-8600K、GTX 1650 SUPER（`10de:2187`，4 GiB，
  PCI `0000:01:00.0`）、NullMoth 2.0.4、RM/OGKM 610.57.04、无 sudo。
- 代码依据：`core/native/macos_rm.c`、`core/native/macos_rm_abi.h`、
  `core/native/rusd_read.h`、`core/src/macos_rm.rs`、`probe-macos-rm/src/*`、
  `docs/design/macos-rm-backend.md`、`docs/reverse-engineering/macos-gtx1650s-access-2026-10-16.md`。
- 实测日志（仓库外 `outputs/`）：`stage1-*`（阶段 1 验收：build/fmt/clippy/tests/5 项硬件/
  ABI/探针/CLI 停止与管道）、`stage3-settings-20261017T035711Z`（**阶段 3 只读验收：21 项
  rc=0**，含 `get-settings` 人类/JSON、pynvoc `query_settings`、显式 Tier-B 只读探针
  `--tierb-read`）、`stage2-telemetry-20261017T034627Z`（**阶段 2 全量验收：
  build/fmt/clippy/tests/5 项硬件/rusd-cpu/rusd-abi/native-lint/probe-lint/probe-tests/
  pynvoc-tests/pynvoc-extension/pynvoc-hw/CLI(JSONL、人类、管道、SIGINT、坏 interval)/Metal 负载，
  全部 rc=0**）、`continuous-*`（单次与连续基线）、`continuous-watch-*`
  （3 分钟只读 + 时钟域探针）、`continuous-load-*`（温和 Metal 负载响应）、
  `stage2-longrun-20261017T025708Z`（**1 Hz × 10 分钟 = 600 样本**：gap p50 1.008 s /
  p95 1.011 s / max 1.012 s，采样耗时 p50 267 µs / p95 283 µs / max 1872 µs，fd 恒为 8，
  RSS 5412→6636 KB 后稳定，600/600 全字段 `advancing`、无 `epoch_reset`、无 `unavailable`，
  正常 `--count` 停止 exit 0）、
  `rusd-*`/`nvoc-access-*`（遥测与属性基线）、
  `stage9-vulkan-20261017T041959Z`（**阶段 9 主负载：原 Vulkan 渲染路径复用**——
  loader/ICD 自动解析、无 env 冷启动 rc=0、错误路径 rc=1、5/10/30 s 阶梯、60 s stock 负载
  同步遥测、SIGINT/SIGTERM 停止）、`stage9-stress-20261017T041107Z`
  （**阶段 9 正确性校验补充**：`stress-macos` 全量 bit-exact 校验、负例 rc=1、60 s 限时负载
  1206 dispatch/0 失败、SIGINT 0.06 s 停止 + 遥测同步采样）。
  `stage3-paramsizes-20261017T044129Z`（**阶段 3 读面修正证据**：`0x1f` 语义核对——缺方法
  `0x1b` 对照、`BUS_GET_PCI_INFO` 错尺寸 `0x1f` 对照；`PERF_*` 家族分类；Tier-B 三个 GET 的
  正确参数尺寸 2444/3232/524 B 与 payload 非零字数/复读稳定性；修正后 probe 输出
  `plprobe2/5/6/7.c`）。
  `stage9-accept-20261017T045311Z`（**阶段 9 整体验收**：3 组 feature 构建 + fmt/clippy/tests、
  `--vulkan-only` 冷启动/错误路径/阶梯、`metal_check` 全量校验/负例 rc=1/限时负载、
  信号停止语义——`--vulkan-only` SIGINT rc=-2 / SIGTERM rc=-15 立即终止，`metal_check`
  `--stress 300` 收到 SIGINT 后 rc=0 优雅停止）、`stage11-shared-entry-20261017T045726Z`
  （**阶段 11 pynvoc 共享后端串验收**：`pynvoc-tests` 11 passed、release 扩展模块构建 rc=0、
  `discover_gpus("auto")`/`query_info|status|settings(...,"auto")` rc=0 且与 `"macos-rm"`
  解析到同一后端、字段集一致）、`stage10-build-20261017T050001Z`（**阶段 10 构建边界**：
  `--no-default-features --features stressor-external` 的 check/build rc=0；`--help` 正常，
  `optimize` 以 NVAPI/NVML 缺库告警退出不 panic）、`stage10-eval-20261017T050525Z`
  （**阶段 10 只读评测**：`macos-eval` 20 s 实机 rc=0——GFX 1950→1965 MHz、util 均值 87%/峰值 99%、
  功耗 12.5→13.5 W、温度 52→57.6 ℃、风扇升速；`stress-macos --verify` pass；
  `settings_written=false`；坏参数/坏路径 rc=1）。
  `stage11-srv-macos-20261017T052753Z`（**阶段 11 srv 只读后端构建/测试**：
  `cargo fmt --check -p nvoc-srv`、`cargo check -p nvoc-srv --all-targets`（macOS）、
  `cargo test -p nvoc-srv`（**52 passed**，含 5 个新增 macOS 映射/拒绝测试）、
  `cargo clippy -p nvoc-srv --all-targets` 全部 rc=0；首次 `check` rc=101 的
  `&mut Box<dyn C>` unsize 报错与最小修复记录在同目录 `stage11-srv-macos-20261017T051729Z`）、
  `stage11-srv-smoke-20261017T052801Z`（**阶段 11 srv 实机只读 smoke**：配置故意写
  `mode="pid"` → 启动 `WARN` 归一为 `Auto`；15/15 断言 PASS——`/status`/`/api/status` 活温度、
  `/api/info` 带 `backend:"macos-rm"`/`read_only:true`、`/api/history` 1 Hz 样本、
  `POST /mode?value=pid|manual` 与 `/fan` 返回 **409**、`/api/oc/*` 与 `/api/reset` 返回
  `ERR: read-only macOS backend…`、`/oc_global` 被控制循环拒绝、`/api/vfcurve` 502、
  `/shutdown` 后进程 **exit 0**；两个 bundle 均附 `srv-shas.txt` 六个源文件 sha256）；
  `stage12-regression-20261017T052812Z`（**阶段 12 全量回归**：17 项 rc=0——fmt/workspace check/
  clippy(workspace)/core 89+cli 80+srv 52+stressor+optimizer 61+pynvoc 11 passed/硬件只读测试/
  native-lint/rusd-abi/rusd-cpu/probe-lint/probe-tests/metal-verify/cli 构建，并含功能表引用
  bundle 的存在性核对 `citation-check.txt`：9/9 命中）。

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
| `get-settings` | Read NVAPI overclock settings | **部分已实现且实测**：只读配置投影（P-state、enforced/requested 功耗上限、PCIe 代际、board/SKU、显存总量与用量）+ `capabilities{settings_read:true, settings_write:false, settings_restore:false}` + `unavailable`/`notes` 列出无法读取的族与探针证据；偏移/锁/VF/风扇/温度策略不可读 |
| `get-status` | Read live GPU status (macOS RM: P-state and RUSD telemetry; unavailable fields include reasons; --interval streams JSONL samples) | **已实现且实测**：P-state + ABI v3 遥测（图形/显存/视频频率、温度族、功耗族、限频原因、逐风扇 RPM、利用率/逐引擎、PCIe 代际、显存用量）+ `--interval` 连续监控；不可用字段带原因，JSONL schema `nvoc/macos-rm-sample/3` |
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
| `nvoc-python`（pynvoc） | **部分已实现**：`backends="macos-rm"` 的 `discover_gpus`/`query_info`/`query_status`/`query_settings` 走原生 RM，单位归一化到与 NVAPI/NVML 相同的字段（`gpu_clock_mhz`/`temperature_c`/`power_w`/`power_limit_current`…），配置族缺口经 `unavailable` 暴露；写入仍拒绝。新增 `backends="auto"`/`"default"`（`37f786f`）：macOS→RM、其他→NVAPI+NVML，与 CLI `Auto` 同一规则，供 GUI/TUI 复用 | 实机扩展模块端到端见 `stage2-telemetry-…`/`stage3-settings-…`/`stage11-shared-entry-…`（`pynvoc-hw`） |
| `gui` | **部分已实现**：原生后端不再硬编码 `"both"`，`discover_gpus`/`query_info`/`query_status`/`query_settings` 统一传共享后端串 `auto`（macOS 解析为 RM 只读路径）；NVAPI 专属面板（超频/VF/风扇）在 macOS 上仍按既有路径明确失败 | 客户端 `gui/tests/` 281 passed / 13 skipped、ruff 干净；macOS 实机由 pynvoc `auto` 端到端覆盖（`stage11-shared-entry-…`）；GUI 本体未在 macOS 跑（测试机无 customtkinter） |
| `tui` | **部分已实现**：同 GUI，`NativeService` 统一传 `auto` | 客户端 `tui/tests/` 214 passed、ruff 干净；同 GUI 的 macOS 本体限制 |
| `srv`（ControlBackend） | **只读部分已实现且实测**：后端持有类型泛化为 `Box<dyn ControlBackend>`；新增 `srv/src/macos.rs::MacosRmBackend`（复用 `nvoc_core::macos_rm` 快照，`supports_writes()=false`），`/status`/`/api/status`/`/api/gpus`/`/api/info`/`/api/history`/`/config` 可用；写入在访问驱动前被拒（`/mode?value=pid|manual`、`/fan` → 409；`/api/oc/*`、`/api/reset` → `ERR: read-only…`；`/oc_global` 由循环拒绝），控制模式钉在 `Auto` | 非 macOS 路径逐字未变（`supports_writes()=true`）；macOS 上**无闭环控制**——PID/风扇/manual 需要写通道（阶段 4–7 的同一外部条件）。证据：`stage11-srv-macos-…` + `stage11-srv-smoke-…`，review `review-stage11-srv-macos-backend-20261017.md` |
| `auto-optimizer` | **部分**：`stressor-external` 非 CUDA 构建在 macOS 可编译运行；新增只读 `macos-eval`（`195e7af`）走 `BackendSet::MacosRm` 选设备 + 原 Vulkan 负载 + Metal 校验 + RM 遥测，`settings_written=false`；原 NVAPI 命令、`has_nvapi` 筛选与默认 feature 均未改 | 阶段 10 后续：候选/SET/回读/接受-回退/恢复需 macOS 写通道；见 `review-stage10-*` |
| `cli-stressor-cuda-rs`（`--vulkan-only` / `--vulkan`） | **已实现且实测**：原 Vulkan 渲染负载在 macOS 直接复用，只新增 loader/ICD 解析（`vulkan_loader`，`4993a42`）——自动使用 `/Library/GPUBundles/nvmtl/` 的 Khronos loader + NVK ICD，无需手工导出 env；`--vulkan-window` 仍限 Windows（macOS 固定 offscreen） | CUDA GEMM 与 CUDA+Vulkan 混合模式在 macOS 不可用（无 CUDA）；渲染负载不做读回比对，计算域校验见下一行 |
| `stress-macos`（新增） | **已实现且实测**：Metal 计算负载 + bit-exact CPU 参考校验（`1dcf27a`）；自检、负例、限时负载、SIGINT 停止全部通过 | 补 `--vulkan-only` 缺的**计算正确性**维度；是校验器，不是 CUDA/OpenCL 负载的等价替代 |
| `cli-stressor-opencl` | 未接入 macOS | 本机无 OpenCL 运行时；计算域正确性校验由 `stress-macos` 承担 |
| `probe-macos-rm` | 已存在（只读探针 + CPU 回归），阶段 1 追加 ABI 契约测试；阶段 3 追加显式只读 `--tierb-read`（campaign GET：rails-info/status、vfpoints），默认电池仍不发送这些 ID | 不整组运行 stage 3（含写入），不发送任何 SET |

## 三、阶段路线图现状（0→12）

| 阶段 | 目标 | 现状（2026-10-17） | 下一步 |
| --- | --- | --- | --- |
| 0 基线/功能表 | 起点、边界、待办、证据索引 | 完成：本文件 + SSH/硬件/分支核对 + 101 条命令表 | 随每阶段更新并复核 |
| 1 正确性与资源生命周期 | 修 ABI/释放顺序/旧数据/重连/输出/停止 | **完成**：`78d299d`、`a687453`、`8fb1aeb`；实机 5 项硬件测试、ABI 指纹、broken-pipe、SIGINT 全通过 | 保持回归 |
| 2 基础信息与遥测 | 设备/信息/状态/连续监控 + 公共字段 | **完成**：样本 ABI v3（14 节/41 槽）落地——限频原因、逐风扇、有效/目标/视频/SM 频率、显存用量、逐引擎利用率、PCIe 代际、多温度、功耗族全部实测；`clkThrottleInfo`/`temps`/`avgPowerUsage`/`tgpmW`/`fanCoolerStatus.rpmCurr[16]`/`procUtil`/`pciBusData` 读取生效，`POLL_MEMORY` 经实测确认 610.57.04 恒返回 `RUSD_TIMESTAMP_INVALID`（`memEcc`/`memRowRemap` 不携带）、SM 域 Turing 为 0/0 → `unavailable`；`get-status` 人类/JSON/JSONL（schema `nvoc/macos-rm-sample/3`）一致；**1 Hz × 10 分钟长跑**（`stage2-longrun-…`）+ **全量验收**（`stage2-telemetry-20261017T034627Z`，15 项 rc=0）+ Metal 负载响应（gfx 1950 MHz / 54.9 C / 12.7 W / util 91% / gr 91.5% / fan 4243 rpm）；pynvoc `backends="macos-rm"` 的 discover/info/status 已分发并归一化 | 电压读取（`get-status` 无 voltage 字段）留给阶段 3/7 的当前设置读取 |
| 3 当前设置/范围/恢复基础 | `get-settings`、范围/默认值/粒度、快照/回读/恢复 | **只读部分完成**：`get-settings --macos-rm` 输出驱动可见配置 + 能力位 + 缺口原因（core/CLI/pynvoc 三处一致）；写入族全部保持拒绝。**此前"驱动拒绝"是误判——根因是我们传错参数尺寸**（修正见 `a87876a`）：`0x1f` = `NV_ERR_INVALID_ARGUMENT`（尺寸/参数不符），对照缺方法 `0x1b`、`BUS_GET_PCI_INFO` 传错尺寸也返回 `0x1f`。据此复核：`PERF_GET_LEVEL_INFO_V2`(0x2080200b) 容量 0 → `NV_OK` 但 0 条、容量 ≥1 → `0x1f`，**驱动不提供频率域表**；Tier-B GET `rails-get-info`(0x2080B201) **2444 B**、`rails-get-status`(0x2080B202) **3232 B**、`vfpoints-get`(0x208090A2) **524 B** 在正确尺寸下被接受（`NV_OK`），但 payload 只有 16/0/1 个非零字且 `rails-get-status` 复读不一致 → **接受 ≠ 可读，布局未解码**；`edpp-limit-info`/`pmumon-clk-samples` 返回 `0x56` | 公开 SDK（610.57.04 `ctrl2080perf.h` 全量 `PERF_*`）**无任何时钟/电压偏移或 VF 编辑命令**；唯一的公开读写对是 RATED_TDP 仲裁（读 0x2080206e 实测可用，写 0x2080206f 见阶段 5）。偏移/VF 需非公开接口或另一条读通道。在此之前不声明范围/恢复 |
| 4 核心/显存频率控制 | 偏移 GET/SET、锁频、reset | 未开始：公开 RM 控制面**无任何时钟/电压偏移读写命令**（`ctrl2080perf.h` 全量 `PERF_*`；`ctrl2080volt.h`/`ctrl2080clkavfs.h` 不含命令；无 VFP/VF-point 命令）——见 `review-stage4-7-read-channel-20261017.md`。**追加证据（该 review §6）**：三个 campaign ID 在本机全部 NVIDIA 二进制（`NVRM`/`NVRMFB`/`NVAccel`/`NVRMAGDC.kext`、`libvulkan_nouveau.dylib`、`air-opt`）里 **0 命中**，而同一扫描对公开 ID 有 **387/415（93%）**命中——本机没有可反汇编的实现体 | 结构在 GPU 侧 RM（GSP/固件），需供应商资料/非公开 SDK/固件逆向；写入依赖阶段 3 与授权 |
| 5 功耗/温度/运行策略 | W/%、温度墙、Boost | 遥测可读；**RATED_TDP 仲裁读面实测可用**：`NV2080_CTRL_CMD_PERF_RATED_TDP_GET_CONTROL`(0x2080206e) 对全部 5 个 client 返回 `NV_OK`/`DEFAULT(0)`（GLOBAL/OS/PROFILE 的 vPstate=TURBO_BOOST），越界 client(5/255)→`0x1f` 证明**查找经校验**；姊妹 STATUS 命令 SDK 只给 message id（推导 id `0x2080206d`→`0x56`）。新增只读 flag `--rated-tdp-read`，证据 `stage-ratedtdp-20261017T055450Z`。仍**无写入** | 唯一公开写对 = `PERF_RATED_TDP_SET_CONTROL` 0x2080206f（client GLOBAL→`FORCE_EXCEED`=解除 RATED_TDP 越点限制，**可逆、无偏移无电压**）——本战役目前找到的**最低风险写候选**；已在 probe 中实现为 V3.4（`--write --rated-tdp-set C:A`，带 pre-record/readback/restore，证据 `stage-ratedtdp-set-gate-20261017T060217Z`；lint + CPU 回归 9/9 + 三路门控 rc=2），但**不随任何 stage 组运行、属性能写、门控未发**；EDPP/温度墙写面在公开 610.57.04 `ctrl2080perf.h/thermal.h/pmgr.h` 中**不存在** |
| 6 逐风扇/模式/曲线 | 风扇模式、曲线读写 | **逐风扇 RPM 读面已有**：RUSD 16 槽 → `fan_rpm`（最高实时值）+ `fan_rpms`（非零逐槽列表），负载中实测 `fan_rpm=1833`、`fan_rpms=[1833]`（`stage6-fan-read-20261017T060900Z`）；CLI/pynvoc 均已归一化（`cli/src/output.rs`、`nvoc-python/src/lib.rs:798`）（此前'只有最大 RPM'的说法过时）。**缺**：风扇百分比/模式、曲线读写、FanStop | 百分比/模式/曲线/FanStop 无公开 RM 读/写命令；写面按 stage 6 门槛 |
| 7 V/F、电压点、高级超频 | 曲线点/偏移/导入导出 | 未开始：`vfpoints-get`(0x208090A2) 以 524 B 参数被接受但 payload 仅 1 个非零字，非曲线数据；该 ID 及 `rails-get-*` 在公开 OGKM 610.57.04 中**完全不存在**（非公开固件控制），公开面也无 VF 命令；该 ID 在本机所有 NVIDIA 二进制里 0 命中（review §6），即结构在固件侧 | 需供应商资料/非公开 SDK/固件逆向；写入依赖授权 |
| 8 其余功能族 | VBIOS、EDID、限频原因、策略 | 部分：`get-vbios -i` 离线解析可用 | 逐项给依据 |
| 9 自动压力执行与验证器 | 限时负载、停止、结构化结果、正确性校验 | **主负载 + 校验器完成**。主负载＝原 `--vulkan-only` 渲染路径（零算法改动，仅补 loader/ICD 解析 `4993a42`）：无 env 冷启动 rc=0，5/10/30 s 阶梯与 60 s stock 负载 rc=0，错误路径（`--gpu-index` 越界 / `--pci-bus` 不匹配）rc=1，`--vulkan-window` 在非 Windows 回退 offscreen rc=0；同步遥测显示 util 峰值 100%/均值 83%、gfx 1.53→1.95 GHz、功耗 11.5→13.5 W、温度 54→62 ℃、风扇 1778→1920 rpm。校验器＝`stress-macos`（`1dcf27a`）：全量 bit-exact 比对、启动自检、负例 rc=1、60 s 限时 1206 次 dispatch/0 失败、SIGINT 优雅停止。**整体验收**（`stage9-accept-20261017T045311Z`）：3 组 feature 构建 + fmt/clippy/tests 全 rc=0；`--vulkan-only` 冷启动 rc=0、错误路径 rc=1/1/0、阶梯 rc=0；信号语义实测 `--vulkan-only` SIGINT rc=-2 / SIGTERM rc=-15（OS 默认处置、立即终止），`metal_check` `--stress 300` 收 SIGINT 后 rc=0 优雅停止。**HEAD 复跑**（`stage-vulkan-recheck-20261017T055856Z`，8/8 PASS）：无 env 冷启动 rc=0、`--gpu-index 0` 选中真设备 `GTX 1650 SUPER (NVK TU116-A)` PCI `0000:01:00.0`、越界索引 rc=1；loader=`/Library/GPUBundles/nvmtl/libvulkan.1.4.362.dylib`、ICD=`nvk_icd.json`→`libvulkan_nouveau.dylib`（**NVK，非 MoltenVK**） | 渲染负载无读回校验、`--vulkan-only` 无进程内取消（靠 SIGINT/SIGTERM 默认处置）；阶段 10 需把负载/校验器结果接进 optimizer 闭环 |
| 10 自动扫描/优化/结果应用 | 受限闭环、保存/应用/恢复 | **部分：非 CUDA 构建 + 只读评测入口落地**。`--no-default-features --features stressor-external` 的 check/build rc=0；新增子命令 `macos-eval`（`195e7af`、证据 `stage10-eval-…`）：用 `BackendSet::MacosRm` 选设备（不再要求 `has_nvapi`），复用原 `cli-stressor-cuda-rs --vulkan-only` 限时负载 + NullMoth RM 遥测采样 + `stress-macos/metal_check --verify`，输出 `nvoc/macos-stock-eval/1`（pre/under_load/post、FPS、校验结论、`settings_written=false`）。实机 20 s：gfx 1950→1965 MHz、util 均值 87%/峰值 99%、power 12.5→13.5 W、temp 52→57.6 ℃、校验 pass。**无候选/SET/回读/接受-回退/恢复**：macOS 无公开写通道，这些仍不能跑。**HEAD 复跑**（`stage10-recheck-20261017T060320Z`）：`macos-eval --duration 8` rc=0，1530→1965 MHz、功率 11.4→13.6 W、温度 52.2→59.5 C、util max 100%、fan→2517 rpm，`metal_check` passed / 0 mismatch（全量 bit-exact），`settings_written=false` | 需要 macOS 写通道或非公开接口；在此之前闭环停在"预检+原状态快照+限时负载+校验"这半 |
| 11 共享入口与界面集成 | pynvoc/GUI/TUI/srv 接入 | **pynvoc + GUI/TUI + srv 均已完成**：pynvoc 新增 `auto`/`default` 后端串（macOS→RM、其他→NVAPI+NVML，与 CLI `Auto` 同规则，`37f786f`），GUI/TUI 全部改传该串；客户端 GUI 281 passed/13 skipped、TUI 214 passed，macOS 实机 pynvoc `auto` 的 discover/info/status/settings rc=0 且与 `macos-rm` 同后端同字段集（`stage11-shared-entry-…`）。**srv** 完成只读接入：`SharedBackend` 泛化为 `Box<dyn ControlBackend>`，新增 `MacosRmBackend`（`supports_writes()=false`；`discover_backend()` 用 `cfg!` 让 macOS/NVAPI 两个分支在**每次**构建里都被类型检查），macOS 上发现设备 + 监控循环 + `/status`/`/api/*` 可用，写入在驱动前被拒、模式钉在 `Auto`——构建/测试 bundle `stage11-srv-macos-…`（52 passed）、实机 smoke `stage11-srv-smoke-…`（15/15 PASS、`/shutdown` exit 0），review `review-stage11-srv-macos-backend-20261017.md`。**追加：TUI 本体实机跑通**——测试机 user-level 装 uv + CPython 3.12.15，maturin 现场构建 `pynvoc`（abi3，`backend_macos_rm=true`），真 `nvoc-tui` 渲染真机遥测并以 1.0 s 周期刷新 29 次后 `ctrl+c` 退出 rc=0（`stage11-tui-macos-20261017T054628Z`、`review-stage11-tui-macos-20261017.md`） | macOS 上无闭环控制（需写通道）；**TUI 本体已实机跑通**（`stage11-tui-macos-20261017T054628Z`：真 `nvoc-tui` + 真 `pynvoc` + CPython 3.12.15，1 Hz × 29 刷新、`ctrl+c` rc=0），**GUI 本体已实机跑通（只读）**：改用 user-level uv CPython 3.12.15（**Tk 9.0**）后，真 `gui/main.py` 起窗（`App built (window up)`、geometry `768x712+5+30`），pynvoc `auto`→`macos-rm` 发现真机并读到功耗设置；此前 CLT Python 3.9 的 Tk 8.6 启动报错（`have instead 15 (1506)`）被绕过，无需 admin（`stage11-gui-macos-20261017T060733Z`、`review-stage11-gui-macos-20261017.md`）。GUI 的 VF 页在 macOS **惰性**（走 NVAPI 私有面、无实现，捕获异常不崩；阶段 7 无通道）；无截图（SSH 无屏幕录制权限）；写入在各入口**都在驱动之前拒绝**且都不崩——CLI/srv 用 macOS 专用文案（rc=2 / 409），`pynvoc`→GUI/TUI 落到「无 NVAPI/NVML 后端」的通用错误（`stage11-entry-parity-20261017T054912Z`），文案层不同但无写入、无崩溃 |
| 12 全量回归与交付 | 逐项复核、契约对照、CI 与 review 材料 | **本机可跑范围完成**：`stage12-regression-20261017T052812Z` **17/17 rc=0**（workspace `check`/`clippy`、core 89 / cli 80 / srv 52 / stressor 17 / optimizer 61 / pynvoc 11 passed、硬件只读 5 项、native/rusd/probe lint+测试、`metal_check --verify` 全量比对、CLI 构建与冒烟）＋功能表引用 bundle 存在性核对 **9/9**；交付索引 `outputs/INDEX.md`、阶段 review `review-stage12-regression-20261017.md`；随后清掉本工作流引入的最后一条 clippy 告警（`manual_is_multiple_of`）并按 §四已知限制做非 macOS 交叉构建（`stage12b-xplat-20261017T054019Z`）。阶段 1/2/3/9/10/11 均已有实机 bundle；阶段 4–7 有明确读写阻塞，`srv` 已从"启动即失败"变为"只读可用、写入受限" | 剩余缺口见 `review-stage12-regression-20261017.md` §5：非 macOS 平台**原生运行**证据（交叉类型检查已补：`stage12b-xplat-20261017T054019Z`，Linux/Windows 工作区 `--all-targets` 与 Linux Vulkan 路径全 rc=0）、阶段 4–7 写通道（GUI/TUI 本体 macOS 冒烟已补：`stage11-gui-macos-20261017T060733Z`、`stage11-tui-macos-20261017T054628Z`） |

## 四、已知限制（本表不得当成支持声明）

- 多卡：传输只选择第一个匹配的 NVAccel/NVRM 服务；`--gpu` 仍按现有 CPU 侧索引。
- 其他机器/驱动：仅核对过上述单台 GTX 1650 SUPER + NullMoth 2.0.4 + RM 610.57.04；
  未验证其他 macOS 版本、驱动版本、Apple Silicon（不支持，仅 x86_64）或 mobile/OEM 路径。
- 资源观察只覆盖 `/dev/fd` 与 RSS；未做 Mach port / IOKit 句柄全量审计。
- RUSD 时效为策略实现（本会话推进 + 主机单调 5 s 窗口），不是驱动契约。
- Vulkan 渲染负载（`--vulkan-only`）只报告 FPS，不读回比对；上游
  `cli-stressor-cuda-rs/src/vulkan_render.rs` 的背景 pass fragment 循环当前为空
  （`TEMP-DEBUG: loop removed entirely`，来自上游 `b0e6dd7`），负载主要来自几何/粒子/ROP。
  不能由「渲染完成」推断计算正确性——该维度只由 `stress-macos` 提供。
- `--vulkan-only` 无进程内取消/清理：SIGINT/SIGTERM 走 OS 默认处置立即终止（实测 rc=-2/-15），
  不执行 `device.destroy_device()`/teardown；只有 `stress-macos` 有进程内优雅停止。
- Tier-B campaign GET（`rails-get-info`/`rails-get-status`/`vfpoints-get`）在正确参数尺寸下返回
  `NV_OK`，但 payload 只有 16/0/1 个非零字且 `rails-get-status` 复读不稳定 → **"被接受"不等于
  "可读"**：这些返回值不是可用读数，不得据此声明范围/曲线/恢复支持。
- 阶段 11 的界面：**TUI 与 GUI 本体均已在 macOS 实机跑通**（真入口 + 真 `pynvoc` + CPython 3.12.15）：
  TUI `stage11-tui-macos-20261017T054628Z`（只读 dashboard，1 Hz × 29 刷新、`ctrl+c` rc=0）；
  GUI `stage11-gui-macos-20261017T060733Z`（真 `gui/main.py` 起窗、geometry `768x712+5+30`、
  `auto`→`macos-rm` 发现真机 + 读到功耗设置）。此前的阻塞（CLT Python 3.9 的 Tk 8.6 在 `Tk()` 报
  `macOS 15 (1508) or later required, have instead 15 (1506)`）由 **user-level uv CPython 3.12.15 的 Tk 9.0** 绕过，无需 admin。
  **仍未做人工视觉验收**：SSH 会话无屏幕录制权限，`screencapture` 失败（`could not create image from display`），
  窗口存在由 boot 标记/geometry/title 佐证。GUI 的 VF 页在 macOS **惰性**（走 NVAPI 私有面、无实现，捕获不崩）。
- 非 macOS 平台证据：`discover_backend()` 用 `cfg!`（不是 `#[cfg]`），两个平台分支在**每次**构建里
  都被类型检查；另在测试机上对 `x86_64-unknown-linux-gnu` 与 `x86_64-pc-windows-msvc` 做了交叉
  `cargo check`（Linux/Windows 工作区 `--all-targets` rc=0，Linux 上
  `cli-stressor-cuda-rs --features vulkan` rc=0；bundle `stage12b-xplat-20261017T054019Z`）。
  **这是类型检查，不是原生运行**：不链接、不执行测试、不证明这两个平台的运行时行为，
  `core/native` 的 C 也仍只在 macOS 验证。
- `srv` 在 macOS 上是**只读观测服务**（`MacosRmBackend`，`supports_writes()=false`）：
  `/status`/`/api/status`/`/api/gpus`/`/api/info`/`/api/history`/`/config` 可用，控制模式恒为
  `Auto`；`/mode?value=pid|manual`、`/fan` 返回 409，`/api/oc/*`、`/api/reset` 返回
  `ERR: read-only macOS backend…`，`/oc_global` 由控制循环拒绝。**不存在** macOS 风扇 PID 闭环、
  手动占空比、偏移/限值写入——这些需要可写 RM/固件接口（与阶段 4–7 同一外部条件）。
  非 macOS 平台走原 `NvapiBackend`，行为逐字未变。
- `auto-optimizer` 在 macOS 上：可构建、`--help`、并新增只读 `macos-eval`（真实负载 + 遥测 + 校验），
  但**不能扫描/应用/恢复**——原 `optimize`/`autoscan` 按 `has_nvapi()` 选目标且 SET 全走 NVAPI，
  本机 NVAPI/NVML 均缺库。真实闭环在拿到 macOS 写通道前无法运行，`macos-eval` 只是它的只读一半。
- 阶段 4–7 读通道调查结论（`review-stage4-7-read-channel-20261017.md`）：campaign GET
  `0x2080B201/0x2080B202/0x208090A2` 在公开 OGKM 中完全不存在（sanity grep 通过）；公开控制面里
  `ctrl2080volt.h`/`ctrl2080clkavfs.h` 不含任何命令、也无 VFP/VF-point 命令；NVAPI 私有 VF 入口在
  macOS 无运行时。偏移/VF/电压读取**需非公开固件结构**这一外部条件，在此之前相关实机操作保持门控。收尾追加证据（同 review §6，`outputs/probe-scripts/`）：这三个 ID 在本机全部 NVIDIA 二进制（NVRM/NVRMFB/NVAccel/NVRMAGDC kext、libvulkan_nouveau、air-opt）里 **0 命中**，而同一扫描技术对公开控制 ID 有 **387/415（93%）命中**（灵敏度对照：`PERF_GET_LEVEL_INFO_V2`、`BUS_GET_PCI_INFO`、`BUS_GET_INFO_V2` 均命中）→ 本机**不存在可反汇编的实现体**，params 结构在 GPU 侧 RM（GSP/固件）
- 未运行：完整 workspace `--ignored`、stage 3 探针组、任何性能 SET。

## 五、本阶段优先级与验证范围（2026-10-17 局部调整）

用户将压力测试阶段的优先级明确为：**先复用原 NVOC 的 Vulkan 压力实现（`--vulkan-only`），
Metal 校验器作为补充**。功能表据此确认选择依据与已/未实测范围如下。

- **选择依据（只读确认，未改驱动）**：
  1. 原 `cli-stressor-cuda-rs` 已有 `--vulkan-only` 路径，**不需要 CUDA**，且本身是跨平台离屏实现；
  2. 黑苹果已在 `/Library/GPUBundles/nvmtl/` 提供 Vulkan loader、`libvulkan_nouveau.dylib`
     与 `nvk_icd.json`（NullMoth 的 Metal 插件底层同样走 NVK）；
  3. 因此**不默认必须经过 MoltenVK**，也不预先断定 macOS 无法复用原 Vulkan 压力——先按能力探测推进。
- **实现方式**：只新增 macOS 的 **loader/ICD 解析与设备选择适配**（commit `4993a42`，文件
  `cli-stressor-cuda-rs/src/vulkan_loader.rs`），**原负载、参数、停止、进度与结果流程零算法改动**，
  其他平台行为不动。
- **已实测范围**：
  - **压力负载（Vulkan）**：无 env 冷启动 rc=0；5/10/30 s 阶梯与 60 s stock 负载 rc=0；
    错误路径（`--gpu-index` 越界 / `--pci-bus` 不匹配）rc=1；`--vulkan-window` 在非 Windows 回退
    离屏 rc=0；同步遥测 util 峰值 100%/均值 83%、gfx 1.53→1.95 GHz、功耗 11.5→13.5 W、
    温度 54→62 ℃、风扇 1778→1920 rpm（`stage9-vulkan-20261017T041959Z`、`stage9-accept-20261017T045311Z`）。
  - **正确性校验（Metal/CPU-reference，补充）**：`stress-macos`（`metal_check --verify`）做全量
    bit-exact 比对 + 启动自检 + 负例 rc=1；60 s 限时 1206 次 dispatch/0 失败、
    `--stress 300` 收 SIGINT 后 rc=0 优雅停止（`stage9-stress-20261017T041107Z`）。
- **范围边界（不得据此宣称稳定）**：Vulkan 负载只报告 FPS，**不读回比对**（上游背景 pass 的
  fragment 循环当前为空，见 §四）；它是**压力负载**而非正确性证据。正确性只由补充校验器给出，
  且校验器本身与超频偏移无关。**渲染成功或未崩溃 ≠ 超频稳定**。
- **HEAD 复跑证据**：`stage-vulkan-recheck-20261017T055856Z`（见阶段 9 行），确认 loader/ICD/真设备选择在当前 HEAD 仍成立。
- **失败记录要求**（若后续复用失败）：记录具体阶段、NV_STATUS/错误、设备能力与最小修复尝试；
  只在拿到明确阻塞证据后才评估替代路径。
- **安全门槛未变**：低强度/短时/可停止优先；进程级配置；不改全局驱动配置、EFI、boot-args；
  写入仍门控；客户端不部署、不自动重启、不操作 PR。
