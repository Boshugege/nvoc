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
  bundle 的存在性核对 `citation-check.txt`：9/9 命中）、
  `stage11-throttle-shared-20261017T065005Z`（**阶段 11 限频原因共享读取面**：`QueryThrottleReasons`
  在 macOS 改由 core 从 RM 快照解码，CLI 与 pynvoc 共用同一九条 NVML 词汇——`cargo check --workspace
  --all-targets` rc=0、core 74 passed、真机硬件 **6 passed**（含新
  `shared_throttle_reason_query_reports_the_nvml_vocabulary`，按原始 mask 位独立核对 active）、
  `nvoc-cli` 80 passed、pynvoc 12 passed；实机 `--macos-rm get-throttle-reasons` 人类/JSON 均 rc=0、
  pynvoc `query_throttle_reasons(gpu,"macos-rm"|"auto")` 返回九条原因且与 `throttle_reason_mask=0`
  一致、默认 `both` 得到单条 macOS 只读文案）、
  `stage12-recheck-20261017T070215Z`（**限频原因共享读取面之后的阶段 12 复核**：18 项 rc=0——fmt/
  `check --workspace --all-targets`/clippy-workspace、core 74 / cli 80 / srv 52 / stressor 14+17 /
  optimizer 61 / pynvoc 12 passed、真机只读硬件 **6 passed**（14.58 s）、native-lint/rusd-abi/rusd-cpu/
  probe-lint/probe-tests（Ran 11 OK）/metal-verify/build-cli、功能表引用 bundle 存在性 24/24；
  首跑 probe-lint/probe-tests 因 runner 未导出 `OGKM` 而 rc=1，带 OGKM 重跑 rc=0，`summary.txt` 已按
  重跑结果更正并加注）、
  `stage12-recheck-20261017T071500Z`（**eval throttle 汇总之后的复核**：`b74b604` 上 **19 项 rc=0 首跑全绿**——
  fmt/check/clippy-workspace、core 74 / cli 80 / srv 52 / stressor 14+17 / optimizer **63** / pynvoc 12 passed、
  真机只读硬件 6 passed、native-lint/rusd-abi/rusd-cpu/probe-lint/probe-tests（Ran 11）/metal-verify/build-cli、
  引用 bundle 存在性 25/25）、
  `stage-perf-read-20261017T071100Z`（**公开 perf/pmgr 读面普查** `--perf-read`：`PMGR_GET_MODULE_INFO`
  `NV_OK`（moduleId 0 / nvswitchSupport 2=NOT_APPLICABLE）、`PERF_GET_VID_ENG_PERFMON_SAMPLE` 四个引擎
  均 `NV_OK`（空闲 busy 0%、采样窗 1000000 us）；v1 level-info/GPU_IS_IDLE/Tegra 三项**显式跳过并记录理由**；
  默认只读电池回归 29 项 rc=0（V2.5/V2.7 `0x56`、V2.15 `0x1f` 与既有结论一致）；probe lint OK、
  CPU 回归 **Ran 12 tests OK**（新增 `--perf-read` 显式 opt-in 用例）；两次运行结果逐字一致）、
  `stage12-recheck-20261017T073000Z`（**读面普查后复核**：`6226ddf` 上 **19 项 rc=0 首跑全绿**——fmt/check/clippy-workspace、core 74 / cli 80 / srv 52 / stressor 14+17 / optimizer 63 / pynvoc 12 passed、真机只读硬件 6 passed、native-lint/rusd-abi/rusd-cpu/probe-lint/probe-tests（**Ran 12**）/metal-verify/build-cli、引用 bundle 存在性 **26/26**）。

## 一、CLI 命令表（101 条，来源：`cli/src/lib.rs` 的 `CommandSpec`）

表中"原 NVOC 行为"取自命令自身的说明（`nvoc-cli <命令> --help` 是完整参数来源）。

### GPU 与显示信息（`Info`，13 项）

| 命令 | 原 NVOC 行为（摘要） | macOS RM 状态 |
| --- | --- | --- |
| `clear-edid` | Clear display EDID through NVAPI | 未实现：macOS RM 在访问驱动前拒绝 |
| `get-display-list` | List NVAPI display IDs for EDID operations | 未实现：macOS RM 在访问驱动前拒绝 |
| `get-edid` | Read display EDID through NVAPI | 未实现：macOS RM 在访问驱动前拒绝 |
| `get-gpu-list` | List discovered GPUs and available backends | **已实现且实测**：设备发现/身份（`get-gpu-list`） |
| `get-info` | Read NVAPI GPU identity and capability information | **已实现且实测**：PCI 身份/子系统/rev、显存总量/可用/BAR1/L2、驱动版本、board id/chip SKU/project（`BIOS_GET_SKU_INFO`）；**无 VBIOS 版本字符串**（见阶段 8：公开面不可得） |
| `get-settings` | Read NVAPI overclock settings | **部分已实现且实测**：只读配置投影（P-state、enforced/requested 功耗上限、PCIe 代际、board/SKU、显存总量与用量）+ `capabilities{settings_read:true, settings_write:false, settings_restore:false}` + `unavailable`/`notes` 列出无法读取的族与探针证据；偏移/锁/VF/风扇/温度策略不可读 |
| `get-status` | Read live GPU status (macOS RM: P-state and RUSD telemetry; unavailable fields include reasons; --interval streams JSONL samples) | **已实现且实测**：P-state + ABI v3 遥测（图形/显存/视频频率、温度族、功耗族、限频原因、逐风扇 RPM、利用率/逐引擎、PCIe 代际、显存用量）+ `--interval` 连续监控；不可用字段带原因，JSONL schema `nvoc/macos-rm-sample/3` |
| `get-throttle-reasons` | Read throttle reasons (NVML vocabulary; macOS RM-backed) | **已实现且实测**：RUSD reason mask → 9 条 NVML 原因/顺序 + `active` 标志，`violation=null`（RM 无 per-policy 计数）；证据 `stage8-throttle-reasons-20261017T064017Z`。**2026-10-17 改为共享实现**：core 的 `QueryThrottleReasons` 在 macOS 从 RM 快照解码，pynvoc `query_throttle_reasons` 同源（`stage11-throttle-shared-20261017T065005Z`） |
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
| `nvoc-python`（pynvoc） | **部分已实现**：`backends="macos-rm"` 的 `discover_gpus`/`query_info`/`query_status`/`query_settings`/`query_throttle_reasons` 走原生 RM，单位归一化到与 NVAPI/NVML 相同的字段（`gpu_clock_mhz`/`temperature_c`/`power_w`/`power_limit_current`…），配置族缺口经 `unavailable` 暴露；写入仍拒绝。新增 `backends="auto"`/`"default"`（`37f786f`）：macOS→RM、其他→NVAPI+NVML，与 CLI `Auto` 同一规则，供 GUI/TUI 复用 | 实机扩展模块端到端见 `stage2-telemetry-…`/`stage3-settings-…`/`stage11-shared-entry-…`/`stage11-throttle-shared-20261017T065005Z`（`pynvoc-hw`） |
| `gui` | **部分已实现**：原生后端不再硬编码 `"both"`，`discover_gpus`/`query_info`/`query_status`/`query_settings` 统一传共享后端串 `auto`（macOS 解析为 RM 只读路径）；NVAPI 专属面板（超频/VF/风扇）在 macOS 上仍按既有路径明确失败 | 客户端 `gui/tests/` 281 passed / 13 skipped、ruff 干净；macOS 实机由 pynvoc `auto` 端到端覆盖（`stage11-shared-entry-…`）；GUI 本体未在 macOS 跑（测试机无 customtkinter） |
| `tui` | **部分已实现**：同 GUI，`NativeService` 统一传 `auto` | 客户端 `tui/tests/` 214 passed、ruff 干净；同 GUI 的 macOS 本体限制 |
| `srv`（ControlBackend） | **只读部分已实现且实测**：后端持有类型泛化为 `Box<dyn ControlBackend>`；新增 `srv/src/macos.rs::MacosRmBackend`（复用 `nvoc_core::macos_rm` 快照，`supports_writes()=false`），`/status`/`/api/status`/`/api/gpus`/`/api/info`/`/api/history`/`/config` 可用；写入在访问驱动前被拒（`/mode?value=pid|manual`、`/fan` → 409；`/api/oc/*`、`/api/reset` → `ERR: read-only…`；`/oc_global` 由循环拒绝），控制模式钉在 `Auto` | 非 macOS 路径逐字未变（`supports_writes()=true`）；macOS 上**无闭环控制**——PID/风扇/manual 需要写通道（阶段 4–7 的同一外部条件）。证据：`stage11-srv-macos-…` + `stage11-srv-smoke-…`，review `review-stage11-srv-macos-backend-20261017.md` |
| `auto-optimizer` | **部分**：`stressor-external` 非 CUDA 构建在 macOS 可编译运行；新增只读 `macos-eval`（`195e7af`）走 `BackendSet::MacosRm` 选设备 + 原 Vulkan 负载 + Metal 校验 + RM 遥测，`settings_written=false`；报告新增 `under_load.throttle`（`50bb01b`：负载期间 reason mask 的按位并集 + 断言过的原因名 + 实际带 mask 的样本数——空 `active_reasons` 只表示"驱动没断言"，不等于"没读"）；原 NVAPI 命令、`has_nvapi` 筛选与默认 feature 均未改 | 阶段 10 后续：候选/SET/回读/接受-回退/恢复需 macOS 写通道；见 `review-stage10-*` |
| `cli-stressor-cuda-rs`（`--vulkan-only` / `--vulkan`） | **已实现且实测**：原 Vulkan 渲染负载在 macOS 直接复用，只新增 loader/ICD 解析（`vulkan_loader`，`4993a42`）——自动使用 `/Library/GPUBundles/nvmtl/` 的 Khronos loader + NVK ICD，无需手工导出 env；`--vulkan-window` 仍限 Windows（macOS 固定 offscreen） | CUDA GEMM 与 CUDA+Vulkan 混合模式在 macOS 不可用（无 CUDA）；渲染负载不做读回比对，计算域校验见下一行 |
| `stress-macos`（新增） | **已实现且实测**：Metal 计算负载 + bit-exact CPU 参考校验（`1dcf27a`）；自检、负例、限时负载、SIGINT 停止全部通过 | 补 `--vulkan-only` 缺的**计算正确性**维度；是校验器，不是 CUDA/OpenCL 负载的等价替代 |
| `cli-stressor-opencl` | 未接入 macOS | 本机无 OpenCL 运行时；计算域正确性校验由 `stress-macos` 承担 |
| `probe-macos-rm` | 已存在（只读探针 + CPU 回归），阶段 1 追加 ABI 契约测试；阶段 3 追加显式只读 `--tierb-read`（campaign GET：rails-info/status、vfpoints），默认电池仍不发送这些 ID | 不整组运行 stage 3（含写入），不发送任何 SET |

## 三、阶段路线图现状（0→12）

| 阶段 | 目标 | 现状（2026-10-17） | 下一步 |
| --- | --- | --- | --- |
| 0 基线/功能表 | 起点、边界、待办、证据索引 | 完成：本文件 + SSH/硬件/分支核对 + 101 条命令表 | 随每阶段更新并复核 |
| 1 正确性与资源生命周期 | 修 ABI/释放顺序/旧数据/重连/输出/停止 | **完成**：`78d299d`、`a687453`、`8fb1aeb`；实机 5 项硬件测试、ABI 指纹、broken-pipe、SIGINT 全通过 | 保持回归 |
| 2 基础信息与遥测 | 设备/信息/状态/连续监控 + 公共字段 | **完成**：样本 ABI v3（14 节/41 槽）落地——限频原因、逐风扇、有效/目标/视频/SM 频率、显存用量、逐引擎利用率、PCIe 代际、多温度、功耗族全部实测；`clkThrottleInfo`/`temps`/`avgPowerUsage`/`tgpmW`/`fanCoolerStatus.rpmCurr[16]`/`procUtil`/`pciBusData` 读取生效，`POLL_MEMORY` 经实测确认 610.57.04 恒返回 `RUSD_TIMESTAMP_INVALID`（`memEcc`/`memRowRemap` 不携带）、SM 域 Turing 为 0/0 → `unavailable`；`get-status` 人类/JSON/JSONL（schema `nvoc/macos-rm-sample/3`）一致；**1 Hz × 10 分钟长跑**（`stage2-longrun-…`）+ **全量验收**（`stage2-telemetry-20261017T034627Z`，15 项 rc=0）+ Metal 负载响应（gfx 1950 MHz / 54.9 C / 12.7 W / util 91% / gr 91.5% / fan 4243 rpm）；pynvoc `backends="macos-rm"` 的 discover/info/status 已分发并归一化。**追加（公开温度读面）**：`NV2080_CTRL_CMD_THERMAL_SYSTEM_EXECUTE_V2`(0x20800513，ctrl2080thermal.h，A-tree) 只读批实测可用——1 target(GPU)/1 sensor、读数 45–47 °C、range [0,127]，与 RUSD `temperature_c` 一致（`stage-thermal-read-20261017T061320Z`，新增只读 flag `--thermal-read`） | 电压读取（`get-status` 无 voltage 字段）留给阶段 3/7 的当前设置读取 |
| 3 当前设置/范围/恢复基础 | `get-settings`、范围/默认值/粒度、快照/回读/恢复 | **只读部分完成**：`get-settings --macos-rm` 输出驱动可见配置 + 能力位 + 缺口原因（core/CLI/pynvoc 三处一致）；写入族全部保持拒绝。**此前"驱动拒绝"是误判——根因是我们传错参数尺寸**（修正见 `a87876a`）：`0x1f` = `NV_ERR_INVALID_ARGUMENT`（尺寸/参数不符），对照缺方法 `0x1b`、`BUS_GET_PCI_INFO` 传错尺寸也返回 `0x1f`。据此复核：`PERF_GET_LEVEL_INFO_V2`(0x2080200b) 容量 0 → `NV_OK` 但 0 条、容量 ≥1 → `0x1f`，**驱动不提供频率域表**；Tier-B GET `rails-get-info`(0x2080B201) **2444 B**、`rails-get-status`(0x2080B202) **3232 B**、`vfpoints-get`(0x208090A2) **524 B** 在正确尺寸下被接受（`NV_OK`），但 payload 只有 16/0/1 个非零字且 `rails-get-status` 复读不一致 → **接受 ≠ 可读，布局未解码**；`edpp-limit-info`/`pmumon-clk-samples` 返回 `0x56` | 公开 SDK（610.57.04 `ctrl2080perf.h` 全量 `PERF_*`）**无任何时钟/电压偏移或 VF 编辑命令**；唯一的公开读写对是 RATED_TDP 仲裁（读 0x2080206e 实测可用，写 0x2080206f 见阶段 5）；温度**读数范围**可由 `THERMAL_SYSTEM_EXECUTE_V2` 的 SENSOR_READING_RANGE opcode 公开读到（本卡 [0,127]）。偏移/VF 需非公开接口或另一条读通道。在此之前不声明范围/恢复 **本轮追加七（2026-10-17，静态）**：G1「当前偏移」的缺口的**精确定义**——`0x2080901b@2108` 的容器/掩码/几何全部钉死，handler 也**确实**每条被选中记录写 12 个 value dword，但 stock 下它们全是 0，因此**任何 stock 读都不能命名 offset 字段**；记录图与目录见 `gsp-method-table-and-legacy-forwarding.md` §14（详见阶段 4 行追加七）。**仍未发 SET、未提权、未重启**。 |
| 4 核心/显存频率控制 | 偏移 GET/SET、锁频、reset | 未开始：公开 RM 控制面**无任何时钟/电压偏移读写命令**（`ctrl2080perf.h` 全量 `PERF_*`；`ctrl2080volt.h`/`ctrl2080clkavfs.h` 不含命令；无 VFP/VF-point 命令）——见 `review-stage4-7-read-channel-20261017.md`。**频率读**一侧：RUSD 页（gpc/mem kHz）可用；`CLK_PMUMON_CLK_DOMAINS_GET_SAMPLES`(0x20801037) 实测 `0x56`（`stage2-read-battery-20261017T062835Z`）。**追加证据（该 review §6）**：三个 campaign ID 在本机全部 NVIDIA 二进制（`NVRM`/`NVRMFB`/`NVAccel`/`NVRMAGDC.kext`、`libvulkan_nouveau.dylib`、`air-opt`）里 **0 命中**，而同一扫描对公开 ID 有 **387/415（93%）**命中——**更正**：只证明「本机 host 二进制里没有实现体字面量」，不能推出本机无实现：同族私有 id 由 host 按 legacy 位整块转发给 **GSP 固件**，GSP 自带表内 CLK 家族 87 行 / VOLT 家族 17 行（**11:50Z 更正**：旧记的 169/151 是 `0x0a` internal 与 `0x20` PERF 两个家族，属家族误标，见文档 §3 更正框 / §13）（见本行末与 `gsp-method-table-and-legacy-forwarding.md`）。**公开读面普查（`--perf-read`，2026-10-17）**：perf/pmgr 家族每个 GET/INFO/STATUS/READ 都有结论——`PMGR_GET_MODULE_INFO`、`PERF_GET_VID_ENG_PERFMON_SAMPLE` 实测 `NV_OK`，`PERF_GET_LEVEL_INFO` v1 参数是内核指针不可发送，`PERF_GPU_IS_IDLE` 实为 P-state 激进化**通知**（属写，未发），`PERF_GET_TEGRA_PERFMON_SAMPLE` Tegra-only；且 `ctrl2080volt.h`/`ctrl2080clkavfs.h`/`ctrl2080fan.h`/`ctrl2080pmumon.h` 的 `NV2080_CTRL_CMD_*` 定义数均为 **0**、`ctrl2080power.h` 只有 GC6_ENTRY/EXIT——公开 SDK **没有任何**电压/风扇/功耗上限读命令；同一调查的另一半——**RUSD 共享页 `NV00DE_SHARED_DATA` 的 18 个字段全部列出**（15 项已解码进 status，memEcc/memRowRemap/avgPowerUsage 驱动不携带），**没有一个字段承载偏移/电压/VF/风扇控制状态**——三条已知读通道逐条排查完毕（`stage-perf-read-20261017T071100Z`）。**私有 ClockClient/VfPoints 读面普查（`--clkvf-read`，2026-10-17）**：把 Windows 后端读时钟域/偏移/VF 用的那批私有 GET 直接发到 macOS RM，并带**阴性对照**锚定判读（`0x2080ffff`→全尺寸 `0x1b`、`0x2080a0ff`→全尺寸 `0x56`，证明 `0x1f` 不是"未知命令"而是**参数块校验**）。结果：①`0x20809061`（582.41 编号的 VFP GET_INFO）在 **paramSize 776**（8 B 头 + 32×24 B 记录）被接受、复读逐字一致，payload 解出 **2 个头 mask（0xFF / 0x7FF）+ 11 条已填记录**——每条含 tag `0x0303`、槽位标记、一个 **kHz 量级的值**（1950000 / 1860000 / 1964000 / 404488 / 6000472 / 1380000 / 810000 / 108000 / 540000 / 1800000）与两个带符号尾字段；②`0x2080B213`（RAILS_GET_CONTROL）在 **1036**（12 B 头 + 32×32 B）被接受，payload = mask `0xFF` + 全零（与"stock 全零"语义一致）；③**R610 编号那批**（`CLK_MEASURE_FREQ` 0x20809006、`CLK_DOMAINS_GET_CONTROL` 0x2080901b、`VFP_GET_INFO/STATUS/CONTROL` 0x20809021/22/23、`VFE_EQU/VAR_GET_*` 0x2080A0B1/B3/B5/B6）在**全部 60 档尺寸**（4 B…4 343 300 B，含文档 magic 0x10964/0x261A4/0x1182C/0x12420 与 esc 缓冲 0x98240/0x100440）**一律 `0x1f`** → 这些命令**在本驱动里存在**（否则会像阴性对照那样回 0x1b/0x56）、只是本扫描没构造出它要的 header/stamp；**两个可接受面都未解码，不当作读数**（`stage-clkvf-read-20261017T072432Z`）。**第二步（版本戳扫描，2026-10-17）**：那批私有控制的参数块带 `(version<<16|size)` 戳（`0x10964` = "V1, 2404 B"，不是字节数），所以"`0x1f` 满堂"可能是缺戳而非缺尺寸——于是对同一批 id 再发 11 个已知戳 `0x10964/0x261A4/0x1182C/0x12420/0x474604/0x14420/0x620C0/0xB3F3C/0x149224/0x1000C/0x10024`（`dword[0]=magic`、`paramsSize = magic & 0xFFFF`）：**没有任何 id 因带戳而被接受**，全部仍 `0x1f`；两个可接受面（776/1036）与两个阴性对照（`0x1b`/`0x56`）均不变 → 下一步变量在**参数块内容/对象或接口层**，不是尺寸也不是已知戳（`stage-clkvf-stamped-20261017T072941Z`）   **文档封送 pass（`--clkdoc-read`，2026-10-17）**：不再用尺寸扫描，改为每个面只发**一个**文档三元组（paramSize, dword[0] 戳, 输入 seed）。结果：`0x20809019`(2520/0x109D8、296620/0x486AC、329388/0x506AC)、`0x2080901b`(2404/0x10964 + mask@+8 seed 0/0xFF/全1、V2 24996/0x261A4)、`0x20809006`(32/0x10020 + domain 0/1/2/4、V2 0x20020)、`0x20809061` 大档(493060/0x78604、83996、369796/0x5A484、157692、249844)、`0x20809062`(2000388/0x1E8604)、`0x20809023`(4343300/0x474604 + 82976/401472/737404/1348740)、`0x20809025`(3148/0x10C4C)、`0x20809026`(2508/0x109CC + mask seed)、`0x2080B202`(3212) **全部 `0x1f`**；**两个面取得可用读数**：①`0x20809061`@**776**（8 B 头 + 32×24 B 记录）= 11 条**每域 kHz 表**（bit3=405000 与该卡显存空载 405 MHz 吻合、bit4=6000984≈GDDR6 6 GHz、bit0/1/2 在 1455–1965 MHz 摆动），seed 被驱动覆写（不是 mask-seeded）；**负载 A/B 已定性：是实时读数**（`0x1` 与 RUSD GPC 15/15 逐样本相等、`0x10` 与 RUSD MCLK 15/15 相等且负载下不动，见下条）；②**`0x2080B201`@2444**（=12+32×76）与 @500008 **NV_OK**，稳定返回 mask 0xFF + 一条含 **800000 µV / 756250 µV** 的记录，3 个 seed 变体 payload 逐字相同（驱动无条件填充），其余 31 条记录为空 ⇒ rail 域信息**可读**；判据是 `(id,size,stamp,seed)` 组合而非 id 存在性（**推翻**此前口径）；`0x2080B213`@1036 仍只回 mask 0xFF + 32×32 B 全零记录。另：这些私有 id 在本机 NVRM/air-opt/libvulkan_nouveau **字面量 0 命中**却被驱动接受（公开 id 命中 1 次）⇒ **二进制字面量搜索不能证「未实现」** **频率域范围读（新，2026-10-17）**：`PERF_GET_LEVEL_INFO_V2`(0x2080200b@780) 是活表里唯一的时钟形态命令，实测 `NV_OK` 但 **0/32 条 clk-info**（`--clklevel-read`）⇒ 不能供范围 UI；`0x2080901b` 的 offset/range 解码器保持未用（该 id 仍 `0x1f`；进一步在**去掉 NVAPI 层 magic**、按 2404 / 2348 / 2316 / 2444 四个**有依据的推导尺寸**重试后仍全部 `0x1f`，且缓冲未被驱动改写，`--clkdom-read`，`stage-clkdom-20261017T091500Z`）；目前唯一可用的频率读仍是**未归属**的 `0x20809061@776` 实时 kHz 表（无 offset/range 字段）。**活方法表 + 分发链（新，2026-10-17）**：本机 `com.nullmoth.NVRM` 是 NullMoth 用 OGKM 610.57.04 源码重编（版本串 `NVIDIA UNIX Open Kernel Module for x86_64 610.57.04 Release Build (NullMoth Systems)`，磁盘 UUID 与 `kmutil showloaded` 一致）；客户端控制经 `NV_ESC_RM_CONTROL`→`_rmapiRmControl`→`rmapiutilGetControlInfo`（NVOC 导出表**二分查找**、handler 前**精确 paramSize 比较**，`control.c:445` 与 `:448-458`）。从加载镜像提出 **604 条 (methodId,paramSize,name) 活表**（**更正 §16：同法放宽后 **608** 条**，多出 4 条 `flags==0` 的真实行 `0x20801350@1`/`0x20801351@544`/`0x20802204@32`/`0x20803124@80`，均不在私有族；私有 `0x208090xx`/`0x2080b2xx`/`0x2080d0xx` 在 host 表仍 **0 命中**）（`outputs/method-table-nvrm-610.57.04.tsv`；工具 `probe-macos-rm/tools/extract-nvrm-method-table.py`）：**CLK(cat10) 仅 1 条**（0x20801037/6024）、**VOLT(cat32)=0 条**、**legacy non-privileged CLK/VOLT(cat90/b2)=0 条**、**风扇=0 条**、**THERMAL 仅 1 条**（0x20800513/1432）——把「公开头没有」升级为「**本驱动自己的编译方法表里没有**」。同表还证明 **`0x56` ≠ 不在表**：已注册的内部 PM 读（0x20800ace/a80/ad4/a81，精确尺寸）实测 `0x56`（handler 存在但拒绝）。**应答者已定位＝GSP 固件（2026-10-17，推翻此前「尚未定位」）**：`0x20809061@776`、`0x2080B201≥2444` 不在 host 活表内、在全栈 host 二进制里 **u32=0/u16=0/ASCII=0**、且接受**尺寸敏感**——这三件事同时成立的原因不是「有隐藏 handler」，而是 **host 根本不查表**：`NV_ESC_RM_CONTROL` → `_nv04ControlWithSecInfo` → `entry_points.c:498 RmDeprecatedGetControlHandler` → 不在 8 条 host NVOC 表内则走 `IsGssLegacyCall(cmd)=!!(cmd & 0x8000)`（`rmapi_deprecated.h:41`，受 `IS_GSP_CLIENT(pGpu)` 保护，`rmapi_deprecated_control.c:95/133`）→ `rmapi_gss_legacy_control.c:38 RmGssLegacyRpcCmd`（源码注释即「这些在 RM 里已不支持，直接全部转给 GSP 让它判断有效性」）→ 只做 空指针一致性 + 特权位 `0xC000` 判权 + `pRmApi->Control(PHYSICAL)` 翻译转发。**故判据是「id & 0x8000」而非表存在性，`paramSize` 由 GSP 自己校验**——此前「本机无 GSP 固件」是错的：固件在 `/Users/Shared/nvfw/nvidia/610.57.04/gsp_tu10x.bin`（前述搜索路径之外）。同一解释一次性说明全部旧异常：`0x2080FFFF→0x1b` 是特权位 `0xC000`×非 root（**权限答案，不是命令不存在**）、`0x2080A0FF→0x56` 是转发成功但 GSP 无此法、`--service nvrm/nvaccel` 逐字节相同（同一 GSP）。**GSP 自带导出表已提取**（新工具 `probe-macos-rm/tools/extract-gsp-method-table.py`，`gsp_tu10x.bin` ELF64/RISC-V `e_machine=243`，`.fwimage` @0x40 size 0x1c01dc8 base 0x4000000）：**889 行**（**更正 2026-10-17 §16：实为 1118 行**——提取器的 `flags<0x10000` 守卫静默丢了 229 条真实行；见本表末尾「本轮追加八」），record 32 B（`+0 id +4 paramSize +8 pClassInfo +0x10 pFunc +0x18 flags +0x1c accessRight`）；家族直方图 **CLK 87 行（`0x90` 68 读 + `0xd0` 18 写 + 公开 `0x10` 1）、VOLT 17 行（`0xb2` 13 + `0xf2` 4）**（**更正 2026-10-17 §16：CLK `0x90` **74** 读 + `0xd0` **19** 写；VOLT 仍是 13 + 4，未受影响**）（11:50Z 更正）（host 表各为 1 和 0）⇒「公开头没有 / host 表没有」不能推出「本机无实现」。权威 `paramSize`：`0x20809019=12336`(48+32×384)、`0x2080901b=2108`(60+32×64)、`0x2080901d=32680`、`0x20809025=7188`(20+32×224)、`0x20809061=776`、`0x2080b201=2444`、`0x2080b213=1036`、SET 孪生 `0x2080d01c=2108`/`0x2080d024=66060`/`0x2080f214=1036`；**`0x20809006` 与 `0x20809062` 在 GSP 表里**（**作废 2026-10-17 §16**：两条都是真实行，`paramSize` 恰为 **8** / **648**，只是 `flags` 高位置位（`0x81074a`/`0x82004a`）被提取器的 `flags<0x10000` 守卫丢掉；旧记「不在表里、8/648 不可推导」作废，这两个尺寸现在是**固件推导值**）。详见 `docs/reverse-engineering/nvapi/gsp-method-table-and-legacy-forwarding.md`（bundle：`stage-gsp-method-table-20261017T0215Z`、`stage-regcmp-20261017T081500Z`） **本轮追加（2026-10-17，只读）**：该 GSP 对 legacy 转发命令按**精确 paramSize** 校验，所以文档里 NVAPI 侧 magic （`0x10964`/`0x10C4C`/`0x109CC`/`0x261A4`）的字节数在本机一律 `0x1f`——这是**尺寸不匹配**，不是「命令不存在」。`0x20809025@7188` 已解出逐域偏移**范围**（`20 + 32×224`，mask@+4=`0x7`，3 条记录类型 3/4/5，rec+8=`1638400` kHz 最高 VF 频率、rec+20=`-18750`、rec+24=`+18750`，即 ±18.75 MHz 偏移上下限），三值与本仓 `nvapi-rs` 记录的 TU116（magic `0x10C4C`、mask `0x7`、`{1638400, -18750/+18750}`）逐字吻合、相对间距恒差 `0x24` ⇒ 同一控制器树的更宽记录版本，不是别的控制。GET→SET 配对是表事实：low byte `+1`、类别字节 `0x90`→`0xd0`（VOLT `0xb2`→`0xf2`）且 paramSize 相同 ⇒ `0x20809025/0x20809026/0x20809019` **没有 SET 孪生**（只读视图），本块内唯一与时钟域控制 GET 配对的写面是 **`0x2080d01c@2108`**（`0x2080901b@2108` 的 SET 孪生，同时是 VF 之外的简单域偏移候选）。**当前偏移与写入步长仍未取得，写入门槛未满足，本轮未发送任何 SET**；注意 `0x20809026@5332` 是**整场 wedge 的触发点**（本次触发用的是 `0xA5` 填充块；「该 id」与「该内容」无法分离，见文档 §6），因此代码把它当作**无条件下拒绝**的已知危险 id，而不是「换零背景重试」的候选；GSP 仍处 `0x62 NV_ERR_RESET_REQUIRED`（2026-10-17T09:40Z 复检）。详见 `docs/reverse-engineering/nvapi/gsp-method-table-and-legacy-forwarding.md` §5.1/§5.2/§9。**本轮追加二（2026-10-17，只读）**：①`0x2080901b@2108` 的 `+4` 是**输入 mask**——`0xA5` 背景下唯一被解释的字段；先前放 `+0`/`+8` 的零背景运行是 `NV_OK` 但记录空，因为 `+4` 的 mask 仍为 0（mask=0 被接受且不选记录），**不是**"该字段被忽略"；②该 id 的记录**只写类型字节**（`+60=08 08`、`+124=09 09`），offset/range/applied 全零 ⇒ **当前偏移仍不可读**，与 `nvapi-rs` 对 V1 handler 的描述逐字吻合；③889 行**每行一个** pFunc 且都是同一形态 thunk（`X->[0x1148]`→`vtbl[0]`→`ret->vtbl[0x38]`），SET `0x2080d01c` 走**另一对象** `X->[0x1150]` 的 `vtbl[0x28]` 且多两道前置门 ⇒ **写面不是读面的镜像**，二进制搜不到 handler 体（body 是 vtable 槽位）；④`--clkctl-hex` 扩为**全部**只读时钟块（901b 两档 mask、9025、9019、901d 最后）以成为**无损记录**（`0xA5` delta 打印报 106 留 40、报 352 留 60）；⑤`core`/`cli`/`pynvoc` 里"本机无可读 RM 控制面"的旧文案按族更正（偏移族＝范围可读、current 不可读；写侧仍 `settings_write=false`）。**本轮未发任何 SET**，GSP 仍 `0x62`（`stage-clkctl-hex-v2-20261017T100350Z` 复检：新电池首发改 901b 后整批中止）。**本轮追加三（2026-10-17，静态+只读）**：把「写面缺失」定位到**权限层**——`rmapi_deprecated.h` 的 `RM_GSS_LEGACY_MASK_PRIVILEGED = 0xC000`，私有 id 的**写半区 = 读半区 `|0x4000`**（`0x90→0xd0`），`RmGssLegacyRpcCmd` 对 `privLevel < RS_PRIV_LEVEL_USER_ROOT` **在 GSP 之前**直接回 `0x1b`；GSP 889 行里 82 条可转发的写 id 有 80 条带 `0xC000`（时钟/电压 SET 全在其中）。同时解出 SET thunk（`0x2080d01c` 的 pFunc `0x4d5c3c0`）的**五道前置门**（G1/G2→`0x56`、G3 `x+0x179`→`0x6c NV_ERR_GPU_IN_DEBUG_MODE`、G4/G5→`0x56`）⇒ **SET 实现体存在**，缺的是权限与运行时门，不是协议也不是固件实现。详见附录「本轮追加三」与 `docs/reverse-engineering/nvapi/gsp-method-table-and-legacy-forwarding.md` §2.1/§4.2/§4.4/§5.2/§9。**本轮追加四（2026-10-17）**：① **VOLT 读半区普查 + 「wedge 是按命令的」更正**——按 GSP 权威尺寸一次性读完整 VOLT(cat 0x32) 读半区后，13 个 id 里只有 `0x2080b201 @2444` 仍 `NV_OK`（16 个非零 dword、复读 0 变化、稳定），而同类别兄弟 `0x2080b202 @3232` 与 `b205/b207/b209/b210/b211/b212/b213/b214/b216/b217/b218`、`0x208090a2 @524` **全 `0x62`** ⇒ 当时据此判定「`0x62` 不能用来区分 host 实现与 GSP 派发」；**该判定已于「本轮追加五」作废**（`0x62` 恢复为「该命令需要 GSP 且转发已死」的判据，`0x2080b201` 为 host 应答）；同一次会话对 `NVRM/NVAccel/NVRMFB/NVRMAGDC/libvulkan_nouveau.dylib` 做 u32 小端扫描，`0x2080b201/b202/b213` 与 `0x20809019/901b/9026/9061` **全部 0 命中**，而同一次扫描里公开的 `0x20800102` 命中 19 次、`0x2080122a` 2 次（对照阳性）。② **上一层名字、写规则与权限前置（源码静态核对）**：把私有时钟域族在**上一层**的名字、写规则与权限前置补齐——① `nvapi-rs` 记录了该族的 NVAPI 名字/ID：`ClockClkDomainsGetInfo/GetControl/SetControl` = `0x57B5A5DF`/`0xF58938F5`/`0xD14B69CF`（`sys/src/gpu/clock.rs:1365-1375`、`:1930-1976`）；② 本机 **2108 B 块 ≠ NVAPI 调用者块（2404 B，magic `0x10964`）**：我们发的是 **RM** 块，`head+8=0x01010000` 是应答者写的常量 ⇒ **不要往 RM 块种 magic**，唯一能拒我们的尺寸门是 GSP 自己的 `paramSize`（两半都是 2108）；③ 写配方（快照→补副本→SET→回读→不符则恢复）可逐字移植，外加一条硬规则：**SET 绝不发宽 mask**（GET 才是按 mask 播种的发现手段）；④ **值槽顺序未定**：article 说 slot 0 = signed freq offset，而 Blackwell 字段表说 `VALUES[2]`（`rec+0x114`），经 §4.3 实测的 `0x24` 位移后 RM `rec+8` 与 `rec+16` **都是**当前偏移候选 ⇒ G1 仍缺，只能靠逐位 counter 图 + 单步 SET 同字段回读判定（§4.3 由此从「有首选行的排序表」降级为「候选表，槽序未定」）；⑤ V1 只 marshal 类型 `{1,3..9}`（类型 `2` 被**静默丢弃**），本机实测类型 `03`/`08`/`09` 全在其中，故首个单域写是**可定义**的，但「GET 成功」不能推出「SET 可用」；⑥ **G0 权限前置在 macOS 侧源码闭环**：`kexts/NVRM/os-xnu.cpp:174` = `(kauth_getuid() == 0)`、`entry_points.c:185`/`escape.c:381` 把它映射为 `RS_PRIV_LEVEL_USER_ROOT`、`nvstatuscodes.h:56` 确认 `0x1b` 就是这道门的返回码 ⇒ **root 应能过 host 门**；root 仍回 `0x1b` 才说明存在 **GSP 侧第二道权限**（`ctrl2080gpu.h` 也把 `0x1b` 当控制返回值）。**本轮无新增 GPU 请求、未提权、未改驱动/EFI/boot-args、未重启**；GSP 仍 `0x62`。详见 `docs/reverse-engineering/nvapi/gsp-method-table-and-legacy-forwarding.md` §11（§11.1–§11.5）。**本轮追加五（2026-10-17T11:29Z，只读实机；新 `--anchor-read`）**：用**四个已测量过的 GET**（非扫描、非新 id）把「需要 GSP」与「host 应答」分开——① `0x2080b201 @2444` = **NV_OK**（16 个非零 dword）；② `0x2080b202 @3232` = `0x62`；③ **`0x2080a0ff @776`（阴性锚：转发成功但 GSP 无此法）从 wedge 前的 `0x56` 变成 `0x62`** ⇒ **legacy 转发根本到不了 GSP**；④ `0x2080ffff @776` 仍 `0x1b`（host 权限门照常，说明 host 侧入口仍在跑）。⇒ 既然转发已死，`0x2080b201` 的 `NV_OK` **不可能**来自 GSP，**它是 host 应答**；追加四① 的「按命令 gate」结论由此**撤回**，`0x62` 重新成为「该控制需要 GSP」的判据（文档 §6.1/§7/§10 行 8 已按此更正，新增 §12）。**公开面同样中招（同窗口 A/B）**：`--reg-read` 22 条 host 已注册 id 里 7 条翻转——`perf-get-current-pstate`/`perf-get-powerstate`/`perf-level-info-v2`/`pmgr-get-module-info` （`NV_OK`→`0x62`）、`perf-get-gpumon-util-samples`/`perf-get-vid-eng-perfmon`（`0x1f`→`0x62`）、`clk-pmumon-domains`（`0x56`→`0x62`）；不变：`fb-get-static-bar1-info`/`kgr-get-gfx-gpc-tpc-info`/`kgr-get-gpc-mask`（NV_OK）、`internal-*` 与 `timer-gpu-cpu-correlation`（`0x56` 锚）、`fb-get-info-v2`/`gpu-get-info-v2`/`kgr-get-info`（`0x1f`）。stage2 电池同向：`fb-get-info-v2`/`fb-stats-get`/`perf-*`/`rated-tdp-get-control`/`gpumon`/`pmumon`/`rusd-page`/thermal 两形态 wedge 前 `NV_OK`、现全 `0x62`，而 `bios-sku-info`/`bus-get-pci-info` 仍 `NV_OK`。两个要点：**(a) `0x62` 现在先于参数尺寸校验**（两条 `0x1f`→`0x62`）⇒ 今天的「哪些 id 还答」量的是「哪些 id 绕开 GSP」，不是「哪些 handler 存在」；**(b) wedge 不是 CLK 专属、也不是私有 id 专属** ⇒ 阶段 4–7 的 VFP/VF、风扇、温度、功耗读面在本会话**同样不可推进**，全部等重启。新增代码：`--anchor-read`（`probe-macos-rm/src/nvx_suite.c` 的 `item_anchor_reads`/`nvx_stage2_anchor_read` + `main.c` 接线，四发 GET、无写、无扫描）。**本轮无任何写、未提权、未改驱动/EFI/boot-args、未重启**；GSP 仍 `0x62`。详见文档 §7/§12。**本轮追加六（2026-10-17T11:43Z，只读实机 + 静态反汇编）**：**撤回追加五的结论**——legacy 转发**没有死**，`0x2080b201` 是 **GSP 应答**，`0x62` 是「该 GSP 控制需要 reset」。判据换成一条**不依赖任何人对 `0x2080a0ff` 的解读**的实测：把 `0x2080b201` 发到**低于 2444 的尺寸**（88/164/620/**2440**），wedge 前是 `0x1f`、wedge 中变成 **`0x62`**（subdevice 与 device 两载体皆然），而**同一次运行**里一个**确凿的 host NVOC id**（`0x2080122a`）发**故意过短的 20** 仍得 **`0x1f`**（2–3 µs）⇒ **host 尺寸门与 GPU fault state 无关**，b201 的 `>=2444` 门**随 fault state 移动**、只能出自**转发之后**；而一个 legacy id 只有在**转发把它答出来**时才能同时给出 `NV_OK` **和 payload**。由此：① 之前「host 第四条通路」的待办（§12.2）**作废**——没有第四条通路要找；② `--reg-read` 里 `perf-*` 的 `0x1f`→`0x62` 与 b201 的 `0x1f`→`0x62` 是**同一现象**（转发之后 fault-state 判定先于尺寸比较），而 host 侧 `0x1f` 序列不变 ⇒ 同名状态码在两侧含义不同，**用 GPU 状态敏感性区分，不用码值**；③ **latency 不再作为证据**：host 应答与 GSP 往返在本机都在个位 µs（6–7 µs，队列+doorbell），battery 里那行已改名为「not discriminating」并在文档里声明不得引用；④ 静态反汇编**确认** `0x2080d01c` 的 SET thunk：五道门前置（G1 `*(s1+0x2050)`、G2 `*(a4+0x4f4)`、G4 `X+0x1150`、G5 `*(a0+0x20)` → `0x56`；G3 `*(x+0x179)` → **`0x6c NV_ERR_GPU_IN_DEBUG_MODE`**，已在 `nvstatuscodes.h:137` 核实），GET 用 `X+0x1148` 的 `vtbl[0x38]`、SET 用 `X+0x1150` 的 `vtbl[0x28]` ⇒ handler 体是 **vtable 槽位**、二进制里搜不到字面量。新增可复核项：`probe-macos-rm --anchor-read` 扩到**六**发（新增 `0x2080122a@20` host 短尺寸对照；verdict 改为由 b201 自身的 `NV_OK` 判「转发活着」）、`outputs/anchor-size-ctl-20261017T114330Z/`、`outputs/clkobj-wedge-20261017T114050Z/`（wedge 中 2440 的 `0x62`）、wedge 前对照 `outputs/stage-clkobj-20261017T074744Z/`。**本轮无任何写、未提权、未改驱动/EFI/boot-args、未重启**；GSP 仍 `0x62`，`0x2080901b` 等时钟域读面**只能等重启**（不是转发损坏、也不是缺实现）。详见文档 §1/§6.1/§7/§12。**本轮追加七（2026-10-17，静态：字段更正）**：`gsp-method-table-and-legacy-forwarding.md` §14 把两件事定死。① **撤回**「`0x2080901b` 只写类型字节」——那个读数来自被截断的 0xA5 delta 打印（声明 106、只留 40）；106 = 2 头 + 8 条×13 dword、`last @564` = 60+7×64+56，即 handler **每条被选中的记录写 13 个 dword**、其中 12 个 value dword 在 stock 下**就是 0**（0xA5 背景下写成 `00000000`，`rec+4/+48/+60` 保持 `a5a5a5a5`）。所以 G1 缺的是**字段身份**（12 个零值 dword 里哪个是 `offset_kHz`），不是容器、不是命令存在性——这也是「零值写入无法自证」的形式化。② `pFunc` 是**每命令一个**（889 行互不相同）的 thunk，真正的 body 在**对象 vtable 槽**（GET `obj+0x10148`→`vtbl[0x38]`、SET `obj+0x10150`→`vtbl[0x28]` 且多 5 道门，`obj = *(device+0x1ff88)`）⇒ 「版本/尺寸」是**对象**的属性而不是 id 的属性，尺寸/戳扫描**原理上**无法定出记录布局；表中 GET/SET 双半区尺寸仍是权威（`0x2080901b@2108` / `0x2080d01c@2108`，大号 V2 24996 只存在于 Windows 驱动）。③ 用本机**零背景无损 dump**解出 `0x20809019@12336` = 48 B 头 + 32×384 B **按目录顺序打包**（不是 bit-sparse），记录只用前 0x50 字节；`rec+0` 类型戳（`08 08`/`09 09`/`03 03`）、**`rec+4` = 域位**（1,2,0x10,4,8,0x100000,0x80000,0x40,0x200000,0x20，即 0,1,4,2,3,20,19,6,21,5）、`rec+40`=30000（仅 GPC）、`rec+44/+46` = 有符号 i16 对（GPC `(-1000,+1000)`、域位 4 `(-1000,+3000)`），头部 `+12/+20 = 0x0038007F` 正是在用域位掩码 ⇒ 与 `0x2080901b` 同一序、同一类型戳，**确认 `+4` mask 是「第 i 条目录项」选择器、域位是记录内数据**（故 mask 0xFF 填 8 条而只有 7 位命名域）。④ 负结果（正是「版本映射」陷阱的实例）：`nvapi-rs` 记的 R610.74 填充环读 RM `+64/+68/+80/+84/+88/+89`，**在本 GSP 记录里全是 0**——Windows 侧字段偏移**不可移植**，解码器必须对着 dump 验证。⑤ 工具侧 `81ae055`：`--clkctl-hex` **首发**改为 0xA5 背景 + `mask@+4=0xFF`（= 楔前已实测 `NV_OK` 的同一请求）并自报「与 seed 不同的 dword 数」，零背景各档仍按原序跟随 ⇒ 一次运行同时得到**写集合**与**stock 字节**。**本轮仍无任何 GPU 请求、无 SET、未提权、未改驱动/EFI/boot-args、未重启**；G5 未满足（GSP 仍 `0x62`，uptime 21h40m）。**本轮追加九（2026-10-17，静态重读已录字节）**：把 `0x2080901b` 的**价值槽类型条件性**钉死——`0xA5` 电池那 106 个被改 dword 的**精确构成是 `14（头部）+ 7×13（类型 08/09 各 12 个 value dword）+ 1（唯一一条类型 03 只写自己的戳）`**，用 `0x20809019` 目录的 `entry:domain_bit:type` 序对齐得出；同时**作废** §14.1 的「2 header dwords + 8×13」与 `gsp-method-table-and-legacy-forwarding.md` §10 的「other types 11」。**代价是上一轮的一个期待作废**：类型差**不能**把 12 个槽缩成一对，字段身份仍只由「非零 value dword」或「门控单域最小步 SET ＋ 同字段回读」给出。已登记证伪预测供重启后首发证伪。详见该文档 §18 与下方「本轮追加九」。 |
| 5 功耗/温度/运行策略 | W/%、温度墙、Boost | 遥测可读（**功耗墙 min/rated/max 窗口读面不可用**：`GET_EDPP_LIMIT_INFO` 0x20800AFD 实测 `0x56`，桌面 eDPp 语义不适用；enforced limit 走 RUSD=100 W，`stage2-read-battery-20261017T062835Z`）；**RATED_TDP 仲裁读面实测可用**：`NV2080_CTRL_CMD_PERF_RATED_TDP_GET_CONTROL`(0x2080206e) 对全部 5 个 client 返回 `NV_OK`/`DEFAULT(0)`（GLOBAL/OS/PROFILE 的 vPstate=TURBO_BOOST），越界 client(5/255)→`0x1f` 证明**查找经校验**；姊妹 STATUS 命令 SDK 只给 message id（推导 id `0x2080206d`→`0x56`）。新增只读 flag `--rated-tdp-read`，证据 `stage-ratedtdp-20261017T055450Z`。仍**无写入** | 唯一公开写对 = `PERF_RATED_TDP_SET_CONTROL` 0x2080206f（client GLOBAL→`FORCE_EXCEED`=解除 RATED_TDP 越点限制，**可逆、无偏移无电压**）——本战役目前找到的**最低风险写候选**；已在 probe 中实现为 V3.4（`--write --rated-tdp-set C:A`，带 pre-record/readback/restore，证据 `stage-ratedtdp-set-gate-20261017T060217Z`；lint + CPU 回归 9/9 + 三路门控 rc=2），但**不随任何 stage 组运行、属性能写、门控未发**（授权前审阅方案见 `probe-macos-rm/WRITE-VERIFICATION-PLAN.md`：pre-record→arm→SET→readback→restore + 中止/恢复失败判据）；EDPP/温度墙写面在公开 610.57.04 `ctrl2080perf.h/thermal.h/pmgr.h` 中**不存在**；内部 PM/PFM 族（`ctrl2080internal.h`）只读探针（`--pm-read`）四个 GET 全部 `0x56 NV_ERR_NOT_SUPPORTED`（`stage-pm-read-20261017T061658Z`）。**活表审计修正（新，2026-10-17）**：该族**写命令确实编译进去了**（`0x20800acf` SetPm1State、`0x20800ad0` UpdateEdppLimit、`0x20800ad1` UpdateTgpuLimit、`0x20800ad2` ConfigureTgpMode、`0x20800ad3` ConfigureTurboV2、`0x20800ad6` SetVpstate、`0x2080205b` SetPowerstate、`0x2080206f` RatedTdpSetControl），但**所有能观察前置状态的读孪生都回 `0x56`**（`0x20800a80/a81/ace/ad4/afd`）⇒ 没有 snapshot 证据，写仍全部门控；功耗窗口 min/rated/max 也因此无来源。见 `docs/reverse-engineering/nvapi/registered-vs-recorded-rm-ids-audit.md` |
| 6 逐风扇/模式/曲线 | 风扇模式、曲线读写 | **逐风扇 RPM 读面已有**：RUSD 16 槽 → `fan_rpm`（最高实时值）+ `fan_rpms`（非零逐槽列表），负载中实测 `fan_rpm=1833`、`fan_rpms=[1833]`（`stage6-fan-read-20261017T060900Z`）；CLI/pynvoc 均已归一化（`cli/src/output.rs`、`nvoc-python/src/lib.rs:798`）（此前'只有最大 RPM'的说法过时）。**缺**：风扇百分比/模式、曲线读写、FanStop | 百分比/模式/曲线/FanStop 无公开 RM 读/写命令（**完整 CMD 宏扫描**：无 fan/cooler/acoustic 宏，见 §四）； **活表审计反证（新，2026-10-17）**：加载镜像 604 条活表里**没有任何 fan/cooler 类别行**——整个 cat 05 只有 `THERMAL_SYSTEM_EXECUTE_V2`；全仓被引用的 114 个 `0x2080xxxx` id 中只有 49 个在表内，**且全部出自公开 SDK**，风扇唯一的 RM cmd `0x20800532` 不在其中（Windows 侧走的是 escape `0x07000198`/`0x07000038`，无 macOS 对应物）⇒ 风扇控制在 kernel RM 侧**无实现体**，不是「未解码」；逐风扇 RPM 仍走 RUSD。见 `docs/reverse-engineering/nvapi/registered-vs-recorded-rm-ids-audit.md` |
| 7 V/F、电压点、高级超频 | 曲线点/偏移/导入导出 | 未开始：`vfpoints-get`(0x208090A2) 以 524 B 参数被接受但 payload 仅 1 个非零字，非曲线数据；该 ID 及 `rails-get-*` 在公开 OGKM 610.57.04 中**完全不存在**（非公开固件控制），公开面也无 VF 命令；该 ID 在本机所有 NVIDIA 二进制里 0 命中（review §6），即结构在固件侧。**追加（私有读面普查，2026-10-17）**：Windows 后端读 VF 曲线的三个私有 GET（`VFP_GET_INFO/STATUS/CONTROL` 0x20809021/22/23，SET 孪生 0x2080D024）在本驱动**全部 60 档尺寸 `0x1f`**（=命令在、参数块未构造出，见阶段 4 行）；只有 582.41 编号的 `0x20809061`（776 B，非曲线本体）可用，**曲线读取仍未打通**（`stage-clkvf-read-20261017T072432Z`）   **文档封送 pass（2026-10-17）**：VFP `0x20809061` 的 R610 whitelist 五档全 `0x1f`，但同 id 的 **776 B 形态**可读，得到 11 条每域 kHz 表（见阶段 4 行）；`0x20809062`/`0x20809023` 全部文档 stamp 仍 `0x1f` ⇒ **曲线本体仍未打通**，但频率域读数已达（`stage-clkdoc-20261017T074256Z`）  **活方法表反证（新，2026-10-17）**：活表里 cat 0x90/0xd0（CLK legacy）、0x32/0xb2（VOLT）、0xf2（VOLT privileged）**全部 0 行** ⇒ 私有 VF/电压 id 在 kernel RM 无注册、`rmapiutilGetControlInfo` 在本机无法解析；两个被接受的形态（`0x20809061@776`、`0x2080B201@≥2444`）在已加载 host 二进制里无任何字面量——**原因已查明：host 对这类 id 只测 legacy 位 `0x8000` 后整块转发，应答者是 GSP 固件**（见阶段 4 行与 `docs/reverse-engineering/nvapi/gsp-method-table-and-legacy-forwarding.md`）；GSP 表内 VOLT 家族 **17 行**、CLK 家族 **87 行**（11:50Z 更正，旧 151/169 是 PERF/internal 家族），故私有 VF/电压 id **在 GSP 有实现体**（host 表 0 命中只说明它们不是 host 实现）。**活表审计（新，2026-10-17）**：cat 0x32(含 b2/f2 三种编码) 被引用 4 个 id（`0x2080b201/b202/b213/f214`）**在 host 活表里 0 命中**；全仓 114 个引用 id 中仅 49 个在表内且全部来自公开 SDK。**更正口径**：host 表无实现体 ≠ 本机无实现体——私有电压/VF 族经 legacy 位转发给 **GSP**，GSP 表内 VOLT 家族有 **17 行**（11:50Z 更正：旧记 151 行是 PERF 家族）（含 `0x2080b201=2444`/`0x2080b202=3232`/`0x2080b213=1036`/`0x2080f214=1036`(SET)），故这些 id **有 GSP 侧实现体**。**新的分层证据**：GSP 因私有 GET 被喂坏内容而整条 legacy 路径考机后（`0x65`→`0x62 NV_ERR_RESET_REQUIRED`），`0x2080B201@2444` **仍照常 `NV_OK` 返回 mask 0xFF + 800000/756250 µV**，而 `0x20809019/61` 全部 `0x62` ⇒ `0x2080b201` 是该族里**唯一不查 fault state 的 GSP handler**。**11:43Z 更正**：先前把这行读成「b201 不经 GSP 转发（host 应答或另一条非 legacy 通道）」是**错的**——尺寸边界实测证明它的 `>=2444` 门**随 GPU fault state 变化**（wedge 前同一请求 `0x1f`、wedge 中 `0x62`），而 host NVOC 门不随之为转移（同一次运行里 `0x2080122a@20` 仍 `0x1f`）⇒ 校验发生在**转发之后**，b201 由 GSP 应答，转发**是活的**（见 `gsp-method-table-and-legacy-forwarding.md` §7/§12 与「本轮追加六」）。见 `docs/reverse-engineering/nvapi/registered-vs-recorded-rm-ids-audit.md` **本轮追加七（2026-10-17，静态）**：VF 之前先要的是「12 个零值 dword 里哪个是 offset」——见阶段 4 行追加七与 `gsp-method-table-and-legacy-forwarding.md` §14.1/§14.3：`0x2080901b` 的 64 B 控制记录**确实被写满 12 个 value dword（stock 全 0）**，而 `0x20809019` 的同一序记录给的是**目录/范围**（`+44/+46` 有符号对）⇒ 偏移与曲线两步仍按原门槛走，VF 曲线继续后置。 |
| 8 其余功能族 | VBIOS、显示/EDID、限频原因、运行策略、ECC/OEM/多卡 | **逐项判定**（详见 `stage8-throttle-reasons-20261017T064017Z/README.md` §2）：**限频原因＝完成**——RUSD `clkThrottleInfo.reasonMask` 已进 status，专用命令 `get-throttle-reasons` 本轮在 macOS 可用（9 条 NVML 词汇/顺序、`violation=null`），且该读数现由 core 的共享 `QueryThrottleReasons` 从 RM 快照解码——CLI 与 pynvoc `query_throttle_reasons(gpu,"auto"|"macos-rm")` 同源、不会各自漂移（`stage11-throttle-shared-20261017T065005Z`）；**VBIOS＝部分**——`BIOS_GET_SKU_INFO` 已读 board/sku/project（实测 NV_OK）、`get-vbios -i` 离线解析可用，但**版本串/镜像不可得**：公开 BIOS 命令族全为读且 GET_INFO_V2 只给 REVISION 数字，内核接口无 ROM escape（全树 ROM grep 仅命中寄存器 `NV_PES_XVU_ROM_ACCESS`），刷写＝NVAPI/Windows→N/A；**显示/EDID＝未接入（无可达证据）**——`NV0073_CTRL_CMD_SPECIFIC_GET_EDID_V2` 属 display class 0x0073，需 display 对象，macOS RM 链未暴露，未盲扫；NVAPI 显示 API→N/A；**API/运行策略＝N/A**（autoboost/ppab/whispermode2/dnotifier/pmgr-arbiter/api-restriction 均 NVAPI 读写，无运行时）；**ECC/OEM/Blackwell/移动＝N/A**（TU116 桌面、无 ECC、非 OEM、非 Blackwell）；**多卡＝本机单卡**（`GPU_GET_ATTACHED_IDS` 单 gpuId 0x100，后随 0xffffffff） | 逐项有依据；VBIOS 版本串/EDID 依赖未暴露的 RM 通道或 NVAPI，已按"未知/不适用"分开 |
| 9 自动压力执行与验证器 | 限时负载、停止、结构化结果、正确性校验 | **主负载 + 校验器完成**。主负载＝原 `--vulkan-only` 渲染路径（零算法改动，仅补 loader/ICD 解析 `4993a42`）：无 env 冷启动 rc=0，5/10/30 s 阶梯与 60 s stock 负载 rc=0，错误路径（`--gpu-index` 越界 / `--pci-bus` 不匹配）rc=1，`--vulkan-window` 在非 Windows 回退 offscreen rc=0；同步遥测显示 util 峰值 100%/均值 83%、gfx 1.53→1.95 GHz、功耗 11.5→13.5 W、温度 54→62 ℃、风扇 1778→1920 rpm。校验器＝`stress-macos`（`1dcf27a`）：全量 bit-exact 比对、启动自检、负例 rc=1、60 s 限时 1206 次 dispatch/0 失败、SIGINT 优雅停止。**整体验收**（`stage9-accept-20261017T045311Z`）：3 组 feature 构建 + fmt/clippy/tests 全 rc=0；`--vulkan-only` 冷启动 rc=0、错误路径 rc=1/1/0、阶梯 rc=0；信号语义实测 `--vulkan-only` SIGINT rc=-2 / SIGTERM rc=-15（OS 默认处置、立即终止），`metal_check` `--stress 300` 收 SIGINT 后 rc=0 优雅停止。**HEAD 复跑**（`stage-vulkan-recheck-20261017T055856Z`，8/8 PASS）：无 env 冷启动 rc=0、`--gpu-index 0` 选中真设备 `GTX 1650 SUPER (NVK TU116-A)` PCI `0000:01:00.0`、越界索引 rc=1；loader=`/Library/GPUBundles/nvmtl/libvulkan.1.4.362.dylib`、ICD=`nvk_icd.json`→`libvulkan_nouveau.dylib`（**NVK，非 MoltenVK**） | 渲染负载无读回校验、`--vulkan-only` 无进程内取消（靠 SIGINT/SIGTERM 默认处置）；阶段 10 需把负载/校验器结果接进 optimizer 闭环 |
| 10 自动扫描/优化/结果应用 | 受限闭环、保存/应用/恢复 | **部分：非 CUDA 构建 + 只读评测入口落地**。`--no-default-features --features stressor-external` 的 check/build rc=0；新增子命令 `macos-eval`（`195e7af`、证据 `stage10-eval-…`）：用 `BackendSet::MacosRm` 选设备（不再要求 `has_nvapi`），复用原 `cli-stressor-cuda-rs --vulkan-only` 限时负载 + NullMoth RM 遥测采样 + `stress-macos/metal_check --verify`，输出 `nvoc/macos-stock-eval/1`（pre/under_load/post、FPS、校验结论、`settings_written=false`）。实机 20 s：gfx 1950→1965 MHz、util 均值 87%/峰值 99%、power 12.5→13.5 W、temp 52→57.6 ℃、校验 pass。**无候选/SET/回读/接受-回退/恢复**：macOS 无公开写通道，这些仍不能跑。**HEAD 复跑**（`stage10-recheck-20261017T060320Z`）：`macos-eval --duration 8` rc=0，1530→1965 MHz、功率 11.4→13.6 W、温度 52.2→59.5 C、util max 100%、fan→2517 rpm，`metal_check` passed / 0 mismatch（全量 bit-exact），`settings_written=false`。"
    "**追加（`50bb01b`，2026-10-17）**：报告新增 `under_load.throttle`——负载期间的 reason mask 并集 + 断言过的原因名 + 带 mask 的样本数；实机 `--duration 8 --interval 1` 得 `samples_with_mask=5, mask_any=0, active_reasons=[]`（stock 负载下驱动未断言任何限频原因），`stressor exit=0`、`metal_check passed`、`settings_written=false`（`stage11-throttle-shared-20261017T065005Z`） | 需要 macOS 写通道或非公开接口；在此之前闭环停在"预检+原状态快照+限时负载+校验"这半 |
| 11 共享入口与界面集成 | pynvoc/GUI/TUI/srv 接入 | **pynvoc + GUI/TUI + srv 均已完成**：pynvoc 新增 `auto`/`default` 后端串（macOS→RM、其他→NVAPI+NVML，与 CLI `Auto` 同规则，`37f786f`），GUI/TUI 全部改传该串；客户端 GUI 281 passed/13 skipped、TUI 214 passed，macOS 实机 pynvoc `auto` 的 discover/info/status/settings rc=0 且与 `macos-rm` 同后端同字段集（`stage11-shared-entry-…`）。**srv** 完成只读接入：`SharedBackend` 泛化为 `Box<dyn ControlBackend>`，新增 `MacosRmBackend`（`supports_writes()=false`；`discover_backend()` 用 `cfg!` 让 macOS/NVAPI 两个分支在**每次**构建里都被类型检查），macOS 上发现设备 + 监控循环 + `/status`/`/api/*` 可用，写入在驱动前被拒、模式钉在 `Auto`——构建/测试 bundle `stage11-srv-macos-…`（52 passed）、实机 smoke `stage11-srv-smoke-…`（15/15 PASS、`/shutdown` exit 0），review `review-stage11-srv-macos-backend-20261017.md`。**追加：TUI 本体实机跑通**——测试机 user-level 装 uv + CPython 3.12.15，maturin 现场构建 `pynvoc`（abi3，`backend_macos_rm=true`），真 `nvoc-tui` 渲染真机遥测并以 1.0 s 周期刷新 29 次后 `ctrl+c` 退出 rc=0（`stage11-tui-macos-20261017T054628Z`、`review-stage11-tui-macos-20261017.md`） | macOS 上无闭环控制（需写通道）；**TUI 本体已实机跑通**（`stage11-tui-macos-20261017T054628Z`：真 `nvoc-tui` + 真 `pynvoc` + CPython 3.12.15，1 Hz × 29 刷新、`ctrl+c` rc=0），**GUI 本体已实机跑通（只读）**：改用 user-level uv CPython 3.12.15（**Tk 9.0**）后，真 `gui/main.py` 起窗（`App built (window up)`、geometry `768x712+5+30`），pynvoc `auto`→`macos-rm` 发现真机并读到功耗设置；此前 CLT Python 3.9 的 Tk 8.6 启动报错（`have instead 15 (1506)`）被绕过，无需 admin（`stage11-gui-macos-20261017T060733Z`、`review-stage11-gui-macos-20261017.md`）。GUI 的 VF 页在 macOS **惰性**（走 NVAPI 私有面、无实现，捕获异常不崩；阶段 7 无通道）；无截图（SSH 无屏幕录制权限）；写入在各入口**都在驱动之前拒绝**且都不崩——CLI/srv 用 macOS 专用文案（rc=2 / 409）. **追加（写拒绝文案对齐，2026-10-17）**：`pynvoc` 现在对任何解析到 NVAPI/NVML 的后端请求（= 全部写入口 + 显式 `nvapi`/`nvml`/`both` 读）在**发现前**返回单条 macOS 只读文案（不再出现 `dlopen("nvml.dll")`/`LibraryNotFound` 级联），`discover_gpus()` 无参默认改为平台默认（macOS→RM，与 `auto` 一致）；`auto-optimizer` 的 NVAPI 专属命令（`optimize`/`autoscan-vfp*`/`reset-vfp`/`import-vfp`/`export-vfp`）在 macOS 上**快速失败**并提示用 `macos-eval`。证据 `stage11-entry-parity-macos-20261017T062523Z`（pynvoc 12 passed 含新测试、auto-optimizer 62 / nvoc-cli 80 passed、fmt OK、`macos-eval` rc=0）。CLI 侧 `--macos-rm` 写命令 rc=2 与默认 Auto rc=1 的分类差异保留（均为同一文案、无写入、无崩溃，未改以不破坏 macOS 上既有 `validate_invocation` 测试契约）。**追加（限频原因共享读取面，2026-10-17）**：`QueryThrottleReasons` 在 macOS 改为由 core 从 RM 快照解码（单点实现），`get-throttle-reasons`（CLI）与 `query_throttle_reasons`（pynvoc，新增可选 `backends`，默认 `both` 与 info/status/settings 同族）共用同一九条 NVML 词汇/顺序；实机 human/JSON rc=0、默认 `both` 得到单条 macOS 只读文案——证据 `stage11-throttle-shared-20261017T065005Z` |
| 12 全量回归与交付 | 逐项复核、契约对照、CI 与 review 材料 | **本机可跑范围完成**：`stage12-regression-20261017T052812Z` **17/17 rc=0**（workspace `check`/`clippy`、core 89 / cli 80 / srv 52 / stressor 17 / optimizer 61 / pynvoc 11 passed、硬件只读 5 项、native/rusd/probe lint+测试、`metal_check --verify` 全量比对、CLI 构建与冒烟）＋功能表引用 bundle 存在性核对 **9/9**；交付索引 `outputs/INDEX.md`、阶段 review `review-stage12-regression-20261017.md`；随后清掉本工作流引入的最后一条 clippy 告警（`manual_is_multiple_of`）并按 §四已知限制做非 macOS 交叉构建（`stage12b-xplat-20261017T054019Z`）。阶段 1/2/3/9/10/11 均已有实机 bundle；阶段 4–7 有明确读写阻塞，`srv` 已从"启动即失败"变为"只读可用、写入受限" | 剩余缺口见 `review-stage12-regression-20261017.md` §5：非 macOS 平台**原生运行**证据（交叉类型检查已补：`stage12b-xplat-20261017T054019Z`，Linux/Windows 工作区 `--all-targets` 与 Linux Vulkan 路径全 rc=0）、阶段 4–7 写通道（GUI/TUI 本体 macOS 冒烟已补：`stage11-gui-macos-20261017T060733Z`、`stage11-tui-macos-20261017T054628Z`）。**追加（新 HEAD 回归复核，2026-10-17）**：`stage12-recheck-20261017T064248Z` **18 项 rc=0**（fmt/check/clippy-workspace、core 74 / cli 80 / srv 52 / stressor 14+17 / optimizer 61 / pynvoc 12 passed、真机只读硬件测试 5 passed、native/rusd/probe lint+tests、`metal_check --verify`、`build-cli --locked`、功能表引用 bundle 存在性 **27/27**）。**追加（限频原因共享读取面 `18b3957` 之后复核，2026-10-17）**：`stage12-recheck-20261017T070215Z` **18 项 rc=0**（fmt/check/clippy-workspace、core 74 / cli 80 / srv 52 / stressor 14+17 / optimizer 61 / pynvoc 12 passed、真机只读硬件 **6 passed**（含新 mask 位断言，14.58 s）、native/rusd/probe lint+tests、`metal_check --verify`、`build-cli --locked`、功能表引用 bundle 存在性 **24/24**）。**再现复核（`50bb01b` eval throttle 之后，2026-10-17）**：`stage12-recheck-20261017T071500Z` **19 项 rc=0，首跑全绿**（optimizer **63** passed、硬件 6 passed、probe Ran 11 OK、引用 bundle 存在性 **25/25**）。**读面普查后复核（2026-10-17）**：`stage12-recheck-20261017T073000Z` 在 `6226ddf` 上同样 **19 项 rc=0 首跑全绿**（probe CPU 回归 **Ran 12 OK**、引用 bundle 存在性 **26/26**）。**私有 clocks/VF 读面普查后复核（2026-10-17）**：`stage12-recheck-20261017T072634Z` 在 `10754b9` 上同样 **19 项 rc=0 首跑全绿**（probe-tests **Ran 13 OK**，含新增 opt-in 测试；该次运行核对引用 bundle 存在性 27/27，本行补上本 bundle 引用后为 **28/28**，已单独核对）。**版本戳 pass 之后复核（2026-10-17）**：`stage12-recheck-20261017T073055Z` 在 `6e79faa` 上同样 **19 项 rc=0 首跑全绿**（core 74 / cli 80 / srv 52 / stressor 14+17 / optimizer 63 / pynvoc 12 passed、真机硬件 6 passed、probe **Ran 13 OK**、引用 bundle 存在性 28/28） |

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
- **公开写面普查（2026-10-17，完整 CMD 宏扫描）**：对整个 `$OGKM/src/common/sdk/nvidia/inc` 抽取
  `NV2080_CTRL_CMD_*` 并按族过滤，结果只有 `THERMAL_SYSTEM_EXECUTE_V2`(+`_PHYSICAL`) 命中 temperature 族、
  **零** 命中 fan/cooler/acoustic 族；逐头核对：`ctrl2080power.h` 仅 `GC6_ENTRY/EXIT`、`ctrl2080pmgr.h` 仅
  `PMGR_GET_MODULE_INFO`、`ctrl2080clk.h` 仅 `PMUMON_CLK_DOMAINS_GET_SAMPLES`、`ctrl2080volt.h`/`ctrl2080clkavfs.h`
  **无任何命令**。→ 公开面上**唯一**的性能写对仍是 RATED_TDP SET（阶段 5 门控），
  温度**墙**写、风扇写、偏移/VF 写在 610.57.04 公开头中**不存在**。
- **内部 PM/PFM 族可达性否定（2026-10-17，只读 `--pm-read`）**：`ctrl2080internal.h`（`INTERNAL_INTERFACE_ID`）下的 `GET_PM1_STATE` 0x20800ace / `GET_VPSTATE_INFO` 0x20800ad4 / `PERF_GPU_BOOST_SYNC_GET_INFO` 0x20800a80 / `PERF_GET_AUX_POWER_STATE` 0x20800a81 实测**全部** `0x56 NV_ERR_NOT_SUPPORTED`（两次一致）→ 同族写候选（`UPDATE_TGPU_LIMIT`/`SET_VPSTATE`/`CONFIGURE_TGP_MODE`/`CONFIGURE_TURBO_V2`/`SET_PM1_STATE`/`UNSET_DYNAMIC_BOOST_LIMIT`）整块不可达。据此**公开面唯一可达写对仍是 RATED_TDP SET（阶段 5 门控、未执行）**（`stage-pm-read-20261017T061658Z`）。
- **私有 clocks/VF 读面普查（2026-10-17，`--clkvf-read`）**：`0x20809061`(776 B)/`0x2080B213`(1036 B)
  被接受且复读稳定，但**布局只解到"2 个头 mask + N×24/32 B 记录"**，记录内字段语义未定 →
  只登记为**探针数据**，不得当作时钟/偏移读数。R610 编号的私有时钟域/VF 命令群在 60 档尺寸
  （4 B…4.34 MB）**一律 `0x1f`**，需正确的 header/stamp 才能继续；两个阴性对照
  （`0x2080ffff`→`0x1b`、`0x2080a0ff`→`0x56`）证明该判读不是把"未知命令"当"尺寸不符"。
- **私有 clock/VF/rail 读面「文档封送」pass（2026-10-17，只读 `--clkdoc-read`）**：每个面只发**一个**文档
  (paramSize, dword[0] 戳, 输入 seed) 三元组（尺寸扫描发零填充，满足不了校验 stamp/mask 的 handler）。取得
  **两个可用读数**：`0x20809061`@776 的 11 条每域 kHz 表（bit3=405000 与该卡显存空载 405 MHz 吻合、
  bit4=6000984≈GDDR6 6 GHz、bit0/1/2 在 1455–1965 MHz 摆动；1 s 内逐字不变、分钟级漂移；seed 被驱动覆写
  ⇒ 非 mask-seeded；current vs max 语义**未定**，须负载 A/B），以及 `0x2080B201`@2444（=12+32×76）/@500008
  的 rail 域信息（mask 0xFF + 一条含 800000 µV / 756250 µV 的记录，seed 变体 payload 逐字相同）。
  其余文档形态（`0x20809019`、`0x2080901b`+mask、`0x20809006`+domain、`0x20809061` 大档、`0x20809062`、
  `0x20809023`、`0x20809025/26`、`0x2080B202`）**全部 `0x1f`**。判据更正：**是 `(id,size,stamp,seed)` 组合，
  不是 id 存在性**；且这些私有 id 在本机二进制里**字面量 0 命中**却被驱动接受 ⇒ 二进制字面量搜索
  **不能**用来证"未实现"。bundle `stage-clkdoc-20261017T074256Z`/`T074448Z`，review
  `outputs/review-stage-clkdoc-20261017.md`。
- **频率读负载 A/B + 对象载体扫描（2026-10-17，只读 `--clk-track` / `--clkobj-read`）**：`--clk-track` 1 Hz
  同时读 RUSD 的 gpc/mem 与 776 表；负载用**原 NVOC 负载**（`cli-stressor-cuda-rs --vulkan-only --duration 24`，
  `[VKGFX-H]` ×9 证明确实在跑）。结果：idle 表值逐字稳定（1530000/1455000/1545000/405000/6000984/…），
  负载期 `0x1`→1935000、`0x2`→1845000、`0x4`→1950000、`0x20`→1380000、`0x100000`→1785000，
  **`0x1` 与 RUSD GPC、`0x10` 与 RUSD MCLK 全部逐样本相等**，负载退出 2 s 内回落 ⇒ 776 表是**实时每域时钟
  读数**（GPC/MCLK 交叉确认），不是静态能力表。`--clkobj-read` 把「参数不对」与「挂错对象」分开：失败群
  subdevice≈device=`0x1f` 而 client=`0x56`（`0x20809062`/`0x20809023` 三载体全 `0x1f`）；`0x2080B201` 在
  subdevice **和 device** 均 OK 且边界为「≥2444」（88/164/620/2440 拒）⇒ handler 侧区间校验，与
  OGKM `rmapiutilGetControlInfo` 的**精确尺寸匹配**路径形成对照（`0x20809061`@776、`0x2080B213`@1036 各只收
  一个尺寸 = metadata 锚定），见 `probe-macos-rm/../docs` 引用与 review §3/§4。
- 未运行：完整 workspace `--ignored`、stage 3 探针组、任何性能 SET。

- **GSP 应答体权威 + 频率域偏移**范围**解码（2026-10-17 本轮，只读）**：见上表阶段 4 行的「本轮追加」。
  要点：①该 GSP 按**精确 paramSize** 校验，NVAPI magic 尺寸一律 `0x1f`＝尺寸不符（不是命令不存在）；
  ②`0x20809025@7188` 解出逐域 ±18.75 MHz 偏移**范围**并与 `nvapi-rs` 的 TU116 记录三值吻合；
  ③GET→SET 配对 `+1`/`0x90`→`0xd0` 且 paramSize 相同 ⇒ `0x9025/0x9026/0x9019` 无 SET 孪生，本块唯一写面
  `0x2080d01c@2108`；④**当前偏移/步长仍未取得 ⇒ 写入门槛未满足，未发 SET**。证据：`gsp-table-tu10x-610.57.04.tsv`、
  `stage-gsp-clkctl-20261017T092536Z/clkctl-hex.txt`（7188/12336 全量 hex）、`stage-clkctl-recheck-20261017T094018Z`
  （wedge 复检 `0x62`）。
- **本轮追加二：集中突破一条频率域偏移通道（2026-10-17，只读）**：要点见上表阶段 4 行的「本轮追加二」。
  新增可复核项：`stage-clkctl-hex-v2-20261017T100350Z`（新 `--clkctl-hex` 在 wedge 下首发
  `0x2080901b in@4=0xFF`、`0x62` 后中止整批 = 顺序与中止逻辑的实机验证）、
  `stage-gsp-clkctl-20261017T092536Z/clkctl-offsweep-run1-A5.txt`（delta 截断的原始记录）、
  文档 §4/§4.1/§5.2/§8/§9。**仍未发送任何 SET，未改驱动/EFI/boot-args，未重启。**
- **本轮追加三：把「写面缺失」定位到权限层 + SET 前置门（2026-10-17，静态 + 只读重解）**：
  1. **方法表完整性与版本映射**：全量表与 `flags` 语义补齐——`flags & 0x04`=写、`flags & 0x08`=读
     （`0x44`/`0x48` 是主要取值，但 `0x448` 这类组合也存在，**不能只看低字节猜写面**）；889 行中写标记
     159 行、其中带 legacy 位（可被 host 转发）82 行、再其中带 `0xC000`（特权）**80 行**。
     `+1/类别+0x40` 的 GET→SET 约定是**局部规律**：300 条 legacy 读里只有 41 条有**同尺寸**孪生、
     15 条孪生尺寸不同、244 条无孪生；且 `0x208090b2→0x2080d0b3` 的孪生**两行都是读**（`0x48`）。
  2. **写面缺失的层＝权限**：`rmapi_gss_legacy_control.c` 的
     `if (((cmd & 0xC000) == 0xC000) && (pSecInfo->privLevel < RS_PRIV_LEVEL_USER_ROOT)) return 0x1b;`
     在**转发前**执行，而该 id 空间的写半区就是「读 id `|0x4000`」⇒ 非 root 客户端**根本到不了 GSP 的 SET**；
     这同时一次性解释 `0x2080FFFF→0x1b`（特权位，不是"命令不存在"）。
  3. **SET 侧前置门（反汇编 `0x2080d01c` 的 pFunc `0x4d5c3c0`）**：G1 `*(s1+0x2050)!=NULL`、G2 其 `+0x4f4`
     字节非 0、G4 `*(*(s1+0x1f88)+0x1150)!=NULL`、G5 该对象 `+0x20` 访问器非空 —— 四处全部回
     `0x56 NV_ERR_NOT_SUPPORTED`；G3 `x+0x179 != 0` 回 **`0x6c NV_ERR_GPU_IN_DEBUG_MODE`**。
     GET thunk（`0x4d3b464`）**没有**这些门，只走 `x` 的访问器 + `vtbl[0x38]`；SET 走**另一个对象**
     `x+0x1150` 的 `vtbl[0x28]` ⇒ **读成功不能推出写可用**，`0x6c` 必须是首次写入的预期中止码之一。
  4. **记录写图更精确（`0xA5` 值图重解）**：head 的 14 次写不是"14 个 dword"——`+0` 是 **u8**、
     `+12` 是 **u16**、`+56` 是 **u8**、`+8` 与 `+16…+52` 是 dword；每条记录的 13 个槽是 **1×u16 类型戳
     + 12×dword**。`+12`/`+56` 在 head 里是窄写、在记录里是 dword ⇒ head 与记录是**两个不同结构**。
  5. **实际验证范围**：①~④全部为**静态证据 + 已录只读运行的重新解读**（`e_machine=243` 固件反汇编、
     OGKM 610.57.04 源码、GSP 表 tsv、`clkctl-hex.txt` / `offsweep-run1-A5.txt` 重解析）；
     **没有新增任何 GPU 写、没有 root、没有改驱动/EFI/boot-args、没有重启**，GSP 仍处 `0x62`。
     仍未取得：当前偏移值、写入步长、以及 G1/G2/G3 在本机的运行时取值（只有一次写入尝试能回答，
     且按 §2.1 需要特权客户端）。
- **本轮追加四：VOLT 读半区普查 / 按命令 gate 更正；上一层名字、写规则与权限前置（2026-10-17）**：要点见上表阶段 4 行的「本轮追加四」。
- **本轮追加五（已被追加六撤回）：anchor 电池读成「转发已死 ⇒ `0x2080b201` 是 host 应答」（2026-10-17T11:29Z，只读实机）**：要点见上表阶段 4 行的「本轮追加五」，**结论保留在记录里但已作废，不得引用**。
  新增可复核项：`probe-macos-rm --anchor-read`（新，四发 GET）、`outputs/anchor-20261017T112952Z/`、
  `outputs/wedge-scope-20261017T112552Z/`（V2.1–V2.15 + `--reg-read` 22 条 + 四类 thermal）、与 wedge 前基线
  `outputs/stage-reg-20261017T082800Z/reg-read.txt` 的逐 id A/B；文档 §6.1 更正、§7 重写为「host-answered」、§10 行 8 再更正、新增 §12（§12.1 A/B 表 / §12.2 `0x2080b201` 仍是第四条 host 通路，未命名）。
  **追加六已把「§7 = host-answered」「§12.2 = 第四条 host 通路」两处一并作废**（见上表阶段 4 行与附录「本轮追加六」）。
- **本轮追加七：私有面「家族地图」+ 家族计数更正 + 风扇族缺席的证据（2026-10-17T11:50Z，静态，无 GPU 请求）**：
  1. **家族计数更正**：文档 §3 的「cat 0x10(CLK)=169、cat 0x32(VOLT)=151」不成立——按 id
     低 16 位 bit15=`0x80`（legacy 读半区）/ bit14=`0x40`（特权写半区）分组后，**CLK 家族 = 87 行**
     （`0x90` 68 读 + `0xd0` 18 写 + 公开 `0x10` 一条 `0x20801037`）、**VOLT 家族 = 17 行**
     （`0xb2` 13 + `0xf2` 4）；169 与 151 恰好是 **`0x0a` internal 家族**与 **`0x20` PERF 家族**，
     即**家族误标**而非不同口径。原论点（「GSP 有实现体、host 没有」）不受影响。
  2. **全表家族地图（新工具）**：`probe-macos-rm/tools/dump-gsp-families.py`（只读；可按 byte 或
     category 打印，并标出哪些 id 同时被 host NVOC 表注册）。重数后 **1118 行 GSP / 608 行 host /
     两者都有 592 行 / GSP-only 526 行**（889 行时代的 589+170 是丢行结果）；五个阶段 4–7 相关家族的**读半区**合计 **286 个私有 GET**（VOLT 13 + CLK 74 +
     THERMAL 31 + PMGR 39 + PERF 129；**1118 行重数**，旧记 258 来自丢了 229 行的 889 行提取），
     host 一个都不认识。
  3. **THERMAL 私有读半区 = 31 条 `0x208085xx`**（`0x20808501@1036`、`0x20808524@4`、
     `0x2080852a@1784`、`0x2080852b@3324` …）——温度侧过去只发过公开批量
     `0x20800513 EXECUTE_V2`，**私有半区从未测过**；全部 GSP 派发 ⇒ 本会话被 wedge 挡住，
     但它们是重启后温度族的**首批候选**（表内尺寸、只读）。**但必须按 §6 的单发规则**：`0x20809026` 就是一条**只读** GET 把 GSP 打坏的，
     未试过的私有 GET 属同一风险类 ⇒ 只允许单发、作为一次会话的最后一条、且操作者知情，**不做 31 条批量**。
     **配对更正（§17）**：`0x2080852a@1784` 与 `0x2080852b@3324` **不是**一对 GET/SET——两条的
     bit14 都是 0，都是**读**控制；`0x2080852a` 的真实 SET 孪生是 `0x2080c52b`，而它在固件表里
     **不存在**。旧工具用「按族加类别步长」推配对时把 `0x2080852b` 报成它的孪生，是一个**假配对**
     （该 id 存在，所以抽查发现不了）。现规则是**按位**的：`twin = 翻转 bit14 + index+1`，
     对全表 1118 行成立、且双向枚举得到同一组 63 对。
  4. **风扇族在 GSP 表里不存在**：整个 `.fwimage` 只有两个 fan 字符串（`RmFan2XOverride`、
     `MIN_FAN_LEVEL`），公开 SDK 里 `ctrl2080fan.h` 是 33 行 `#pragma once` 空壳（无 `CTRL_CMD_*`、
     无 `INTERFACE_ID`），GSP 表也没有任何 FAN/COOLER interface ⇒ 本机风扇**不是「还没找到私有 id」**，
     这条 GSP-RM 路不存在；风扇调查必须走公开 thermal/GPIO/I2C 或 PMU 侧。
  5. **固件自带字段名**（`.fwimage` 字符串，release 构建只剩 40 条含 `->` 的断言，其中 9 条含
     `NV2080_CTRL_`）：`pSingle1x`/`pPhysicalSingle2x`（每条 rail 的
     `NV2080_CTRL_PMGR_PWR_CHANNEL_OUTPUT_VOLTAGE_*`）带 `clkDomainIdx` + `ignoreClkDomainIdx`，
     并含子结构 **`softFloor`**（`perfCfControllerIdx` / `perfCfControllerClkIdx` / `clkPropTopIdx`）
     ⇒ **电压 rail 记录里就带着时钟域绑定与频率下限**；`NV2080_CTRL_CLK_CLK_DOMAIN_INDEX_INVALID` /
     `NV2080_CTRL_PERF_PERF_CF_CONTROLLER_INDEX_INVALID` 为哨兵。这给 VOLT/PMGR 读半区的字段解读
     一个明确方向（也是「电压」与「时钟域」不能当无关家族处理的证据）。
  6. **公开头为什么永远查不到**：`ctrl2080volt.h`/`ctrl2080clkavfs.h`/`ctrl2080fan.h`/
     `ctrl2080vfe.h`/`ctrl2080perf_cf.h`/`ctrl2080perf_cf_pwr_model.h` 全部是 `#pragma once` 空壳
     （33/36/33/42/33/32 行，零 `CTRL_CMD_*`、零 `INTERFACE_ID`）⇒ 私有面只能以 GSP 镜像为一手来源。
  边界：本轮**无任何 GPU 请求/写、无提权、未改驱动/EFI/boot-args、未重启**；GSP 仍 `0x62`。
  详见 `docs/reverse-engineering/nvapi/gsp-method-table-and-legacy-forwarding.md` §3 更正框与 §13。
  **撤回**：追加四① 的「按命令 gate」；**未变**：本轮无任何写、未提权、未改驱动/EFI/boot-args、未重启。

  新增可复核项（①）：`outputs/volt-read-20261017T111107Z/`（`volt-read.txt` + `.jsonl`，整半区普查）、`probe-macos-rm/src/nvx_suite.c` 的 `--tierb-read` 扩展（GSP 表权威尺寸、`0x65` 中止、`0x62` 记录继续、512 KiB BSS）、文档 §7（改写为「`0x2080b201` survives the wedge: the gate is per command」）与 §10 第 8 行。
  新增可复核项（②）：`nvapi-rs/sys/src/gpu/clock.rs:1365-1375/1393/1766-1773/1930-1976`、`nvapi-rs/src/clock.rs:384-410`
  （`v1_marshalable`/`v2_marshalable`）、`nvapi-rs/src/gpu.rs:2218`（mandated reversible recipe，含「SET 不发宽 seed」）、
  `nvapi-rs/src/clock.rs:2244-2246`（±18750 的中间层口径）；NullMoth 树 `kexts/NVRM/os-xnu.cpp:174`、
  `kexts/NVRM/NVRM.cpp:1487-1496`、`tools/test_security_hardening.py:80`；OGKM `entry_points.c:185`、`escape.c:381`、
  `os.c:648`、`nvstatuscodes.h:56`、`ctrl2080gpu.h:1222/2427/2556`。**仍未发送任何 SET、未提权、未改驱动/EFI/boot-args、未重启。**

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
- **共享读取面补全（限频原因，2026-10-17）**：`get-throttle-reasons` 从 CLI 私有实现改为
  core 的共享 `QueryThrottleReasons`（macOS 分支读 RM 快照的 `clkThrottleInfo.reasonMask`），
  pynvoc `query_throttle_reasons` 因此与 CLI 同源；只加了一个可选 `backends` 参数，
  NVML/NVAPI 路径逐字未变（`stage11-throttle-shared-20261017T065005Z`）。
  **负载下读取**（同一 bundle）：25 s stock `--vulkan-only`（rc=0，fps 30–63）期间并发读
  `get-throttle-reasons`/`get-status` 九次——util 87–99%、gfx 1.935–1.95 GHz、温度 53.8→56.9 ℃、
  功耗 12.7–13.2 W、风扇 0→2740 rpm，**每次 `mask=0`、`active_reasons=[]`**：stock 负载下驱动
  **没有**断言任何限频原因；active 位的解码正确性由 core 的 mask 逐位单测与真机硬件断言保证，
  不以"渲染成功"推断（`macos-eval` 报告同结论：`samples_with_mask=5, mask_any=0`）。
- **入口行为对齐（写拒绝）**：macOS 上 CLI、srv、pynvoc→GUI/TUI 三个入口都拒绝写、都不崩——
  CLI 用同一 macOS 文案（`--macos-rm` rc=2 / 默认 Auto rc=1）、srv 返回 HTTP 409、
  pynvoc 现在返回单条 macOS 只读文案（`stage11-entry-parity-macos-20261017T062523Z`）；
  `auto-optimizer` 的 NVAPI 专属命令改为**快速失败**并指向只读的 `macos-eval`。
- **失败记录要求**（若后续复用失败）：记录具体阶段、NV_STATUS/错误、设备能力与最小修复尝试；
  只在拿到明确阻塞证据后才评估替代路径。
- **RM 控制分发链与活方法表（新，2026-10-17）**：本机 NVRM 由 OGKM 610.57.04 源码重编；`NV_ESC_RM_CONTROL` 经 `rmapiutilGetControlInfo` 对 NVOC 导出表二分查找并在 handler 前精确比较 paramSize。从加载镜像提取 604 条 `(methodId,paramSize,name)` 后，clock/VF/voltage/fan 私有族在 **host kernel RM 侧确实不存在**；但两个私有 id 仍被接受且接受**尺寸敏感**，却在整个 host 二进制里无字面量 ⇒ 应答层**已定位为 GSP 固件**（host 只按 legacy 位 `0x8000` 整块转发；`/Users/Shared/nvfw/nvidia/610.57.04/gsp_tu10x.bin` 的导出表有 889 行，CLK 169 + VOLT 151）。细节与复现命令见 `docs/reverse-engineering/nvapi/nvrm-610-method-table-and-dispatch.md` 与 `docs/reverse-engineering/nvapi/gsp-method-table-and-legacy-forwarding.md`。
- **活表交叉验证 + 频率域读数负结论（新，2026-10-17）**：`--reg-read` 把活表里每个 id 按**其注册 paramSize** 发一次。结果分三类：`NV_OK` 且回填（PMGR/KGR/PERF/`FB_GET_STATIC_BAR1_INFO`）、**在注册尺寸上仍回 `0x1f`**（`0x20800102@580`、`0x20801303@1028`、`0x20802087@12`、`0x20802096@55888`、`0x20801201@32`）、`0x56`（已注册的内部 PM 族）。第二类说明 `0x1f` **也可能是 handler 侧的入参拒绝**（空 info list / 未设 engine 索引），不能只看错误码断言"尺寸错"。**频率域控制读取**：唯一注册的时钟形态命令 `PERF_GET_LEVEL_INFO_V2`(0x2080200b@780，结构 = 4+4+32×24+4，与 SDK 头逐字节一致) 实测 `NV_OK`，并对 level 0/1/2 稳定回显 mode 位（`0x2/0x0/0x4`），但 **32 条 clk-info 记录 0 条被填**（`--clklevel-read`，`stage-clklevel-20261017T083400Z`）⇒ 属"**成功但只占位**"层，不能当范围读数；公开 610.57.04 源码里该 id 的非 VF HAL 是 `NV_ASSERT_PRECOMP(0)→NV_ERR_NOT_SUPPORTED` 存根、vGPU 侧 TU116 落 `rpcPerfGetLevelInfo_STUB`，两条路径都不填表，而实测却回 `NV_OK` ⇒ 与 `0x20801037` 回 `0x56` 的事实合看，两个 id 走的不是同一条代码路径。**用户客户端 A/B**：`--service nvrm` 与 `--service nvaccel` 对整批只读返回**逐字节相同**结果 ⇒ 排除"NVAccel 侧独立 shim 应答私 id"的假设。
- **安全门槛未变**：低强度/短时/可停止优先；进程级配置；不改全局驱动配置、EFI、boot-args；
  写入仍门控；客户端不部署、不自动重启、不操作 PR。

- **RM 封送纠错表（新，2026-10-17）**：分清 base 口径后，「76 B stride 却出现 >76 字段」的矛盾解决——记录里的 `0x3C/0x40/0x44/0x48/0x68/0x6C/0x77` 是 **escape 容器绝对偏移**（容器头 14 dword = `0x38`，`buf[13]`=RM cmd），减去 `0x38` 后全部落进 76 B 记录内；本机实测 2444 = **12 B 头 + 32×76**，与 5 个记录字段逐一对上（mask `|=1<<bit`、type=2、`959`、`1`、rail class=1），两个 µV 读数保持记录中的 36 B 间距。`(version<<16|size)` magic 属 **NVAPI 层**，RM 参数块里没有 magic（dword0 是输出 mask，seed 0xFFFFFFFF 也回 0xFF）。同表另证 `0x2080B201` **不是精确尺寸**而是 **≥2444**（2448/2480/2500/500008 均 `NV_OK`，2440 及以下 `0x1f`），**device 对象也答 `NV_OK`**、client 答 `0x56` ⇒ 不经 NVOC 精确尺寸闸门，与活表查无此 id 互相印证。详见 `docs/reverse-engineering/nvapi/rm-param-marshall-corrections-r610.md`。

- **底层驱动自发 PERF_BOOST（新，2026-10-17，静态 + 日志交叉）**：`libvulkan_nouveau.dylib`
  （sha256 `cfa7e20f…9d19a51c7`）里 `0x2080200a` 只有两处，均为 **NVK 自己发**，不是 agent：
  ① `_nvkmd_nvrm_enum_pdev`（设备创建）发 `flags=0x12`（`BOOST_TO_MAX｜CUDA_YES`）+ `duration=INFINITE`；
  ② `_nvkmd_nvrm_pdev_destroy`（设备销毁）发 `flags=0`（`CMD_CLEAR`）+ `duration=0`。
  两处同一个门 `nvrm610_on("BOOST")`（env `NVRM_610_BOOST` → `/Library/GPUBundles/nvmtl/nvrm610.conf`
  的 `BOOST=` → 默认 **ON**；本机实测 ON）。`outputs/` 里 **50 条**
  `nvrm: b81 PERF_BOOST flags 0x12 INFINITE rc 0x0` 证明它确实成功下发；host 侧
  `kern_perf_boost.c` 对非 GB10Y **没有** clear 分支 ⇒ 释放完全依赖客户端的 `CMD_CLEAR`。
  影响：**运行期频率升高可能来自这条 boost 而非 offset 写入**，因此偏移实验的成功判据里
  RUSD 实时 kHz 明确**不作依据**（`probe-macos-rm/WRITE-VERIFICATION-PLAN.md` §7.3-⑤/§7.6）。
  详见 `docs/reverse-engineering/nvapi/nvk-perf-boost-internal.md`。**本轮无任何 GPU 写入**。

- **`0x2080901b` 写入集重测（新，2026-10-17，静态重读已录字节）**：用**计数器背景**
  （`dword i = i+1`，任何写入都可见，不像 `0x00`/`0xA5` 会漏或假）确定几何：
  ① **域掩码在 `+4`**，bit *i* 选记录 *i*（`+4=2` 只写 record 1，其类型戳在 **byte 124**；
  `+4=0` 一条记录都不写）——此前 "mask@+8" 电池把掩码放错字段，这正是它总回 "3 non-zero bytes" 的原因；
  ② 头 = **60 B，写 14 个 dword**（`+0` = u8 `0xFF`、`+8` = `0x01010000`、`+12…+56` = 0），
  且**只有在选中 ≥1 条记录时才写**；③ 记录 *i* 在 `60 + 64*i`，写 **13 个 dword**
  （`+0` u16 类型戳、`+8…+44`、`+52`、`+56`），**`+4`/`+48`/`+60` 从不写**；
  ④ **stock 下 12 个 value dword 在非零背景上全为 0** ⇒ 「当前偏移 = 0」现在是**测出来的**，
  不是在零背景 dump 上推断的。⑤ 未决：`0xA5` 那次报的 `106` 与 `14 + 8×13 = 118` 对不上，
  该文件出自「打印被截断」的同一构建，其计数器不可信，**等 wedge 解除后重测**。
  ⑥ **缺失层（有证据）**：OGKM 610.57.04 全树对 `2080901b`/`clkDomainIdx`/`CLK_DOMAINS_GET_CONTROL`/
  `CLK_CLK_DOMAIN_INDEX` 均 **0 命中**，macOS host kext 同样 0 命中，GSP 镜像里仅有的 `clkDomainIdx`
  字串属 **PMGR 电源策略**断言 —— 因此缺的是**闭源 GSP CLK 控制实现体**（经运行时对象 vtable 分派），
  不是协议/尺寸/戳，也不是权限。详见 `docs/reverse-engineering/nvapi/gsp-method-table-and-legacy-forwarding.md` §15。
  **本轮无任何 GPU 请求、无 SET、未提权、未改驱动/EFI/boot-args、未重启。**

- **单域最小步长写入载体已实现（新，2026-10-17，未执行）**：`probe-macos-rm --write
  --clk-offset-set BIT:OFF:DELTA`（`V3.5a..e`）就是 `WRITE-VERIFICATION-PLAN.md` §7.3 的执行载体，
  与 RATED_TDP 的 `V3.4a..c` 同风格。拒绝先于打开驱动：无 `--write` / idle-only / `|DELTA|>1000 kHz`
  三条实测 rc=2；`OFF` **无默认值**且必须在 12 个已测 value dword 内（G1 未定，工具不猜）；
  pre-record GET 失败则**不写**。成功 = SET `NV_OK` ∧ 同字段回读 == 原值±DELTA ∧ 该记录其它 value dword
  逐字节相同 ∧ restore 后整块 2108 B 逐字节相同；**实时 kHz 不作判据**（NVK 自发 PERF_BOOST）。
  本轮验证：`LINT=1 build.sh` rc=0、`build.sh` rc=0、三条拒绝路径 rc=2、解码器 selftest ok。
  **未执行任何 SET。** 提交 `ed95433`。

- **风扇族 = 接口层面关闭（新，2026-10-17，静态）**：不是「还没找到 id」，而是**不存在** RM 风扇命令：
  ① 公开 SDK 的 `ctrl2080fan.h` 是 **33 行空壳、0 个 `NV2080_CTRL_CMD_*`**；② host NVOC 表 **0 行**风扇族；
  ③ GSP 导出表 **0 行**风扇族；④ 文档化的 thermal executor 只有两个 opcode
  （`GET_STATUS_SENSOR_READING=0x1500`、`GET_INFO_SENSOR_READING_RANGE=0x0540`），**没有风扇 opcode**。
  剩下只有两条**板级**路径：RUSD/NV00DE 遥测页的 `fan` 字段（实测驱动不填：`0 rpm`），或文档化的**裸 I2C**
  （host 表已注册：`0x20800601 I2C_READ_BUFFER@532`/`0602@276`/`0603@276`/`0604@276`，家族 0x06 四行与
  头文件逐一吻合）+ GPIO（0x03，8 行）。裸 I2C 需要板级风扇控制器的从地址与寄存器图（属板卡资料，不是 RM
  接口），且 I2C **写**属硬件写、同样受现有门控。另：`ctrl2080thermal.h:137` 直接给出私有别名
  `NV2080_CTRL_CMD_THERMAL_SYSTEM_EXECUTE_V2_PHYSICAL (0x20808513)` ⇒ **私有 thermal 族 = 公开 id | 0x8000
  且参数结构有文档**，与 §2.1 的转发编码推导一致。详见
  `docs/reverse-engineering/nvapi/nvrm-610-method-table-and-dispatch.md`「Fan and thermal」节。


### 本轮追加八（2026-10-17，静态重提取：方法表完整性更正 + 一处安全门收口）

**为什么做**：用户本轮要求「先定位已成功私有 GET 的真实应答路径，纠正**方法表完整性**和版本映射假设」。路径上一轮已定位（host 按 `id & 0x8000` 整块转发给 GSP，GSP 用**自己的** `paramSize` 校验），所以这轮把「GSP 表」这个判据本身重新做了一遍可信度审计。

**发现的工具缺陷**：`probe-macos-rm/tools/extract-gsp-method-table.py` 过去用 `flags < 0x10000 and accessRight < 0x10000` 当有效性守卫。**GSP 行的 `flags` 不是有界枚举**（实测 `0x48`/`0x448`/`0x10248`/`0x81074a`/`0x82004a`/`0x83034a`/`0x5a14438`），所以这个守卫是**静默丢行器**：889 行里有 229 行被丢。放宽后（只保留结构性判据：id 类别 `0x2080`、`0 < paramSize < 8e6`、`pClassInfo`/`pFunc` 落在已加载 `.fwimage` 范围内）：**1118 行**；4 字节对齐扫描与 32 字节步长扫描给出**同一集合** ⇒ 放宽没有引入伪行。

**更正后的数字**（同一工具、同一固件）：CLK 读 `0x90` **74** / 写 `0xd0` **19**；THERMAL `0x85` 31 / `0xc5` 6；PMGR `0xa6` 39 / `0xe6` 12；PERF `0xa0` 129 / `0xe0` 32；VOLT `0xb2` **13** / `0xf2` **4**（未受影响）。私有**读半区**在转发后面的总数：74 + 31 + 39 + 129 + 13 = **286**（旧记 258）。host 表同法放宽：**608** 行（旧 604），与 GSP 表**共享 592 个 id、GSP-only 526 个**；**私有族字节（`0x90/0xd0/0x85/0xc5/0xa6/0xe6/0xa0/0xe0/0xb2/0xf2`）在 host 表 0 命中** ⇒ §2「应答者是 GSP」的论证**不依赖**被丢掉的那 229 行。`dump-gsp-families.py` 的 host 解析过去用「连续 run + `flags>0` + `access<=0x3FFFF`」走法（589 行，与提取器的 604/608 三个数字互不一致），已改成与提取器同一条结构判据（608），并把 `0xc0|c` 写半区误标成「read half」的显示 bug 一并修掉。

**两个「不在表里」的结论作废**：`0x20809006`（CLK_MEASURE_FREQ）是 **`paramSize 8`** 的真实行、`0x20809062` 是 **648** 的真实行（flags 分别 `0x81074a`/`0x82004a`）——正是此前用过又被判「不可推导」的两个尺寸。这直接把「频率测量」这条**只读交叉证据**从猜测变成**固件推导尺寸**。**实测口径（旧记录里的状态码，非新请求）**：`0x20809062@648` 当时回 **`0x3a NV_ERR_INVALID_PARAM_STRUCT`** ⇒ 尺寸被**接受**、只是结构/戳不合 ⇒ **648 得到实机独立确认**；`0x20809006@8` 当时回 **`0x1f NV_ERR_INVALID_ARGUMENT`** ⇒ **未确认**：8 B 块只有一个输入 dword（未播种的选择子）或该行 `flags 0x81074a` 表示非客户端可达，两种解释都还开着 ⇒ 已把 `d0=1/d0=2/d1=1` 三个**有依据的**播种变体加进读电池作为判别序列。同一批新可见的 CLK 行：`0x20809001@8`、`0x20809035@1164`（写孪生 `0x2080d036`）、`0x20809038@4`、`0x20809063@520`、写侧 `0x2080d055@332`。

**写入面的事实更正**：GET→SET 孪生规则是「低字节 +1、类别字节 `0x90`→`0xd0`」，**但 `paramSize` 不保证相同**（`0x20809040@4808` 对 `0x2080d041@648`），所以 SET 不得继承 GET 的尺寸假设。**（规则表述已由本轮追加九/§17 收紧：不是「类别步进」而是翻转 bit14 + index+1；尺寸独立的结论不变。）**另：`0x2080901d@32680` = **40 + 32×1020**（mask 是 32 位选择器 ⇒ 32 条记录 ⇒ 步长只能是 1020；"128" 读法需要 255 位选择器），且与 Windows 侧 V2 注释里的 *internal* dwords `[32..36,41..43]`（字节 128..148 / 164..176）相容、与 `0x2080901b` 的 64 B 记录**不相容**——这是**登记预测**，由重启后的 `gc-1d 32680 zeroed` 单发来证伪，不当作读数。

**安全门收口（本轮唯一的代码改动）**：审计中发现 `--clkctl-offsweep` 的零背景清单里**仍然在发** `0x2080901c@808`，而同一行注释说「battery 里没人发它所以没列进 wedge id」。但 2026-10-17 那次 wedge 里，`0x2080901c@808` 正是**第一个异常**（0xA5 块回 `0x65 NV_ERR_TIMEOUT`），紧接着 `0x20809026@5332` 把整个会话锁进 `0x62`。⇒ 把它按同一规则**收进 `nvx_known_wedge_ids`**（消息说明是「在 wedge 序列里超时、与 latch 无法分离」），offsweep 会打印 REFUSED 而不是发送。**未新增任何会自发请求的新 id**；读电池的 id/size/顺序与上一轮评审过的完全一致。

**验证范围（如实）**：以上全部是**静态 + 工具**结论（host kext / GSP 固件 / 两个提取器 / 两个纯 Python 工具），**本机没有发出任何 GPU 请求**；测试机 GSP 仍处 `0x62 NV_ERR_RESET_REQUIRED`（wedge 未清）。工具侧本地已过：`py_compile`、`dump-gsp-families.py`（1118/608/592/526）、`decode-clk-containers.py --selftest`；C 侧改动只在测试机构建与实测拒绝路径。**写入面结论不变**：`settings_write=false`，未发任何 SET。详见 `docs/reverse-engineering/nvapi/gsp-method-table-and-legacy-forwarding.md` §16。


### 本轮追加九（2026-10-17，静态：`0x2080901b` 价值槽的**类型条件性**被钉死，106 计数结清）

**为什么做**：用户本轮要求「从**实际封送逻辑**还原偏移 GET/SET，不继续扩大无依据的尺寸/ID 扫描」。可用的「实际封送逻辑」有两处：① 仓库内自带的 Windows NVAPI 侧封送代码（`nvapi-rs`），② **已录盘的实机字节**（handler 自己写出的写集）。本轮把 ② 做到底，因为它才是 macOS 这一侧的唯一权威。

**已确认的封送事实（Windows 侧，作为交叉参照，不是假设）**：`nvapi-rs/sys/src/gpu/clock.rs` 的 V1 调用块 `magic 0x10964`＝`2404 B`、记录 stride **72**、bit-sparse 于 `+100`，记录内 value 槽在 `+44..+60`（`OFFSET_KHZ=44`、`RANGE_MIN=48`、`RANGE_MAX=52`、`APPLIED=56`）；`src/clock.rs` 明确「V1 fallback 从 rec+44..60 填 slots 0..4」，且 **slot 语义是 driver-opaque**——文章说 slot 0 是有符号 kHz 偏移，但「只有 A/B SET + MEASURE 才能确认哪个是哪个」。V2 走 `rec+268..296`（8 个 dword，GET/SET 对称）。⇒ **Windows 侧也只能提供候选槽，不能命名**。这条写在这里是为了防止后人再从 nvapi-rs 里「推出」一个字段身份。

**实机侧的决定性结果（静态重读已录字节，零新请求）**：`0x20809019` 的无损 dump 是一个**目录**，按条目序给出 `entry:domain_bit:type`＝`0:1:0808 1:2:0909 2:16:0808 3:4:0909 4:8:0909 5:1048576:0909 6:524288:0303 7:64:0808 8:2097152:0909 9:32:0909`（`decode-clk-containers.py --selftest` 通过，并在本机对 `clkctl-hex.txt` 复现）。把它与 `0x2080901b` 的 `0xA5` 电池（`mask@+4=0xFF`，工具自报 `106/527` dwords changed）对齐：

* 那份 delta 打印**是前缀不是抽样**——它覆盖 `+0..+180` 后停止，保留了 40 个：**头部 14 个 dword**（`+0,+8,+12,+16..+56`；`+4` 是输入掩码不写回）＋ **record 0（类型 `08`）13 个** ＋ **record 1（类型 `09`）13 个**。
* 掩码 `0xFF` 选条目 0..7，其类型为 `[08,09,08,09,09,09,03,08]`；被打印的正是条目 0 和 1，类型与打印**互证**（序不是假设）。
* 余下 `106-40=66` 个 dword 属于被截断的 6 条。其中 5 条（条目 2/3/4/5/7）与已实测的条目 0/1 **同为类型 `08`/`09`** ⇒ 若写集是类型的函数（§15.3 的布局对同类型必然一致），它们各写 13 ＝ 65；**唯一的一条类型 `03`（条目 6）写 `66-65=1` 个 dword，即只有它自己的类型戳**。

```
106 = 14（头部） + 7×13（七条类型 08/09） + 1（唯一一条类型 03）
```

**同时作废两条旧表述**：§14.1 的「2 header dwords + 8 records × 13」（右到答案、左错两处）与 §10 表格第 2 行的「other types 11」（要求同为类型 `08` 的条目 2/7 写 11 而条目 0 写 13）。**这是 G1 的一次真实收窄，但也有一个作废的期待**：价值块是**按类型全有/全无**（`08/09`＝12 个 value dword，`03`＝0 个），所以它**不能**再像上一轮希望的那样把 12 个槽缩成「一对」——**字段身份仍然只能由 §15.3 的两件仪器给出**（非零 value dword，或门控单域最小步 SET ＋ 同字段回读）。这条要挡住的是「用类型差猜出偏移槽」这个诱人但错误的近路。

**已登记的证伪预测（重启后 `--clkctl-hex` 首发，零写入）**：头部 14 个 dword；条目 0/1/2/3/4/5/7 各 13 个；条目 6 **只写 1 个**（其戳应在 byte `60+6*64=444`、值 `03 03`）；任何条目的 `+4/+48/+60` 都不被写；总数 **106**。任何别的切分都证伪「类型条件」模型——这正是跑它的目的，而不是为了凑数。

**验证范围（如实）**：以上全部为**静态 + 已录盘字节**重读，外加本机（开发端）跑 `decode-clk-containers.py --selftest` 与其对 `clkctl-hex.txt` 的解码；**测试机本轮未发出任何 GPU 请求**（GSP 仍 `0x62`，见 §6 与 INDEX.md 最新一条轮询）。**写入面结论不变**：`settings_write=false`，未发任何 SET、未提权、未重启。详见 `docs/reverse-engineering/nvapi/gsp-method-table-and-legacy-forwarding.md` §18（并见 §17 的孪生位规则）。


### 本轮追加十（2026-10-17，静态：偏移通道接入公共 core 的清单已勘定 + 公开理由措辞更正）

**为什么做**：第一验收标准是**用户可见行为一致**，而可见面走的是公共 `get-settings`/`set-*` 词汇，不是验证探针。探针有这条通道、公共 core 没有——把「没接上」具体是什么查清楚，接的时候才是机械动作而不是重新设计。本轮的交付物是 [macos-offset-channel-integration-plan.md](macos-offset-channel-integration-plan.md)（含逐条证据）。

**载重结论：不需要新传输层。** `core/build.rs:19-23` 已经把 `core/native/macos_rm.c` 与 `probe-macos-rm/src/nvx_xnu.c`、`nvx_rm.c` 一起编译 ⇒ 公共 macOS 后端用的就是探针那套 `nvx_rm_control()`；`core/native/macos_rm.c:133` 的 `read_live_controls()` 每个采样已经在发两条实时 RM 控制（P-state、FB heap）。所以 `0x20809019`/`0x20809025`/`0x2080901b` 的接入是**既有调用路径的有界扩展**（多一次控制调用 + ABI + Rust 管道），**不动内核、不动驱动**。

**同时记下公共面今天的状态与原因**：`core/src/macos_rm.rs` 里**一次 RM 控制调用都没有**，三个 id 只出现在 `notes` 散文（`:183`）与不可读理由串（`:110`）里。读路径已按「可在 G1 之前就上线」写好规范：三条 GET 各自带状态（缺读数**绝不当 0**）、**条目索引 ≠ 域位**、**范围块与承载偏移的字段不得混同**。

**写入门槛不动**：本计划**只加读**，因此不可能削弱 `settings_write=false`；并写明「可称已实现且实测」的四个条件。

**两个未决项如实写下**：① G1（槽身份）；② `0x20809025` 三条记录的**域归属**——其记录首 dword 读作 `0x01030101`/`0x01040101`/`0x01050101`（中字节 `03/04/05` 呈计数序），而 `0x20809019` 目录枚举的是**域位**（`1,2,16,4,8,…`），`03/04/05` 是否域索引**尚未确立** ⇒ 范围只能**暂按记录**报告、不硬贴域名。

**公开措辞更正（本轮唯一代码改动）**：`CLOCK_OFFSET_UNREADABLE_REASON` 原写「no current offset … come back zero」，读起来像「驱动从不填值」——那正是 §15/§18 已经推翻的读法。实测相反：handler**确实会写**它的 value dword，在「写不会被漏掉」的背景上它们**全为 0** ⇒ 正确表述是「**当前没有施加偏移**」，缺的只是**承载偏移的那个 dword 的身份**。理由串与 `notes` 已按此更正，并把**按类型全有/全无**的写集一并写进 notes。「族仍留在 `unavailable`」是刻意的：公共会话**根本还没携带该块**，两个能力位保持 false，**只有陈述的成因变了**。字符串级改动，无测试断言旧措辞。

**验证范围（如实）**：静态读 `core/build.rs`、`core/native/*`、`core/src/macos_rm.rs`、`nvoc-python/src/lib.rs` 与已录盘探针记录；**开发端不构建**。代码改动在**测试机**（macOS，真实 cfg）验证：源文件 sha256 与开发端一致（`13c2589b…`），`cargo fmt -p nvoc-core -- --check` **rc=0**、`cargo check -p nvoc-core --offline` **rc=0**（22.39 s）。**未发出任何 GPU 请求、未发任何 SET、未提权、未改驱动/EFI/boot-args、未重启、未操作 PR。**

### 本轮追加十一（2026-10-17，无 GPU：把 9019/9025/901b 接进公共 core 的**读**路径，状态语义分层）

**做了什么（两个可独立 review 的提交）**：`b1e24c8` 加 `core/src/macos_offset.rs`——三条私有命令的**封送几何解析**（纯 `&[u8]` → 类型化数据，平台无关、可无 GPU 单测）；`cf2848c` 把解析接到设备：native 侧新增 `nvoc_macos_rm_session_offset()`，Rust 侧新增 `MacosRmSession::read_offset_channel()`。

**选择依据（为什么这是有界扩展而不是新传输层）**：`core/build.rs:19-23` 已把探针的 `nvx_xnu.c`/`nvx_rm.c` 编进本库，`core/native/macos_rm.c:133` 的 `read_live_controls()` 每个采样**已在发实时 RM 控制** ⇒ 这是**同一条既有调用路径上的第四次调用**，内核与驱动都不用动。

**几何取自实测，不是猜**：三个容器尺寸逐字节吻合（`48+32*384`、`20+32*224`、`60+32*64`），并有单测断言该吻合——步长错一点记录就会跨出容器末尾。控制记录携带的 12 个 value dword 正是 0xA5 预填证明handler**会写**的那些；`+4`/`+48`/`+60` 从不写入，故不携带，且单测在这三处埋标记，证明它们不会被误当成 value。

**状态语义分层（本轮的关键设计）**：native 侧按 kind + **精确尺寸**索取，并在调用前清零缓冲，使**拒绝绝不会留下半填的块**；只有返回 0 才代表 payload 有效。Rust 侧把该区分一路上抛为 `Parsed` / `Refused(status)` / `Malformed(parse error)`，三个块**各自独立**——一个块被拒不得丢掉另外两个，且 `0x62` **绝不能被渲染成「偏移为 0」**。这正是本轮最高优先第 4 条（不得以假值代替）在公共面的落实。

**刻意不定的两件事**：① 12 个 dword 里**哪个**是偏移仍然不定——stock 读不出来，只有「写入 + 同字段回读」能定，所以解析器只**逐字报告**这 12 个值；② 9025 记录首 dword（`0x01030101/0x01040101/…`）的**域归属**仍未确立，故按记录报告。

**实际验证范围（如实）**：`rustfmt --check` **rc=0**；`cargo clippy -p nvoc-core --all-targets --offline -- -D warnings` **rc=0**（强制 `touch` 后确认确有 `Compiling nvoc-core`）；`cargo test -p nvoc-core --lib --offline` **80 passed / 0 failed**（含 ABI 布局镜像测试，故 ABI_VERSION 3→4 两端同步已被机器校验）；测试二进制**链接**了新的 C 符号，这正是 native 侧确实被编译的证据（若 C 未重编，链阶段会因未定义符号失败）。**分类：已实现、已编译、待实测**——本机 wedge 未解，三条 GET 现在都会以 `Refused(0x62)` 返回，所以「拿真实 GSP 读到目录/范围/当前值」这一步**仍未发出**。ABI 版本 3→4：**结构布局零改动**（头文件所有断言仍成立），版本号只因「调用方无法从布局判断这条通道是否存在」而上移。

**未发出**：任何 GPU 请求、任何 RM 会话、任何 SET、提权、驱动/EFI/boot-args 改动、重启、PR 操作。

### 本轮追加十二（2026-10-17，无 GPU：用**设备原始字节**钉住解析器 + 一处验证流程缺陷的更正）

**做了什么**：`core/tests/macos_offset_fixture.rs` + `core/tests/data/*.bin`（commit `953b25e`）。fixture 是探针在目标卡上 **pre-wedge `NV_OK`** 两次运行的**逐字节 payload**——只有 9019 目录块与 9025 范围块曾经被原始 dump 过，正好就是本轮要钉的两块。它们**精确复现**录盘值：10 条目录条目（各带域位与类型戳）、3 条范围记录（`-18750..+18750 kHz`，tag `0x01030101/0x01040101/0x01050101`）；并断言前 8 条的**类型序列**，因为那是「106 个写入 dword」论证的载重部分——类型一变，算术就得重做，所以它不该只活在散文里。

**能被证明与不能被证明的，分开写**：覆盖面 = 「记录的步长/字段偏移一旦漂移，在这里而不是在 GPU 上失败」；不能证明 = ① 控制块的 geometry（其 payload 从未被原始 dump，只有写集被 dump ⇒ 靠合成单测 + 目录类型序列）；② **12 个 dword 里哪个是偏移**——stock 全零，任何字节 fixture 都定不了。另断言「把一块的字节喂给另一条命令的解析器」必报错（短缓冲是**错误**，绝不是更短的零块），以及 `Parsed`/`Refused`/`Malformed` 三者**互不相等**（`0x62` 绝不能长得像「偏移为 0」）。

**⚠️ 更正上一轮（追加十一）的验证范围——工作区缺陷，如实记录**：上一轮我用
`rsync -a --checksum SRC mac@host:'$HOME/Desktop/…'` 同步，该写法**静默什么也不做**（rc=0、只打印 `building file list ... done`、不列任何文件名）；`'~/Desktop/…'` 写法正常。后果：`cf2848c`（FFI 接线）那轮的「`Compiling nvoc-core` + 80 测试全绿 + 链接到 C 符号」是**在没收到 FFI 文件的树上跑的**，结论**名义成立但不真实**。本轮已按**逐文件 sha256 核对 8 个文件全部到位**后重做：`rustfmt --check` **rc=0**、`clippy -p nvoc-core --all-targets -- -D warnings` **rc=0**（0 error，`touch` 后确认真编译）、`cargo test -p nvoc-core --all-targets` **80 lib + 5 fixture + 15 non-GPU 全过**（50 个硬件门控测试保持 ignored）。**结论：`cf2848c` 的代码本身无问题，但它的验证直到本轮才算真的做过。**

**新增常驻规矩**：同步后**必须**核对文件清单与逐文件 sha256，**不得以 rsync 退出码代替**；远端路径一律用 `~/` 写法。这条已写入 INDEX 与 review，防止重犯。

**仍未发出**：任何 GPU 请求、任何 RM 会话、任何 SET、提权、驱动/EFI/boot-args 改动、重启、PR 操作。

### 本轮追加十三（2026-10-17，公共面实测 + 一处新发现的诚实性风险）

**做了什么**（commit `dd0f5fe`）：把「限制」从散文变成**测量**。原状：`±18750 kHz` 只存在于一条静态 reason 字符串里，而计划 §5 验收条件 3 明确要求「上报的范围必须是设备的、不是常数」。现：`Device::settings()` 打开会话读三个块，经**纯函数** `apply_offset_channel` 折叠进 `DeviceSettings`，并新增公开字段 `clock_offset_limits_khz`（`{min_khz, max_khz, records}`）；测试用**已提交的设备字节 fixture**，所以断言里的数字是测出来的。

**守住的界线**：① 拒绝/畸形块 ⇒ 字段**缺席**并写明 RM 状态（楔形下绝不显示「限制为 0 kHz」）；② 只在**所有可读记录一致**时才给出窗口，且携带 `records` 计数——因为「哪条记录属于哪个命名域」尚未确立，把某一条当窗口就是未挣得的断言；不一致、空选择、被拒各有各的原因。③ **偏移本身仍不进数字**：stock 实测为 0，但报 0 等于断言了 G1 正在追的那个槽位身份，故两族继续留在 `unavailable`，且 reason 已改为指向该字段而非复述常数。

**公共面实测（楔形状态下，只读）**：`nvoc-cli get-settings --macos-rm` 输出 `Clock Offset Limits kHz: N/A`，notes 末尾为 `offset channel: not read (macOS RM: RUSD allocate/map: 0x00000062)` —— **会话都没开起来，于是不给任何值**，正是要的行为。分类：**已实现并实测（拒绝路径）/ 正路径待重启后实测**。

**验证范围**：`cargo fmt --all --check` **rc=0**；`clippy -p nvoc-core -p pynvoc --all-targets -- -D warnings` **rc=0**；`cargo test -p nvoc-core --all-targets` **83 lib（含 3 个新投影用例）+ 5 fixture + 15 non-GPU 全过 / 0 失败**，50 个硬件门控保持 ignored。**如实记录一处环境限制**：`pynvoc` 的 Rust 单测在本机**无法链接**（默认 `extension-module` 下 pyo3 不链 libpython → `_PyBytes_AsString` 未定义；`--no-default-features` 又找不到 CommandLineTools 的 libpython3.9）——**旧 checkout 同样失败**，属既有环境问题；故本次 pynvoc 侧映射**只经 clippy 类型检查，未被执行**。

**新发现的诚实性风险（未定，不猜）**：楔形下 `get-status --macos-rm` 报 `pstate = 0`、`memory_used_bytes = 0`，而这两个字段**不在 `unavailable` 里**——即 `PERF_GET_CURRENT_PSTATE` 与 `FB_GET_INFO_V2` 返回了 `NV_OK`，同时所有 RUSD 字段都正确地 null 并注明 `0x00000062`。同一次开机 wedge 前的记录里，同一控制返回 `P1` 与非零 heap。若楔形 GSP 以**全零结构**回答 `NV_OK`，则 `publish_live` 标称的 `advancing` 是错的——正是本项目禁止的假零。**不在此下结论**：健康且**空闲**的基线从未记录过，空闲读数本就可能是 0。**重启后第一件事**（不加任何负载）读一次 `get-status --macos-rm`：若非零，则楔形确实在清零。**在拿到该读数之前不改代码**——此处猜测本身就是另一种假值。已写入整合计划 §6。

**未发出**：任何性能 SET、提权、驱动/EFI/boot-args 改动、重启、PR 操作。本轮新发的 GPU 请求只有三次只读：`get-settings`、两次 `get-status`。

### 本轮追加十四（2026-10-17，域目录成为数据 + 位命名空间的调查结论）

**做了什么**（commit `a069cee`）：把「0x20809019 返回 10 域目录」这句散文变成**数据**——`Device::settings()` 新增 `clock_offset_domains`（每项 `index`/`domain_bit`/`type_stamp`），仍由同一个纯投影函数从设备自己的块填入；**被拒 ≠ 空列表**（`None`，不是空数组）。`pynvoc` 侧输出为对象数组。

**不声称名称，只报设备给的位**：该字段是**位位置**而非索引——条目 2 带的是 bit 4 而不是 bit 2，这一条本身就否掉了「按索引命名」的诱惑。

**本轮由测量确立的两件事**（写进类型文档并附证据）：
1. **条目索引 = `0x2080901B` 选择掩码位**。记录证据：A5 前导运行 `seed@+4=0xFF` → `NV_OK, 106/527 dwords changed (first @0 last @564)`，106 = 14（头）+ 7×13（值 dword）+ 1（仅时间戳）= 与掩码选中 8 个条目（其中恰一个只写时间戳）**算术自洽**；同一次运行的 seed sweep 显示 `seed@+0/+8/+12/+16` 全部 `0x25 (NV_ERR_INVALID_DATA)`，**只有 +4 给 NV_OK**——掩码在 +4 是实验结论，不是推测。打印出的明细里 `+60:a5a5a5a5>a5a50808`、`+124:a5a5a5a5>a5a50909` 与目录条目 0、1 的类型戳逐一对应（索引对齐的直接证据）。
2. **类型戳分裂为 3×`0x0808` / 6×`0x0909` / 1×`0x0303`，仅写时间戳的那个是索引 6**。计数与目录给出同一个答案（106 里少掉的 12 个 dword 恰是一个只写时间戳的条目）。**如实标注**：A5 明细打印在记录 1 之后被截断（"last @564" 属记录 7），所以「索引 6」是**计数 + 目录类型分裂**两条独立证据的合取，而非逐记录打印。

**名称命名空间：调查结论 + 未证假设 + 证伪路径**（不再写「未知」）：设备用的十位是 bits 0..6 与 19..21；**公共 OGKM 头文件里没有任何时钟域枚举**（`grep DOMAIN_XBAR|MSD|LEG|PWR|HOTCLK|GPUCACHE` **零命中**）——所以不能从驱动树引用名字；唯一的交叉引用是 NVAPI/Windows 的 `NV_GPU_CLOCK_DOMAIN_ID`（32 宽，`nvapi-rs/sys/src/gpu/clock.rs:255`），其 0..6 = GPC/XBAR/SYS/HUB/M/HOST/DISP、19..21 = LEG/PWR/MSD，**与本设备用的两组簇完全同形**。**工作假设**（刻意不落到用户可见输出）：bit j = 域 j ⇒ 目录索引 0（bit 0）是核心引擎、索引 2（bit 4）是显存域。**证伪路径**：单域偏移读（或写入解禁后单域写）。在此之前偏移留在 `unavailable`、窗口保持域无关——而这**零代价**，因为三条范围记录本就一致。

**验证范围**：`fmt --all --check` **rc=0**；`clippy -p nvoc-core -p pynvoc --all-targets -- -D warnings` **rc=0**；`cargo test -p nvoc-core --all-targets` = **85 lib（含 2 个新用例）+ 5 fixture + 15 non-GPU 全过 / 0 失败**（50 硬件门控 ignored）。楔形下公共 CLI 实测：两字段均为 `null` + `offset channel: not read (… 0x00000062)`——**没读过的块绝不编造域列表**。分类：**已实现并实测（拒绝路径）**；正路径待 GSP 能应答。

**未发出**：任何性能 SET、提权、驱动/EFI/boot-args 改动、重启、PR 操作。本轮新发 GPU 请求仅 2 次只读（`get-settings`、`--anchor-read`）。
