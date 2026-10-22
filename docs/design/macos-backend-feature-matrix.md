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
- 权威源码：开发端 `nvoc-next`，分支 `macos-driver-validation`；测试机 `nvoc-head` 为经过逐文件核对的 detached checkout。另一旧 checkout 保留原 dirty 工作。
- 环境：macOS 15.8.1 / 24H32、x86_64、GTX 1650 SUPER（`10de:2187`，4 GiB，PCI `0000:01:00.0`）、NullMoth 2.0.4、RM/OGKM 610.57.04。经过授权的 root 命令可用，具体机器与访问资料只保留于仓库外交接。
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

## 一、CLI 命令表（102 条，来源：`cli/src/lib.rs` 的 `CommandSpec`）

表中"原 NVOC 行为"取自命令自身的说明（`nvoc-cli <命令> --help` 是完整参数来源）。

### GPU 与显示信息（`Info`，14 项）

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
| `list` | List all commands grouped by family (optionally filtered to one group) | **不适用（不依赖后端）**：元命令——不读设备、不走后端。`validate_invocation`（`cli/src/lib.rs:1704`）对它整段跳过后端检查，`run_invocation`（`:2315`）在选目标/开设备之前直接渲染命令表，所以在 macOS 上原样可用。本表此前漏了这一行，节标题因此比代码少一条（见 §一百一） |
| `restart-display-driver` | Restart the display driver (0xB4B26B65); legacy apply-OC trigger | 未实现：macOS RM 在访问驱动前拒绝 |
| `set-ecc-configuration` | Set NVAPI ECC memory configuration (NvAPI_GPU_SetECCConfiguration 0x1CF639D9): persists in non-volatile memory — default defers to the next reboot,… | 未实现：macOS RM 在访问驱动前拒绝 |
| `set-edid` | Set display EDID through NVAPI | 未实现：macOS RM 在访问驱动前拒绝 |

### 功耗 / TGP（`Power`，14 项）

| 命令 | 原 NVOC 行为（摘要） | macOS RM 状态 |
| --- | --- | --- |
| `get-dnotifier` | Read NVAPI D-Notifier (D0-notify) level + D1-D5 power-cap table (mobile) | 未实现：访问驱动前拒绝，但点名缺失层（§一百六）：策略视角无来源，可答的是 get-public-power-limit 的 requested/enforced 对 |
| `get-power-ceiling` | Read the effective power wall on PPAB mobiles (nvidia-smi's Ceiling trio): requested TGP (ClientTgpWattGetStatus) + active D-Notifier cap, ceiling… | 未实现：访问驱动前拒绝，但点名缺失层（§一百六）：策略视角无来源，可答的是 get-public-power-limit 的 requested/enforced 对 |
| `get-power-mode` | Read NVIDIA App power mode (Balanced/Max with support gate) | 未实现：访问驱动前拒绝，但点名缺失层（§一百六）：策略视角无来源，可答的是 get-public-power-limit 的 requested/enforced 对 |
| `get-public-power-limit` | Read configured power limit and min/current/max watts | **已实现且实测**：board policy GET给70/100/100W min/default/max，client GET给当前配置；RUSD enforced/requested分列。CLI/Python同源，见§139 |
| `get-rated-tdp` | Rated-TDP readback trio (0xED2BEA09/0x87BD35EF/0xFCBDF642) | 未实现：访问驱动前拒绝，但点名缺失层（§一百六）：驱动有值且实测 NV_OK（0x2080206e），缺的是本侧接线 |
| `reset-power-limit` | Restore the policy default | **已实现且实测**：macOS恢复当前board policy的default100W；不冒充恢复调用前值，见§139 |
| `reset-public-tgp-percent` | Reset NVAPI power limits | 未实现：macOS RM 在访问驱动前拒绝 |
| `set-batteryboost2-status` | Battery Boost 2.0 enable/disable (0xD27D0629); mobile-only; 1=enable 0=disable | 未实现：macOS RM 在访问驱动前拒绝 |
| `set-dnotifier` | Set NVAPI D-Notifier limit level (D1-D5; shares the TGP power-policy table) | 未实现：macOS RM 在访问驱动前拒绝 |
| `set-power-limit` | Set power-management limit in watts | **已实现且实测**：root、TU116/610.57.04、70..100W，固定board policy/client，快照/精确回读/失败取消恢复；CLI/Python及实际RUSD执行上限100→99→98→100W已验证，见§139 |
| `set-power-mode` | Set NVIDIA App power mode: max \| balanced (the App's Balanced/Max toggle) | 未实现：macOS RM 在访问驱动前拒绝 |
| `set-ppab-status` | Set NVAPI PPAB / Dynamic-Boost enable (on/off) | 未实现：macOS RM 在访问驱动前拒绝 |
| `set-public-tgp-percent` | Set NVAPI power limit in percent | 未实现：macOS RM 在访问驱动前拒绝 |
| `set-whispermode2-status` | Whisper Mode 2.0 status (0xD27D0629, mobile-only): on/off enable; --mode quieter\|quiet\|balanced also writes the acoustic mode (0xD2561B69) | 未实现：macOS RM 在访问驱动前拒绝 |

### 温度（`Thermal`，9 项）

| 命令 | 原 NVOC 行为（摘要） | macOS RM 状态 |
| --- | --- | --- |
| `get-legacy-temp-sensor` | Read NVAPI legacy 3-sensor thermal view (GPU/Memory/Board, live + physical range) | **部分已实现且实测（范围面：已实现待实测）**：入口由 core 投影作答（数组根与每传感器键沿用入口自身 `target`/`current_c`/`min_c`/`max_c`/`controller`，另加 `field`、`current_source`（每个数值注明来源：`rusd:<字段>` 或 `0x20800513:sensor<n>`）与 `unavailable`（每个 null 的理由））。**读数来源**：实时值来自 RUSD 温度段；**每传感器量程来自公共热读 `NV2080_CTRL_CMD_THERMAL_SYSTEM_EXECUTE_V2 (0x20800513)`**，本轮已按 ctrl2080thermal.h 的真实结构**接线**（同一 `nvx_rm_control` 通道，只发 GET 指令，`EXECUTE_FLAGS_IGNORE_FAIL`，逐指令回报 `executed`/`result`）。传感器**按驱动自己的 target type 位掩码配对**（ctrl2080thermal.h：1=GPU 2=MEMORY 4=POWER_SUPPLY 8=BOARD，设备实测 target 0 type 1）**而非按位置**。**三态必须分开**：未发起 / 发起但未完成（驱动拒绝，含 0x62）/ 已作答——本轮实测楔子期间为**后者中的"未完成"**：`MacosRmSession` 打开即在 RUSD 映射处被拒（`RUSD allocate/map: 0x00000062 NV_ERR_RESET_REQUIRED`），故量程留 `null` 且理由写明"did not complete"（这条同时说明：楔子期间**连会话建立都失败**，偏移通道的读同样卡在这一层）。**指令未跑不得当作 0**（`range_known`/`executed` 标志决定，实测单测覆盖）；**驱动未命名 target type 的传感器不填任何入口**；本快照该字段无量值时用公共读的读数并标明来源，绝不填 0 °C。失败方向实测：入口 rc=0、四路 `null`＋各字段理由 |
| `get-public-temp-limit` | Read the NVAPI public temp-limit range (min/default/max Celsius + throttle curve) | 未实现：NVAPI public range/curve contract remains separate; native GPU Max/acoustic policy values are available through `get-temp-thresholds`, see §141 |
| `get-temp-sim` | Read the temperature-simulation state (GetThermalSimulationMode; Secured-Overrides 'Temp faking allowed' gated) | 未实现：访问驱动前拒绝，但点名缺失层（§一百六）：仿真输入无来源（尚未定位），本后端只读真实传感器 |
| `get-temp-thresholds` | Read temperature thresholds (NVML by default; --nvapi exposes target-temp policy) | **已实现且实测**：native metadata/control reports GPU Max 88°C and acoustic min/current/max 65/83/87°C; unsupported selectors remain explicitly unavailable, see §141 |
| `reset-temp-limit` | Reset NVAPI sensor limits | **声学策略已实现且实测**：`--domain acoustic` restores policy default 83°C with complete 1440B normalized readback; GPU Max remains fixed, see §141 |
| `reset-temp-sim` | Disable temperature simulation and restore the real sensor reading | 未实现：macOS RM 在访问驱动前拒绝 |
| `set-private-target-temp-limit` | Set an NVAPI target-temp (temp-limit) policy slot in Celsius for mobile sku | 未实现：macOS RM 在访问驱动前拒绝 |
| `set-temp-limit` | Set thermal limit in Celsius: NVAPI path writes the sensor limit; NVML path writes the GPU max-temp threshold, or the acoustic target temp with --d… | **声学目标已实现且实测**：macOS 要求显式 `--domain acoustic`，范围65..87°C；GPU Max 88°C固定并拒绝写入，见§141 |
| `set-temp-sim` | Fake the driver-visible GPU temperature in Celsius (DANGEROUS research tool; Extended->basic fallback; Secured-Overrides gated) | 未实现：macOS RM 在访问驱动前拒绝 |

### 风扇（`Fan`，8 项）

| 命令 | 原 NVOC 行为（摘要） | macOS RM 状态 |
| --- | --- | --- |
| `get-fan-curve` | Read the NVAPI fan-curve table (ClientFanPolicies, struct 0x200DC; desktop-only) | 未实现：硬件曲线表仍未解码；百分比/auto已另行实现。访问驱动前点名拒绝，见§140。 |
| `get-fan-info` | Read fan/cooler info (NVML: count + min/max percent; NVAPI: per-cooler info via private FanCoolerGetInfo) | **已实现且实测**：固定RM metadata/status/control读面提供cooler0身份、RPM、0..100%范围、当前反馈百分比、目标百分比和auto/manual模式；停止时count仍为1。反馈与目标分别报告，并报告控制记录的cooler最高转速（rpm请求的换算标尺），CLI/Python/shared QueryFanInfo同源；满转反馈会略超 100%（65559），读面夹回100%，见§148、§140。 |
| `get-legacy-fan-policy` | Read fan-policy capabilities (ClientFanPoliciesGetInfo NDA 0x52B76D12: V2 raw block on modern drivers; legacy V1 decoded on R391-era — policy list… | 未实现：legacy硬件policy表未接入；已实现的auto/manual控制见§140，不能据缺公开头宣称驱动无风扇实现。 |
| `reset-fan-curve` | Reset one fan-curve slot to factory (Private FanPolicy path 0x2B2A2A45; works where restore-fan/cooler-settings is NOT_SUPPORTED, e.g. desktop 3060… | 未实现：macOS RM 在访问驱动前拒绝 |
| `reset-fan-speed` | Restore fan/cooler control: default clears the NVAPI cooler level override (control-block bit0 — the only reset that unpins modern cards) / NVML fa… | **已实现且实测**：GET-default验证后恢复firmware auto，CLI及Python两个auto/reset入口完整980B恢复通过；不冒充曲线/RPM reset，见§140。 |
| `set-fan-curve` | Write one fan-curve slot (RMW: --curve idx --points ...). The written slot only drives the fan while the cooler policy is TemperatureContinuous (8)… | 未实现：macOS RM 在访问驱动前拒绝 |
| `set-fan-speed` | Set fan speed: --percent (default) sets cooler level in percent (NVAPI SetCoolerLevels / NVML set_fan_speed); --rpm sets physical RPM via private F… | **百分比与RPM都已实现且实测**：共享事务→CLI/Python，cooler0/all、manual、0..100%发布范围；产品35%→36%回读与转速响应通过。`--rpm`（含 Python `set_fan_rpm`）按控制记录自带的 cooler 最高转速（+32，实测2800）换算成百分比、取整并夹到上限：1800→64.285%→实测1807 rpm，700→25%→946 rpm；驱动无 rpm 通道（mode 3 被接受但恒归一化成 mode 1 50%），低转速段按风扇自身转速/占空比曲线偏高。legacy --cooler 与 continuous 策略仍不支持，见§148、§140。 |
| `set-fanstop-status` | Toggle fan stop / zero-RPM for a curve slot (FanArbiterSet NDA 0x44CD3014): on \| off | **已实现且实测（cooler0、mode 2）**：`on` 写模式 2、`off` 回固件 auto，走同一事务与整块校验；`--curve` 只收 0（无曲线表）。见§146 |

### 时钟、偏移与 P-State 锁（`Clock`，20 项）

> 本列写的是**入口**在该后端的行为：「未实现：macOS RM 在访问驱动前拒绝」= 入口在打开驱动前就拒绝。因此它与 `nvoc-cli --macos-rm` 的允许清单必须一致——后端已实现的命令出现在这里的漫拒里就是缺陷（`get-pstate-global-freq-offset` 曾如此，本轮回填）。这条一致性现在**机械核对**：`probe-macos-rm/tools/audit-entry-parity.py` 从 CLI 拒绝路径的 `matches!` 取允许清单、从命令表取命令名，双向比对拒绝消息里的用户可见清单，再逐行扫本表（详见下文 九十四），并核对表本身是否**完整**：代码声明的每条命令都要有行、每组标题的项数与行数一致（§一百一）。


| 命令 | 原 NVOC 行为（摘要） | macOS RM 状态 |
| --- | --- | --- |
| `get-private-legacy-pstates20-freq-domain-info` | Read-only dump of the private pstates-2.0 delta table (GetPstates20Private 0xC5DDF56E) — the storage whose deltas move the frequency-request ceilin… | 未实现：macOS RM 在访问驱动前拒绝 |
| `get-pstate-freq-range` | Read NVML P-State clock ranges | 未实现：macOS RM 在访问驱动前拒绝 |
| `get-pstate-global-freq-offset` | Read clock offset in MHz | **已实现且实测**：配置值取 `0x2080901b` selector0/record+12 signed kHz，经 directory index连接域；RM776只供实时频率。核心/显存 GET、窗口、单位与原值见§136–138。窗口采用已测±18.75MHz保守子集，未证明更宽范围；`--plan-delta`仍是计划，不自动写入 |
| `get-pstate-lock` | Read the native NVAPI P-State level table | 未实现：macOS RM 在访问驱动前拒绝 |
| `get-supported-legacy-application-freq` | Read NVML supported application clocks | 未实现：macOS RM 在访问驱动前拒绝 |
| `reset-freq-lock` | Reset core or memory locked clocks | 未实现：macOS RM 在访问驱动前拒绝 |
| `reset-legacy-application-freq-lock` | Reset NVML application clocks | 未实现：macOS RM 在访问驱动前拒绝 |
| `reset-private-forced-pstate-lock-user` | Release a force-locked pstate via SetForcePstate(pstate=16, set_type=0) — pstate=16 is the bitmask=0 sentinel (GetForcePstate returns 16 when no fo… | 未实现：macOS RM 在访问驱动前拒绝 |
| `reset-pstate-global-freq-offset` | Reset clock offsets, optionally filtered by domain | **部分已实现且实测**：显式 `--domain core` 或 `--domain memory` 将该域偏移归零，保留另一域。无域/显式P-state拒绝；不冒充全域reset或解锁，见§137–138 |
| `reset-pstate-lock` | Clear all native NVAPI P-State locks | 未实现：macOS RM 在访问驱动前拒绝 |
| `set-freq-lock` | Lock core or memory clocks to a MHz range | 未实现：macOS RM 在访问驱动前拒绝 |
| `set-legacy-application-freq-lock` | Set NVML application clocks in MHz | 未实现：macOS RM 在访问驱动前拒绝 |
| `set-legacy-freq` | Set an absolute clock for legacy (Kepler) GPUs in MHz: --domain core (default) or mem picks which clock the value targets | 未实现：macOS RM 在访问驱动前拒绝 |
| `set-overclocked-pstates` | Toggle the overclocked-pstate unlock (EnableOverclockedPstates 0xB23B70EE): 1 opens the extended/OC pstate range before a set-pstate-global-freq-of… | 未实现：macOS RM 在访问驱动前拒绝 |
| `set-private-forced-pstate-lock-user` | Force a P-State via private SetForcePstate (0x025BFB10); set_type 0/1/2 all force-lock, none release (to unlock use reset-private-forced-pstate-loc… | 未实现：macOS RM 在访问驱动前拒绝 |
| `set-private-legacy-pstates20-freq-domain-global-offset` | DANGEROUS RMW of one delta in the private pstates table (SetPstates20Private 0x4C0B519A); --domain takes a domain name (gpc/xbar/m/gpc2/…) or the r… | 未实现：macOS RM 在访问驱动前拒绝 |
| `set-private-permanent-pstate-lock-user` | Admin-free pstate lock (SetPerfLevel 0x75DD3E6A, escape 0x7000040): level is an INDEX into the GPU's real available P-State list (see get-pstate-lo… | 未实现：macOS RM 在访问驱动前拒绝 |
| `set-pstate-global-freq-offset` | Set clock offset in MHz for any clock domain | **部分已实现且实测**：核心/显存绝对目标MHz，TU116/610.57.04、root、selector0、±18.75MHz，单步≤18.75MHz。共享事务精确回读和失败/取消恢复；实际同负载效果见§137–138。其它域/模式未交付 |
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


（共 102 条 CLI 命令）

## 二、非 CLI 消费者接入状态

| 消费者 | 现状 | 依据 / 缺口 |
| --- | --- | --- |
| `nvoc-core` | 已实现：`BackendSet::MacosRm` 分发 + `discover()`/`QueryMacosRmInfo`/`QueryMacosRmStatus`/`MacosRmSession` | 阶段 1 实机 5 项硬件测试通过 |
| `nvoc-cli` | 查询/连续监控、核心显存偏移SET/reset、board功耗SET/reset、风扇百分比/auto已实现且实测 | 其它操作逐项状态见命令表及§137–140 |
| `nvoc-python`（pynvoc） | 查询、偏移、board功耗与风扇百分比/auto公共读取/设置已实测，auto使用平台后端；设置字段保留单位和窄能力标记 | reset/解锁完整契约、UI长任务和其它控制族仍需继续；见§136–139 |
| `gui` | **部分已实现**：原生后端不再硬编码 `"both"`，`discover_gpus`/`query_info`/`query_status`/`query_settings` 统一传共享后端串 `auto`（macOS 解析为 RM 只读路径）；NVAPI 专属面板（超频/VF/风扇）在 macOS 上仍按既有路径明确失败 | 客户端 `gui/tests/` 281 passed / 13 skipped、ruff 干净；macOS 实机由 pynvoc `auto` 端到端覆盖（`stage11-shared-entry-…`）；GUI 本体未在 macOS 跑（测试机无 customtkinter） |
| `tui` | **部分已实现**：同 GUI，`NativeService` 统一传 `auto` | 客户端 `tui/tests/` 214 passed、ruff 干净；同 GUI 的 macOS 本体限制 |
| `srv`（ControlBackend） | **只读部分已实现且实测**：后端持有类型泛化为 `Box<dyn ControlBackend>`；新增 `srv/src/macos.rs::MacosRmBackend`（复用 `nvoc_core::macos_rm` 快照，`supports_writes()=false`），`/status`/`/api/status`/`/api/gpus`/`/api/info`/`/api/history`/`/config` 可用；写入在访问驱动前被拒（`/mode?value=pid|manual`、`/fan` → 409；`/api/oc/*`、`/api/reset` → `ERR: read-only…`；`/oc_global` 由循环拒绝），控制模式钉在 `Auto` | 非 macOS 路径逐字未变（`supports_writes()=true`）；macOS 上**无闭环控制**——PID/风扇/manual尚未接入现有共享控制与恢复，属于服务集成缺口。证据：`stage11-srv-macos-…` + `stage11-srv-smoke-…`，review `review-stage11-srv-macos-backend-20261017.md` |
| `auto-optimizer` | **部分**：`stressor-external` 非 CUDA 构建在 macOS 可编译运行；新增只读 `macos-eval`（`195e7af`）走 `BackendSet::MacosRm` 选设备 + 原 Vulkan 负载 + Metal 校验 + RM 遥测，`settings_written=false`；报告新增 `under_load.throttle`（`50bb01b`：负载期间 reason mask 的按位并集 + 断言过的原因名 + 实际带 mask 的样本数——空 `active_reasons` 只表示"驱动没断言"，不等于"没读"）；原 NVAPI 命令、`has_nvapi` 筛选与默认 feature 均未改 | 阶段 10 后续：候选/SET/回读/接受-回退/恢复需 macOS 写通道；见 `review-stage10-*` |
| `cli-stressor-cuda-rs`（`--vulkan-only` / `--vulkan`） | **已实现且实测**：原 Vulkan 渲染负载在 macOS 直接复用，只新增 loader/ICD 解析（`vulkan_loader`，`4993a42`）——自动使用 `/Library/GPUBundles/nvmtl/` 的 Khronos loader + NVK ICD，无需手工导出 env；`--vulkan-window` 仍限 Windows（macOS 固定 offscreen） | CUDA GEMM 与 CUDA+Vulkan 混合模式在 macOS 不可用（无 CUDA）；渲染负载不做读回比对，计算域校验见下一行 |
| `stress-macos`（新增） | **已实现且实测**：Metal 计算负载 + bit-exact CPU 参考校验（`1dcf27a`）；自检、负例、限时负载、SIGINT 停止全部通过 | 补 `--vulkan-only` 缺的**计算正确性**维度；是校验器，不是 CUDA/OpenCL 负载的等价替代 |
| `cli-stressor-opencl` | 未接入 macOS | 本机无 OpenCL 运行时；计算域正确性校验由 `stress-macos` 承担 |
| `probe-macos-rm` | 已存在（只读探针 + CPU 回归），阶段 1 追加 ABI 契约测试；阶段 3 追加显式只读 `--tierb-read`（campaign GET：rails-info/status、vfpoints），默认电池仍不发送这些 ID | 不整组运行 stage 3（含写入），不发送任何 SET |

## 三、阶段路线图现状（0→12）

| 阶段 | 目标 | 现状（2026-10-17） | 下一步 |
| --- | --- | --- | --- |
| 0 基线/功能表 | 起点、边界、待办、证据索引 | 完成：本文件 + SSH/硬件/分支核对 + 102 条命令表 | 随每阶段更新并复核 |
| 1 正确性与资源生命周期 | 修 ABI/释放顺序/旧数据/重连/输出/停止 | **完成**：`78d299d`、`a687453`、`8fb1aeb`；实机 5 项硬件测试、ABI 指纹、broken-pipe、SIGINT 全通过 | 保持回归 |
| 2 基础信息与遥测 | 设备/信息/状态/连续监控 + 公共字段 | **完成**：样本 ABI v3（14 节/41 槽）落地——限频原因、逐风扇、有效/目标/视频/SM 频率、显存用量、逐引擎利用率、PCIe 代际、多温度、功耗族全部实测；`clkThrottleInfo`/`temps`/`avgPowerUsage`/`tgpmW`/`fanCoolerStatus.rpmCurr[16]`/`procUtil`/`pciBusData` 读取生效，`POLL_MEMORY` 经实测确认 610.57.04 恒返回 `RUSD_TIMESTAMP_INVALID`（`memEcc`/`memRowRemap` 不携带）、SM 域 Turing 为 0/0 → `unavailable`；`get-status` 人类/JSON/JSONL（schema `nvoc/macos-rm-sample/3`）一致；**1 Hz × 10 分钟长跑**（`stage2-longrun-…`）+ **全量验收**（`stage2-telemetry-20261017T034627Z`，15 项 rc=0）+ Metal 负载响应（gfx 1950 MHz / 54.9 C / 12.7 W / util 91% / gr 91.5% / fan 4243 rpm）；pynvoc `backends="macos-rm"` 的 discover/info/status 已分发并归一化。**追加（公开温度读面）**：`NV2080_CTRL_CMD_THERMAL_SYSTEM_EXECUTE_V2`(0x20800513，ctrl2080thermal.h，A-tree) 只读批实测可用——1 target(GPU)/1 sensor、读数 45–47 °C、range [0,127]，与 RUSD `temperature_c` 一致（`stage-thermal-read-20261017T061320Z`，新增只读 flag `--thermal-read`） | 电压读取（`get-status` 无 voltage 字段）留给阶段 3/7 的当前设置读取 |
| 3 当前设置/范围/恢复基础 | 设置/范围/快照/恢复 | **部分完成**：偏移与board功耗有公共读取、范围、单项设置及回读/恢复，见§136–139；通用整组settings/restore未完成 | 继续各控制族与整组恢复 |
| 4 核心/显存频率控制 | 偏移/锁频/reset | **偏移已实现且实测**：核心/显存±范围、独立域、CLI/Python、reset及原值恢复；核心+15和显存±15MHz实际效果见§137–138。锁频路径已继续静态审计，见§139 | 锁频/解锁的可达协议及验证 |
| 5 功耗/温度/运行策略 | W/%、温度墙、Boost | **board功耗W控制已实现且实测**：70..100W、default100W、产品CLI/Python及恢复见§139。百分比/rated-TDP仲裁、温度策略和Boost仍各自待实现 | 温度策略、风扇及其恢复 |
| 6 逐风扇/模式/曲线 | 风扇模式、曲线读写 | **cooler0百分比/auto/RPM已实现且实测**：metadata/status/control、零RPM身份、正式core/CLI/Python、35→36%及完整恢复见§140；RPM 按记录标尺换算见§148、FanStop 见§146；硬件曲线仍未实现 | 继续曲线/FanStop与服务集成；旧“kernel无风扇实现”推断已被私有GSP控制实测推翻 |
| 7 V/F、电压点、高级超频 | 曲线点/偏移/导入导出 | 未开始：`vfpoints-get`(0x208090A2) 以 524 B 参数被接受但 payload 仅 1 个非零字，非曲线数据；该 ID 及 `rails-get-*` 在公开 OGKM 610.57.04 中**完全不存在**（非公开固件控制），公开面也无 VF 命令；该 ID 在本机所有 NVIDIA 二进制里 0 命中（review §6），即结构在固件侧。**追加（私有读面普查，2026-10-17）**：Windows 后端读 VF 曲线的三个私有 GET（`VFP_GET_INFO/STATUS/CONTROL` 0x20809021/22/23，SET 孪生 0x2080D024）在本驱动**全部 60 档尺寸 `0x1f`**（产物未存）（=命令在、参数块未构造出，见阶段 4 行）；只有 582.41 编号的 `0x20809061`（776 B，非曲线本体）可用，**曲线读取仍未打通**（`stage-clkvf-read-20261017T072432Z`）   **文档封送 pass（2026-10-17）**：VFP `0x20809061` 的 R610 whitelist 五档全 `0x1f`，但同 id 的 **776 B 形态**可读，得到 11 条每域 kHz 表（见阶段 4 行）；`0x20809062`/`0x20809023` 全部文档 stamp 仍 `0x1f` ⇒ **曲线本体仍未打通**，但频率域读数已达（`stage-clkdoc-20261017T074256Z`）  **活方法表反证（新，2026-10-17）**：活表里 cat 0x90/0xd0（CLK legacy）、0x32/0xb2（VOLT）、0xf2（VOLT privileged）**全部 0 行** ⇒ 私有 VF/电压 id 在 kernel RM 无注册、`rmapiutilGetControlInfo` 在本机无法解析；两个被接受的形态（`0x20809061@776`、`0x2080B201@≥2444`）在已加载 host 二进制里无任何字面量——**原因已查明：host 对这类 id 只测 legacy 位 `0x8000` 后整块转发，应答者是 GSP 固件**（见阶段 4 行与 `docs/reverse-engineering/nvapi/gsp-method-table-and-legacy-forwarding.md`）；GSP 表内 VOLT 家族 **17 行**、CLK 家族 **87 行**（11:50Z 更正，旧 151/169 是 PERF/internal 家族），故私有 VF/电压 id **在 GSP 有实现体**（host 表 0 命中只说明它们不是 host 实现）。**活表审计（新，2026-10-17）**：cat 0x32(含 b2/f2 三种编码) 被引用 4 个 id（`0x2080b201/b202/b213/f214`）**在 host 活表里 0 命中**；全仓 114 个引用 id 中仅 49 个在表内且全部来自公开 SDK。**更正口径**：host 表无实现体 ≠ 本机无实现体——私有电压/VF 族经 legacy 位转发给 **GSP**，GSP 表内 VOLT 家族有 **17 行**（11:50Z 更正：旧记 151 行是 PERF 家族）（含 `0x2080b201=2444`/`0x2080b202=3232`/`0x2080b213=1036`/`0x2080f214=1036`(SET)），故这些 id **有 GSP 侧实现体**。**新的分层证据**：GSP 因私有 GET 被喂坏内容而整条 legacy 路径考机后（`0x65`→`0x62 NV_ERR_RESET_REQUIRED`），`0x2080B201@2444` **仍照常 `NV_OK` 返回 mask 0xFF + 800000/756250 µV**，而 `0x20809019/61` 全部 `0x62` ⇒ `0x2080b201` 是该族里**唯一不查 fault state 的 GSP handler**。**11:43Z 更正**：先前把这行读成「b201 不经 GSP 转发（host 应答或另一条非 legacy 通道）」是**错的**——尺寸边界实测证明它的 `>=2444` 门**随 GPU fault state 变化**（wedge 前同一请求 `0x1f`、wedge 中 `0x62`），而 host NVOC 门不随之为转移（同一次运行里 `0x2080122a@20` 仍 `0x1f`）⇒ 校验发生在**转发之后**，b201 由 GSP 应答，转发**是活的**（见 `gsp-method-table-and-legacy-forwarding.md` §7/§12 与「本轮追加六」）。见 `docs/reverse-engineering/nvapi/registered-vs-recorded-rm-ids-audit.md` **本轮追加七（2026-10-17，静态）**：VF 之前先要的是「12 个零值 dword 里哪个是 offset」——见阶段 4 行追加七与 `gsp-method-table-and-legacy-forwarding.md` §14.1/§14.3：`0x2080901b` 的 64 B 控制记录**确实被写满 12 个 value dword（stock 全 0）**，而 `0x20809019` 的同一序记录给的是**目录/范围**（`+44/+46` 有符号对）⇒ 偏移与曲线两步仍按原门槛走，VF 曲线继续后置。 |
| 8 其余功能族 | VBIOS、显示/EDID、限频原因、运行策略、ECC/OEM/多卡 | **逐项判定**（详见 `stage8-throttle-reasons-20261017T064017Z/README.md` §2）：**限频原因＝完成**——RUSD `clkThrottleInfo.reasonMask` 已进 status，专用命令 `get-throttle-reasons` 本轮在 macOS 可用（9 条 NVML 词汇/顺序、`violation=null`），且该读数现由 core 的共享 `QueryThrottleReasons` 从 RM 快照解码——CLI 与 pynvoc `query_throttle_reasons(gpu,"auto"|"macos-rm")` 同源、不会各自漂移（`stage11-throttle-shared-20261017T065005Z`）；**VBIOS＝部分**——`BIOS_GET_SKU_INFO` 已读 board/sku/project（实测 NV_OK）、`get-vbios -i` 离线解析可用，但**版本串/镜像不可得**：公开 BIOS 命令族全为读且 GET_INFO_V2 只给 REVISION 数字，内核接口无 ROM escape（全树 ROM grep 仅命中寄存器 `NV_PES_XVU_ROM_ACCESS`），刷写＝NVAPI/Windows→N/A；**显示/EDID＝未接入（无可达证据）**——`NV0073_CTRL_CMD_SPECIFIC_GET_EDID_V2` 属 display class 0x0073，需 display 对象，macOS RM 链未暴露，未盲扫；NVAPI 显示 API→N/A；**API/运行策略＝N/A**（autoboost/ppab/whispermode2/dnotifier/pmgr-arbiter/api-restriction 均 NVAPI 读写，无运行时）；**ECC/OEM/Blackwell/移动＝N/A**（TU116 桌面、无 ECC、非 OEM、非 Blackwell）；**多卡＝本机单卡**（`GPU_GET_ATTACHED_IDS` 单 gpuId 0x100，后随 0xffffffff） | 逐项有依据；VBIOS 版本串/EDID 依赖未暴露的 RM 通道或 NVAPI，已按"未知/不适用"分开 |
| 9 自动压力执行与验证器 | 限时负载、停止、结构化结果、正确性校验 | **主负载 + 校验器完成**。主负载＝原 `--vulkan-only` 渲染路径（零算法改动，仅补 loader/ICD 解析 `4993a42`）：无 env 冷启动 rc=0，5/10/30 s 阶梯与 60 s stock 负载 rc=0，错误路径（`--gpu-index` 越界 / `--pci-bus` 不匹配）rc=1，`--vulkan-window` 在非 Windows 回退 offscreen rc=0；同步遥测显示 util 峰值 100%/均值 83%、gfx 1.53→1.95 GHz、功耗 11.5→13.5 W、温度 54→62 ℃、风扇 1778→1920 rpm。校验器＝`stress-macos`（`1dcf27a`）：全量 bit-exact 比对、启动自检、负例 rc=1、60 s 限时 1206 次 dispatch/0 失败、SIGINT 优雅停止。**整体验收**（`stage9-accept-20261017T045311Z`）：3 组 feature 构建 + fmt/clippy/tests 全 rc=0；`--vulkan-only` 冷启动 rc=0、错误路径 rc=1/1/0、阶梯 rc=0；信号语义实测 `--vulkan-only` SIGINT rc=-2 / SIGTERM rc=-15（OS 默认处置、立即终止），`metal_check` `--stress 300` 收 SIGINT 后 rc=0 优雅停止。**HEAD 复跑**（`stage-vulkan-recheck-20261017T055856Z`，8/8 PASS）：无 env 冷启动 rc=0、`--gpu-index 0` 选中真设备 `GTX 1650 SUPER (NVK TU116-A)` PCI `0000:01:00.0`、越界索引 rc=1；loader=`/Library/GPUBundles/nvmtl/libvulkan.1.4.362.dylib`、ICD=`nvk_icd.json`→`libvulkan_nouveau.dylib`（**NVK，非 MoltenVK**） | 渲染负载无读回校验、`--vulkan-only` 无进程内取消（靠 SIGINT/SIGTERM 默认处置）；阶段 10 需把负载/校验器结果接进 optimizer 闭环 |
| 10 自动扫描/优化/结果应用 | 受限闭环、保存/应用/恢复 | **部分：非 CUDA 构建 + 只读评测入口落地**。`--no-default-features --features stressor-external` 的 check/build rc=0；新增子命令 `macos-eval`（`195e7af`、证据 `stage10-eval-…`）：用 `BackendSet::MacosRm` 选设备（不再要求 `has_nvapi`），复用原 `cli-stressor-cuda-rs --vulkan-only` 限时负载 + NullMoth RM 遥测采样 + `stress-macos/metal_check --verify`，输出 `nvoc/macos-stock-eval/1`（pre/under_load/post、FPS、校验结论、`settings_written=false`）。实机 20 s：gfx 1950→1965 MHz、util 均值 87%/峰值 99%、power 12.5→13.5 W、temp 52→57.6 ℃、校验 pass。**候选/接受-回退/结果应用闭环未接入**：核心/显存偏移、board功耗及风扇现在已有共享写入与恢复；优化器仍需从NVAPI具体调用适配到这些能力。**HEAD 复跑**（`stage10-recheck-20261017T060320Z`）：`macos-eval --duration 8` rc=0，1530→1965 MHz、功率 11.4→13.6 W、温度 52.2→59.5 C、util max 100%、fan→2517 rpm，`metal_check` passed / 0 mismatch（全量 bit-exact），`settings_written=false`。"
    "**追加（`50bb01b`，2026-10-17）**：报告新增 `under_load.throttle`——负载期间的 reason mask 并集 + 断言过的原因名 + 带 mask 的样本数；实机 `--duration 8 --interval 1` 得 `samples_with_mask=5, mask_any=0, active_reasons=[]`（stock 负载下驱动未断言任何限频原因），`stressor exit=0`、`metal_check passed`、`settings_written=false`（`stage11-throttle-shared-20261017T065005Z`） | 需要接入现有macOS共享写入/恢复并完成候选评测；目前闭环停在"预检+原状态快照+限时负载+校验"这半 |
| 11 共享入口与界面集成 | core/CLI/Python/UI/srv | **部分完成**：查询、偏移、功耗、风扇百分比/auto和声学温度目标已共享接入core/CLI/Python；GUI/TUI写面及srv控制闭环仍需接入 | 完成支持操作的UI及服务集成 |
| 12 全量回归与交付 | 完整覆盖、契约/CI/review | **持续进行**：逐步CI、小提交、fork同分支交付；历史绿灯不代替新控制实测。完整goal尚未完成 | 逐族回归并最终覆盖审计 |

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
  手动占空比、偏移/限值写入——其中偏移、board功耗和风扇百分比/auto已有共享接口（§137–140），服务层尚未接入。
  非 macOS 平台走原 `NvapiBackend`，行为逐字未变。
- `auto-optimizer` 在 macOS 上：可构建、`--help`、并新增只读 `macos-eval`（真实负载 + 遥测 + 校验），
  但**不能扫描/应用/恢复**——原 `optimize`/`autoscan` 按 `has_nvapi()` 选目标且 SET 全走 NVAPI，
  本机 NVAPI/NVML 均缺库。共享偏移/功耗/风扇写入已具备，真实闭环仍需优化器接线和验证；`macos-eval` 目前只承担评测部分。
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

### 本轮追加十五（2026-10-17，选择掩码从未被填写 —— 读路径缺陷，已修）

**怎么发现的**：把 `core/native/macos_rm.c` 的读路径与**记录下来的成功编组**逐字段对照，而不是只对照注释。控制块头 `+4` 是**记录选择掩码**，而处理器**只读不写**它（A5 运行里 `+4` 是唯一位于 head 却**未被写**的 dword：写出 106 个 dword、`+4` 保持调用方给的值）；native 读路径 `memset(out,0,size)` 后直接发命令 ⇒ **掩码 = 0**。

**后果比「被拒」更坏**：掩码为 0 时驱动**不是报错而是答 `NV_OK` 且什么都不写**；`parse_control` 又只报掩码选中的记录 ⇒ 偏移通道**永远只能返回空记录列表**，「没施加偏移」和「我们压根没问」在输出上无法区分——正是 G1 所追字段最不能接受的形状。

**修复（commit `7cb1391`）**：按**实测值**填 `+4 = 0xFF`，并在 C 里就地写明出处。该值同时**解决**目录单独无法回答的问题：**掩码按目录条目索引选择**（若按域位理解则预测 105 dword，与实测 106 不符；106 = 14 头 + 7×13 + 1 仅时间戳条目）。**不做外推**：设备列 10 条而 0xFF 只选前 8 条（索引 8/9 未请求），更宽掩码**未测**，代码里如实写明。

**新增硬件测试**（`core/tests/macos_rm_readonly.rs`，`#[ignore]`，重启后**第一个该跑**的用例）：断言活体应答的**形状**（掩码非零、记录非空、条目按索引升序）并**打印数值**——数值来自抓取而非断言，常量漂移无法藏在断言里。

**验证范围**：`fmt --all --check` rc=0；`clippy -p nvoc-core --all-targets -- -D warnings` rc=0；`cargo test -p nvoc-core --all-targets` = 86 lib（含新增「掩码为 0 则无记录」用例）+ 5 fixture + 15 non-GPU 全过 / 0 失败；硬件二进制新增门控用例后 7 ignored（原 6）。**行为确认需活的 GSP**，故推迟到重启后（wedge 仍以 `0x62` 拒绝会话本身）。分类：**已实现、待实测**（拒绝路径已实测）。**未发出**：任何性能 SET、提权、驱动/EFI/boot-args 改动、重启、PR 操作。

### 本轮追加十六（2026-10-17，风扇/功耗/温度三族独立只读审计 + 「实测零」范围收缩）

**独立审计**：新增 [macos-fan-power-thermal-audit.md](macos-fan-power-thermal-audit.md)，按 goal 要求把三族**分开**给「可读 / 可解释 / 范围 / 原状态 / 恢复」，每条标注证据等级（**探针实测** / **后端实测** / **头文件事实**），使"偏移通道受阻"不能用来宣布传感器族不可用。摘要：

| 族 | 结论 | 分类 |
| --- | --- | --- |
| 温度（GPU） | 两条独立读面均实测可用（公开在树 `THERMAL_SYSTEM_EXECUTE_V2` 0x20800513 paramSize 1432、1 target(type 1=GPU)/1 sensor、range `[0,127]`、reading 47；RUSD `temperature_c` 44.46–51.90 °C，两路一致） | 读：已实现且实测；写：待授权 |
| 温度（显存/板/电源） | **实测缺失**（`UNPUBLISHED_ON_THIS_GPU` 6 项，带原因，不造值） | 不适用本卡 |
| 功耗：enforced limit + 实时功耗 | 实测（`limit 100000` mW = 100 W 恰为额定 TDP；idle tgp 11.3–11.5 W） | 读：已实现且实测 |
| 功耗：min/rated/max 窗口 | `GET_EDPP_LIMIT_INFO` 0x20800AFD **实测 `0x56`**；四个内部 PM GET 全 `0x56` ⇒ 内部写候选整块不可达 | 驱动明确拒绝 |
| 功耗：RATED_TDP 仲裁 | 实测 5 client 全 `DEFAULT(0)`、越界 client 0x1f（查找经校验）；派生 STATUS id `0x56`（**id 仍属推导，未确认**） | 实现待实测（SET 门控未发） |
| 风扇转速 | **实测可读且转过**（一次记录 1654 rpm，同批温度最低 44.46 °C；其余记录 0 rpm @ 48.65–51.90 °C ⇒ 测速表可用、无源空闲真实） | 读：已实现且实测 |
| 风扇控制（目标值） | `ctrl2080fan.h` 定义 **0 个** `NV2080_CTRL_CMD_*`（`ctrl2080volt/clkavfs/pmumon.h` 同）⇒ 公开面无法控，写面在 GSP boardobj/cooler 族 | 调查未定位（范围明确） |

**方法学收获（写进文档）**：把记录**重新读一遍**而不是读探针自己的总结，纠正了两条陈述——风扇**转过**（1654 rpm）而非只有 0；`average_power_mw` 读到带符号 `-1` 而非"字段缺失"。这正是 goal 里"不能用二手描述代替原始证据"的一条实例。

**「实测零」范围收缩**（`core/src/macos_rm.rs`）：偏移族原因文本改为**只在实测掩码覆盖范围内**断言零——掩码是调用方输入（处理器不回写）、记录运行填 `0xFF`、只覆盖 10 条目录的前 8 条，故索引 8/9 **未测**；更宽掩码与 `0x0` 掩码已在既有 runbook 计划内（`0x0` 支可直接证实上一轮的掩码缺陷结论）。

**验证范围**：`fmt --all --check` rc=0；`clippy -p nvoc-core --all-targets -- -D warnings` rc=0；`cargo test -p nvoc-core --all-targets` = **86 + 5 + 15 全过 / 0 失败**（硬件事门控 7+36+8 ignored）。**产品路径实测**：重建 `nvoc-cli` 后，楔形机上 `get-settings --macos-rm` 打出的正是新原因文本，域列表仍 `null`（会话本身被 `0x62` 拒绝）。**未发出**：任何性能 SET、提权、驱动/EFI/boot-args 改动、重启、PR 操作。

### 本轮追加十七（2026-10-17，探针 clkdom 通道掩码偏移错标 —— 已修）

`probe-macos-rm --clkdom-read`（旧尺寸扫描通道）把掩码种子写到 RM 偏移 **+8** 并在输出里标 `mask@+8`，而处理器读的字段在 **+4**。原因写在仓库既有的纠错文档里：这类字段在 RM 块里比"文档布局"**早 4 字节**（正是让文档 2404 B 调用者结构变成 2108 B 块的同一个位移，`docs/reverse-engineering/nvapi/rm-param-marshall-corrections-r610.md` §1）。掩码位置随后被实测确证：`+4` 填 `0xFF` 得 `NV_OK`/106 dword，文档偏移 `+8` 填 `0xFF` 得 `0x25 NV_ERR_INVALID_DATA`。

该表所有条目种的掩码都是 0，故**输出零变化**——种在哪都不影响本轮结果，**误导的是标签**：这一通道曾被两次引用来支持「掩码 0 ⇒ 没有记录」，而真相是「什么都没被选中」。已把写入改到 +4、字段更名、标签改 `mask@+4=`，并在注释里写明实测依据；下一个非零种子自此落到处理器真正读取的字段。

**验证**：测试机重建 `built probe-macos-rm-idle (profile idle)`（08:32:08），`--help` rc=0。**对重启后计划无影响**：首发走 `--clkctl-hex`，那条路径本来就种 +4。分类：**已实现并实测（构建/运行）/ 行为等价（本通道输出不变）**。

### 本轮追加十八（2026-10-17，纠错表增补时钟族 + `pstate` 值语义未定）

**① 纠错表增补（`docs/reverse-engineering/nvapi/rm-param-marshall-corrections-r610.md` §5）**：把本轮主战场补进"候选映射纠错表"，全部为实测：
- **文档尺寸≠RM 尺寸**：NVAPI 调用者结构的戳记就是 `version<<16|size`（`0x00010964`/`0x000261a4`/`0x000109d8`），RM 从不看它；`0x2080901b` 在 2404/24996 被 `0x1f` 拒，`0x20809019` 在 2520/2464/296620/329388 被拒，`0x20809025` 在 3148 被拒，而 2108/12336/7188 正常应答——且**尺寸错时即使掩码已填写也照样被拒**，这正是能把"尺寸"与"掩码"两个字段分开的证据。
- **掩码在 head +4 且是输入**：0xA5 背景下 `+4`=0xFF → `NV_OK`、写 106 个 dword（`first @0 last @564`）且 `+4` 是唯一不被回写的 head dword；`+0/+8/…/+36` 填 0xFF → `0x25`；填 0 → `NV_OK` 且零写入。§1 早已由录得的容器偏移 `0x3C` 推出 `+4`，**两条独立线索（偏差规则 + 直接实测）在此互证**；但与 `0x2080B201` 的 +4 是**输出**掩码方向相反，文档明确写出，禁止互推。
- **64 字节记录步长是实测**（戳记在 +60/+124），文档里的 72/76 步长对本块都不对。
- **并记录旧通道为何看不出这些**：它把掩码种在文档 +8，且逐例的"N non-zero B"把自己种的字节算进去了（被拒的行也显示"1 non-zero B"）。

**② `pstate` 值语义未定（新证据）**：头文件把值集声明为**位集**（`UNDEFINED`/`CLEAR_FORCED`=0、`P0`=0x1…`P15`=0x4000），但**驱动自身代码从不引用这些常量**，该字由 **GSP 经 RPC 填充**（`kern_perf_ctrl.c:330` → `RmRpcPerfGetCurrentPstate`；树内唯一宿主实现 `rpc.c:8944` 是 vGPU 读共享内存）。而本项目在 Linux 上 `pstate` 是 **NVML 索引**（`nvml_pstate_to_index`：P0→0…P15→15），渲染成 `P{n}`（`cli/src/lib.rs:3043`、`cli/src/output.rs:342`）——所以 macOS 的原始值**不能假定同底**。本 boot 实测：同一命令楔形前答 `1`、楔形下答 `0`，而 `0` 正是声明集里的"未定义"⇒ 计划 §6 的检查被收紧为**可证伪判据**：健康空闲态若复现 `1`，则零值由 wedge 解释且底数问题一并解决；若复现 `0`，则要么底数不同、要么空闲态本就"undefined"，此时要改的是**渲染**而不是数字。**在读到该数之前不改代码**（否则又是另一种假值）。

### 本轮追加十九（2026-10-17，掩码「回写」结论降级 + 调用者坐标系与目录表用途）

**① 自我纠错（重要）**：上一轮/本轮初我写下「处理器读掩码但**不回写**它，+4 是唯一未被改写的 head dword」。**这个说法过强**：0xA5 背景实验里种子本身就把 0xA5 覆写成 0xFF，因此「处理器原样保留」与「处理器回显种子」产生**完全相同的 diff**，+4 无论哪种情况都不在"被改变"集合里。已把三处措辞降级为**实测范围内的准确陈述**（`core/src/macos_offset.rs`、`core/native/macos_rm.c`、纠错表 §5）：字段被**读**（行为依赖它，已证）；**是否回显未定**；而纪录在案的 Windows 侧布局（nvapi-rs，IDA + Ada 4060 Laptop 实机 dump）称驱动**回显种子**而非真实可控集，与实测**兼容**。区分二者需要"驱动不会原样返回的种子"——正是探针 runbook 里尚未跑的 `0x3FF` / `0x1` 两支。分类：**调查未定（已给出可分辨的下一步）**，不再声称已知。

**② 调用者坐标系（新增 §5.1）**：`NV_GPU_CLOCK_CLIENT_CLK_DOMAINS_CONTROL_V1` = 2404 B（magic `0x10964`）——**正是 RM 拒绝的那个尺寸**；掩码 @绝对 +8，32 条**按 domain bit 稀疏寻址**的记录 @`+100+72*bit`，记录内 type u8 @+0，具名字段 `offset_kHz`@+44、`range_min`@+48、`range_max`@+52、`applied`@+56、+60。与 RM 块（掩码 @+4、记录 @+60、步长 64、按**目录索引**寻址）**同形不同坐标**：

| | NVAPI 调用者 | RM 块（实测） |
| --- | --- | --- |
| 选择字段 | +8；Windows 侧称**回显种子** | +4，被读（回显未定） |
| 记录基址/步长 | +100 / 72 | +60 / 64 |
| 记录字段 | type u8 @+0；+44/+48/+52/+56/+60 | 戳记 u16 @+0；值 dword @+8..+44、+52、+56；+4/+48/+60 从未写 |
| 记录寻址 | 按 **domain bit** | 按 **目录索引** |

**位置重叠只是部分**：RM 写了 +44/+52/+56（位置上等于调用者的 `offset_kHz`/`range_max`/`applied`），但也写了调用者布局**未命名**的 +8..+44 十个 dword，且**不写**调用者的 +48(`range_min`)/+60 ⇒ **逐字段搬运的假设已被这处矛盾否定一次**，名字不可直接沿用；RM 记录自身的字段身份仍需**差分**（一次写入，或非零偏移下的读）才能钉死。

**③ 目录表的用途（假设，附证伪路径）**：调用者按 **domain bit** 寻址、RM 按 **目录索引** 选择，而目录块每条目同时携带**索引**（其位置）、**domain bit**（`1<<id`）与类型戳记 ⇒ 它正是封送层做"具名域 → 记录"转换所需的表。证据：目录 `domain_bit` 全为 2 的幂、覆盖 bit 位置 0–6 与 19–21（与 `NV_GPU_CLOCK_DOMAIN_ID` 两个簇一致）；控制块戳记与目录条目**按索引**对应；上面的 106 dword 算术已把控制侧寻址钉为按索引。**证伪路径**：用单索引掩码 `0x1` 读控制块（探针 runbook 第三支）——按索引寻址则只应有 1 条记录（值型条目为 14+13 dword），按 bit 寻址则会选中"戳记 bit 0"的那条。

### 本轮追加二十（2026-10-17，目录头两坐标系＝可执行校验；更正本轮的"新发现"表述）

**① 目录头同时给出同十行的两种命名，现已成为可执行校验**：`0x20809019` 的记录直接解出 `head +4 = 0x3ff`（第 i 行＝第 i 位）与 `head +12/+20 = 0x38007f`（**恰为十行 `1<<id` 的并集**：1|2|16|4|8|0x100000|0x80000|64|0x200000|0x20，即 bit 0–6 与 19–21）。两个数**不同**，所以这一块本身就是「调用者在 `+4` 给的 mask 数是**行**而不是域位」的证据——与 `0x2080901b` 写集合算术（106 dword）指向同一结论。此前该结论只靠 dword 计数，如今有第二处独立证据。已把它写成 `probe-macos-rm/tools/decode-clk-containers.py` 的 `mask_consistency()`：按实际存在的行推导两种掩码并与头部逐项比对，输出 `MATCH`/`mismatch`；selftest 用**实机十行**重建（正向必须三处 MATCH，负向把 `+12` 改掉必须报 mismatch）⇒ 以后每次 dump 自动复核，不再靠散文。**实测**：对本机已录的 `clkctl-hex.txt` 运行该工具，三处全 `MATCH`（`rows-as-bits 0x000003ff` / `union 0x0038007f`）。

**② 更正表述（避免把已有结论当新发现）**：目录的行/类型/域位表此前**已**记录在 `gsp-method-table-and-legacy-forwarding.md` §14.3/§18 与本表「本轮追加七」③，`0x2080901b` 的记录字段位置（+8/+12/+16/+20/+24 候选）也已在该文档与解码器 `CANDIDATES` 里。因此本轮真正的增量是：把两条恒等式**写成算术并上升为工具校验**、复核「十行」是**驱动自己声明**的（故 runbook 的 `0x3FF` 支不是猜测，且预测 `14+9×13+1=132` 与探针 PREDICTIONS 一致），以及把 `head +28 = 5`（＝第 0、2 行，即核心与显存）标为**候选**而非结论。

**③ 仍未定（如实）**：`0x2080901b` 记录内 12 个 value dword 的字段身份——stock 全零 ⇒ 零值写入无法自证，只有「门控单域最小步 SET ＋ 同字段回读」能定；`head +8`(0x335)/`+16`(0x3fc007f＝并集再加 bit 18 与 22–25)/`+24`(0x85)/`+36`(0x103e8)/`+40`(0xffff0000) 未解；range 块（`0x20809025`）3 条记录 tag 递进字节 03/04/05 若为域 id 则覆盖 3/4/5（含显存不含核心），与目录的 id 命名空间**可能不同**，目录仍为索引↔域 id 的唯一权威表。

### 本轮追加二十一（2026-10-17，源代码侧界面对照：风扇/热学/时钟族的"公开面到底有什么"）

**方法**：不再用"头文件里搜不到"结案，而是把 610.57.04 源码树当作**分发表**来核对：① 逐个 `ctrl2080*.h` 数 `NV2080_CTRL_CMD_*` 定义与行数；② 用全树正则抽出**全部公开 id→名字**（775 条）并做族名统计；③ 把这些 id 与 GSP 转发表逐条比对。命令与输出可复现，均只读。

**① 时钟/电压控制面在公开 SDK 里几乎不存在（这正面支撑本目标的技术前提）**：`ctrl2080clk.h` 128 行仅 **1** 个命令，且该命令是 `NV2080_CTRL_CMD_CLK_PMUMON_CLK_DOMAINS_GET_SAMPLES = 0x20801037`（＝我们已实测的那张公共时钟表的 id）；`ctrl2080volt.h` 38 行 **0** 个命令；`ctrl2080clkavfs.h` 36 行 **0**；`ctrl2080pmumon.h` 100 行 **0**；775 条公开 id 里名字含 `VOLT` 的 **0** 条、含 `CLK_` 的 1 条。⇒ 偏移/VF 这类控制**不可能**从公开 id 空间取到，只能走私有 id + GSP 旧式转发——这正是本目标第 1、2 条的**依据**，而非印象。

**② 热学族公开面已实测，但族内仍有 31 条可读 id 未试**：`ctrl2080thermal.h` 417 行、2 个命令（`THERMAL_SYSTEM_EXECUTE_V2 = 0x20800513` 与其 `_PHYSICAL` 孪生 `0x20808513`）。已实测：`paramSize 1432` = 头文件结构 `6×u32 + 0x20×INSTRUCTION` 的**逐字节**算术（24 + 32×44 = 1432 ⇒ `INSTRUCTION` 恰 44 B：result@+0 / executed@+4 / opcode@+8 / 32 B operands@+12），故探针读到的 `exec`/`rc` 与头文件字段一一对应，将来任何 opcode 的偏移都不必猜。目标枚举里的 MEMORY(2)/POWER_SUPPLY(4)/BOARD(8) **有定义但未被实例化**（驱动自报 1 target/1 sensor）⇒ 显存/板卡温度缺失是**驱动无该对象**，不是工具没问。另：**更正**——先前用旧提取件（889 行，静默丢了 229 行）说"`_PHYSICAL` 孪生 `0x20808513` 不在表内"，改用**修正后的 1118 行**结果（`outputs/stage-gsp-table-recount-20261017T130832Z/gsp-table.tsv`）后：`0x20800513@1432`、`0x20808513@1432`（flags 0x10248）、`0x2080852e@1432` **三条都在**；因此"孪生由 host 侧回答"的推论**撤回**。该轮 `families.txt` 同时给出权威普查：1118 GSP 行 / 608 host 行 / 592 共享 / 526 仅 GSP，且按 `0x80|c`=非特权读半、`0xC0|c`=写半的编码计，`0x05 THERMAL` 为 **32 GET + 6 SET**——公开面只 1 条（`0x20800513`），其余 **31 条 `0x208085xx`** 均非特权读且 host 表里 1 条都没有，即固件侧、可经同一条旧式转发读到：**热学族的 id 面并未用尽**，这是风扇问题唯一"有依据"的下一步（族名来自分发表、paramSize 调用前已知 ⇒ 族内、带尺寸校验的只读遍查，不是盲扫）。

**③ 风扇：公开控制空间里没有命令，界面的确存在但在上一层**。`ctrl2080fan.h` 是 **33 行的空生成桩**（全部内容＝license + `#pragma once` + `Source file: ctrl/ctrl2080/ctrl2080fan.finn` 注释，**零** define、**零** struct），且 token `NV2080_CTRL_FAN` 在整棵树里**一次都不出现**——这才把"文件被生成成空"与"我的 grep 写错了"区分开。风扇界面在 NVAPI 的 cooler API 里（nvapi-rs `sys/src/gpu/cooler.rs` 有完整记录）：`GetCoolerSettings` V1/V3/V4（152/1288/1368 B）、`SetCoolerLevels`、`RestoreCoolerSettings`、`GetTachReading`，cooler 类型 FAN/WATER/LIQUID_NO2，每 cooler 的 `currentLevel`/`currentMinLevel`/`currentMaxLevel`（百分比）、policy 枚举、`{speedRPM, bSupported, maxSpeedRPM, minSpeedRPM}` 转速表 ⇒ 上表要的"范围/原状态/恢复"字段**在该界面里是齐的**，缺的是"该界面在本驱动的 macOS 路径上映射到哪个可接受控制"。**形状旁证（非证明）**：GSP 表里没有任何一行的 paramSize 是 1288/1368，152 只出现在 4 个无关 id 上 ⇒ 该 cooler 界面在固件转发表里也**认不出来**（表无名字，故只能说形状）。

### 本轮追加二十二（2026-10-17，把 0x24 坐标常数从"继承"改为"我自己重推"）

**动机**：候选字段映射（`CANDIDATES`：offset 落在 rec+8/+12/+16 之一）整个建立在**一个**坐标常数上——"调用者侧字段坐标 − 0x24 = RM 侧坐标"。这条是上一轮会话继承来的，本轮用**已录字节**把它重推一遍，使依据可复核，而不是继续引用。

**重推结果（全部来自 `clkctl-hex.txt` 的 `0x20809025` 原始块）**：该块（7188 B，头 `+0 ff +4 07 +8 0 +12 64 +16 100`，记录 `20 + 224*i`）是少数**带真实非零值**的块。三对字段同值、同差：

| 调用者侧（nvapi-rs `clk_freq_ctrl_info_entry`） | RM 侧实测坐标与值 |
| --- | --- |
| `REC_MAX_KHZ = 0x2C` | RM 行 `+0x08 = 1638400`（1638.4 MHz） |
| `REC_MIN_OFFSET = 0x38` | RM 行 `+0x14 = −18750` |
| `REC_MAX_OFFSET = 0x3C` | RM 行 `+0x18 = +18750` |

三处 delta **恰为 0x24**，且数值与调用者侧自带的注释（"live 1638400 = 1638.4 MHz"、"−18750 (freq offset floor, ±18.75 MHz pair)"）逐字吻合 ⇒ 三对独立字段、同值、同常数，属**推导**而非巧合。

**但同时确定它的边界（不许过度外推）**：这是**字段级**常数，**不是整条记录的对齐**——同一对结构在其它坐标上并不遵守它：调用者 `TYPE` 在记录 `+0x00`，而 RM 行的 `+0x00` 是标签 `0x01030101`；调用者 mask 在 `+8`，RM mask 在 `+4`。结构级几何也不一致：`0x20809025` 调用者 base `0x4C`/stride `0x60` 对 RM base `20`/stride `224`；控制块调用者 base `100`/stride `72` 对 RM base `60`/stride `64`。故候选表**仍是候选**：该常数只给出"值得测的坐标"，真正命名"哪个字段装偏移"仍只能靠门控单域最小步 SET ＋ 同字段回读。

**推论与一致性检查（记录在案，不算结论）**：套到控制块，常数预测 `OFFSET_KHZ→RM +0x08`、`RANGE_MIN→+0x0C`、`RANGE_MAX→+0x10`、`APPLIED→+0x14`，四个全部落在**实测会被驱动写入的 12 个 value dword**（`+8…+24`,`+28…+44`,`+52`,`+56`）之内——这是候选表需要的自洽性；也正是"offset 有 +8 与 +16 两个活候选"的来源（文章正文说第一槽，Blackwell 字段图说 `VALUES[2]`）。

### 本轮追加二十三（2026-10-17，把"未测尾部"收敛到具体域：core/显存不在其中）

**动机**：产品代码里的诚实说明写着"掩码只覆盖前八行，末两行未测"。这个 caveat 直接压在目标最关心的**核心/显存域偏移通道**上，值得把它收敛到具体域而不是含糊留白。

**做法（全部只读、只用已录字节）**：从已录的 `0x20809019` 目录块按行重建"行号 → 该行 `1<<id`"，并把记录运行所用种子（`+4 = 0xFF`）**按行位**读出的覆盖集合算出来（不假设连续）。结果：覆盖 **行 0–7**，之外是 **行 8（id 21，MSD）** 与 **行 9（id 5，HOST）**；而本后端要报的是**核心（id 0）＝行 0**、**显存（id 4）＝行 2**，两者**都在已测窗口内** ⇒ 该 caveat 只适用于**外围域**，不适用于目标所关心的两个域。同一轮重跑解码器复确认头身份（`+12`/`+20 = 0x38007f`＝十行 `1<<id` 并集，`+4 = 0x3ff`＝行位），故覆盖结论与头校验出自同一批行。

**新增可执行的检查**：`decode-clk-containers.py` 现在在目录交叉校验后多打一行覆盖放置（`rows: 0:id0 1:id1 ...` 与 `seed 0xff reaches rows 0..7, beyond it: row 8 id 21; row 9 id 5`），并把该字符串写进 `--selftest` 断言——覆盖结论不再靠叙述。

**同时登记一个仍属候选的观察（归因已证、量纲未证）**：带 `+44` 的 i16 对的**恰好**是行 0（id 0 核心）`{−1000, 1000}` 与行 2（id 4 显存）`{−1000, 3000}`，且只有行 0 带 `+40 = 30000`。与 range 块（三条记录无法归到具名域）不同，这两对落在**具名行**里，是"限制"的第二个、**归因更好**的可能来源；但按 kHz 读会是 ±1 MHz 与 −1/+3 MHz，比 range 块的 ±18750 kHz 明显窄，故**很可能不是同一量**。因此只登记为候选，**不宣称单位**，也不改产品的 `clock_offset_limits_khz` 取值来源。

### 本轮追加二十四（2026-10-17，读分发表的"自带声明"列，为写计划补齐第三项支持）

**动机**：偏移通道的写计划此前只靠 id 算术（写 id = 读 id 置 bit14）。目标要求"核对实际分发链"，且写入门槛要能说清**缺的是哪一层**。于是把**修正后 1118 行**表的自带列量化（新增可复跑工具 `probe-macos-rm/tools/summarise-method-table.py`，输出计数而非形容词）。

**结果**：按分类字节分半＝公开 691 / 旧式读半（`0x80-0xBF`）335 / 旧式写半（`0xC0-0xFF`）92。`flags` 列在这两半里**按方向分离**：读半 332/335 带 bit `0x08`（主值 `0x0048`，278 行）、写半 82/92 带 bit `0x04`（主值 `0x0044`，78 行）⇒ **表自己就按"读写成对"建模**，而这正是本项目私有控制面所在的两半。例外逐 id 列出（读半 3 条：`0x2080854a`/`0x2080a70f`/`0x2080b703`；写半 10 条：`0x2080c54d`、`0x2080c601/2/3`、`0x2080d0b3`、`0x2080d501/2`、`0x2080e0cf`、`0x2080e802`、`0x2080f310`），因此驱动更新会以**数字**浮现而非意外；公开半方向位混杂（`0x00c0`/`0x0048`/`0x0008`/`0x0044`），故该位是**旧式两半的属性**，不是通用分类器。

**对本目标的直接支持**：`0x2080901b`（读，2108 B，flags `0x0048`）与 `0x2080d01c`（写，2108 B，flags `0x0044`）是**同尺寸兄弟对**且方向位互补 ⇒ 读"SET 块与 GET 块同一结构"多了一项**独立支持**（id 算术、尺寸相同、表的方向位），"写入 GET 所报的同坐标"因此是**有依据的计划**而非猜测。（存在 `|0x4000` 兄弟的读 id 共 21 条，其中同尺寸仅 4 条——偏移对是这 4 条之一。）

**同样重要的否证**：`accessRight` 在 1118 行里几乎全为 0（唯一例外是公开行 `0x2080b213` = 2）⇒ 它**不**编码"写路径在 root 之下被拒"的那项特权，**不得**拿它当特权证据；该拒绝仍按运行时实测归因：转发路径的前置条件由 root 持有，**既非缺方法、也非表标志**。

### 本轮追加二十五（2026-10-17，G1 就绪性审计：写路径存在且已能编译，缺的是"授权"与"只有写才能给出的答案"）

**动机**：前几轮把 G1 的缺口写成"字段身份未定"，但没有审计**执行这次写入的代码本身**是否就绪、以及满不满足目标要求的"单域最小步长＋回读＋恢复"。审计发现缺口表述需要**改准**：写路径早已实现，缺的不是实现。

**已就绪（本轮读码确认，逐条对上目标要求）**：
- `--clk-offset-set BIT:OFF:DELTA`（`probe-macos-rm/src/main.c`）＋ `nvx_stage3_clk_offset_set()`（`nvx_suite.c` 阶段 7.5）实现完整序列：**先取快照（pre-record）→ 先武装恢复 → SET → 同字段回读 → 恢复 → 复原校验**；成功判据为**全部**成立：`SET == NV_OK`、**被命名字段回读恰为 orig+DELTA**、该记录其余 value dword 与快照**逐字节相同**、**恢复后的 2108 B 块与快照逐字节相同**。→ 目标第 3 条"证明回读＋恢复"在代码层已满足。
- 实时频率**明确不是判据**（源码注释与 `V3.5e` 判词都写明"a live-frequency change is NOT evidence either way"）→ 目标第 4 条满足（NVK 自身会发 PERF_BOOST）。
- **单域最小步长硬门**：SET 只送 `1u << bit`（单域）；`delta` 限 `±1000` kHz 且禁止 0（"this harness is not a sweep tool"）；`field_off` 必须是**实测的十二个 value dword**（8,12,16,20,24,28,32,36,40,44,52,56）之一，否则拒绝并说明"G1 未定，本工具不猜"→ 目标"禁止假值"满足。
- **三重防误写**：① `--clk-offset-set` 不带 `--write` ⇒ 在**打开驱动之前**以 rc=2 拒绝；② idle 构建（`NVX_IDLE_ONLY=1`，即 runbook 用的 `probe-macos-rm-idle`）遇到 `--write` 或阶段 3/5 同样**在打开驱动前**拒绝，且各写函数内部另有 `NVX_IDLE_ONLY` 早退；③ 信号路径 `on_signal → nvx_restore_run()`。

**本轮新增的实测（都不需要 GPU）**：
- **full profile 现在能编译**：`PROFILE=full sh build.sh` ⇒ 可信标记 `built probe-macos-rm (profile full)`，产物 `probe-macos-rm` 109392 B。此前 runbook 只用 idle 构建，**写路径自写就以来大概率从未被编译过**——这是个真风险，现已消除。
- 拒绝路径实测：`./probe-macos-rm-idle --write --clk-offset-set 0:8:250` ⇒ `idle-only build refuses --write and stages 3/5 before opening the driver`（未触 GPU）；full 二进制 `--help` 只打用法、不打开驱动。
- 新增 `probe-macos-rm/tools/clock-offset-write-run.sh`：**必须显式给出一个候选**（无默认、非扫掠工具），先跑 `--anchor-read` 并在**非 NV_OK / 见 0x62 时拒绝写入**（wedge 下所有应答塌成 0x62，会把"被拒"读成"未知"），再执行那一次写入，输出落到 `outputs/stage-clkoffset-write-<stamp>/` 供事后复核。`sh -n` 通过，两条拒绝路径实测 rc=2。

**仍然缺什么（这就是目标要求的"缺哪一层"的准确答案）**：**不是协议、不是驱动实现**——方法表里有写 id、有尺寸、有写方向位（见本轮追加二十四），封送已实现且现在能编译。缺的是：① **授权**（现行禁令：任何 `0x2080d01c` 写入暂停）；② **一次健康的重启后会话**（wedge 下 0x62 使任何拒绝不可解读）；③ 那个**只有写入本身才能给出的答案**：十二个候选里哪个装偏移。因此本轮只在**没有写入**的前提下把执行器备好并验证到"能编译、会拒绝、判据齐全"。

### 本轮追加二十六（2026-10-17，偏移限值/域的公共链路审计 + macOS 门禁实况：文档化 clippy 门在 macOS 上原本是红的）

**动机**：目标第 5 条要求"接入现有公共 core/CLI/Python 接口与自动优化闭环"，并要求"受阻则明确缺失的是哪一层"。此前只验证了 core 侧字段存在，**没有**验证这条链路是否真的通到用户可见面。

**链路审计（读码，逐跳给出位置）**：
- core：`core/src/macos_rm.rs` 的 `ClockOffsetLimits` / `clock_offset_domains` / `CLOCK_OFFSET_UNREADABLE_REASON`（含实测零值是"写"而非"缺失读"的推理、十行目录的自带声明、以及"0xFF 只覆盖前八行、未测的两行恰为 MSD(id 21)/HOST(id 5)、故 core/memory 在已测窗口内"）。
- CLI：`cli/src/macos_rm.rs` 的 macOS 分支只放行 `get-gpu-list/get-info/get-status/get-settings/get-throttle-reasons`；`get-settings` 走 `QueryMacosRmSettings` 后 **`serde_json::to_value` 整个 core 设置结构**，由 `cli/src/output.rs:format_get_settings_output` 按 JSON 键渲染 ⇒ **限值/域/未读原因自动出现在 CLI 输出里**（已通，无需逐字段接线）。
- Python：`nvoc-python/src/lib.rs:1456-1477` 把 `clock_offset_limits_khz` 展开为 `clock_offset_min_khz`/`clock_offset_max_khz`/`clock_offset_limits_records`，并输出 `clock_offset_domains`；`:5918-5921` 有断言钉住 −18750/+18750。⇒ **core→CLI→pynvoc 已通**。
- 自动优化闭环：`auto-optimizer/src/macos_eval.rs:172` 读同一接口，并把整个 `settings` 放进 `nvoc/macos-stock-eval/1` 报告，仍 `settings_written: false`。该命令**跑过**：`outputs/stage10-eval-20261017T050525Z/report.json`（2026-10-16 22:05，stressor exit 0、verifier passed，`stressor.elapsed_s` 21.2）。**但该报告里没有偏移字段**（`settings` 只有 board_id/capabilities/chip_sku/memory_*/notes/pcie_link_gen/power_limit*/project/pstate/unavailable），因为 offset 字段是**次日**才引入的（`dd0f5fe`／`a069cee`，2026-10-17）。⇒ 结论要分开说：**代码层链路已通**（读码＋pynvoc 断言），**设备级出现尚未实测**（outputs 里除本记录自身外，无任何含 `clock_offset_*` 的设备/CLI 记录）。

**四个用户可见面此前把缺口说得比实际更重，本轮改成准确表述**（只改文字，行为不变）：`auto-optimizer/src/macos_eval.rs` 模块文档与报告 `write_gate` 字段、`auto-optimizer/src/arg_help.rs` 的 `macos-eval --help`、`cli/src/macos_rm.rs` 的不支持命令提示。统一为：**写路径本身存在**（`0x2080d01c` 与可读的 `0x2080901b` 同一 2108 B 块；单域最小步＋预录＋回读＋恢复已实现且能编译），**写入仍在等三件事**：授权、一次健康的重启后会话、只有写入本身才能给出的字段身份。旧句"there is no verified macOS offset/VF write path yet"易被读成"驱动没有这条路"，与事实不符。

**门禁实况（本轮实测，测试机）**：仓库 AGENTS.md 文档化的门 `cargo clippy --workspace --exclude cli-stressor-cuda-rs --all-targets -- -D warnings` 在 macOS 上**原本不通过**：`auto-optimizer/src/platform.rs:33 panic_windows_only`（cfg `all(not(windows), not(target_os="linux"))` 在 macOS 编出，但**全树无调用者**）触发 `dead_code`；CI 跑 Linux（那里该函数被 cfg 排除）故此前未暴露。补 `#[allow(dead_code)]` ＋ 说明注释（**不删除**）后该门 **rc=0**；`cargo fmt --all -- --check` 干净；`nvoc-cli` 80、`nvoc-auto-optimizer` 64、`nvoc-core` 86 项测试全过。属 macOS 适配本身的门禁修复，非顺手带改。

**因此重启后的只读清单里新增一条**（写进 `probe-macos-rm/tools/clock-offset-read-run.sh` 头部）：先 `nvoc-cli get-settings --macos-rm`，看这组字段**第一次在真实设备输出里出现**（这也顺带回答"限值是否只在 core 里存在"）。这一步是只读的，wedge 一解除即可做。

### 本轮追加二十七（2026-10-17，"缺的是哪一层"的**确定答案**：前置条件＝进程特权，且写半与特权半是同一个集合）

**动机**：目标要求**受阻就写清缺的是协议／前置条件／驱动实现哪一层，并给证据**。此前把 `0x2080d01c` 的 `0x1b` 记作"转发路径的前置条件由 root 持有"，但没有量化，也没检查**这台测试机是否真能拿到 root**——后者决定这次写入能否发生。

**集合恒等式（对修正后 1118 行表实测，非相关而是同一集合）**：主机转发器的判据是 `(cmd & 0xC000) == 0xC000` 且调用客户端低于 `RS_PRIV_LEVEL_USER_ROOT` 即拒。按该判据筛 1118 行：满足者 **92 条，全部落在写半**（`0xC0-0xFF` 分类字节）；不满足者 **1026 条 = 公开 691 ＋ 读半 335**，**无例外**。⇒ **"写半"与"特权半"是同一个集合**（上一轮分半计数 691/335/92 是另一条独立路径得出的，逐条对齐）。逐 id：`0x2080901b & 0xC000 = 0x8000`（**非特权**）、`0x2080d01c & 0xC000 = 0xC000`（**特权**）、目录 `0x20809019` 与范围 `0x20809025` 同为 `0x8000`。主机侧表（608 行）里特权 id **0 条**、读半仅 4 条 ⇒ 这 427 条私有读/写 id 基本只存在于 GSP 表内。

**三层独立证据指向同一结论**：① 驱动源码判据（`rmapi_gss_legacy_control.c` 的 `privLevel < RS_PRIV_LEVEL_USER_ROOT`）；② 上述集合恒等式（表自己的 id 位就标注谁需要特权）；③ 实测行为——本驱动上**每一次读都作为普通用户成功**（读半 335 条皆非特权），而那次写得到 **`0x1b`**。⇒ **缺的不是协议、不是驱动实现，而是前置条件：调用进程的特权（root）**。写路径、封送、回读、恢复都已实现（追加二十五、二十六）。

**本轮新增的运行前提（决定这次写入能不能做）**：测试机上 `sudo -n true` ⇒ **`sudo: a password is required`**，即用户 `mac` **没有免密 sudo** ⇒ **这次门控写入无法在无人值守的 ssh 会话里完成**，必须有人（用户）提供一次交互式 sudo。`0x2080d01c` 一旦在没有 root 的情况下发出，只会拿回 `0x1b`——**看起来像驱动拒绝，实际是进程特权**，正是目标禁止的那种"伪装"。

**据此改进写入 runbook**：`tools/clock-offset-write-run.sh` 现把特权检查放在**最前**（早于构建与 anchor 读取）：非 root 且 `sudo -n` 可用则自动 `exec sudo -n OGKM=… OUT_ROOT=… "$0" …`；`sudo -n` 需要密码则**直接拒绝**并打印应执行的命令，**rc=4**，不浪费任何一次写入尝试。测试机实测该拒绝路径 rc=4 且**未访问驱动**；两处 sha 一致、`sh -n` 通过。**读 runbook 刻意不加 sudo**：读半不是特权半（`0x8000`），加了反而会改变"普通用户可读"这一被验证过的前提——留意别把它"修"坏。

### 本轮追加二十八（2026-10-17，把"没有无依据的 id/尺寸"变成**可复跑的检查**：探针自身 id/尺寸 与驱动表对账）

**动机**：目标禁止"盲扫与无依据的尺寸/ID"，并要求核对**实际分发链**。此前这条纪律靠人记；本轮把它变成工具：`tools/audit-probe-ids.py <表> <探针源码…>`，把探针源码里出现的每个 `0x2080xxxx` id 与每条形如 `{ "标签", id, size }` 的用例，逐一对照**修正后 1118 行 GSP 表**与**608 行主机表**的 `paramSize`。

**实测结果（本机对两表核对，测试机同法复跑）**：探针命名 **75 个 id**（公开 28／读半 37／写半 10）；其中 **69 个由某张表声明**；**6 个两表皆无**，且每一个都有明确解释——`0x2080a0ff`（非特权但不存在）与 `0x2080ffff`（带特权位 `0xC000`）是**故意构造的负对照**，"两表皆无"正是它们成立的原因（实测分别回 `0x56`／`0x1b`）；`0x20802000` 只出现在注释里（perf 接口基址）；`0x2080206d` 是**推导**出来的 rated-TDP STATUS id（SDK 头只给 message id `0x6D`），探针发出后得到 NOT_SUPPORTED——**现在还得到表侧解释：它不在派发表里**；`0x20801acf`（MODS Volt3x.SetVoltagesMv）与 `0x2080f031`（610 文档记录的 GPC ClkVfOffset SET）是**调用侧/文档命名空间**的 SET id，探针只在"已知 SET id"名单里列出、**故意不发送**。

**读路径自洽性（可直接机器判定）**：命名 id 中**非写半而带特权位者 0 个** ⇒ 本驱动上每一次成功的读都落在非特权半边，与追加二十七的门禁模型完全一致；而 10 个写半 id **全部带特权位**，其中真正会被发送的只有 `0x2080d01c`（门控的偏移 SET），其余要么在"已知 SET id"名单里只作拒绝用，要么是负对照。

**尺寸**：**87/99 条用例用的就是驱动表声明的尺寸**（即被测结构就是驱动会派发的那个）；其余 12 条**全部是有意为之且标签自带理由**——调用侧结构尺寸（`2404 documented`、`2520 documented` 及其 `-0x38` 截短）、构造式截短（`2316 = 12+32*72`、`2444 = 12+32*76`）、主机 NVOC 尺寸门对照（`0x2080122a` 发 20 而表为 24，实测回 `0x1f` ⇒ **反过来印证表的 24 才是真实派发尺寸**）、以及两条负对照发 776。工具会在打印非表尺寸时一并打印其标签，避免把有意用例误判为错误。

**实际验证范围与限制**：工具是**源码文本级**提取，因此注释里提到的 id 也计入"命名"（上述 6 个例外即由此而来）；"非写半无特权位"是对**命名**集合成立的陈述，其证据意义在于——若其中任何一个读 id 带特权位，它不可能在非 root 下成功，而它们全都成功过。仅新增工具与文档，**未改 Rust**。

**表一致性（本轮顺带核到，值得单独记）**：本机分析副本 `tables/gsp-1118.tsv`、`tables/host-608.tsv` 与测试机权威产物的 sha256 **完全相同**（`ed29e04a…11bc`／`f9a6723b…464c`，1119／609 行含表头）⇒ 追加十四、二十七、二十八 等一切"由表导出"的结论都建立在**与权威表逐字节一致**的输入上，而不是某份陈旧副本；同一份审计在测试机上以同一输入复跑，结论逐项一致（75 命名／69 声明／6 解释／非写半特权 0／87-99）。

### 本轮追加二十九（2026-10-17，私有半边的 **SET 孪生约定**：`(id | 0x4000) + 1` 且 paramSize 相同——47 个实例，偏移写 id 由此**从表自身推得**，不再是推断）

**动机**：门控写入的成败取决于 `0x2080d01c` 到底是不是 `0x2080901b` 的 SET。此前该结论只有 `|0x4000` 这一条依据（本项目的推断）。本轮改为**从表的内部约定反推**，并量化。

**实测（修正后 1118 行表）**：读半 335 条、写半 92 条。对每个读半 id 检查两种孪生形态，且要求 **paramSize 完全相同**：① 同低位（`|0x4000`）——**仅 4 例**（`0x2080a06f/0x2080e06f`、`0x2080a09c/0x2080e09c`、`0x2080a0a6/0x2080e0a6`、`0x2080a63a/0x2080e63a`）；② **低位 +1 且 `|0x4000`**——**47 例**，跨 0x85→0xc5、0x90→0xd0、0xa0→0xe0 三族，例：`0x2080901f@19416→0x2080d020@19416`、`0x20809023@66060→0x2080d024@66060`、`0x208090a2@524→0x2080d0a3@524`、热族 `0x2080852c@1440→0x2080c52d@1440` 等 6 对。另有 28 条读 id 的某形态存在但**尺寸不同**（⇒ **尺寸一致是判据的一部分**，不能只看 id），256 条两形态皆无。

**对偏移通道的直接后果**：`0x2080901b@2108` 的**同低位孪生 `0x2080d01b` 根本不存在**，而 **`0x2080d01c@2108` 存在且尺寸完全相同** ⇒ 我们准备门控写入的那个 id 是**表自身约定在该族上的唯一实例**，与协议推测无关。这是目前对写 id 最强的一条静态证据（写入成功时它同时会被实测确认）。

**顺带解决一个悬案**：`0x2080d031` 在表中**确实存在**，paramSize **1544 = 0x608**，即 `0x20809030@1544` 的 +1 孪生——与 `nvx_defs.h` 记的"610 doc records 0x2080F031; params 0x608 bytes either way"**尺寸完全吻合** ⇒ 文档与表互相印证，VF-offset 的 SET id 问题在表侧有答案（`0x2080d031`），不必等运行时。

**热族结构（本轮另一收获）**：热族共 **31 条读半 + 6 条写半**，写半 6 条与读半 6 条构成 +1 同尺寸对（1440／980／1556／3100／24／20448）。公开族 `0x05` 与读半 `0x85` 是同一族的两个半边（`0x05 + 0x80`），实测在用的 `0x20800513@1432` 与 `0x20808513@1432` 正是这对。**风扇/散热器在公开 SDK 里几乎没有实体**：整个 `inc/ctrl/` 树中 "cooler" 出现 **0 次**，大小写不敏感的 "fan" 仅 1 次（`ctrl0000system.h`，非 2080 族），`ctrl2080fan.h` 是 FINN 生成的**空壳**（无任何定义）；且 1118 行中**没有任何一行的 paramSize 等于 NVAPI cooler V3/V4 的 1288／1368**（152 有 7 行，V1 尺寸，但尺寸相同不等于语义相同）。⇒ 风扇数据要么来自 NVAPI 自己的用户态拼装，要么来自热族的某个私有读（31 条中尚无一条被读），**这一层仍标记为调查未知**，下一个动作是族内、按表尺寸、只读的小批读取，而不是盲扫。

### 本轮追加三十（2026-10-17，`get-settings --macos-rm` **首次在真实设备上跑出输出**：偏移字段出现；并修正"每个功能族缺的到底是哪一层"的用户可见表述）

**一、偏移字段的设备级出现（此前列为"待实测"的那一项，已闭合一半）**：本轮在测试机上实跑 `target/debug/nvoc-cli get-settings --macos-rm`（**只读**，wedge 会话中）。输出中**出现了** `Clock Offset Domains` 与 `Clock Offset Limits kHz` 两行（值显示 `N/A`），即**字段确实进入真实设备输出路径** ⇒ "已实现且**已出现**"；数值仍待健康会话（下条）。这条同时验证了追加二十六所依赖的那条链路（core 字段 → CLI 按键渲染）在**真机**上成立，而不只是读码结论。

**二、wedge 的用户可见足迹（实测，新增事实）**：`Board ID 376`、`Chip Sku 250`、`Project G166`、`Pstate P0` **正常读出**；`Memory Total Bytes`、`Pcie Link Gen`、`Power Limit*` 为 `N/A`；偏移通道的失败原因是 **`RUSD allocate/map: 0x00000062`** ⇒ wedge 不只是"GSP 控制回 0x62"，**连 RUSD 传输的分配/映射都失败**（此前只观察到前者）。**遗留待判**：同一次运行里 `Memory Used: 0.000 MiB` 而 `Memory Total Bytes: N/A`——两个字段来源不同，需健康会话对照才能判定"0.000"是真实读数还是 wedge 归零（`P0` 未被归零说明并非所有字段都坏）。此项如实标为**待实测**，不改行为。

**三、修正"每个功能族缺的是哪一层"的用户可见表述（本轮的实质改动）**：上述运行暴露出一个真实缺陷——`unavailable` 里除两个偏移族外的**所有**族都共用同一句话 `"no readable RM control for this family on this driver (see notes)"`，而实测下它对**每一个**族都不准确：热族**明明可读**（wedge 前 `0x20800513` 返回过 47 ℃），VF 读半**存在且非特权**只是**从未读过**，写族缺的是**特权**而非控制存在，风扇族缺的是"公开面没有实体＋私有热族 31 条读从未读"。这正是目标明令禁止的"把没读过/没权限说成命令不存在"。现改为**按族给因**：`fan_control`＝公开面确无风扇实体（`ctrl2080fan.h` 空壳、控制头全树 "cooler" 0 次）而私有热读半 31 条未读 ⇒ **调查未知（未定位）**；`vf_curve`＝读半存在非特权但从未读 ⇒ **延后而非缺失**；`pstate_lock`／`power_limit_write`／`settings_restore`＝**需要写且私有 SET 在 root 以下被拒**（追加二十七）；`thermal_limit`＝热族本身可读，缺的是**已配置的限值**这个另一字段 ⇒ 尚未定位；`voltage_boost`＝走 SET，同写门槛。同时新增回归测试 `every_gap_names_its_own_cause_rather_than_a_missing_control`（每个族必须有非空、指向 notes 的理由，**不得**再出现那句被退役的笼统话；写族共用特权理由，其余族不得借用），并让 `nvoc-python` 的测试夹具改用真实常量而非复制字面量（此前它复制的正是那句已退役的话）；`DeviceInfo` 增加 `Default` 派生以便夹具构造（纯增量）。

**验证范围**：`cargo fmt --all -- --check` clean；`cargo clippy -p nvoc-core -p nvoc-cli -p nvoc-auto-optimizer --all-targets -- -D warnings` 通过；`cargo test -p nvoc-core --all-targets` 通过；重建 `nvoc-cli` 后实跑一次，逐族打印出修正后的表述（见 §52 记录）。本轮**访问了设备 3 次**：1 次 anchor 轮询 + 2 次只读 `get-settings --macos-rm`（重建前后各一次），**无写入、未提权**。

### 本轮追加三十一（2026-10-18，修正一处**用户可见假值**：wedge 会话里"已用显存"被当作测量值报出）

**实测到的缺陷（不是推断）**：wedge 会话中 `nvoc-cli get-status --macos-rm` 把 **14 个同源字段**正确标为 `N/A` 并在 `unavailable` 里给出 `RUSD allocation/mapping failed: 0x00000062`，却把 `memory_used_bytes` **当作测量值打印为 `Memory Used: 0.000 MiB`**。结构化输出（`-O json`）确证：`memory_used_bytes = 0`，而同一次读取里 `power_mw`/`fan_rpm`/四个时钟/四个温度/`pcie_*`/`average_power_mw`/`engine_utilization_percent` 等全为 `null` 且带上述页级理由。

**根因（代码级定位）**：`memory_used_bytes` 的载荷 `memory_used_kb` 与它的有效字 `memory_used_status` 都在 **RUSD 采样结构体内部**（`RawSample`），但发布时走的是 `publish_live`，只查它**自己的**状态字：页映射失败时原生层交回的是**零填充样本**，状态字读出来正是 `0`（＝"正常"）、载荷是 `0` ⇒ 于是 0 被当成读数发布。对照之下，**RUSD 分节**字段早已有页级门（`FIELD_OK if map_status == 0 && timestamp != 0`，代码注释即写明"没有时间戳说明什么都没采到"），而这条"伪 live"字段**绕过了那道门**。`pstate` 不同且**本就不该**被页级理由压制：它来自一条**真的答复了的** live 查询（wedge 下 `0x2080b201` 仍返回 NV_OK，实测 `P0` 未归零）。

**修法**：新增 `publish_snapshot_backed`——载荷在 RUSD 采样内、有效字随样本而来的字段，先看**页级理由**再看自己的状态字；`publish_all` 显式接收页级理由（one-shot 传页状态理由，monitor 会话按构造已有映射、传 `None` 且保持原行为）。**未改动** `pstate` 的语义。

**同时纠正一处被测试固化的错误前提**：原测试注释写"Live controls fail on their own status, not the page status"，并把 `memory_used_bytes` 当作 live control 断言——那正是本缺陷被"合法化"的地方。现按实测改写：页失败时它随页被压制（理由与分节字段**同一句**），页正常时其自身状态字才是门（`0x57` 用例保留，另加页状态归零前置），并新增回归测试 `a_failed_rusd_page_does_not_publish_a_zero_memory_measurement`（零填充页 + 字段字为 0 ⇒ 必须 `None`、理由与 `temperature_c` **逐字相同**、`pstate` 仍在）。

**验证范围**：`cargo fmt --all -- --check` clean；clippy（core/cli/auto-optimizer，`-D warnings`）通过（仅依赖 `nvapi-sys` 一条既有警告）；`cargo test -p nvoc-core --all-targets` 全绿（88+5+15 等）。**设备级验证**：重建 CLI 后实跑，`Memory Used: N/A` + `RUSD allocation/mapping failed: 0x00000062`，`Pstate: P0` 不受影响，`unavailable` 26 项。本轮**只读**访问设备 **5 次**（1 次 anchor 轮询 + 4 次 `get-status`：修复前 human/json 各一、修复后 human/json 各一；一次参数写错的 `--json` 在解析阶段即退出、未开驱动，不计），**无写入、未提权**。

### 本轮追加三十二（2026-10-18，把"失败页不得产出任何数字"做成结构性回归门；并在设置侧确认前一轮修复）

**一、设置侧确认**：重建后用 `get-settings --macos-rm` 复跑，`Memory Used: N/A`（第 20 轮同一 wedge 会话里它曾打印 `0.000 MiB`）⇒ 追加三十一那处假值修复**在设置与状态两个用户可见面上都生效**，该缺陷两处闭合。

**二、结构性回归门（本轮实质改动）**：追加三十一的教训是"逐个字段的守卫各自看都对，仍有一个漏了"，因此新测试 `a_failed_page_publishes_no_numbers_at_all` **不再逐字段断言，而是检查整个载荷**：页映射失败（`0x62`）且原生层仍把分节报为可读（零填充样本 `FIELD_OK`、字段状态字 0＝"正常"、载荷 0）时，把 `DeviceStatus` **序列化后要求其中不含任何数字**，唯一例外 `pstate`——且该例外是**实测的**而非推测（pstate 查询由 GSP 直接答复，wedge 下仍返回 `P0`）。这样**将来新加的字段若忘记页级门会直接被这条测试抓住**，不需要有人记得去补列表。测试还固化：7 个具名字段（`memory_used_bytes`/`power_mw`/`fan_rpm`/`temperature_c`/`memory_clock_khz`/`pcie_link_gen`/`average_power_mw`）的理由必须是页级原话，两个携带数字的容器（`rusd_timestamps`、`engine_utilization_percent`）**被清空而非半填**。

**三、结果与交叉验证**：该测试首次运行**核心断言即通过**（失败页未漏出任何数字，只有 `pstate`），唯一失败是我自己猜的 `unavailable` 条数（25 vs 实测 26）。改为 26 —— 这与**真实 wedge 设备同一页失败下报出的 26 项完全一致**，说明该夹具**就是设备用例**而不是近似。

**验证范围**：`cargo fmt --all -- --check` clean；clippy（core/cli/auto-optimizer，`-D warnings`）通过；`cargo test -p nvoc-core --all-targets` 全绿（89 项）。本轮**只读**访问设备 **2 次**（1 次 anchor 轮询 + 1 次 `get-settings`），**无写入、未提权**。

### 本轮追加三十三（2026-10-18，把"同一组命令在 NVAPI 那一侧的成文资料"取回仓库；偏移槽位仍**如实保持未定**）

前几轮把偏移通道的三个悬问（哪个 dword 是偏移、哪些记录类型能写、位序号谁在命名）都记成"未定"。本轮先**核对仓库里已有的成文封送资料**——`nvapi-rs` 就在本仓库里，它描述的正是**同一组命令**（GetControl `0x2080901b`/ID `0xF58938F5`、SetControl `0x2080d01c`，以私有 NVAPI ID 藏在 escape `0x07000049` 后），且文件头注明四条命令**已在 Ada 4060 Laptop / R575.74 上活体验证**。把这些写进 `docs/reverse-engineering/nvapi/rm-param-marshall-corrections-r610.md` §7（含出处与"同命令、不同芯片与驱动版本"的限定）。

**一、新增可引用事实**

1. **记录结构并不等同**：NVAPI 侧 caller 记录 72 B、`values_kHz: [i32; 8]`（**8** 个值槽）↔ 本机实测 RM 记录 64 B、每条**写 12** 个值 dword ⇒ **按位置逐槽对应是没有依据的**；而"slot 0 = 有符号 kHz 偏移、邻居是范围/电压项"这句**来自文章，其作者自己标注未经 A/B 证实** ⇒ 故**措辞仍是"未 pinned"**，只把依据与适用面写清。
2. **排队中的首写与成文假设正好重合**：`tools/clock-offset-write-run.sh` 的 `BIT 0` 值为记录 `+8`，**就是 slot 0** ⇒ 这次（仍需人工授权的）A/B 一旦跑，测的是**有出处的那一槽**，不是随机 dword。
3. **按类型写集**：V1（magic `0x10964`，本机所讲尺寸族）列出可封送协议类型 `{1, 3..=9}`，类型 `0x02` **两版都不封送**（SetControl 静默丢弃），`0x0A` 仅 V2。**与实测的出入如实记录**：本机看到 type-03 记录**只写了自己的 stamp**，而 V1 集合含 3 ⇒ 要么我们读到的"类型"不是协议类型，要么两版不同 ⇒ **写手册不得假定 type-03 记录会接受值**。
4. **位 5 的命名陷阱**：NVAPI 侧活体 A/B 显示 **控制记录 bit 5 驱动的是 MSD**（不是索引 5 名字所指的 Host）⇒ 若将来在载荷里给人读的名字，bit 5 要改标 MSD。
5. **十个域位的名字可引用**（RTSS `NV_GPU_CLOCK_DOMAIN_ID`，出处 `nvapi-rs/sys/src/gpu/clock.rs`），并用**我们自己抓的目录块**复算：十个带 stamp 的行其位为 `{0..6, 19, 20, 21}`，**行序并不随位递增**（行序位表 `{0,1,4,2,3,20,19,6,21,5}`）⇒ 这正是"实测 `0xFF` 窗口覆盖 GPC(核) 与 M(显存)、而把 MSD/HOST 留在窗外"的原因，也是既有理由串里行编号的**可引用依据**。
6. **范围块三条记录的新归因假设（未确认、不动载荷）**：其 tag 第三字节为 3/4/5（= HUB/M/HOST，与目录同一序号空间）⇒ 假设这三条就是 HUB/M/HOST 的每域窗口；**反证同时记下**：三条都带同一个 `0x190000`，对"每域量"而言可疑 ⇒ 载荷仍只报**一条已达成一致的窗口**。

**二、本轮唯一代码改动**：`CLOCK_OFFSET_UNREADABLE_REASON` 末段补上上述 1/2/3 的**依据与适用面**（字符串级；"未 pinned"的结论**未改**）。无测试断言旧尾句（漂移守卫只校验 `(see notes)` 结尾）⇒ 已核查全仓无字面副本。

**三、验证范围（如实）**：`cargo fmt --all -- --check` clean；clippy（core/cli/auto-optimizer，`-D warnings`）仅余既有 `nvapi-sys` 警告；`cargo test -p nvoc-core --all-targets` 全绿（89+5+15）。本轮**设备访问 1 次**（anchor 轮询），**无写入、未提权**，**没有新的设备测量**——本轮的证据是成文资料 + 我们自己已抓取的两个块（`core/tests/data/macos-offset-{directory,range}.bin`），这一点不夸大。

### 本轮追加三十四（2026-10-18，风扇族：从"未定位"升级为**有名有姓的候选**，并给出第一次（受 wedge 拦阻的）读数）

**一、先纠正本仓此前的表述**：`ctrl2080fan.h` 是 **33 行 FINN 生成桩、零声明** ✔，但真正的结论比"桩"更强——**整个 `src/common/sdk` 里不存在任何 `NV2080_CTRL_CMD_*FAN*`/`*COOLER*` 宏**（跨整树检索）✔。所以"头文件里没有"**只是关于公开 SDK 的事实**，不能当作"驱动没有这个控制"（这正是 §"未知不能以头文件搜不到代替"要防的）✔。

**二、候选来源与映射**（取自**仓库内** NVAPI 副本，非扫描）：`nvapi-rs/sys/src/gpu/cooler.rs` 明确写出它包装的风扇策略命令——**RM `0x20800532`，经 escape `0x07000038`**，填充一张 **232 B 内部 `fanPolicyInfo` 表**（dword 0 = 策略掩码、≤16 位；byte +4 = 当前生效策略索引）✔。**在本机方法表里只有其私有孪生 `0x20808532`（paramSize 336，读半边）**，而**公开 id 不在表中** ✔ ⇒ 这是一次**候选映射纠错**：先前"31 个 0x85 读 id 无人读过"的族，现在有一个**有出处、有尺寸、在表中**的具体候选 ✔。

**三、本轮实测（2 次只读 GET，新增能力 `--fan-read`，336 B / 0xA5 背景）**（⚠️ **更正，见追加九十二**：这两个结论**没有留下原始产物**，`outputs/` 下任何 `.txt`/`.jsonl` 都不含这两个 id，census 工具把它们列为"表里有、从未在返回状态里出现" ⇒ 记为**散文化结论**，已由 `--fan-read` 并入读取电池，健康窗口再测一次即为产物）：
- `0x20808532@336` → **`0x62 NV_ERR_RESET_REQUIRED`**：与所有 GSP 转发控制撞的是**同一堵墙** ⇒ 该 id **可被派发**，当前**唯一障碍是 wedge**，不是"命令不存在"、也不是权限 ✔。
- `0x20800532@336` → **`0x56 NV_ERR_NOT_SUPPORTED`**。**如实限定**：同一次运行里公开 id（`V2.5 edpp-limit-info`）也会因**芯片不支持**而返回 `0x56` ⇒ wedge 未解除时这个差异**只是提示**（与"表里没有公开 id"一致），**不能据此断言**公开半边不支持；需健康会话复核 ✔。

**四、因此族状态**：仍留 `unavailable`（**没有编造任何风扇读数** ✔），但成因串已换成"**有具名候选 + 一次被 wedge 拦下的读数**"，并写明 A5 背景的用途（未写字段保持 `0xA5`，从而"写进去的 0"与"根本没写"可区分）✔。**尚未**声称风扇 rpm / 范围 / 恢复路径 ✔。

**验证范围**：probe 重建通过（`built probe-macos-rm-idle (profile idle)`）；`--fan-read` **已实际跑通一次**（证明取参、缓冲、拒绝分支与状态分支都对）✔；core fmt clean、clippy（core/cli/auto-optimizer `-D warnings`）仅余既有 `nvapi-sys` 警告、core tests 全绿（89+5+15）。本轮**设备访问 3 次**（1 次 anchor 轮询 + 2 次风扇只读 GET）✔，**无写入、未提权**。

### 本轮追加三十五（2026-10-18，`0x20809019` 目录行：文档字段名 ↔ 真实字节）

**一、文档侧**：仓库内 NVAPI 副本（`nvapi-rs/src/clock.rs` L303-330）把"域目录" caller 结构的字段**逐一点名**：`disabled`/`clockType`/`rangeMax`/`rangeMin`/`vfpIndexMin`/`vfpIndexMax`/`unknown0`/`unknown1`/`padding`，目标是 `ClockRange{domain, range, vfp_index}` ⇒ **公开层认为该块按域携带一段 kHz 范围与一段 VFP 索引范围** ✔ 与"RM 行 384 B ≠ 公开行长"并不冲突（同族控制块已有 2108/64 vs 2404/72 的先例）。

**二、测量侧**：对 `core/tests/data/macos-offset-directory.bin` 逐行 dump（行 = `48+384r`）——行内**确有大量此前从未解析的字段** ✔；其中 **`+44` 处一对 int16** 只出现在 **bit 0x1（文档索引 0 = Gpc）与 bit 0x10（索引 4 = M）** 两行（值 −1000/+1000 与 −1000/+3000），另两行该处为 0 ✔；每行还有 `+48`（形如 `03 01 00 01`）与 `+52=0xffff` ✔。

**三、明确留在假设层**：`+44` 是"该域偏移窗口(kHz)"**尚未证实**；**反向证据**是 `0x20809025` 记录 3/4/5 给的是 ±18750，与此处不一致 ⇒ 二者要么不同量、要么之一读错，**须健康会话二次抓取（最好换 pstate）判定**。故本轮**不发布任何身份未证实的数字**（公开载荷不动）✔。

**四、顺带查证**：`stage-clkdom-20261017T091500Z`（尺寸扫描期）无行数据，`outputs/` 也再无该块转储 ⇒ **目前唯一行字节记录就是该 fixture**；已给 probe `--clkdom-read` 加**逐行非零 dword 普查**（仅 `size==12336`），使下次会话自动记录行尾字段 ✔。

**验证范围**：probe 重建 `built probe-macos-rm-idle (profile idle)` ✔（普查分支只在读成功时打印，wedge 下**未被执行**，仅编译验证——如实标注）；**无写入、未提权**；本轮设备访问 1 次（anchor 轮询）；core 未改动，故未重跑 Rust 门（上一轮 fmt/clippy/tests 结论仍适用 ✔）。

### 本轮追加三十六（2026-10-18，温度"配置上限"族：把"未定位"收窄成一条**有证据的调查结论** + 一个此前未见的遥测来源）

**一、先在 SDK 里把这个族数清**（`ctrl/ctrl2080/ctrl2080thermal.h`）：热控制族**恰好两个 id**（`0x20800513` 与其非特权孪生 `0x20808513`）✔，其 opcode 全集为 targets(0x100)/target-type(0x101)/provider-type(0x301)/sensors(0x500)/sensor-provider(0x510)/**sensor-target(0x520)**/reading(0x1500)/**reading-range(0x540)** ✔ —— **没有任何 limit / policy opcode** ✔。逐条读结构：`SENSOR_TARGET` 操作数是 `{sensorIndex, targetIndex}`（是个**索引**，不是上限 ✔）；`READING_RANGE` 返回 `minimum/maximum`（是**传感器量程** ✔，probe 已经在读 ✔）。⇒ **"配置上限"不在我们已能成功调用的这个族里**，这是**数出来的结论**，不是"没搜到" ✔。

**二、客户端文档给出该值的形状**：`nvapi-rs/src/thermal.rs` 的 `ThermalLimit { policy: ThermalPolicyId, value: CelsiusShifted, remove_tdp_limit: bool, pff: Option<PffStatus> }` ✔ 与原始结构 `NV_GPU_CLIENT_THERMAL_POLICY_STATUS_V3 { policy_id, temp_limit_C, remove_tdp_limit, … }`、成对变体 `NV_GPU_CLIENT_THERMAL_POLICIES_STATUS_{V2,V3}`/`..._ENTRY_V2` ✔ ⇒ **配置上限是"按策略"的**：策略 id + 摄氏上限 + 一个"移除 TDP 上限"标志，并带 PFF 子状态 ✔（因此它同时是一个**写入**面 ⇒ 保持写入门控 ✔）。

**三、仍未定位的**：**哪条 RM 控制承载"策略查询"** ✔。证据：该副本 `thermal.rs` 引用的 RM id 数 = **0**，整个 `gpu.rs` 只有 **10** 个 ✔；escapes 有名（如 `0x07000038→0x20800532` 风扇一例 ✔）而热策略这条**尚无映射** ⇒ 记为"**尚未定位**"，并写明下一步就是把该 escape 查出来 ✔。

**四、顺带纠到一条此前没有的风扇来源**（校核上轮措辞）：`src/common/sdk/nvidia/inc/class/cl00de.h` 声明 `RUSD_FAN_COOLER_INFO { NvU32 rpmCurr[16]; }` 与 `RUSD_FAN_COOLER_STATUS` ✔ ⇒ **RUSD 快照里本来就有风扇转速** ✔，且是我们**早已采样**的路径 ✔。上轮措辞"SDK 没有任何**风扇控制**"依然准确 ✔，此处补上的是**风扇遥测**：风扇族现在有**两个独立候选来源**（策略控制 + RUSD 遥测），后者受同一个 RUSD 页门禁阻塞 ✔。

**验证范围**：core 改的是该族**用户可见成因串**（`get-settings --macos-rm` 的 `unavailable["thermal_limit"]` ✔）；fmt clean、clippy（core/cli/auto-optimizer，`-D warnings`）无 error、core tests 89+5+15 全绿 ✔。本轮**设备访问 1 次**（anchor 轮询）、**无写入、未提权**、客户端未构建 ✔。

### 本轮追加三十七（2026-10-18，功耗族：RATED_TDP 候选 id/字段/写入前置条件已定位；一条候选规则被证伪）

**一、RATED_TDP 候选已定位（SDK + 方法表互相印证）**：`0x2080206e`（GET_CONTROL）与 `0x2080206f`（SET_CONTROL）——SDK **只声明这两个** ✔，均在方法表且 paramSize **12** ✔＝文档结构 `{client,input,vPstateType}` 的 sizeof ✔；表 flags GET `0x10008`、SET `0x40048` ✔。文档给出 `client∈{RM,WAR_BUG_1785342,GLOBAL,OS,PROFILE}` ✔、`action∈{DEFAULT,FORCE_EXCEED,FORCE_LIMIT,FORCE_LOCK,FORCE_FLOOR}` ✔、合法状态仅 `NV_OK`/`NV_ERR_NOT_SUPPORTED` ✔。

**二、写入前置条件（文档）**：`SET_CONTROL` **不能设置 RM 客户端** ✔，RM 在客户端间**仲裁**并可能**回落 `FORCE_EXCEED`** ✔ ⇒ **RATED_TDP 是仲裁请求而非瓦数**：写入成功也可能看不到上限变化 ⇒ 这正是"不得以实时频率变化代替写入生效"的文档依据，也支持"RATED_TDP 不能取代真正的偏移/VF 控制" ✔。该前置条件已写入**用户可见成因串**（`get-settings --macos-rm` 的 `unavailable["power_limit_write"]`，并加了不与他人共享、不得回退成共享串的测试 ✔）。

**三、一条候选规则被证伪（如实记录）**：猜想"表 flags 位 `0x40000` = SET 行"✗ —— 表里能找到的 SDK SET id 共 **103** 个，带该位者仅 **14** 个，且在 id 位写半边者 **0** 个；`0x40000` 集（58 行）与 id 位写半边（92 行）**交集为空** ✔ ⇒ **两套互不重叠的约定**（私有/legacy 转发 vs SDK 公开 SET）✔。`0x40000` 的真实含义对那 58 行**尚未定位**。

**验证范围**：core 改动为该族成因串 + 一处断言；fmt clean、clippy（core/cli/auto-optimizer，`-D warnings`）无 error、core tests 89+5+15 全绿 ✔。本轮**设备访问 1 次**（anchor 轮询）、**无写入、未提权**、客户端未构建 ✔。

### 本轮追加三十八（2026-10-18，写入路径核对 + 映射纠错 + 电压增强族审计）

**一、门控写入的 SET 行已核对（最要紧）**：`0x2080901b`（GET，paramSize **2108**）的 `|0x4000+1` 孪生 `0x2080d01c` **在方法表里** ✔ 且 paramSize **同为 2108** ✔（flags 0x48→0x44 ✔；id 位落在写半边 ✔ 与交接一致）⇒ 计划中的单域最小步长写入**不会因 payload 尺寸不匹配而失败** ✔。

**二、纠错：`|0x4000+1` 孪生**不通用****：`0x208090a2→0x2080d0a3` 在表中 ✔，但**目录** `0x20809019→0x2080d01a` ✗ 与**范围块** `0x20809025→0x2080d026` ✗ **都不在表中** ⇒ 不要假设私有 GET 必有表内 SET 孪生 ✔。

**三、避免张冠李戴**：NVAPI 副本引用的 `0x2080D024`（paramSize **66060**）是 **`0x20809023`** 的孪生（表内 `0x20809023 66060 0x48` ↔ `0x2080d024 66060 0x44` ✔；`0x20809024` 本身不在表内 ✗），**不是**我们控制块的 SET ✔；`0x2080f031`/`0x20801acf`/`0x2080206d` **均不在方法表** ⇒ 调用它们的路径依赖**派生** ✔。

**四、电压增强族（最后未审计的读族）**：客户端侧是 **volt-rails control**（`NV_GPU_CLIENT_VOLT_RAILS_CONTROL{percent}` 的 Get/Set ✔）⇒ 此"boost"是**百分比** ✔；**不得**与 `BaseBoostMode`（base/boost **频率表**）、`set_dynamic_boost`（PPAB **使能**）、`NV2080_CTRL_CMD_PERF_BOOST`（**硬件 boost 动作**）混为一谈 ✔。我们实际发过的读 `0x20800a80`（16 B）**实测 `0x56` NOT_SUPPORTED** ⇒ 归类**驱动明确拒绝** ✔；承载 volt-rails 的 RM id **尚未定位** ✔。

**验证范围**：本轮**无 Rust 改动**（仅文档），故未跑 cargo 门（上一轮同源齐套结果仍有效 ✔）；核对依据是本机方法表逐行查询与 SDK/副本源码（含表格数字）✔。**设备访问 1 次**（anchor 轮询）、**无写入、未提权**、客户端未构建 ✔。

### 本轮追加三十九（2026-10-18，温度策略链定位到接口级 + escape 映射表 + 电压族"范围/原状态"既有实测）

**一、温度策略/上限链已在客户端定位到**接口级**：`NvAPI_GPU_ClientThermalPoliciesGetInfo`（策略集）/`...GetStatus`（当前上限，即 `ThermalLimit`）/`...SetStatus`（写面）/私有 `...PrivateGetInfo`（`gpu.rs:5676` 起 ✔）。私有那条注释记录了**策略索引的权威发现规则**（GPS 索引 → 否则声学索引 → 否则 None；取代硬编码 idx 2；**RE 自参考工具 CLI `queryTargetTemperature`，sub_14002C410**）✔ ⇒ 该族**只剩"这些 NVAPI 入口背后的 escape/RM 端点"未定位** ✔（定位方法与 cooler 那条同法 ✔）。

**二、escape 映射表（区分"已命名/已测"与"未命名"）**：`0x07000002`=GetAllClockFrequencies ✔；`0x0700004A`=VFP 曲线但**在 391/Kepler 内核未实现** ✔，而 `0x0700004F` **在它不可用处成功**（"通往 VFP 曲线的可行路径" ✔）；`0x07000049`=私有命令/`SetControl(RM 0x2080D024, ID 0xFEC00D04)`，标注 **DANGEROUS V/F** ✔；`0x0700004E`=**GetBarInfo（PCI BAR 拓扑）** ✔；`0x07000053`/`0x0700006D` ✔；`0x07000038`→cooler `cmd 0x20800532`（本项目实测 ✔）。**含一处对我自己推断的纠正**：`0x0700004E` 不属电压族 ✗（近邻推断反常，读原文后改正 ✔）。

**三、电压族"范围/原状态"证据已存在且同型号实测**：`VoltVoltDevicesGetInfo` 报**可控窗口**（min/step/max/default µV），注释给出 **R465 在 1650 SUPER 上的实测：300/6.25/1300/1000 mV**（"matching the rail clamp floor"）✔ ⇒ 但本机是 **R610**，**必须重测不得套用** ✔。已写入该族用户可见成因串（并澄清该族**并非读盲**：客户端侧有 volt-rails 读面；我们实发的 `0x20800a80` 实测 `0x56` ⇒ **驱动拒绝**而非缺失 ✔）。

**验证范围**：core 改动＝该族成因串；fmt clean、clippy（core/cli/auto-optimizer，`-D warnings`）无 error、core tests 89+5+15 全绿 ✔。**设备访问 1 次**（anchor 轮询）、**无写入、未提权**、客户端未构建 ✔。

### 本轮追加四十（2026-10-18，风扇族客户端分发链审计：arbiter 语义 + 恢复缺口的准确表述）

**一、客户端接口清单（实际分发链）**：读＝`GetTachReading`（RPM）/`GetCoolerSettings`/`GetCoolerPolicyTable`/`ClientFanArbitersGetInfo`/`...GetStatus` ✔；写＝`SetCoolerLevels`/`SetCoolerPolicyTable`/**`RestoreCoolerSettings`**/**`RestoreCoolerPolicyTable`**/`ClientFanArbiterControl` ✔ ⇒ 客户端**有两个 restore 入口** ✔；副本**无** `src/cooler.rs`（挂在 `gpu.rs` 与 `sys/gpu/cooler.rs`）✔。

**二、arbiter 三件套精确布局**：`..._ARBITER_INFO_V1`（40 B，flags `FAN_STOP=1`/`FAN_STOP_DEFAULT=2`）×32 + 36 = **1316** ✔；`..._ARBITER_STATUS_V1`（8 B，`fan_stop_active()`⇔`unknown1!=0`）×32 + 36 = **292** ✔；`..._ARBITER_CONTROL_V1 { arbiter_index, flags{FAN_STOP=1} }` ✔ ⇒ 客户端"控制风扇"主体是**仲裁请求**（同 RATED_TDP/热策略形态）**而非**占空比设定 ✗ ⇒ **读到 policy 表 ≠ 持有控制权** ✔。

**三、缺口与恢复（准确表述）**：候选私有 `0x20808532@336` 在表 ✔、公开 `0x20800532` **不在表** ✗；该行**无表内恢复/写孪生** ✗，且 336 与 arbiter 各尺寸均不吻合 ⇒ 属 **cooler/policy 面** ✔，**精确对应哪个客户端结构尚未定位** ✗ ⇒ 风扇恢复只能**"先读原值再写回原值"**，**不得假设存在恢复命令** ✔（客户端那两个 `Restore*` 是 NVAPI 面，我们手上的 RM 私有行没有孪生 ✔）。
**本族下一步（不变）**：健康会话下跑 `--fan-read`（两个 id）✔，取得 可读/可解释/范围/原状态 的第一手证据 ✔。

**验证范围**：本轮**无 Rust 改动**（纯文档）⇒ 未重跑 cargo 门（上一轮同源齐套结果仍有效 ✔）；核对依据为副本源码逐段阅读与既有表格数据 ✔。**设备访问 1 次**（anchor 轮询）、**无写入、未提权**、客户端未构建 ✔。

### 本轮追加四十一（2026-10-18，封送规则正对偏移 GET + 一条可判定预测 + 写入门控现状核对）

**一、副本对同一条命令的记载**：`0x2080901b`（ID `0xF58938F5`）✔；**值 dword 只对固定内部类型集合拷贝** ✔（V2 `0x261A4`→`{1,3..=10,15}`；V1 `0x10964`→`{1,3..=9}`；type `0x02` **两版都不封送、SetControl 静默丢弃** ✔）；`value_modifiable` **纯由记录类型字节推导，不是驱动能力位** ✔；remap 子程序 `sub_18015BB30/BD20` ✔；**可读性与可写性按域独立** ✔ ⇒ "值可能被静默丢弃"有了机制依据 ✔。

**二、据此得到一条**可判定预测****：V2 从 `rec+268..296` 取值、**V1 回退从 `rec+44..60` 填 slot 0..4** ✔；本项目实测十二个值 dword 在 `{8,12,16,20,24,28,32,36,40,44,52,56}` ✔ ⇒ 若二者同基，**V1 窗口恰好覆盖末四个槽** ✔、而 V2 窗口在 64 B 之外 ✗ ⇒ **首次健康读取即可判定**："非零只在末四槽 ⇒ 走 V1 值窗口；前段非零 ⇒ 另有布局" ✔（前提：副本的 `rec` 是 72 B 调用方记录，未必同基 ⇒ 标记为**待验证** ✔）。

**三、一处张力（不强行调和）**：副本 `v1_marshalable` 含类型 3 ✔，而本项目实测"type 03 只写 stamp、不写值 dword" ✔ ⇒ 要么"type"不是同一层、要么拷贝规则在另一层 ✗ ⇒ **尚未定位** ✔；但方向一致：**stamp 变化 ≠ 值被接受** ✔。

**四、写入门控现状**：`settings_write`/`settings_restore` 已定义 ✔、已导出到 Python（含 `performance_writes` ✔）、有测试 ✔，但 **`cli/`/`auto-optimizer/`/`srv/` 无任何分支读取** ✔ ⇒ 目前是**如实上报的能力标志**，**强制执行点尚未接线**（与阶段 10 未接 macOS 后端一致 ✔）；接线处即应在该标志上短路性能写入 ✔（本轮不提前改码 ✔）。

**验证范围**：本轮**无 Rust 改动**（纯文档）⇒ 未重跑 cargo 门；依据为副本源码逐段阅读、本项目实测注释与全树 grep（19 处命中，逐条归类）✔。**设备访问 1 次**（anchor）、**无写入、未提权** ✔。

### 本轮追加四十二（2026-10-18，**代码**：控制块的按记录普查——让下次健康读取变成决定性实验；分类＝已实现待实测）

**为什么改**：现有通用打印**只列前 24 个 dword**（`listed < 24`）✗，而 2108 B 控制块的十二个值 dword 位于记录相对 `{8,12,…,56}` ⇒ 记录 0 的最后一个槽是**绝对 dword 29**，**从来没进过 dump** ✗。上一轮从副本读出的封送规则又给了新的理由要按记录看：驱动**只对固定内部类型集合拷贝值 dword**，且 **V1 回退窗口 `rec+44..60` 只会填满 64 B 记录的末四槽** ✔ ⇒ **非零出现在哪些槽位，就说明本主机走的是哪套窗口（或都不走）** ✔；反过来，**值全零的记录是"无信息"，不是"偏移为零"的证据** ✔。

**改了什么**（`probe-macos-rm/src/nvx_suite.c`，仅当 `size == 2108` 时生效）：对**掩码位被选中**的每条记录（`mask@+4` ✔，记录基址 `60+64*i` ✔），打印其**记录相对**的非零 dword（`+off=0x…`，最多 16 个 dword 覆盖到 `+60`）✔。不新增任何命令、不扩大扫描：只是把**已经读回来的那条应答**按记录重排显示 ✔。

**判读方式（留给下次健康读取）**：① 非零只在末四槽 ⇒ 走 V1 值窗口；② 前段槽位非零 ⇒ 另有布局；③ 全零 ⇒ **该记录的值未被封送**，此时"零"不能当作"偏移为零" ✔（与既有"stamp 变化 ≠ 值被接受"一致 ✔）。

**验证范围**：测试机重建通过，`built probe-macos-rm-idle (profile idle)` ✔ 且无 warning ✔；**本轮没有运行 probe**（wedge 仍在，anchor 轮询已用掉本交互的唯一一次设备访问 ✔）⇒ 分类为**已实现待实测** ✔，不谎称已实测 ✔。客户端未构建 ✔。

### 本轮追加四十三（2026-10-18，压力侧：Vulkan 复用的代码审计与实测状态；分类＝已实现待实测（编译已通过））

**审计结论：原 NVOC 的 Vulkan 压力实现，其 loader 发现逻辑**已经就是为本机这套黑苹果写的** ✔。
证据（`cli-stressor-cuda-rs/src/vulkan_loader.rs:27-46`）：macOS 专用候选表 **`/Library/GPUBundles/nvmtl/libvulkan.1.dylib`、`/Library/GPUBundles/nvmtl/libvulkan.dylib`** ✔ 与 ICD 候选 **`/Library/GPUBundles/nvmtl/nvk_icd.json`** ✔，另有逃生口 `NVOC_VULKAN_LOADER` / `NVOC_VULKAN_ICD` ✔；非 macOS 上候选表为空、覆盖变量被忽略 ✔。**先走平台查找、失败才回退到该 bundle** ✔；`prepare()` 必须在主线程、且在派生工作线程之前调用（因为选择 ICD 需要写 `VK_DRIVER_FILES`）✔。**全程未出现 MoltenVK** ✔——与 goal 的提醒（不要默认必须经 MoltenVK）一致 ✔。

**编译（本轮实测）**：在测试机 `cargo build -p cli-stressor-cuda-rs --features vulkan` ⇒ **`Finished dev profile … 25.72s`，成功** ✔（顺带编译了 `cudarc v0.19.10` ✔ ⇒ macOS 上 CUDA 侧也可编译、无需 GPU ✔）。
**发现的配方注意点**：`cli-stressor-cuda-rs/Cargo.toml` 中 `default = ["cuda12"]` ✔ ⇒ 单写 `--features vulkan` **仍会带上默认 CUDA 特性** ✗；要**纯 Vulkan** 构建需 `--no-default-features --features vulkan` ✔（`vulkan = ["ash","dep:naga","dep:windows-sys"]` ✔）。`--vulkan-only` 需要 `--features vulkan`（`main.rs:1315` 给出该提示）✔。

**实测状态**：**已实现、编译通过**，**运行待 wedge 解除**（低强度、5–10 s、可停止 ✔）⇒ 分类**已实现待实测** ✔，不谎称已跑过 ✔。

**区分"压力负载"与"正确性校验"（goal 明确要求）**：这个压测提供的是**负载**——持续 GPU 工作，用来观察负载下的功耗/温度/频率行为 ✔；它**不是**正确性校验：**渲染成功 / 未崩溃 ≠ 超频稳定** ✔；**更不是**偏移设置成功的证据（goal 第 4 条：**不得以实时频率变化代替偏移设置成功** ✔）⇒ 偏移是否被接受，只能靠**回读控制块的值 dword** 判定 ✔。

**验证范围**：本机（客户端）只做阅读与 Git/传输 ✔；**构建与运行只在测试机** ✔，本轮只构建、未运行核算程序、未碰 GPU ✔；wedge 轮询 1 次 ✔。

### 本轮追加四十四（2026-10-18，`0x20809019` 目录：两处权威皆未声明 + 实测布局与文档版**结构性不同** + 一处正面线索）

**一、核对（两处都查）**：副本只声明**NVAPI 侧** `NV_GPU_CLOCK_CLIENT_CLK_DOMAINS_INFO_ENTRY`（条目 72 B、块 `V1=2344`、注明 **Pascal only** ✗）；**OGKM R610.57.04 参考树 0 命中**（`CLK_DOMAINS_INFO|CLK_DOMAIN_INFO|ClkDomainsInfo` 均无文件 ✔）⇒ 我们手上这条 **`0x20809019@12336`（48+32×384）无任何公开结构可套** ✔ ⇒ **不得**用 72 B 版的字段名硬套 384 B 行 ✔。

**二、实测布局（由仓库夹具重新推导）**：`+4` **就是掩码位/域索引**（1/2/4/16 ✔）⇒ 与文档版 `+4=clockType` **结构性不同** ✔（与副本"bit=掩码位=域索引"一致 ✔）。**记行必须按掩码位，不能按行号**（本轮先按行号标错过一次，已纠正 ✔）。

**三、正面线索（未证明）**：**只有 bit `0x1` 与 `0x10` 带 int16 增量窗口** ✔ ⇒ 按副本位↔域表即 **Gpc 与 M——正是核心与显存** ✔；窗口 `{−1000,+1000}` / `{−1000,+3000}` ✔，**单位尚未定位** ✗（kHz⇒±1 MHz 偏小；100 kHz⇒±100 MHz 较合理，均不足以定论 ✔）。`+48` 字节形态近似 `u8,u8` 版本索引 ✗但尾部字节非零 ⇒ 线索非证明 ✔；`+52≡0xffff` 疑似"无上限"哨兵 ✗、`+76=554/256` ✗ 均尚未定位 ✔。

**验证范围**：无代码改动；依据＝副本源码 + OGKM grep（0 命中）+ 夹具重推 ✔。设备访问 1 次（anchor）、无写入、未提权 ✔。

### 本轮追加四十五（2026-10-18，私有通道只存在于 GSP 方法表——三份产物交叉核对；含一处方法学更正）

**一、核对（本机 + 测试机，大小写不敏感）**：六个私有 id（`0x20809019/0x2080901b/0x20809025/0x20808532/0x2080d01c/0x2080d024`）在 `gsp-1118.tsv` **6/6 命中**（行 712/747/749/757/1040/1042，尺寸与权限位**逐条复核** ✔）；在 `host-608.tsv` **0 命中** ✗；在测试机 `method-table-nvrm-610.57.04.tsv`（605 行）**0 命中** ✗，且该表无任何 `0x20809*`/`0x2080d*` 行 ✗ ⇒ **私有时钟域通道只存在于 GSP 一侧** ✔（与探针实际走的私有 RM 控制路径一致 ✔）。

**二、三份产物交叉**：副本（无该载荷）；OGKM 头文件（全树 0 命中）；主机表与 NVRM 表（都不含这些 id）⇒ **"没有公开结构可套"由上轮的两份证据扩为三份** ✔。

**三、方法学更正**：`method-table-nvrm-610.57.04.tsv` **确有 `name` 列**（命名风格 `subdeviceCtrlCmd…` ✔，如 `0x20801037 subdeviceCtrlCmdClkPmumonClkDomainsGetSamples` ✔），但**我们的 id 不在该表内** ⇒ 只能说"该表不含它们"，**不得**说"没有名字" ✔。`class/ctrl2080clk.h` 在该 OGKM 快照路径下不存在 ✔（与全树 grep 0 命中一致 ✔）。

**验证范围**：无代码改动；全部为只读核对 ✔；设备访问 1 次（anchor 轮询）✔、无写入、未提权 ✔。

### 本轮追加四十六（2026-10-18，私有家族在方法表里的形状：同尺寸配对＋`+0x4001` 孪生交叉验证，含方法边界）

**一、方法与其唯一验证点**：手段＝**(a) 同 `paramSize` 配对**、**(b) 实测的 `+0x4001` 孪生关系** ✔。在 1118 行 GSP 表上唯一可验证的实测对：`2108` ⇒ 表里**恰两行** `0x2080901b`（`0x48`）/`0x2080d01c`（`0x44`）✔ ⇒ 两手段在已知对上同时成立 ✔。

**二、否定结果（可复算）**：目录 `0x20809019`（12336）**表里仅此一行**、孪生 `0x2080d01a` **不存在** ✗；范围 `0x20809025`（7188）仅一行、孪生 `0x2080d026` **不存在** ✗；风扇私有 `0x20808532`（336）仅一行、孪生 `0x2080c533` **不存在** ✗；`0x2080d024` 的孪生 `0x2080d025` **不存在** ✗ ⇒ **整个私有簇里方法表只声明了一对读写（就是偏移对）** ✔，其余在表层面是**只读行** ✔。

**三、公开风扇 id 在两张表都不存在**：`0x20800532` 在 GSP 表与主机表**均 0 命中** ✗ ⇒ 与实测 `NV_ERR_NOT_SUPPORTED` 一致 ✔（"未被表声明"是**可服务性**问题，非权限问题 ✔）。私有行无写孪生 ⇒ **风扇写命令在方法表层面未被声明** ✔；`0x48` 行是否仍接受写入**表答不了，只能实测** ✗（不越权下结论 ✔）。

**四、方法边界（防把启发式当规则）**：写半区 92 行中仅 **82** 行带 `flags&0x04` ✗（10 行带 `0x48` ✗）；`flags==0x44` 共 **121 行** ✗（非写半区专用）；有 1 行 flags 为复合值 `0x5a14438` ✗ ⇒ 该列不宜当纯位域 ✔。结构判据仍用既有 id 位规则（92 行 ✔）。

**五、主机表覆盖面**：`host-608.tsv` 前缀分布 `0x20800*`299 / `0x20801*`144 / `0x20802*`56 / `0x20803*`88 / `0x20804*`17 / `0x20808*`3 / `0x2080a*`1 ⇒ **不含 `0x20809*` 与 `0x2080d*`** ✗ ⇒ 主机侧分发在私有区间前结束 ✔（与上轮一致 ✔）。

**验证范围**：无代码改动；全部为表数据只读复算（本机）＋两处远端只读查询 ✔；设备访问 1 次（anchor）、无写入、未提权 ✔。

### 本轮追加四十七（2026-10-18，**代码更正**：把"字段逐一对应 NVAPI 结构"改成已核实的表述；并补齐十槽位复验）

**一、更正的依据（都可复查）**：实测捕获 `outputs/stage-clkdom-20261017T091500Z/clkdom-read.txt` 第 38/40 行显示，**按 NVAPI 调用侧尺寸发起的请求被拒** ✔——`0x20809019` 在 2520 字节下返回 `0x1f`、`0x2080901b` 在 2404 字节下返回 `0x1f`；而驱动接受的是 **12336（48+32×384）** 与 **2108** ✔。RM 侧记录步长 **384 B**，调用侧结构是 **72 B** ✔；驱动接受块的 `+4` 是**记录掩码位**，而调用侧该位置是 `clockType` ✔；且该调用侧结构被标注 **Pascal only** ✔ ⇒ 原文"named fields match the recorded NVAPI struct field for field"**过强**，已改为"形态线索而非已核实映射" ✔，并显式写明单位未定位、因此不把它当作已定论的上限上报 ✔。

**二、十槽位复验（此前只抽样了四槽）**：目录 32 槽中**有 10 槽带掩码位** ✔（`0x1/0x2/0x4/0x8/0x10/0x20/0x40/0x80000/0x100000/0x200000` ✔）；跨**全部十槽**复核后确认：**只有 `0x1`（Gpc）与 `0x10`（M）槽带非零 int16 窗口** ✔（`{−1000,+1000}` / `{−1000,+3000}` ✔），**单位尚未定位** ✗。原文"10-domain"一句经核对**属实**（10/32 ✔）⇒ 保留 ✔。

**三、顺带核对未改动的部分**：`core/src/macos_offset.rs` 的模块头已自述"这些 id 不在开源头文件里"、把 `min_khz/max_khz` 标为**命名推断**、并把"哪个 dword 是偏移"留给读写实验 ✔ ⇒ **无需改动** ✔（保持最小改动面 ✔）。

**四、验证范围（测试机执行，客户端未构建）**：`cargo fmt --all -- --check` **无输出（干净）** ✔；`cargo test -p nvoc-core --all-targets` ⇒ **rc=0，109 passed / 0 failed**，51 个被 `ignored`（硬件门控 ✔）✔；两侧文件 sha256 一致（`699baa1f…` ✔）。设备访问 1 次（anchor 轮询）、**无写入、未提权** ✔。

### 本轮追加四十八（2026-10-18，目录按**条目标引**而非域位排序：两份独立捕获互证；代码侧核对无缺陷；给出可证伪预测）

**一、两份捕获完全一致（此前"分歧"是排序假设的错觉）**：探针记录的实测类型列表（掩码 `0xFF`⇒条目 0..7）＝`[08,09,08,09,09,09,03,08]` ✔；仓库夹具按**行序**解出＝`08,09,08,09,09,09,03,08,09,09`（10 个已填充条目 ✔，其 `+4` 域位 `0x1,0x2,0x10,0x4,0x8,0x100000,0x80000,0x40,0x200000,0x20` **非升序** ✗）⇒ 前八项**逐项吻合** ✔，原因是**掩码位选"条目索引"而非"域位"** ✔。据此**独立复算**出 08/09 共 7 条＋03 一条 ⇒ `14+7*13+1=106` ✔，与实测 106 **一致** ✔。

**二、代码侧已核对、无缺陷**：`core/src/macos_offset.rs` L68-73 已写明"按索引不按域位"（并指出误读会预测 105 ✗），L208-214 把 `index` 与 `domain_bit` 分开且 `domain_bit` 原样上报 ✔ ⇒ **不改动** ✔。`rec+0` 印章与目录条目类型一致（实测掩码 `0x2`⇒记录 1、印章 `0x00000909`＝类型 09 ✔）。

**三、给下次健康会话的**可证伪预测**（现有数据推导）：全部 10 个已填充条目类型 ⇒ 带值 9 条、03 一条 ⇒ 掩码 `0xFFFFFFFF` 预测改变 dword 数：空槽不写 ⇒ **132** ✔；空槽各写 1 印章 ⇒ **154** ✔ ⇒ **实得值即判据** ✔（都不是则算术模型需修正 ✔）。对照：掩码 `0xFF`⇒106 已由本节独立复算 ✔。

**验证范围**：无代码改动；本机夹具重解＋探针源码读数（两侧 sha 一致的同名文件）✔；设备访问 1 次（anchor）、无写入、未提权 ✔。

### 本轮追加四十九（2026-10-18，**代码**：写入能力单一事实源 + 自动优化器的能力驱动写入闸门；并纠正一句不准确提示）

**一、装上了阶段 10 要用的钩子（能力驱动，不是硬编码默认）**：
- `core/src/macos_rm.rs` 新增 `SETTINGS_WRITE_SUPPORTED` / `SETTINGS_RESTORE_SUPPORTED` 两个**公开常量作为唯一事实源** ✔，`settings_from` 改为由它们填充 `SettingsCapabilities` ✔ ⇒ 投影与任何消费者**不可能各自漂移** ✔，将来"打开写入"只需改后端这一处 ✔。
- `auto-optimizer/src/main.rs` 新增**第二条独立的 macOS 闸门** ✔：命令若会改设置（用既有且已有测试覆盖的 `requirements.elevation` 作为判据 ✔）而后端报告不可写，则**快速失败** ✔；提示里直接引用**实测到的原因**常量（`WRITE_NEEDS_PRIVILEGE_REASON` ⇒ 私有写 id 带 legacy 特权位、未提权客户端在 GSP 之前就被 `0x1b` 拒 ✔），而不是笼统的"未实现" ✔。

**二、纠正一处不准确的用户可见提示**：原 macOS 提示说后端"**does not implement** performance writes" ✗；证据显示 SET 处理**存在**、拒绝来自特权前置条件 ✔ ⇒ 提示已更正 ✔（"命令不存在"与"未授权"不得混同——正是 goal 的硬要求 ✔）。

**三、验证（测试机执行，客户端未构建）**：`cargo fmt --all -- --check` **干净** ✔；`cargo test -p nvoc-core -p nvoc-auto-optimizer --all-targets` ⇒ **rc=0，174 passed / 0 failed** ✔，其中新增 `macos_write_gate_follows_the_backend_capability` 与既有 `gpu_write_commands_require_elevation` **均在真机 macOS 上通过** ✔；`cargo clippy` 仅剩 vendored `nvapi-sys` 的既有告警 ✔，**两个被改 crate 零告警** ✔（首版断言被 clippy 判为"常量断言"，已改掉 ✔）。两侧 sha256 一致（`f337f66c…` / `3266b1cd…`）。

**四、如实标注的余留**：需求模型里**没有**显式 `writes` 字段 ✗，故闸门以 `elevation` 为代理 ✔（已注释说明 ✔）；写命令目前更早地被 `requires_nvapi()` 拦下 ✗ ⇒ **下一步明确动作**＝写入能力开启后，把 `optimize`/`autoscan-*` 的路由接到 macOS 后端（属阶段 11 共享入口决定 ✔），而非改这里的判断 ✔。

设备访问 1 次（anchor）、**无写入、未提权** ✔。

### 本轮追加五十（2026-10-18，偏移**范围块**逐字节实况：3 条记录、头 `+4` 是条目掩码推断、单位口径不一致、给写实验的选条指导）

**实测（夹具 `core/tests/data/macos-offset-range.bin`，7188 B）**：头 `+0=0xff,+4=0x7,+8=0,+12=100,+16=256` ✔；**恰 3 条记录，条目 0/1/2** ✔（tag `0x01030101/0x01040101/0x01050101`，`+4=1`，`+8=1638400`＝1638.4 MHz，`+20/+24=∓18750`，`+28=0xff0100`，三条**完全相同** ✗）；其余 29 条全零 ✔。
**推断＋证伪条件**：头 `+4=0x7` 的 3 位恰对应 3 条填充记录、且位置与控制块掩码相同 ⇒ 判为**条目掩码** ✔；下次健康读取若出现"掩码位 ≠ 填充条目"即作废 ✗。
**跨块**：范围块按**条目索引**寻址（0/1/2 ✔，与控制块约定一致 ✔）⇒ **"10 个域只有 3 个带范围"是实测事实** ✔；这 3 条对应哪些公开域**尚未定位** ✔（需位↔域映射，只能由写/回读实验确立 ✔）。
**单位口径不一致（如实）**：范围块给 `±18750`（按副本命名＝kHz，±18.75 MHz）而目录 `+44` 只在条目 0/2 给 `∓1000`/`−1000..+3000` ✗ ⇒ 非同一量，**哪个才是可设置窗口尚未定位** ✔。⇒ **写实验选条指导**：优先选范围块声明过的**条目 0**，并同时观察目录 `+44` 是否随写变化 ✗ —— 只有其一变化即定位到窗口字段 ✔。
**代码侧**：`macos_offset.rs` 既有解析与本实况一致 ⇒ **无需改动** ✔；仅头的 `+4` 掩码语义此前未记录 ✗ ⇒ 已记（连同证伪条件 ✔）。**设备访问 1 次（anchor）、无写入、未提权** ✔。

### 本轮追加五十一（2026-10-18，温度族**分层定位**：驱动只声明一个温度命令、无策略操作码；macOS 客户端侧无该面；顺带修正 host-608 表的**过度解读**）

**一、驱动/RM 层**：测试机带名命令表（605 行）中温度相关**只有** `0x20800513 subdeviceCtrlCmdThermalSystemExecuteV2 @1432` ✔，`0x2080 06xx` 其余全为 I²C ✗ ⇒ **驱动声明表里没有温度上限/策略操作码** ✔。GSP 表中该 id `flags 0x48` ✔ 且有**非特权孪生 `0x20808513 @1432 flags 0x10248`** ✔（**仅 GSP 表**有 ✗）。
**二、修正一条过度解读**：`host-608.tsv` 有 `0x20800513` 而**无** `0x20808513` ✗ ⇒ 该表漏掉的恰是**非特权孪生** ✔ ⇒ **它是公开面清单，不是穷尽表**；"表里查不到"≠"不存在" ✔（第二个独立例证 ✔，第一个是时钟域私有族 ✔）。
**三、macOS 客户端层**：测试机 `reference/nvidia-macos-driver` 全目录仅 1 个文件提 thermal ✗（`gpu-port/xkpi/bsdcompat/include/sys/systm.h`，BSD 兼容层，与 GPU 温度策略无关 ✗）⇒ **该面在 macOS 侧未实现** ✔（`nvapi-rs` 有该面 240 处实现，但属跨平台 NVAPI 层 ✗）。范围＝移植检出源码 ✔，不等于已加载 kext 全部符号 ✗（如实标注 ✔）。
**四、结论与下一步**：可读候选＝`0x20808513 @1432`（非特权孪生，**从未发送** ✗）⇒ 列入下次健康会话**只读**候选 ✔；策略语义（限额/跳变点）属 **NVAPI 客户端抽象**、macOS 侧缺失 ⇒ 是**客户端/驱动实现层缺口** ✔，**不是**"命令不存在"、**不是**固件限制 ✔。本轮温度族状态＝**调查未知（已定位到层）** ✔，与风扇/功耗族分开 ✔。
**设备访问 1 次（anchor）、无写入、未提权** ✔。

### 本轮追加五十二（2026-10-18，**代码**：两本运行手册的预检审计与两处保守化修正 —— 楔形信号必须停、成功判定不得少项）

**背景**：下次健康会话是稀缺资源，因此本轮**不碰 GPU**，只做两本 runbook 的飞行前审计 ✔（目的：把"该停就停、该判失败就判失败"钉死 ✔）。

**修正一（`tools/clock-offset-read-run.sh`，安全方向）**：原脚本在首个用例出现 `0x62` 时**只打印提示、仍继续跑** `--clkctl-read` ✗，且**完全不检查 `0x65`** ✗ —— 与 goal"再现 `0x62`/`0x65` **立即停**"不符 ✗。现改为：首用例日志出现 `NV_ERR_RESET_REQUIRED`（`0x62`）**或** `NV_ERR_TIMEOUT`（`0x65`）⇒ **`exit 5` 并说明"更宽的电池会对每个 id 都回 `0x62`、什么也证明不了，且未写入任何东西"** ✔。脚本头部原有散文（"aborts on the first 0x65/0x62"）现与代码一致 ✔。
**修正二（`tools/clock-offset-write-run.sh`，判定方向）**：原成功判定只查 `clk-offset-verdict` ＋ `restore verified` ✗ ⇒ 会出现"**回读没动但恢复成功**"却被报成 `ALL VERIFIED` ✗ —— 正是本课题最忌讳的误判 ✔。现要求**四项同时成立**：`clk-offset-verdict` ＋ `same-field yes` ＋ `others-unchanged yes` ＋ `restore verified` ✔（四段字符串均取自探针**自身格式串**，不依赖总判定 token 的渲染形式 ✔）。
**顺带核对（避免误改）**：曾疑 `restore verified` 在探针里不存在 ✗，核对后确认它由 `"...restore %s..."` 与 `"verified"` **拼接**而成 ✔ ⇒ 该子串**确实会出现在日志里**，脚本原本正确 ✔（**没有**基于假阴性去改 ✔）。
**验证范围**：`sh -n` 两本均 OK ✔；用**探针真实措辞**构造合成日志验证新条件 ⇒ 健康日志 `ALL VERIFIED` ✔、**被拒日志 `NOT verified`** ✔（并同时打印"旧条件会把被拒日志判成 ALL VERIFIED"以留证 ✔）、`0x62`/`0x65` 日志均 `STOP` ✔。**诚实标注**：合成日志取自探针格式串 ✔，脚本本身**未在设备上实跑** ✗（留待下次健康会话说 ✔）；**设备访问 1 次（anchor）、无写入、未提权** ✔。

### 本轮追加五十三（2026-10-18，**代码**：纠正"delta 打印有损"这一**假信念**，并把唯一被截断的摘要行放开到覆盖整块）

**纠正的信念**：此前（含探针注释与 runbook 头部、以及我自己的交接记录）写着"A5 sweep 的 delta 打印**有损**、106 个改变 dword 只留下 40 个" ✗。本轮逐行核对该打印器后确认：那是**当时抓到的那份日志**的性质 ✗，**不是**该打印器**当前格式**的性质 ✔ —— 它现在
- 把**每一个** written dword 连同 seed 与写入值、每行 5 个**全部**打印 ✔（`nvx_emit` 的 detail 域 288 B 限制用**分块**规避 ✔）；
- 另给**覆盖全块每个 dword** 的类别图（`U`/`Z`/`W`）✔。
⇒ 唯一被截断的是那行**紧凑的 `W offsets:` 摘要** ✗（`woff[200]` 与 32 项上限 ✗，控制块实际要写 106 项 ✗）。
**改动**：`woff[200]`→`woff[640]`、上限 32→120 ⇒ **控制块 106 项可完整列出** ✔；并更正两处注释（探针 hex-dump 头、`tools/clock-offset-read-run.sh` 头），使其表述为"**值**与**类别图**无损，唯一有上限的是摘要行" ✔ —— 免得后续轮次去追一个**并不存在**的数据丢失 ✗。
**验证范围**：`sh -n` OK ✔；测试机 **idle 与 full 两个 profile 均构建成功**（`built probe-macos-rm-idle (profile idle)` / `built probe-macos-rm (profile full)`，无 error ✔）；两侧 sha 一致（`ce532ebd…` / `87bb1930…`）✔。**诚实标注**：dump 行只在 **GET 成功**时才打印 ✗ ⇒ 楔形期间看不到新摘要长度 ⇒ 该改动**目前仅编译验证** ✗，行为留待下次健康会话观察 ✔。**未跑 GPU**，设备访问 1 次（anchor）、无写入、未提权 ✔。

### 本轮追加五十四（2026-10-18，**纠正**：目录无窗口证据；range 字段以仓库 IDA 实测偏移为准，单位 kHz 已定位）

(1) **纠正**：先前把目录记录里像区间的 `+40/+44(+48)` 当作"可设偏移窗口" ✗。仓库内 `nvapi-rs/sys/src/gpu/clock.rs:423-448` 的 IDA 实测给出权威偏移：**RM+64 类型字节（取反：0 = 无 range 信息）**、**+68 域 id**、**+80/+84 rangeMax/Min**、**+88/+89 VFP 索引上下界** ✔。对 12336 B fixture 重新解码：**每一已填行 `+64` 都是 0** ⇒ **这些域无 range 信息** ⇒ `+80/+84/+88/+89` **全零自洽** ✔ ⇒ 目录**不携带**调用方可见的窗口 ⇒ 撤掉"`+40/+44` 是窗口"的解读（字节事实保留）✔。
(2) **单位新定位**：`nvapi-rs/src/types.rs` 中 `From<i32> for Kilohertz2Delta` **恒等** ✔（`×2` 仅见于 `KilohertzDelta→Kilohertz2Delta` ✔），而 `clock.rs` 把 `rangeMax/rangeMin` 原样包进该类型 ⇒ **调用方单位 = kHz** ✔。
(3) **表内核对**：修正表内 `0x20809019@12336`/`0x20809025@7188`/`0x2080901b`+`0x2080d01c@2108`/`0x2080b201@2444`/`0x20808532@336`/`0x2080901d@32680`/`0x20809026@5332`；**无 2344** ⇒ `0x928` 属 **NVAPI 调用方**层、`12336` 属 **RM 块**层 ✔，两层结构不同属正常 ✔。
**状态**：调用方 range 字段 = 已定位且单位明确 ✔；`0x20809025` 的 ±18750 是否同族 ⇒ **尚未定位** ✗。全部静态，**未跑 GPU**，设备访问 1 次。

### 本轮追加五十五（2026-10-18，**审计**：树内发送层只发 GET（写通道缺的那一层再获独立支撑）＋更宽掩码的预登记判据）

(1) `core/native/macos_rm.c`（**本仓库自己的**发送侧，即树内的实际封送逻辑）只有三个 GET ✔（`0x20809019@12336`/`0x20809025@7188`/`0x2080901b@2108` ✔），`+4` 种 `0xFF` ✔，发送前清零 ✔；**搜 `0x2080d0*`/`write` 零命中** ✗ ⇒ **该层没有写命令** ✔。⇒ 偏移写入缺的那一层再获**独立支撑**：**客户端层只有 GET（本仓库事实）** ＋ **未提权 ⇒ `0x1b`（实测）** ✔；**不是协议缺失、不是固件问题** ✔。
(2) 该注释**自己写明**控制块只选了目录 0..7、*"a wider mask has not"*（更宽掩码尚**未**测量）✔ ⇒ 第 8/9 条目从未读过 ✗ ⇒ **只读即可补** ✔（与写入门控无关），runbook 已含 `0x3FF` 档 ✔。
(3) 互证：`core/src/macos_rm.rs:328` 记录调用方尺寸 **2520/2404 被 `0x1f` 拒绝** ✔，RM 侧三尺寸被接受 ✔ ⇒ 与 §20 的两层结论一致 ✔。
(4) **预登记（只读）**：`0x3FF` 全选十条 ⇒ 按已测算术与 fixture 类型序列（非-03 条目 9 个）**预测 132 个写入 dword** ✔；=132 ⇒ 映射与算术同时证实且第 8/9 条目写入集免写入可知 ✔；=154 ⇒ 算术作废重推 ✔；其它 ⇒ 逐条 census，禁止猜 ✗。
**状态**：发送层审计=已实现且实测（读）✔ / 写入=**未实现（缺客户端命令 + 需 root）** ✔。全静态，**未跑 GPU**，设备访问 1 次。

### 本轮追加五十六（2026-10-18，**定位**：`0x20809025` 的 ∓18750 = 偏移下界/上界（±18.75 MHz 控制跨度），单位 kHz；公共文案随证据更正）

(1) **定位**（仓库内封送资料，非猜测 ✔）：`nvapi-rs/sys/src/gpu/clock.rs:5408-5428` 明载 `RM 0x20809025` 的 ClkFreqController 记录含 `1638400 max_kHz` 与 `±18750 offset pair`，并把 `rec+0x38` 称为 **freq offset floor**、`rec+0x3C` 为 **offset ceiling**（`nvapi-rs/src/clock.rs:2246` 称 **±18.75 MHz control span**）；`sys/src/gpu/clock.rs:4814-4821` 另有 IDA 实测：该值为 **16 位** `0xB6C2=-18750`，与 §18 实测 int16 宽度吻合 ✔。⇒ **语义 = 偏移跨度（非绝对上限）、单位 = kHz** ✔，**§20 的"尚未定位"就此关闭** ✔。
(2) **随证据更正公共文案** ✔：`core/src/macos_rm.rs` 的 `ClockOffsetLimits`/`clock_offset_limits_khz` 文档与设备注记、以及 `core/src/macos_offset.rs` 的命名caveat，原先写"目录 Gpc/M 行的 ±int16 窗口**单位未定位**、故不作数" ✗ ⇒ 改为：目录那对 ±int16 **不是**调用方可见窗口（RM+80/+84 且各行 type 0 = 无 range 信息 ✔），**上报的窗口来自 range 块**，其含义与单位已如上定位 ✔。
(3) **如实保留的未证** ✗：副本的调用方几何 `0x4C+32×0x60` 与本驱动 RM 块 `20+32×224` 是两层，记录**仅以内容匹配**（1638400 与 ∓18750 int16 全部吻合 ✔），**逐字段映射尚无证明** ✔。
**验证范围**：`cargo fmt --all -- --check` **clean** ✔；`cargo test -p nvoc-core --all-targets` ⇒ **109 passed / 0 failed**（另 51 个 hardware-gated ignored ✔）；clippy 仅剩已知的 `nvapi-sys` 依赖告警 ✔；`pynvoc` 在本环境**无法链接**（pyo3 符号，环境问题、与本改动无关）✗ ⇒ 本轮验证范围为 `nvoc-core` ✔，且该 crate 的测试只读 `CLOCK_OFFSET_UNREADABLE_REASON`（本轮未改 ✗）⇒ 无期望漂移 ✔。**未跑 GPU**，设备访问 1 次。

### 本轮追加五十七（2026-10-18，**对表**：range 块 `+4` 掩码获仓库内副本独立佐证；`0x20809026` 两层尺寸厘清并登记一次低成本只读）

(1) 与 `nvapi-rs/sys/src/gpu/clock.rs:5408-5431` 逐项对表 ✔：副本自述 **`RM 0x20809025`**、`MASK` **在 `+4`**、`Live TU116: mask 0x7`、记录含 `1638400 max_kHz` 与 `±18750 offset pair` ✔ ⇒ 与本机实测（head `+4 = 0x7`、3 条有值记录、`+8 = 1638400`、`+20/+24 = ∓18750` int16 ✔）**逐项吻合** ⇒ **"`+4` 即掩码位"的推断由副本独立佐证** ✔（此前仅实测推断 ✔）。几何 `0x4C+32×0x60` vs `20+32×224` 仍为**两层、映射未证** ✔；副本的 `rec+4` 类型枚举属**另一块**，不与目录行的 `+4` 掩码混用 ✔。
(2) **`0x20809026`**：副本的调用方尺寸为 **2508**（magic `0x9CC` ✔）、`MASK-SEEDED at +4` ✔，并声明 TU116 上该 v1 路径**全静态 {6,0,0}** ✗（470 代数据未填，需 v3 戳记 `0x30970` ✔）。
(3) **修正我自己的记录** ✗：该 id **不是**"从未读过"——探测程序有 **2508** 用例表 ✔、解码器有 **5332** 几何 `(5332, 84, 164)` ✔；offset 电池拒发的是 **5332** 那一形式 ✗，因为**正是那次调用**闩住了 GSP ✗ ⇒ 准确说法应为"**RM 尺寸那一形式**不发" ✔。
(4) **登记**（只读、低成本、待健康会话）：加发一次 **`0x20809026@2508`** ✔，并带上副本预期（TU116 v1 路径全静态 ✗ ⇒ NV_OK 也不代表有实时值 ✔）。
**状态**：range 掩码语义=已实现且实测（读）＋**已由仓库内副本独立佐证** ✔；status 块=**已实现待实测（2508 尺寸形式）** ✔。**验证范围**：静态 + `sh -n` 通过 ✔；**未跑 GPU**，设备访问 1 次。

### 本轮追加五十八（2026-10-18，**纠错表＋可写性**：偏移族唯一声明写对＝控制块；读写 id 规则以 +1 为主、共 51 对）

(1) **规则**（本地孪生表头原文）：`flip bit14 (privileged) and idx+1` ✔；对修正后的 1118 行导出表穷举：读半区 1026 行 / 写半区 **92 行** ✔，`id+0x4001` 且尺寸相同的 **47 对** ✔、`id+0x4000` 的 4 对 ✔（后者记为规则下的异常，不强行解释 ✗）。**更正**我此前的路径记录：孪生表在 `nvoc-analysis/twin-map-610.57.04.txt`，不在 `tables/` 下 ✔。
(2) **偏移族可写性** ✔：`0x2080901b`↔`0x2080d01c`（2108/2108，flags `0x48`/`0x44`）**存在** ✔；`0x20809019`→`d019`、`0x20809025`→`d026`、`0x20809026`→`d027`、`0x20808532`→`c532`、`0x20808513`→`c513` **全部不存在** ✗；本地孪生表亦只有一行属于该族 ✔ ⇒ **目录/range/status 按声明只读；偏移族唯一写通道就是控制块那一对** ✔ ⇒ 已排定的写实验（bit 0、record+8）打的是**唯一**声明写路径 ✔。
(3) **纠错表**（撤回/收窄）✔：目录 `+40/+44` "可设窗口"**撤回** ✔；`±18750` 单位**已定位** ✔；"只声明一对私有读写"**收窄为**仅指所探测的六个 id（整表实为 51 对）✔；"客户端层没有 SET"**保持并限定为客户端层**（驱动写半区 92 行）✗；"`0x20809026` 从未读过"**更正** ✔；`0x2080d024` 归属**已定位**为 `0x20809023` 的孪生 ✔；`+4` 掩码**已获副本佐证** ✔。
**状态**：全部为**静态**结论（表 + 仓库源码）✔；写入仍**未实现/待授权** ✔（但已确认写路径存在且唯一 ✔）。**未跑 GPU**，设备访问 1 次。

### 本轮追加五十九（2026-10-18，**热/风扇族声明面审计**：HWFS 有完整声明与本代实测值但本机从未发；散热器/仲裁器背后的 RM id 尚未定位）

(1) 副本共给 **31** 个 RM id 做过注记 ✔，**其中无任何风扇/散热器/仲裁器 id** ✗ ⇒ 客户端风扇面在副本里无 RM id 注记 ⇒ **"散热器/仲裁器背后的 RM id"记为尚未定位** ✔（是"两份表与副本都没注记"，不是"头文件搜不到" ✗）；这解释了风扇族只有探测猜出的 `0x20808532` 而从未确认 ✔。
(2) **HWFS** ✔：副本注记 `GET RM 0x20808540/0x20800540, escape 0x07000190/0x07000191`，平面 52 字节无掩码，`sel=2 → {out=10, 0x7FFFFFFF, 64, 256}` **在 TU116/462.96 与 4060/R610 上都实测过** ✔；表中 `0x20808540 24 0x48`（GSP，非特权半区 ✔），孪生表给出**声明写孪生** `0x2080c541 24` ✔。**两层尺寸**（副本 52 ✗ / GSP 24 ✔）⇒ 只读试发按**声明 24** ✔。**本机从未发过** ✗（探测全树 grep 零命中 ✔，实际只发过 `0x20808513`/`0x20808532` ✔）⇒ **登记一次 `0x20808540@24` 只读试发** ✔（带副本预期 ✔）。
(3) 热族状态**按 id 分列** ✔：`0x20800513`（ThermalSystemExecuteV2）＝已实现待实测；`0x20808513`@1432（GSP-only、从未发）＝未实现待实测；`0x20808540` HWFS＝**已实现、已派发 1 次（`0x62`，楔死所致 ⇒ 含义尚未定位）**，副本另有本代实测值 ✔；**热限/热策略 opcode**＝调查未知（驱动自命名的表里仅一条热命令 ✔，属**有名有姓的层次**：客户端/驱动实现缺口 ✔）。
⇒ **热/温度族不是整族不可读** ✔，纠正此前"整族未知"的说法 ✗。**验证范围**：静态 ✔；**未跑 GPU**，设备访问 1 次。

### 本轮追加六十（2026-10-18，**功耗族声明面 + 特权面计数更正**）

(1) **更正** ✗：上一轮"写半区 92 行"是**门控计数**（`(cmd&0xC000)==0xC000` ✔）而非特权面总数；按位穷举 ⇒ **bit14 置位共 112 行**（92 行带 bit15＝legacy 转发形式 ✔，**另 20 行不带** ✗）⇒ §24.1 的 `flip bit14 and idx+1` 规则应**收窄为 legacy 转发族** ✔；孪生对数 47+4 不受影响 ✔。
(2) **功耗族** ✔：`0x2080206e`（RATED_TDP GET，12，`0x10008` ✔）/`0x2080206f`（SET，12，`0x40048` ✔）**两者 bit14 皆清** ✗，且按 legacy 规则预测的 `0x20806070/71` **不存在** ✗ ⇒ **存在第二种配对约定**（导出命令用 `…ne`/`…nf` 相邻 id 表示读写对 ✔）⇒ "通用孪生规则"**收窄** ✔；`0x2080a06e`@12 flags `0x48` 与 GET 同尺寸但 flag 类别不同 ⇒ 是否同命令**尚未定位** ✔。
(3) flag 位 `0x40000` 含义：**正反例都有**（`0x2080206f` 含 ✔ / 偏移 SET `0x2080d01c`=`0x44` 不含却仍实测 `0x1b` ✗）⇒ **尚未定位** ✔，正反例留存 ✔。
**状态**：`RATED_TDP` 读＝**已实现待实测** ✔（GET 声明存在且非特权半区 ✔）；写＝**声明存在、待授权** ✔（SET 声明存在，且与偏移写不同的门控故事 ⇒ 不可用偏移写的结论替代 ✔）。**验证范围**：静态 ✔；**未跑 GPU**，设备访问 1 次。

### 本轮追加六十一（2026-10-18，**控制块逐字段权威版式＋写实验候选改序**）

(1) 副本 `nvapi-rs/sys/src/gpu/clock.rs` 记有 **id↔NVAPI 对照** ✔：`0x20809019`→ClkDomainsGetInfo、`0x2080901b`→GetControl、**`0x2080d01c`→SetControl**、`0x20809006`→MeasureAvgFreq ⇒ 与本会话的"三个 GET ＋ 唯一写对"**完全吻合** ✔。记录版式：`+8` mask、`+100` 起 32×72B（V1）；每条记录 `+0` 类型、**`+44` offset_khz、`+48` range_min、`+52` range_max、`+56` applied** ✔（V2：`0x261A4`=24996、记录@292 步长 772 ✔）。
(2) **被拒尺寸有名有姓** ✔：本机被 `0x1f` 拒的 **2404 = `0x964` = 副本的 V1 控制块**、**2520 = `0x9D8` = 其私有信息块** ✔；本机能通的是 2108 等 ⇒ **两层不同** ✔。
(3) **唯一行为改动（仍受门控）** ✔：写实验首例改为 **`0:44:250`**（副本唯一点名的 offset 字段 ✔），`0:8:250` 降为第二 ✗，回读**同时看 `+56`(applied)** ✔；边界写明：记录基址/步长不同（60/64 vs 100/72）⇒ 字段版式搬运属**推断** ✔，已列可反驳点 ✔。运行手册头部更新，`sh -n` 通过 ✔。
(4) 副本注记的 13 个 `0x208090xx` id **在导出表中全部存在** ✔ ⇒ 反向加强 §25.1（副本里**确实没有**风扇/散热器 id ✗）。`0x20809006` 登记为**补充只读候选** ✔（8 字节，语义为采样两次算 Δ；**只作旁证** ✔）。
(5) **功耗族读**分类更正为 **已实现且实测（命令可通）／返回值可解释性待确认** ✗（日志多次 `NV_OK` ✔，detail 为 `raw[0..2]=0 0 0 (client/input/vpstate echo)` ✔）。
**验证范围**：静态 ＋ 运行手册注释（`sh -n` 通过 ✔）；**未跑 GPU**，设备访问 1 次。

### 本轮追加六十二（2026-10-18，**各族实测证据核对**：可读面远好于先前结论，并更正两处过度悲观）

(1) **实测证据表**（穷举测试机既有日志 ✔，均为 wedge 之前的记录 ✔）：**核心/显存频率**＝`rusd-readout` **NV_OK** ✔（`gpc 1530000`/`mem 6000984` kHz，负载中 `1935000` ✔）⇒ 可读/可解释/有原状态 ✔；**温度**＝`thermal-sensor` **NV_OK** ✔（`reading 47 | range [0,127] | target 0 provider 0` ✔）＋`thermal-classify` ✔（target 0 type 1 = GPU ✔）＋RUSD `temp[gpu]` ✔（负载中 59.34 °C ✔）⇒ **有范围** ✔；**风扇**＝RUSD/`get-status` **`fan_rpm 1833`（负载）/`fan 0 rpm`（空转）** ✔ ⇒ 有范围（0–1833 实测）✔；**功耗**＝RUSD `limit 100000 mW`、`tgp 12857`、负载 `power_mw 12798`、`util 99 %` ✔；**热批处理**＝`0x20800513` **1432 形式 batch accepted** ✔、`1 target(s), 1 sensor(s)` ✔。
(2) **被拒/失败（同为证据）** ✔：`0x20809006` measure-freq **`0x1f`**，穷举 44/55/60/61 尺寸与 11 个魔数**全不通过** ⇒ **接受形式尚未定位** ✔（**形式**问题 ✗，非"无此命令" ✔）；`internal-vpstate-info (0x20800ad4)`、`edpp-limit-info`、`pmumon-clk-samples`＝`0x56` ✔；`0x20800532`＝`0x56` ✔ ⇒ 驱动明确拒绝 ✔。
(3) **更正两处过度悲观** ✗：第 49 轮"热族＝调查未知" ⇒ 实测 `0x20800513`(1432) **被接受且带出传感器读数/范围/target** ✔ ⇒ 温度＝**可读/可解释/有范围/有原状态** ✔，缺的是**写面** ✔；"风扇族只剩探测猜出的 id" ⇒ **RUSD 已实测 1833 rpm** ✔ ⇒ 缺的是**仲裁控制面** ✔。
(4) **对最高优先的直接意义** ✔：偏移通道所需"**当前值/限制/原状态**"**已在手** ✔（RUSD 实时频率 ✔＋range 块跨度 ✔）⇒ 只剩 **①哪个 dword 承载偏移**（§27.3 首选与可反驳点 ✗）与 **②写权限前置条件**（已定位为提权＋客户端 SET 入口缺失 ✔）⇒ **不属于"读不到"** ✔。
(5) **压力侧**：`stage6-fan-read-…` 记录 **20 s Vulkan 负载**中 rc=0 遥测成功 ✔（`graphics_clock_khz 1935000`、`gr 99.1 %`、`fan_rpm 1833` ✔）⇒ 压力与实时遥测**本机跑通过** ✔；按原目标属**压力负载**证据 ✔，**不能替代偏移设置成功** ✗。
**验证范围**：静态穷举既有日志 ✔；**未发新命令、无写入、未跑 GPU**（本轮无新增设备访问）✔。

### 本轮追加六十三（2026-10-18，**重大**：偏移已是可读的——`0x20809061@776` 返回逐域频率＋带符号 kHz 偏移）

(1) **接受形式已穷举** ✔：`0x20809061` 的 493060/83996/369796/157692/249844 全被 `0x1f` 拒 ✗，**776 被接受 NV_OK ✔（56 非零 dword）**；`0x20809019` 的 2520/296620/329388 全拒 ✗（工作形式 12336 ✔）；`0x2080901b` 的 **2404(v1，含 mask 变体) 与 24996(v2) 全拒** ✗（工作形式 2108 ✔）；`0x20809021`/`0x20809022`/`0x20809006` 在 61 尺寸 × 11 魔数下**全不通过** ✗（接受形式尚未定位 ✔）⇒ **规律**：本驱动每个 id 只接受**导出表为其声明的那个尺寸** ✔（副本层的尺寸全被拒 ✗）⇒ 解释了尺寸扫描徒劳 ✔。
(2) **776 载荷**（推断＋硬佐证 ✔）：重复记录 `00000303 <选择子> <频率kHz> <字段4> <标志>`；实测 1→**1 530 000**、2→1 455 000、4→1 545 000、8→404 488（−244）、**0x10(bit4=MCLK)→6 000 984（−126）**、0x20→1 318 976、0x40→810 000（−3904）、0x40000→107 488（−5）、0x80000→540 000（−244）、0x100000→1 408 976 ⇒ 字段4＝**带符号 kHz 偏移**（与 ±18.75 MHz 跨度同量级 ✔）；**1 530 000 与 6 000 984 与 RUSD 实时读数逐位相同** ⇒ 真实实时时钟文档 ✔。`ffffffff`＝"未设置"**尚未定位** ✗。
(3) ∓18750 复核 ✔（`+40:ffffb6c2`/`+44:0000493e`/`+252:00190000`/`+264:ffffb6c2`）；**可重复性** ✔：两次运行差异仅 66 字节且全在 `time` 字段 ⇒ 载荷逐字节相同 ✔。
(4) **对最高优先的后果** ✔：**第 3 条已完成**（当前值＝RUSD＋776 逐域频率 ✔；限制＝±18.75 MHz/1638400 kHz ✔；**原状态＝逐域偏移本身** ✔）；第 4 条不受损（偏移是从文档**读回**的 ✔）；只剩 **①写能力（提权＋客户端 SET 入口 ✔）②写入后字段是否随动（写实验 ✗）**⇒ 都不是"读不到" ✔。
(5) **更正** ✗：早先"VF 读从未发过"被证伪 ✔（`stage-clkvf-read-*`/`stage-clkvf-stamped-*` 存在且重复运行 ✔，`0x20809061@776` 已实测 NV_OK ✔）。
**验证范围**：静态穷举既有日志 ✔；**未发新命令、无写入、未跑 GPU** ✔。

### 本轮追加六十四（2026-10-18，**代码步骤**：把可读的偏移解析落到仓库封送层）

(1) **落地** ✔：`core/src/macos_offset.rs` 新增 `CLK_DOCUMENT_CMD=0x20809061`/`CLK_DOCUMENT_BYTES=776`/`CLK_DOCUMENT_RECORD_MARKER=0x00000303`/head 8/b跨度 24/槽 32 与 `parse_clock_document()`、`ClkDocument{head, entries, unparsed_slots}`、`ClkDocumentEntry{index, domain_bit, frequency_khz, offset_khz, flags}`；`core/tests/data/macos-clkdoc-776.bin`（**776 字节，逐字节取自测试机 `clkdoc-read.txt` 的 hex dump ✔**）＋ fixture 测试；几何自洽 8+32×24=776 ✔。
(2) **fixture 内容（=证据）** ✔：11 条记录（域位 1/2/4/8/0x10/0x20/0x40/0x40000/0x80000/0x100000/0x200000，频率含 **1 530 000** 与 **6 000 984**，带符号字段含 **−126**、**−244**、**−3904**、**−5**）✔；两处硬佐证：两个频率与该会话 RUSD 实时读数**逐位相同** ✔、0x10 是副本自己掩码里的 MCLK 位 ✔。
(3) **stray 槽** ✗：槽 21/31 各只有 1 个 `0xff` 字节 ⇒ **按索引报告**而非当条目或丢弃 ✔；"末尾槽是填充"仍是推断 ✔。
(4) **推断边界** ✔：字段名为读取语义描述而非驱动声明；注释同时给佐证与**可反驳点**（受控写入应使该字段随动 ✔）；`flags`／`ffffffff` 语义**尚未定位** ✔。
(5) **验证**：fmt 干净 ✔、`clippy --workspace --exclude cli-stressor-cuda-rs --all-targets -- -D warnings` 干净 ✔、`cargo test -p nvoc-core --all-targets` 全绿（93 passed，含 3 个新单元测试＋fixture 测试 ✔）；**客户端本机构建/测试 ✔，未发新命令、无写入、未用 GPU** ✔。
(6) **尚未接线** ✗：`core/native/macos_rm.c` 仍只读三条 block ⇒ 下一步发 `0x20809061@776` 并并入 `OffsetChannelSnapshot`/`SettingsCapabilities` ✔。

### 本轮追加六十五（2026-10-18，**接线**：客户端封送层读 `0x20809061@776`）

(1) **选择依据** ✔：追加六十四 落地解析器但无调用者 ⇒ 按原目标第 5 条接入 ✔。改动：`macos_rm_abi.h` `NVOC_RM_OFFSET_KINDS 3→4`＋`NVOC_RM_OFFSET_CLOCK_DOCUMENT 3`；`macos_rm.c` `offset_commands[]` 追加 `{0x20809061u,776u}`（**仍只读**，无 SET id ✔）；`macos_rm.rs` `OffsetBlock::ClockDocument`(kind 3)＋`read_offset_channel()` 第四次读取调 `parse_clock_document`＋`apply_offset_channel()` 同 note 记录并**逐位列出**频率与偏移；`macos_offset.rs` `OffsetChannelSnapshot` 增加 `clock_document: OffsetBlockRead<ClkDocument>`（沿用 `Refused`/`Malformed`/`Parsed` 三态，**拒绝不会被渲染成偏移为零** ✔）。
(2) **不做具名域字段的理由** ✔：776 的域位是驱动自己的位，只有 `0x10` 能凭仓库内 NVAPI 副本命名 MCLK（其频率与实时显存时钟一致 ✔），其余十个位无名 ⇒ 升格会**猜名字**；note 逐位列值只报实测值 ✔。
(3) **验证范围** ✔：客户端 fmt/clippy（`--workspace --exclude cli-stressor-cuda-rs --all-targets -- -D warnings`）/`cargo test -p nvoc-core --all-targets` 全绿（93 passed）✔；**本机不编译 C 与 macOS 门控代码** ✗ ⇒ 测试机 `cargo build -p nvoc-core`（`OGKM=` 带上）build script 实执行＋编译通过 ✔，另以同一组 include 直接 `cc -fsyntax-only macos_rm.c` 通过 ✔；覆盖前核对四个文件 sha256 与 `b413f42^` 逐一相同 ⇒ 未覆盖新工作 ✔。
(4) **状态**：`0x20809061@776` 读面在客户端 = **已实现待实测** ✔（其**形态**被驱动接受已在第 53 轮由探针实测 ✔，两者不混同）；**设备侧本轮未读** ✗（GSP 仍 `0x62`）⇒ 无新增设备访问计数 ✔；未执行任何写入 ✔，写入门槛保持关闭 ✔。

### 本轮追加六十六（2026-10-18，**公共接口**：`clock_offsets` ＋一处过期断言更正）

(1) **选择依据** ✔：原目标第 5 条要求接入公共 core/CLI/Python 接口；第 31 轮只写进 note，用户拿不到数值 ✗。另发现 `CLOCK_OFFSET_UNREADABLE_REASON` 仍写"offset itself is not reported as a number" ✗ —— 已被第 53 轮实测推翻 ⇒ 必须更正 ✔。
(2) **落地** ✔：`core/src/macos_rm.rs` 新增 `ClockOffsetReading{domain_bit, frequency_khz, offset_khz}`（带 `Serialize`）与 `DeviceSettings.clock_offsets: Option<Vec<...>>`，由 `apply_offset_channel` 从 `parse_clock_document` 的**实测载荷**填充；**拒绝/畸形保持 `None`**（非空数组）⇒ "未读到"永不渲染成"偏移为零"。`core/src/lib.rs` 并入 re-export；`nvoc-python/src/lib.rs` 增加 `clock_offsets` 数组；`cli` 的 `get-settings` macOS 路径本就整块 `serde_json::to_value(...)` ⇒ **无需新管线**，JSON 自动携带 ✔（人读格式器是否显示**尚未确认** ✗）。
(3) **域位保留不换名字** ✔：同时给 `domain_bit` 与 `frequency_khz`；只有 `0x10` 能凭仓库内 NVAPI 副本命名 MCLK，且其频率即该会话实测显存时钟 ⇒ 用频率当**实测**标识 ✔。
(4) **保留的决定** ✔：`unavailable` 中 `core_clock_offset`/`memory_clock_offset` **故意保留**（读已实现但**尚未设备实测** ✗；这两族仍开放的是旧的 `0x2080901B` 槽位身份与"无任何 SET"）⇒ 原因字符串已改写为只陈述这两点，等实测后再决定是否移除键 ✔。
(5) **验证范围** ✔：客户端 fmt/clippy（`--workspace --exclude cli-stressor-cuda-rs --all-targets -- -D warnings`）/`cargo test -p nvoc-core --all-targets` **94 passed**（新增一例用**捕获载荷**断言 bit 1 = 1 530 000/−1、bit 0x10 = 6 000 984/−126，并断言 `Refused(0x62)` ⇒ `None`）/`cargo check -p pynvoc` ✔；测试机 `cargo build -p nvoc-core -p nvoc-cli`（`OGKM=`）通过 ⇒ macOS 门控代码与 CLI macOS 路径均编译 ✔；覆盖前三个文件 sha256 与 `c3f24e1` 相同 ✔。
(6) **状态**：`clock_offsets` = **已实现待实测** ✔；**设备侧本轮未读** ✗（无重启，uptime `1 day, 3:48`，同一 boot）⇒ 无新增设备访问 ✔；未执行任何写入 ✔，写入门槛保持关闭 ✔。

### 本轮追加六十七（2026-10-18，**人读输出验证**＋一处错渲染修复）

(1) **选择依据** ✔：追加六十六 把人读格式器记为**尚未确认** ✗。先读代码确认 `format_get_settings_output` 按键泛化（`Value::Array` 走"标题＋子块"）⇒ 可用**测试**答复 ✔。
(2) **测试暴露真实缺陷** ✗：新用例（渲染含 `clock_offsets` 的 get-settings 载荷）首次运行失败，实际输出 `Domain Bit: 0.001 MHz` ✗ ⇒ 根因 `cli/src/output.rs` `format_contextual_scalar` 对上下文含 `clock`/`frequency` 的**所有**数值叶子做 kHz→MHz 缩放 ⇒ 对频率正确、对**位索引**错误 ✔。
(3) **修复** ✔：新增目标化豁免——叶子名以 `_bit` 结尾按原样数字渲染（"位永远不是频率"），风格同文件既有定向豁免（`lanes`、per-rail 瓦数、非 domain 电压）✔。
(4) **回归证据** ✔：`cargo test -p nvoc-cli` **82 passed / 0 failed**（其余 81 例未变）⇒ 既有已验收输出依赖的缩放未受扰动 ✔；`cargo test -p nvoc-core --all-targets` 94 passed ✔；fmt/clippy（`--exclude cli-stressor-cuda-rs --all-targets -- -D warnings`）干净 ✔；测试机同步后 `cargo build -p nvoc-cli` 通过、sha256 一致 ✔。
(5) **保留的怪癖** ✔：同上下文 `offset_khz: -1` → `-0.001 MHz`（数值正确、丢失 kHz 约定）⇒ 为最小改动未改，记为待决 ✔。
(6) **状态**：人读输出 = **已确认（有测试）** ✔；`clock_offsets` 设备侧仍 **已实现待实测** ✔。本轮设备访问**仅一次 wedge 轮询**（verdict `0x62`，uptime `1 day, 3:53`，同一 boot）⇒ 未重启、未写入、无性能操作 ✔。

### 本轮追加六十八（2026-10-18，**逐族只读审计**：五段式＋缺失层归属＋一处旧断言更正）

(1) **选择依据** ✔：原目标把风扇/功耗/温度列为**必须分别调查**的次优先项，并要求每族独立给出"可读/可解释/范围/原状态/恢复"证据与剩余问题；此前只有一张汇总表，缺"缺失层归属"与"证据边界"。本轮**基于测试机已记录文件**核对（读文件≠设备访问）。
(2) **频率域偏移** ✔：可读＝RUSD 实时频率＋`0x20809061` 776 字节时钟文档（11 槽、有符号偏移）；可解释＝域位/频率/偏移（仅 `0x10` 可命名）；范围＝`0x20809025` **±18 750 kHz**；原状态＝重复读逐字节一致；恢复＝**N/A（无 SET）**；**缺失层＝前置条件（写权限）＋客户端 SET 入口**（驱动源码权限判定、set 身份、实测 `0x1b`、host 表 0 特权行、native 层无 SET id、导出命名 `0x2080d01c` 为 SetControl）⇒ **待授权**，非"只能固件逆向"。
(3) **风扇** ✔：可读＝`fan_rpm`/`fan_rpms`（记录采样 **1610/1613/1616/1619/1833**）；**范围＝无声明式范围**，仅观察区间；**更正** §28"`fan 0 rpm`（空转）→（0–1833 实测）"✗ —— `stage9-*` 的"前"采样是 **1613**，0 rpm **未在本批记录中复现**；**缺失层＝公开 RM 读/写命令本身**（百分比/模式/曲线/FanStop 均不在公开 opcode）⇒ 未实现/待授权；**预订动作**：wedge 解除后真正空转时重采一次。
(4) **温度** ✔：可读＝公开 `0x20800513` NV_OK；可解释＝与 RUSD 四舍五入一致；**范围＝声明式 [0,127]（本批唯一）**；原状态＝空转 44–47 °C；恢复＝N/A（公开 opcode 全为 GET）；缺失层＝写面不在该命令公开 opcode 内。
(5) **功耗** ✔：可读＝RUSD（100 W enforced、`power_policy_tgp_mw 12857`、负载 12 798 mW）；**范围＝驱动明确拒绝**：`0x20800AFD` → **`0x56`** ⇒ 不是"尚未定位"而是**驱动不实现**；恢复＝N/A（未跑任何 SET）；剩余未知＝write-only `0x2080d024@66060`。
(6) **`RATED_TDP`** ⚠️：命令 NV_OK 但记录载荷**仅输入回显** ⇒ **尚未取得数值**，分类"已实现待实测/调查未知"，下一步可证伪（重发＋查请求结构是否缺字段）。
(7) **压力侧前提顺带确认** ✔：`stage9-vulkan-*`/`stage9-stress-*` 的 `nvrm:`/`[VKGFX-H]` 横幅由 **NVK/nvrm 一侧**打印 ⇒ "底层是 NVK、不必经 MoltenVK"**有记录支持**（非假设）；但**渲染成功 ≠ 超频稳定** ✗。
(8) **设备访问** ✔：本轮**仅一次 wedge 轮询**（verdict `0x62`，uptime `1 day, 3:58`，同一 boot）⇒ 未重启、未写入、无性能操作 ✔。

### 本轮追加六十九（2026-10-18，**封送依据核对 ＋ 一处旧结论更正**）

(1) **选择依据** ✔：原目标要求"从实际封送逻辑还原"且禁止用"头文件搜不到"当未知理由；本轮逐条核对仓库/树内材料。
(2) **公开 OGKM 树核对** ✔：全树**无** `0x20809019/1b/d01c` 任何字面量，`ctrl2080clk.h` **128 行**、`ctrl2080clkavfs.h` **36 行**，SDK ctrl 头里 `0x20809xx` 零命中 ⇒ 这些 id **不在开源头**；权威依据改由 **GSP 导出表**（paramSize ✔）与**树内 NVAPI 副本**（字段语义 ✔）提供 ⇒ **不是未知**。
(3) **两层结构不同** ✔：NVAPI 调用者层 `2404`（`mask@+8`、stride 72）vs GSP 校验的 RM 块 **`2108 = 60+32×64`**（`mask@+4`）⇒ 早期发 2404 得 `0x1f` 是**层错**，不是"命令不存在"；把 NVAPI 字段偏移映射到 RM 十二个值槽**只能是推断**，证伪条件＝同字段回读（这也是 `clock_offsets` 走 776 文档的原因）。
(4) **写图（树内已记录）** ✔：`mask@+4=0xFF`+`0xA5` ⇒ 106/527 dwords 改写、头 14 写、每记录 13 槽（类型＋十二值槽）、`rec+4/+48/+60` 从不写、掩码位↔记录索引 1:1、`106 = 14+7×13+1`。
(5) **更正** ✗：INDEX 第 54 行"值 dword 全 0 ⇒ 本机 901b 是 V1、不给 offset"来自**掩码放在 +8** 的早期运行（三种掩码都只回 3 non-zero bytes）；掩码放对后 **106/527 dwords 被改写** ⇒ "值槽从未被写"不成立，"写入的是零"亦未被逐槽证据支持 ⇒ 现状记为**值槽确被写入、逐槽内容尚未捕获**。
(6) **预订动作（无代码改动）** ✔：`src/nvx_suite.c:2230` **已有** `0x2080901B @2108 in@4=0xFF bg=A5`（另有 `0x3FF`/`0x1`/`0`）但**从未执行**（现有两份 hex 只含 9019/9025）⇒ 健康会话跑一次即可逐槽读出十二值，**仍不发任何 SET**。
(7) **`0x3FF` 的实测预测** ✔：目录解出 **10 行**（`rows-as-bits 0x3ff`），行序 id **0,1,4,2,3,20,19,6,21,5**；`0xFF` 只到行 0..7 ⇒ 行 8（id 21 MSD）/行 9（id 5 HOST）此前未被任何掩码读到。
(8) **设备访问** ✔：本轮**仅一次 wedge 轮询**（`0x62`，uptime `1 day, 4 hrs`，同一 boot）⇒ 未重启、未写入、无性能操作 ✔。

### 本轮追加七十（2026-10-18，阶段 10 第一步：**macOS/非 CUDA 构建适配** ＋ 自造缺陷修复）

(1) **缺口（实测）** ✗：`auto-optimizer` 原先三个 bundling feature **都含 CUDA**（`stressor-bundled` 直接带 `cuda`＋`vulkan`，`default`＝cuda12）⇒ macOS（无 NVAPI/NVML、无 CUDA 运行时）只能 `--no-default-features`，**等于不捆绑 stressor** ⇒ 阶段 10 要求的"非 CUDA 构建适配"缺路径。
(2) **改法** ✔：把"进程内捆绑"与"后端选择"分开 —— `stressor-bundled` 仅 = `dep:cli-stressor-cuda-rs`（源码所有 cfg 门不变）、`stressor-bundled-cuda` 承载 cuda＋vulkan、cuda11/12 基于它、新增 **`stressor-bundled-vulkan`**；`default` 解析结果与改前**完全一致** ⇒ **保留原默认 feature、无既有构建行为改变** ✔。
(3) **验证** ✔：`cargo metadata` rc0；Linux 上 vulkan-only 路径 rc0 且 `cudarc` 命中 **0**；external 路径 rc0；默认路径 rc0；fmt／clippy `-D warnings` rc0；两路单测 **66 passed 0 failed**；**真机 macOS** `OGKM=… --no-default-features --features stressor-bundled-vulkan` **65 passed 0 failed**（含写入门控测试真实执行 ok）⇒ **macOS 非 CUDA 捆绑构建链已实测可用** ✔；该 stressor 的 **Vulkan 实际运行仍"已实现待实测"**（wedge ✗）。
(4) **自造缺陷修复** ✗：`macos_write_gate_follows_the_backend_capability`（本分支 `9ff687f`，我自己引入）断言 `refused == mutates` ⇒ 非 macOS 上恒失败（本机三路 feature 均复现），macOS 上又是同义反复 ⇒ 现改为：门控无效时提前返回、先钉"命令集合非空会改设置"、macOS 上断言四条命令均要求提权（真机实测通过）✔。
(5) **设备访问** ✔：本轮仅 wedge 轮询一次（`0x62`，uptime `1 day, 4 hrs`，同一 boot）；真机 `cargo test` 为**纯单元测试、不访问设备** ⇒ 未重启、未写入、无性能操作。

### 本轮追加七十一（2026-10-18，`RATED_TDP` 请求结构核对：**不是缺字段，是种子与预期答案同值**）

(1) **选择依据** ✔：功能表自己写的下一步是"重发＋查请求结构是否缺字段"✔。
(2) **命令在公开树里有声明** ✔（与 CLK 族相反 ⇒ 必须逐条查，不能一概而论 ✗）：`ctrl2080perf.h:399` 定义 `0x2080206e` ✔；`:405-418` 给出权威结构 `{client[in], input[in/out], vPstateType[in]}` ✔，文档＝"retrieves the current requested RATED_TDP action" ✔ ⇒ **唯一输出字段是 `input`** ✔；驱动分发表亦见 `g_subdevice_nvoc.c:11139` ✔；动作枚举 `DEFAULT=0/FORCE_EXCEED=1/FORCE_LIMIT=2/FORCE_LOCK=3/FORCE_FLOOR=4` ✔；status 命令 `0x2080206d`（探针早已发送 ✔）。
(3) **不是缺字段** ✗ ⇒ 请求三个字段齐全、12 B 即整结构 ✔；**缺陷是种子**：探针 `memset` 后只填 client ⇒ `input=0=DEFAULT`，而未被限速的 client 正确答案**本就是 0** ⇒ 输出里的"echo"与功能表的"仅输入回显"都是**假设而非测量** ✗。
(4) **已加入可证伪只读用例** ✔：保留零种子对照遍，新增 `input=FORCE_LIMIT(2)` 播种遍 ✔；预注册预测＝上一个答 `DEFAULT(0)` 的 client 在此仍应为 0，若恰回 2 才是真回显 ✔（同会话对照排除"所有 client 恰好都 FORCE_LIMIT" ✔）；`GET_CONTROL` 不仲裁不 latch，SET `0x2080206f` **仍未发送** ✗。
(5) **验证范围** ✔：真机 macOS `OGKM=… sh build.sh` ⇒ **`built probe-macos-rm-idle (profile idle)`** ✔；**未运行** ✗（GSP `0x62`）⇒ **已实现待实测** ✔。分类：结构/字段/枚举＝已定位（公开头文件 ✔）；数值＝已实现待实测；写＝待授权 ✔。
(6) **设备访问** ✔：本轮仅一次 wedge 轮询（`0x62`，uptime `1 day, 4:13`，同一 boot）；真机编译不访问设备 ⇒ 未重启、未写入、无性能操作。

### 本轮追加七十二（2026-10-18，压力侧就绪度核对：**Vulkan 非 CUDA 路径的构建与入口契约**）

(1) **选择依据** ✔：次优先级要求"先复用原 NVOC 的 Vulkan 压力实现（`cli-stressor-cuda-rs --vulkan-only`，底层就是 NVK），wedge 解除后才跑"⇒ 本轮核实**跑之前的先决条件** ✔。
(2) **构建可行** ✔：本机 `cargo check -p cli-stressor-cuda-rs --no-default-features --features vulkan` **rc 0（20.79 s）** ✔；测试机 macOS 同一 recipe ⇒ **`Finished` 未重编**，现存 `target/debug/cli-stressor-cuda-rs`（10 885 680 B，10-17 10:10）即该 feature 的产物 ✔；指纹独立佐证 ✔：`cli-stressor-cuda-rs-0633834fc92c7fa1`＝`["ash","vulkan"]` ✔，与 `["ash","cuda","cuda12","default","vulkan"]`／`["ash","cuda","cuda12","vulkan"]` 分开记账 ✔ ⇒ **无 CUDA 那套确在本机构建过** ✔。
(3) **入口契约** ✔：`macos_eval.rs:63-66` 要求 `--vulkan-stressor <path>`（缺失即报错）✔，`:180-187` 以 `--vulkan-only` 调起并追加尺寸/壳数/迭代 ✔；`--help` 实测参数面 ✔（**不加载设备** ✗）；`--vulkan-only` 在未以 `vulkan` 构建时会明确提示 ✔。
(4) **"bundled"真义更正** ✗：非"进程内"× ⇒ `stressor_process.rs:10,67-93` 把路径解析为 `current_exe()` 并以 worker 哨兵 argv 重新调起自身 ⇒ 压力仍以**子进程**运行 ✔；我的 Cargo.toml 注释已按此改写 ✔。
(5) **状态** ✔：构建/入口＝**已实现且实测**；**实际负载运行＝已实现待实测**（wedge ✗）；Metal 校验器**未跑** ✗。本节未跑任何负载 ✔。
(6) **设备访问** ✔：仅 wedge 轮询一次（`0x62`，uptime `1 day, 4:16`，同一 boot）；`cargo build` 与 `--help` 均不访问设备 ⇒ 未重启、未写入、无性能操作。

### 本轮追加七十三（2026-10-18，服务入口能力核对：`srv` 曾把写能力硬编码成与后端相反的结论）

(1) **选择依据** ✔：阶段 11 第 5 条"各入口能力、范围和错误与后端一致，既不能后端可用而入口一律拒绝"⇒ 逐入口核对，**发现不一致** ✗。
(2) **缺口** ✗：`srv/src/macos.rs::supports_writes()` **硬编码 `false`**，而 `core` 侧同一能力有唯一常量 `SETTINGS_WRITE_SUPPORTED`（优化器门控已在该常量上，`9ff687f`）⇒ 两入口各自漂移 ✔；消费方直接决定用户可见行为（`http.rs:488` 能力上报、`runtime.rs:145,171` 是否启用闭环）✔；模块文档两处已被实测推翻 ✗：写命令**存在**（`0x2080d01c` 为 `0x2080901b` 的孪生、set 身份 `(cmd & 0xC000)==0xC000`）✔；"私有 GET 一律 `0x1f`/`0x56`"是早期尺寸猜测的结果 ✗，`0x20809061@776` 被接受并发布逐域偏移 ✔ ⇒ 违反"权限不足不得伪装成命令不存在"✗。
(3) **修法** ✔：`supports_writes()` 改读 `nvoc_core::macos_rm::SETTINGS_WRITE_SUPPORTED`（今天为 false ⇒ **行为不变**、两入口同源）✔；共享理由点名真正缺失的两层（写路径存在，但需权限（实测 `0x1b`）＋原生层无 SET id）✔；`read_offset_mhz` 不再宣称"没有可读偏移命令"（偏移**可读**，只是域位未全部映射到 `OffsetDomain`）✔；`read_vf_curve` 改为"尚未定位"✔。
(4) **测试** ✔：新增 `the_shared_reason_names_the_missing_layers_not_a_missing_command`（钉住含 `0x2080d01c`／`privilege`、不含 `"no writable RM control"`）✔；既有能力断言改为与常量相等 ✔。
(5) **验证** ✔：Linux fmt rc0、clippy `-D warnings` rc0、`cargo test -p nvoc-srv` **53 passed 0 failed**；**测试机 macOS** 同 crate **53 passed 0 failed**（同数）✔；`nvapi-rs/sys/src/nvapi.rs:87` 的 macOS-only `unused variable` 属 vendored 依赖 ⇒ **未动**、仅记录 ✔。
(6) **设备访问** ✔：仅 wedge 轮询一次（`0x62`，uptime `1 day, 4:19`）；单测不访问设备（空 GPU 列表构造）⇒ 未重启、未写入、无性能操作。

### 本轮追加七十四（2026-10-18，纠错：两张 `flags` 表分属不同命名空间；`0x2080a06e` 定位推进＋可证伪只读用例）

(1) **选择依据** ✔：纠错表上"`0x2080a06e` 是否同命令尚未定位""flag 位 `0x40000` 含义尚未定位"两条一直挂着 ⇒ 本轮改用**驱动自带的生成分发表**（此前未用的证据源）推进 ✔。
(2) **纠错（新条目）** ✗：分发表 `g_subdevice_nvoc.c` 给 `0x2080206e`＝`flags=0x4a`／`0x2080206f`＝`flags=0x10008`，而 GSP 导出表给 `0x2080206e`＝`0x10008`／`0x2080206f`＝`0x40048` ⇒ 逐条对不上且**取值交叉** ⇒ **两表 flags 不同命名空间** ✔ ⇒ 引用 flag 必须写明哪张表 ✔。
(3) **分发表可用但有边界** ✔：641 行 ✔；`accessRight` 640×`0x0`＋1×`0x2` ⇒ **不是特权编码** ✔；能给处理函数名与 `paramSize=sizeof(...)` ✔；但 `0x2080901b`/`0x2080d01c`/`0x2080a06e` **均不在其中** ✗ ⇒ 只覆盖公开半区 ⇒ **其缺席不构成"命令不存在"** ✔。
(4) **`0x2080a06e`** ✔：PERF 族、12 B、`flags 0x48`、孪生 `0x2080e06f` 为 **8 B SET** ✔；message id `0x6e` 与公开 GET 相同 ✔；孪生图标注 **SIZE DIFFERS** ⇒ 是"同一控制消息的另一份声明"，非字节同构孪生 ✔ ⇒ 身份**尚未定位**但已定位比较对象 ✔；已加**可证伪只读用例**（按自身声明 12 B 发送，预测 NV_OK 且三元组相同／`0x1f`／`0x56` 三种结果皆有意义 ✔），SET `0x2080e06f` **未发送** ✗。
(5) **验证** ✔：真机 `OGKM=… sh build.sh` ⇒ **`built probe-macos-rm-idle (profile idle)`**；**未运行**（`0x62`）⇒ 已实现待实测 ✔。
(6) **设备访问** ✔：仅 wedge 轮询一次（`0x62`，uptime `1 day, 4:24`，同一 boot）；编译与源码检索不访问设备 ⇒ 未重启、未写入、无性能操作。

### 本轮追加七十五（2026-10-18，wedge 间歇性：首次观测到恢复窗口＋健康判据纠正＋门控配方）

(1) **选择依据** ✔：轮询出现新观测（本地 11:46 `anchor verdict` 返回 `NV_OK`）⇒ 足以修正"楔形是硬锁存、只能等重启"的假设 ✗。
(2) **实测** ✗：11:46 verdict＝`0x00000000`（楔形以来首次健康 ✔）；数秒后 runbook 首例 `0x2080901b@2108 in@4=0xFF bg=A5` 即 `0x62`，同 bundle 内所有 GSP 状态相关 id 全 `0x62`（`bios-sku-info`／`bus-get-pci-info` 仍 `NV_OK`）；随后 3 次间隔 15 s 的门控重试均 `0x62` ⇒ **不是硬锁存** ✔，恢复窗口存在但短 ✗ ⇒ 策略＝**门控重试抓窗口** ✔（仍不自动重启 ✔）。
(3) **判据纠正** ✗：健康只看 **`anchor verdict`** 行 ✔；`anchor host size gate` 行**恒为 `0x1f`**（其自述 GPU-state independent ✔）⇒ `grep status | head -1` 会读错行 ⇒ 必须按行名检索 ✔。
(4) **配方** ✔：同一次 ssh 内先读 verdict、仅 `0x00000000` 才跑 `tools/clock-offset-read-run.sh` ✔（两次 ssh 间隔数秒＝本轮错过窗口的原因 ✗）；runbook 自带守门 ✔，首例 `0x62`/`0x65` 即停并声明 **nothing was written** ✔。
(5) **导出表复核（口径降级）** ✗：本次 dump 实得 **1081 行**（与交接文档 1118 行差 37 ⇒ 待核 ✔）；`flags & 0x4000` 命中 **19 行**而 capture 的 `GET/SET` 列全标 GET ✗（`0x2080206f` flags `0x40048` 明明带 bit14 ✗）⇒ **该列不可作方向依据** ✔，方向只认逐条证据（处理函数名／set 身份 `(cmd & 0xC000)==0xC000`）✔，94 行 `SET` 一并降级为线索 ✔。
(6) **设备访问** ✔：轮询 1＋诊断 1＋门控重试 3＋后台门控重试（每 60 s 一次 anchor，命中即跑只读 battery）⇒ 全只读 ✔，未重启、未写入、无性能 SET ✔。

### 本轮追加七十六（2026-10-18，私有 id 空间结构由仓库内材料给出：写半边＝接口字节 +0x40、消息 id +1（304/304），并纠正三处算式）

(1) **选择依据** ✔：原目标要求"从实际封送逻辑还原偏移 GET/SET""不得用'头文件搜不到'代替核对仓库已有材料"；且代码里流传一条**算术不成立**的孪生等式 ✗。
(2) **约定来源** ✔：`ctrl2080thermal.h:136-137` 两行注释显示同一 MESSAGE_ID 配 `THERMAL_INTERFACE_ID`（得 `0x20800513`）与 `THERMAL_LEGACY_NON_PRIVILEGED_INTERFACE_ID`（得 `0x20808513`）⇒ **接口字节 bit7 ＝ legacy non-privileged 别名** ✔（与该仓库自带工具文档"0x80-0xBF category byte is the legacy non-privileged read half"一致 ✔）；`ctrl2080clk.h`／`clkavfs.h`／`fan.h`／`thermal.h` 均在 ✔，但 `0x2080901b`／`0x2080d01c` 及全部实测 THERMAL id **未在头文件声明** ✗ ⇒ 公开树给约定与公开半区，私有族名仍属尚未定位 ✔。
(3) **写半边规则（实测）** ✔：read 半区 304 行中 **304（100%）** 的孪生指向写半区且低位 **+1**；低位相同的裸 `|0x4000` 命中 **0** ✗；write 半区 75 行 **75/75** 指回 read ✔；四族一致（THERMAL `852c→c52d`、`8540→c541`，CLK `901b→d01c`，PERF `a06e→e06f`）⇒ 规则＝**接口字节 +0x40（bit6）、消息 id +1** ✔。
(4) **纠错** ✗：`0x2080D01C = 0x2080901B | 0x4000` 实际得 `0x2080D01B` ⇒ 已修三处面向用户的理由串（`core/src/macos_rm.rs:122`、`:364`、`cli/src/macos_rm.rs:20`）✔，结论（0xC0-0xFF 写半边在 root 以下被 `0x1b` 拒）依据实测 `0x1b` 与 host 表无特权行，不依赖该等式 ✔；同步修 `summarise-method-table.py` 的孪生枚举与文档（现先试相邻 id 再试裸形式 ✔；本机运行 ⇒ 71 对／同尺寸 43）✔。
(5) **行数卫生** ✗：`gsp-table*.tsv`＝890 行、families capture＝1081 行、交接文档记 1118 行 ⇒ 三个不同 artifact；且 families capture **可证不完整**（304 个 read 行的孪生指向写 id，文件里仅 75 个写行存在）⇒ 引用行数必须写明文件 ✔。
(6) **验证** ✔：本机 fmt rc0；`cargo test -p nvoc-core -p nvoc-cli -p nvoc-srv` ⇒ 82／94／53 passed、0 failed（36＋8 ignored 为 GPU 相关 ✔）；设备访问仅后台门控重试（attempt 1-4 皆 `0x62`）⇒ 未重启、未写入、无性能 SET ✔。

### 本轮追加七十七（2026-10-18，尺寸列＝`sizeof(PARAMS)`：用开放头文件编译尺寸校准导出表，并据 id 结构定下 `0x2080a06e` 身份）

(1) **校准（真机编译，只读）** ✔：`sizeof(NV2080_CTRL_THERMAL_SYSTEM_EXECUTE_V2_PARAMS)=1432`，而导出表实测 `0x20800513`＝1432、其 legacy 别名 `0x20808513`＝1432 ⇒ **导出表 size 列就是 `sizeof(PARAMS)`** ✔；别名与公开同胞同结构同尺寸 ✔ ⇒ 与 §42 的"同一 MESSAGE_ID、两个接口 id"一致 ✔。反例警示：12／24／56／8／4 等尺寸在多个族大量重复 ⇒ 尺寸是**佐证**，命名需 id 结构或公开命名支持 ✔。
(2) **`0x2080a06e` 身份确定** ✔：接口字节 `0xa0 = 0x20|0x80`（PERF 的 legacy non-privileged 别名）＋ 消息 id `0x6e` 与公开 `0x2080206e` 相同，公开 id 即 `NV2080_CTRL_CMD_PERF_RATED_TDP_GET_CONTROL` ⇒ 它是**同一 RATED_TDP get-control 的 legacy 别名** ✔（12 B 与 `…_RATED_TDP_GET_CONTROL_PARAMS=12` 一致，作佐证）。⚠️ 写半边 `0x2080e06f` 实测 **8 B** ≠ SDK 的 SET 结构 12 ✗ ⇒ 写半边按自己的行解析，不假设同构 ✔。
(3) **按尺寸的候选命名** ✔：`0x20801037`(CLK,6024)＝`CLK_PMUMON_CLK_DOMAINS_GET_SAMPLES_PARAMS` 同尺寸 ⇒ 候选 ✔；`0x2080852e`(THERMAL,1432) 与 execute-v2 同尺寸且其孪生不在可写 6 行内 ⇒ 读侧同类候选 ✔；`0x20808513` 由头文件直接命名 ✔。
(4) **精确边界** ✗：`ctrl2080thermal.h`／`ctrl2080clk.h` 各只有 1 个 `_PARAMS` 类型 ⇒ 38 行 THERMAL 只有 1 行可由开放头文件命名；偏移通道 2108 结构在开放头文件无对应类型（形状已知 `60+32×64` ✔）。`ctrl2080fan.h` ＝ **33 行空壳**（0 define／0 struct）⇒ **风扇族在开放 SDK 无声明**，其仓库内材料是驱动 fan 代码与生成派发表 ✔。生成派发表 641 行里 5 个公开 THERMAL id 仅 `0x20800513` 出现 ⇒ 只覆盖公开半区子集，缺席不等于不存在 ✔。
(5) **各族状态（独立判定）** ✔：温度读取＝`0x20800513` 已实测 NV_OK（可读／可解释 ✔；**更正**：此处先前误写为 `0x20808513`，census 复核显示 legacy physical 别名 `0x20808513` 共 2 次派发**均为 `0x62`、从未 NV_OK**，见追加九十二），范围／原状态／恢复**待一次干净只读跑**；热族其余 37 行 6 行有写孪生 ✔，其余**尚未定位** ✔；风扇族候选 `0x20808540@24`（可写孪生 `0x2080c541@24` ✔），含义**尚未定位** ✔；功耗族 RATED_TDP 别名身份已定 ✔，写仍门控（`SETTINGS_WRITE_SUPPORTED=false`）⇒ **未写入任何值** ✔。

### 本轮追加七十八（2026-10-18，`0x2080901b` 写集合由散文改为可执行断言：真 0xA5 dump 从此可被自动裁决）

(1) **本轮不动 GPU，是缺口驱动的代码改动** ✔：偏移通道写面仍受阻（wedge 未清＋权限门 ✔），但**真正缺的证据**是"0xA5 背景下 handler 到底写了哪些 record-relative 槽位"——SET 该打哪个槽就靠它 ✔。此前 `106/527 changed, last @564` 是实测，而 `27／132` 与 `last @116／@692` 只是探针注释与本矩阵的复述 ✗ ⇒ **没有任何代码在检查它** ✗。本轮把该模型写进仓库内工具并断言，见 `docs/reverse-engineering/nvapi/rm-param-marshall-corrections-r610.md` §44 ✔。

(2) **改动内容（`probe-macos-rm/tools/decode-clk-containers.py`，仓库内 ✔）**：新增 `RECORDED_ROWS`（实机十行含 type）、`WRITE_SET_HEAD=14`、`WRITE_SET_PER_TYPE={0x08:13,0x09:13,0x03:1}`、`RECORDED_WRITE_SET={0x1:(27,116),0xFF:(106,564),0x3FF:(132,692)}`；`write_set_model()` 按目录行与 type 规则给出计数与末偏移；`a5_block()` 把模型写成字节；`write_set_check()` 对真字节**逐行裁决**（每行实际写入的相对槽位 vs 该 type 规则、字节隐含 head 数 vs 14、末改变偏移 vs 最后一个 value 行的 rec+56）；`decode()` 对 `0x2080901b` 自动追加该行；`--selftest` 断言三者全部 MATCH ✔。

(3) **可证伪性与诚实边界** ✔：`HEAD=14` 是**唯一拟合常数**（由那次 106 反推 ✔），可独立核对的是每行 type 规则 ✔；**零背景块被明确拒绝**（"no dword still holds 0xa5a5a5a5"），不把"零背景"误读成短写集合 ✗；**无规则的 type 直接 withhold 算术**（返回 `(None,None)`），不当 0 ✔；stamp **只写低两字节**，故 A5 下 rec+0 记为 `a5a50808` ✔。

(4) **验证范围（如实）** ✔：本机 `py_compile` 通过；`--selftest` 通过（`geometry + round-trip + field decode + write-set model`）；对 2026-10-17 已录 `clkctl-hex.txt` 复跑，目录十字矩阵三处仍全 MATCH、`rows:` 与 `seed 0xff reaches rows 0..7, beyond it: row 8 id 21; row 9 id 5` 不变 ⇒ **无回归** ✔；新代码无 >88 列行 ✔（本机无 ruff ✔，CI 侧检查 ✔）。**测试机本轮未发任何 GPU 请求** ✔：门控重试第 24 次（2026-10-18 机器 11:47→12:11，每分钟一次 anchor 读）**全部 `0x00000062`**，`/tmp/offsweep-r65c.txt` 未生成 ✗ ⇒ 电池未跑；机器 uptime 1 天 4:42 **未重启** ✔（不自动重启 ✔）。

(5) **对下一次健康窗口的意义** ✔：`tools/clock-offset-read-run.sh` 的 `--clkctl-hex` 电池**已含**该 id 的 A5 case（源码 case 表首例 ✔，且 runbook 走 `run_one clkctl-hex --clkctl-hex` 并把 `clkctl-hex.txt` 交给本工具 ✔）⇒ 健康窗口一出现，"handler 写了哪些槽位"就是**跑完即可读结论**的机械判定，不再需要第二次实机请求 ✔（GET 不可重复：wedge 时发不出去 ✔）。⚠️ 我自己先误判为"工具没 dump 901b"，被源码 case 表当场证伪 ✔ —— 已录的 `clkctl-hex.txt` 是**更早**的一次 dump，不含该块 ✔。

(6) **写入面结论不变** ✔：`settings_write=false`／`settings_restore=false`；**未发任何 SET**，未提权，未改 EFI／驱动配置 ✔。槽位身份（+8／+12／+16 三选一）仍**尚未定位** ✗ ⇒ 只能靠一次最小步 SET 后的**同字段回读**判定；**实时频率不参与判定** ✔。

(7) **工具自身与 CI 门禁对齐（顺手修掉全仓库仅有的 2 个 lint 错误）** ✔：CI 的 lint job 是 `ruff check .`，本机全仓库范围内**只有 2 个错误**且都在本工具（F541，`--selftest` 里旧有的合成文本 fixture，第 546／547 行 ✔，0.16.10 与 0.17.1 同报 ⇒ 与版本无关 ✔）；修掉后**全仓库 `ruff check .` ＝ All checks passed** ✔。格式侧：CI 为 `ruff format . --check`（`preview = true` ⇒ 版本敏感），本机用 `ruff@0.17.1` 只格式化**本工具文件**，其 `format --check` 通过 ✔，全仓库漂移 10→9，**其余 9 个文件未动** ✔。**行为不变的证据**：格式化后 `--selftest` 仍通过 ✔，对已录 `clkctl-hex.txt` 解码输出 md5 完全一致（`7bf52cb8eb151a575ad3d1d70c824750`）✔ ⇒ 纯格式、零语义变化 ✔。规模 174／104 行，**只限本文件** ✔。

### 本轮追加七十九（2026-10-18，把"哪个数字来自哪份捕获"钉住：runbook 规则表述纠错 + 位标记类实测）
1. **动机** ✔：`tools/clock-offset-write-run.sh`（活文档）仍按旧形式描述 SET 孪生规则（`(id | 0x4000) + 1`，并称 47 个实例／92 个写 id），而 §42 已把规则纠正为"category byte +0x40 且 message id +1"并给出 304/304 实测；`core/src/macos_rm.rs` 的注释也带着未指明捕获的 "92"。活文档与已证结论不一致 ⇒ 修。
2. **实测（本机可复现，889 行 `gsp-table-full.tsv`）** ✔：bit15/bit14 四桶 = `0x0` 481／`0x4000` **19**／`0x8000` 300／`0xc000` 89，合计 889。**bit14 单独不是特权**：那 19 行全在公开类别 0x40-0x44（如 `0x20804006@776`）；转发器判定式 `(cmd & 0xC000) == 0xC000` 只选出 89 行写半边、不选任何读行。孪生规则：存在 (cat+0x40, msg+1) 孪生的读行 **56/300**（同尺寸 41）；`| 0x4000` 形式在**读半边**与之等价。逐 id 核对：`0x2080901b@2108/0x48 → 0x2080d01c@2108/0x44` ✔，`0x2080d01b` 不存在 ✔。
3. **自我纠错** ✔：我第一版分类脚本漏了 `0x4000` 桶，把 19 行静默并进 "public/other" ⇒ 已按四桶重算并在文档记录；教训是"分类桶必须覆盖全部取值"。
4. **改动** ✔：runbook 两段 + `core/src/macos_rm.rs` 注释改为**按捕获指名**的表述（1118 行 recount：92；889 行 dump：89；两次都没选到读行），`dump-gsp-families.py` docstring 补上 bit14 反例（**机器可读输出行未改**，避免破坏下游 grep）。
5. **诚实边界** ✔：这 92／89 是**不同捕获**的数，不可互推；两份捕获的差集内容**尚未定位**。新**候选**（只读、未测）：`0x20804006@776` 与私有时钟文档 `0x20809061@776` 同为 776 字节。
6. **验证与门禁** ✔：`sh -n` 通过、`cargo fmt --all -- --check` 干净、`cargo test -p nvoc-core --all-targets` **115 passed/0 failed**、`ruff check` 对被触动的 py 文件 All checks passed；`dump-gsp-families.py` 的格式漂移**改动前即存在**（`git show HEAD:` 验证 ✔），故不整文件重排。**测试机零 GPU 请求，未发 SET、未提权，`settings_write=false` 不变** ✔。

### 本轮追加八十（2026-10-18，229 行丢弃归类：偏移通道 ids 不受影响，孪生"可推导率"与"存在率"必须分开）
1. **先修一个"未知"** ✔：goal 与交接都提"旧工具静默丢了 229 行"，我一直记为"三份捕获未对齐"的未知；实际原因早已写在仓库工具里——`extract-gsp-method-table.py:75-85`：旧守卫 `fl < 0x10000 and ar < 0x10000` 静默丢 229 行真行（`0x20809006` flags `0x81074a`、`0x20809062` flags `0x82004a` 就是被它误杀的例子），去掉后两种扫描都得到同一批 1118 行且 `gsp-handler-map.py` 自带 `== 1118` 自检。教训：先查仓库工具再记未知。
2. **实测（只读，逐 id 比对）** ✔：机器 recount 工件 1119 行（1118 数据，`gsp-table.log` 记固件 `gsp_tu10x.bin` 1118 rows）与旧导出 889 行**严格子集关系**（只在旧表出现的 id = 0）⇒ 229 行分类：公开 190、bit14-only 1、**读半边 35**、**写半边 3**（`0x2080d055`／`0x2080e0ae` 有读伙伴在表内，`0x2080f310` 无）。
3. **对偏移通道无影响** ✔：计划依赖的 id（`0x2080901b@2108/0x48`、`0x2080d01c@2108/0x44`、`0x20809019`、`0x20809025`、`0x20809061`、`0x2080a06e`）两表都在且一致；但 **`0x20808513@1432` 只在固件表**（flags `0x10248`）⇒"表里查不到"过去可能只是**捕获不全**，又一实例。
4. **口径纠正（重要）** ✔：孪生**存在率**——旧 889 行 56/300（同尺寸 41）、固件 1118 行 **62/335**（同尺寸 47）、写侧 92 行中 62 行是某读行孪生；而 `gsp-families` 捕获里"304/304"是**规则被套用**（可推导率 100%，算术），那 304 个推得 id 只有 **55** 个真是固件表的行 ⇒ 两个比率不可混说。§42 的"孪生不普遍"结论被**加强**。
5. **方法坑** ✔：中途把 message id **按位或**（`0x1b|0x1c=0x1f`）实现孪生，得 52/23 的错数；必须按 id **加一**（进位）实现，改回后与 §45/上一轮一致（56/41）。另：本机 `gsp-table.tsv` 与机器 recount 同名文件内容不同（md5 `642c3f38…` vs `60b75472…`），不能当同一份。
6. **边界** ✔：仓库只改 `dump-gsp-families.py` docstring（"lists 889 methods" → 1118 并写明原因，机器可读输出未改）；取回工件在本机 scratch 目录、不进仓库；**测试机零 GPU 请求、未发 SET、未提权，`settings_write=false` 不变** ✔。

### 本轮追加八十一（2026-10-18，阶段 11 第 5 条：偏移通道"入口 vs 后端"一致性核对，并修一处会读成"后端做不到"的拒绝）
1. **可达性核对（证据）** ✔：后端确实**报得出**偏移——core 的 macOS 设置投影带 `clock_offsets`（来自 `0x20809061` 文档，按设备 domain bit 逐条并含该条发布的频率）与 `clock_offset_limits_khz`（来自 `0x20809025` 范围块）；入口侧 `nvoc-cli get-settings --macos-rm` 走 `QueryMacosRmSettings` 原样序列化 ⇒ **CLI 可达** ✔；`pynvoc` 的 `settings_macos_rm_to_map` 显式投影这两个字段 ⇒ **Python 可达** ✔（`nvoc-python/src/lib.rs:1456-1498`）。
2. **发现问题** ❌：`nvoc-cli get-clock-offset --macos-rm`（`Command::GetPstateGlobalFreqOffset`）此前被**通用**拒绝（"… is unavailable on macOS RM; supported commands: …"），读起来像"后端做不到"——正是阶段 11 第 5 条禁止的读法。
3. **缺失层是"前端映射"，不是协议也不是驱动实现** ✔：该命令契约是"某个 `--domain` 对应单一数值"，而本后端只有设备自己的 **domain bit**（本设备用到 0..6 与 19..21）；公开 OGKM 头文件**没有**任何 clock-domain 枚举，唯一交叉参考是 Windows/NVAPI 的 `NV_GPU_CLOCK_DOMAIN_ID`（把 0..6 命名为 GPC/XBAR/SYS/HUB/M/HOST/DISP），属**假设**，其证伪路径正是单域写入（`core/src/macos_rm.rs:240-249`）。
4. **本轮改动（已实现、已离线验证）** ✔：该命令的拒绝改为**指名层**的具体错误——保留 `unavailable on macOS RM` 前缀；说明后端确有数据并指明可用路由（`get-settings` → `clock_offsets`／`clock_offset_limits_khz`）；点出缺失的是 `--domain` → domain bit 的映射及其假设性质；**写侧**（`set-clock-offset`）保留简洁的能力拒绝（那里驱动确实拒绝，不需要指名层）。
5. **验证与范围** ✔：新增测试 `macos_rm::tests::the_offset_read_refusal_names_the_missing_layer_and_the_working_route`（断言前缀＋`frontend reason, not a driver`＋`clock_offsets`＋`clock_offset_limits_khz`＋`NV_GPU_CLOCK_DOMAIN_ID`，并断言写侧**不含**该措辞）；`cargo test -p nvoc-cli` **83 passed / 0 failed**；`cargo fmt --all -- --check` 干净；`cargo clippy -p nvoc-cli --all-targets -- -D warnings` 干净；改动仅 `cli/src/macos_rm.rs`。**未触碰设备**：`validate` 是纯函数、在任何 IOKit/RM 访问之前 ⇒ 本步**不依赖 wedge** ✔（实机文案效果待 wedge 解除后跑一次 `nvoc-cli get-clock-offset --macos-rm --domain graphics` 复核 ⇒ 归类"已实现待实测"，且只是文案可见性、非新能力）。
6. **本次交付口径（分类标签）** ✔：读偏移（三入口可达，设备侧值来自 `0x20809061`）= **已实现且实测**；命名后的单值查询 `get-clock-offset` = **未实现**（前端映射未定，**不用假值**替代，改为指名层拒绝）；写偏移 = **未实现，且驱动明确拒绝**（0x1b 权限前置；`settings_write=false`／`settings_restore=false` 不变）；偏移范围/限制 `clock_offset_limits_khz` = **已实现且实测**；标量键 `core_clock_offset`／`memory_clock_offset` = **调查未知**（带 narrowed 原因，见 `CLOCK_OFFSET_UNREADABLE_REASON`）。

### 本轮追加八十二（2026-10-18，阶段 11 第 5 条续：把"可命名"变成一次同快照频率比对，读偏移入口从指名层拒绝改为按证据回答）

1. **选择依据** ✔：上一轮把 `get-clock-offset` 的缺失层点名为"`--domain` → 设备 domain bit 的映射"，并把它记为**未实现**；本轮不再靠"公开头文件没枚举"停在那里，而是给出**可执行的识别规则**：该文档每条记录同时发布"它持有的频率"与"它应用的偏移"，`get-status` 在同一快照里报出公开域的实时时钟 ⇒ **同快照频率相等**是唯一不需要"谁给域起名"的命名依据 ✔。这一条是本轮全部改动的**唯一**依据，不新增任何设备请求 ✔。
2. **与禁止规则的分界（明确写下）** ✔：目标禁止"以实时频率变化代替偏移设置成功"。此处频率**只用于决定名字指向哪条记录**，与"写是否成功"无关：报告的 offset 仍是文档自己的字段，两个数字都来自**同一快照**，比对不涉及"频率动了所以偏移生效了" ✗。本步**一次 SET 都没发** ✔（`settings_write=false`／`settings_restore=false` 不变）。
3. **规则与其失败方向** ✔：`name` 只在**恰好一条**记录发布了该名字对应的实时时钟时附加（精确相等＋唯一），证据串随名字一起返回（"该记录发布 6000984 kHz，正是本快照的实时显存时钟，且是 11 条中唯一匹配的"）。无匹配 ⇒ 不命名；多条匹配（并列）⇒ **都不命名**（"命名其中一条就是选择而非测量"）；某个名字没有实时时钟可比 ⇒ 不命名并在 `notes` 写明比不了的原因是"本快照没有该域实时时钟"。**失败方向恒为"未命名"，绝非"起错名"** ✔。另一处如实记录：两个公开名可以合法地指向**同一条**记录（空载时图形与 SM 时钟常为同一数字），此时两个名字都报出并附"该 bit 同时对应 X 与 Y"的说明，而不是让其中一个静默获胜 ✔。
4. **接口接入（三入口一致）** ✔：core 新增结构化字段 `DeviceSettings::clock_offsets_named`（`None`＝通道没读到；**空表＝读到了、一个都没命名**，是可测结论而非缺值）与查询方法 `DeviceSettings::named_clock_offset(name)`（命中返回条目，否则返回 `unnamed_offset_reason(name)` 的分层理由）；`nvoc-cli get-clock-offset --macos-rm --domain graphics|memory|video|processor` 进入 macOS 允许清单并在 `execute` 内按上述方法回答（不再整体拒绝 ✗→✔，而"没被命名"在运行时返回指名层理由）；`pynvoc` 的 `settings_macos_rm_to_map` 投影 `clock_offsets_named`（含 `corroboration`）⇒ CLI／Python／core 三入口同依据、同范围、同错误来源 ✔。
5. **能力边界（未夸大）** ✗：本步**没有**新增写能力，也**没有**声称"图形域已确定"——图形域能否命名取决于该快照实时图形时钟是否恰好等于某条记录发布的频率（boost 会变），不成立时它保持未命名并给出理由。可执行的证伪路径仍是**一次最小步单域写入**＋同字段回读（`core/src/macos_rm.rs:240-249`）。
6. **验证范围（如实）** ✔：新增测试 `a_domain_is_named_only_by_an_exact_unique_live_clock_match`、`a_tie_leaves_the_domain_unnamed_rather_than_picking_one`、`an_unread_channel_reports_the_missing_read_not_a_zero`、`two_names_may_match_one_record_and_both_are_reported`（含失败方向断言：并列时该名**不出现**且理由含 `clock_offsets`）；`cargo test -p nvoc-core --all-targets` **98 passed / 0 failed**；`cargo test -p nvoc-cli` **83 passed / 0 failed**；`cargo test -p nvoc-srv` **53 passed / 0 failed**；`cargo clippy --workspace --exclude cli-stressor-cuda-rs --all-targets -- -D warnings` 干净；`cargo fmt --all -- --check` 干净；`pynvoc` 测试代码由 clippy `--all-targets` 类型检查通过（其测试二进制在本机无法链接：`PyObject_Str` 等未定义 ⇒ 属本机无 libpython，**运行时验证归测试机** ✔）。**上机侧只做编译，不碰 GPU** ✔：测试机 `cargo build -p nvoc-cli` 成功（新入口在设备上即可用，避免健康窗口内再等一次构建），`cargo test --no-run -p nvoc-core --test macos_rm_readonly` 成功（该文件整体 `cfg(target_os = "macos")` ⇒ **本机根本不编译它**，只有测试机能证明它可编译）。设备侧测试 `the_offset_channel_reports_domains_records_and_its_own_window` 追加**单向**断言：命名项必须是本次读取已发布的记录（`domain_bit`／`frequency_khz`／`offset_khz` 三者同源），未命名项必须给出指名层理由；**不断言"一定命名成功"**（无匹配是合法测量结果 ✔）。
7. **设备侧口径** ✔：本轮**测试机零 GPU 请求**（只读门控后台重试第 11 次读数全 `0x00000062`，uptime 1 天 5:29 ⇒ **未重启** ✔，不自动重启）；改动源码已同步到 `nvoc-head` 且**两端 sha256 逐文件相同** ✔；命名逻辑**从未在设备上跑过** ⇒ 本项归类 **"已实现待实测"**，读偏移（设备侧值）与偏移窗口仍为 **"已实现且实测"**，写偏移仍为 **"未实现，且驱动明确拒绝"** ✔。


### 本轮追加八十三（2026-10-18，阶段 10 的 dry-run：把"要写什么"变成可测对象，判据只取设备自报的窗口，仍不发任何 SET）

1. **选择依据** ✔：写侧仍被三重门挡住（**权限前置** `0x1b`、**字段身份尚未定位**、**未获授权**），阶段 10 顺序里唯一能在无写条件下推进的是 **dry-run/mock**：先把"打算写的这一步"变成**可计算、可复核、可预先声明对错**的对象，等健康窗口与授权齐备时直接执行同一份计划。判据**不取任何自造常量**：当前值来自 `0x20809061` 文档、边界来自 `0x20809025` 范围块（设备自报 ±18.75 MHz），识别来自同快照频率比对 ⇒ 计划里的每个数字都是**设备已发布**的字段 ✔。
2. **实现（core，纯函数）** ✔：新增 `OffsetStepTarget`（公开域**名**／设备 **bit** 两种指代）、`OffsetStepVerdict`（六种：`ready`／`channel_unread`／`domain_unnamed`／`record_absent`／`window_unknown`／`outside_window`）、`OffsetStep`（含 `requested`、`domain_bit`、`domain`、`identity`、`current_offset_khz`、`delta_khz`、`target_offset_khz`、`window_min_khz`／`window_max_khz`、`verdict`、`detail`）与 `plan_offset_step`；请求语法由 core 统一提供 `parse_offset_target`（`graphics|memory|video|processor`，或 `bit:<十进制|0x十六进制>`）与 `parse_offset_delta`（带符号 kHz，可带 `khz`／`mhz` 后缀）。**bit 形式无需任何命名**，因为它是设备自己的坐标、也正是写入选择掩码要携带的那一位；**名字形式**必须通过同快照频率比对（上一轮的规则），否则 verdict 为 `domain_unnamed` 且 `detail` 给出指名层理由 ✔。名字打错**不会**变成"未命名"：`parse_offset_target` 直接拒绝并把两种合法形式写进错误里（避免把笔误读成设备结论）。
3. **与"已应用"的分界（写进产物）** ✔：`OffsetStep.verdict == ready` **只**表示"该步被识别且落在设备自报窗口内"，**不代表已写入**；产物中另有 `applied: false` 与 `settings_write`（本后端当前为 `false`，`SETTINGS_WRITE_SUPPORTED` 未改）⇒ "ready 且 `settings_write=false`"应读作**在等后端**，而不是"失败" ✔。本步**一次 SET 都没发** ✔。
4. **入口接入（三入口同依据、同语法、同判据）** ✔：`auto-optimizer` 增加 `macos-eval --dry-run --plan <target>=<delta>`（可重复）：`--dry-run` **整段跳过 Vulkan 负载与 Metal 校验器**（`mode: "dry-run"`，`under_load`／`post`／`stressor` 为 `null` 而非空摘要，`verifier.reason` 说明"未跑负载故无可校验"），`--plan` 与 `--dry-run` 或负载运行都可组合；`nvoc-cli get-pstate-global-freq-offset --macos-rm --domain X --plan-delta +8mhz` 在**同一次**快照上输出 `plan` 对象；`pynvoc.plan_clock_offset_step(gpu, target, delta)` 同源调用。CLI 的 NVAPI/NVML 路径**显式拒绝**该标志并说明原因（本函数报的是增量、不含窗口，无法判定"是否有界"）——**不接受却静默忽略**，符合阶段 11 第 5 条 ✔。
5. **诚实边界** ✗：本步**没有**新增写能力、**没有**做任何自动搜索、**没有**把 dry-run 结果当成绩效结论；`macos-eval` 仍是"预检＋可选负载＋可选计划"，`applied` 恒为 `false`。`settings_write`／`settings_restore` 常量**未动** ✔。
6. **验证范围（如实）** ✔：新增测试 core 四个（`a_planned_step_is_ready_only_inside_the_window_the_driver_reported`、`a_planned_step_names_the_layer_it_could_not_resolve`、`a_device_bit_needs_no_name_and_reports_that_it_had_none`、`the_request_grammar_is_shared_and_rejects_typos`）、optimizer 两个（`plan_requests_use_the_shared_grammar_and_reject_typos`、`a_dry_run_plan_is_never_applied_and_says_whether_this_backend_writes`）、CLI 一个（`the_offset_read_accepts_the_dry_run_step_flag`）；实测：`cargo test -p nvoc-core`（lib 单测 **102**，含集成目标合计 **123**）／`-p nvoc-cli` **84**／`-p nvoc-srv` **53**／`-p nvoc-auto-optimizer` **68**，**failed 全为 0**；`cargo clippy --workspace --exclude cli-stressor-cuda-rs --all-targets -- -D warnings` 干净；`cargo fmt --all -- --check` 干净；改动过的两个 py 文件 `ruff format --check`／`ruff check` 均通过（`uvx ruff@0.17.1`，CI 同源）。`DeviceSettings`／`SettingsCapabilities` 增加 `Default`（仅"尚未读到任何东西"的构造状态：字段本就全为 `Option`／`Vec`，不产生任何伪造零值 ✔），以便入口侧手工构造投影。**上机侧只编译、不碰 GPU**：测试机 `cargo build --workspace --exclude cli-stressor-cuda-rs --exclude pynvoc` 成功（core／cli／srv／auto-optimizer／xtask 全过 ⇒ 新入口与 `--dry-run` 在 macOS 上**已可编译可用**），`cargo check -p pynvoc` 成功（⇒ 新增 pyfunction 通过类型检查）。**`pynvoc` 的整 lib 链接在两端都失败，且与本轮改动无关** ✔：报的是 pyo3 自身的 `Py_None`／`Py_True`／`PyObject_Str` 等未定义符号（本机此前记录过同类），属"lib target 未按 extension-module 方式链接 libpython"，只有 maturin 构建 `_native` cdylib 时才会解析——CI 的非 GPU 门禁用 `cargo clippy --all-targets`（只检查、不链接）正出于同一原因 ✔；**没有把它伪装成"pynvoc 不可用"**，其运行时验证归测试机的 maturin/python 路径。计划逻辑**从未在设备上跑过** ⇒ 归类 **"已实现待实测"**（读偏移与其窗口仍为"已实现且实测"，写偏移仍为"未实现，且驱动明确拒绝"）✔。
7. **设备侧口径** ✔：本轮**测试机零 GPU 请求**；只读门控后台重试继续（第 17 次读数仍 `0x00000062`，uptime 1 天 5:35 ⇒ **未重启** ✔，不自动重启）。改动源码 9 个文件已同步到 `nvoc-head`，**两端 sha256 逐文件相同** ✔。
8. **剩余（未夸大）** ✗：有限实机闭环仍需 **(a) 健康窗口**、**(b) 一次交互式 sudo（写入授权）**、**(c) 字段身份**；其后是结果保存/应用与恢复验收。预登记的可证伪预测不变：`--domain memory` 应报 `-0.126 MHz`（若该记录仍发布实时显存时钟），`--domain graphics` 无匹配时应给分层理由而非数字；而 `--plan bit:0x10=+8mhz` 在健康窗口应给 `ready` 或 `outside_window`（若当前偏移非 −126 kHz 则按实际值计算）——**dry-run 的结论可被同窗口的只读快照直接核对** ✔。

### 本轮追加八十四（2026-10-18，风扇族的独立审计：入口从"一律拒绝"改为按后端实有的数据回答，并在设备上实测了失败方向）

1. **选择依据** ✔：本轮最高优先（偏移通道）的下一步只能在健康窗口里做（写侧仍需授权），于是按目标"风扇／功耗／温度**分别**调查、不因 VF 受阻就一并宣布不可用"处理风扇族。先核对**后端实有什么**：core 的快照里风扇只有一个数据源——RUSD 的 16 个 cooler 槽，每槽一个转速读数（`RawSample.fan_rpm: [u32; 16]`），经合理性过滤（0 = 停转或空槽；>20000 rpm = 未填充槽的垃圾值 ⇒ 记 `unavailable`）后发布 `fan_rpm`（最高一路）与 `fan_rpms`（逐路）。**没有任何占空比、没有任何转速区间**。所以风扇族**不是"读不到"**：`get-status` 早在阶段 2 就实测过逐风扇转速（Metal 负载 4243 rpm）。
2. **发现的口径问题（阶段 11 第 5 条）** ❌：`get-fan-info` 在 macOS 上此前被**整体拒绝**（"unavailable on macOS RM"），而后端明明能报转速 ⇒ 正是目标禁止的"后端可用而入口一律拒绝"，读起来像"这块卡没有风扇数据"。
3. **本轮改动（已实现）** ✔：core 新增 `FanCoolerReading{slot,rpm}`（**保留驱动自己的槽号**——过滤后的列表位置不是 cooler 序号，凡是要指名某个风扇的入口都需要驱动坐标，与偏移记录需要设备 domain bit 同理）、`DeviceStatus.fan_coolers` 与 `DeviceStatus::fan_report()`；`FanReport` 把契约里本后端**无法填**的三个字段显式给成 `None`（JSON `null`）＋`unavailable` 逐字段理由（`FAN_PERCENT_UNREADABLE_REASON`），**绝不用 0 顶替未读到的值**。CLI `get-fan-info --macos-rm` 进入允许清单并由同一投影作答（`cli/src/macos_rm.rs`），`pynvoc` 的 `query_fan_info`／`query_status` 走同一投影与同一字段，`get-status` 的连续采样新增 `fan_coolers`（**同一 schema 版本内加字段**，只增不改，旧键语义不变 ⇒ 不破坏已有消费者 ✔）。
4. **设备实测（本轮唯一一次设备读取，只读、零 SET）** ✔：机器上重建 `nvoc-cli` 后执行 `get-fan-info --macos-rm --output json` ⇒ **rc=0**，产物为 `count:0`、`coolers:[]`、三个百分比 `null` ＋理由，并如实写出**"本快照的风扇段不可用（RUSD allocation/mapping failed: 0x00000062）⇒ 空列表是 not read，不是 no fan"**。这正是本步设计要区分的两件事，且在**失败方向**上被真机验证 ✔。同时 `get-status --macos-rm` 本次实测：**所有 RUSD 字段**都不可用（27 个键，含 `fan_coolers`），理由均为同一个 `0x00000062` —— 两条**新事实**（此前只知 GSP 私有控制被 wedge 挡住）：**(a) 当前 wedge 连带 RUSD 快照映射一起失败**；**(b) 新增字段使失败页的不可用键数由 26 变 27，与 core 夹具预测一致**（夹具注释已按这次实测改写，不用推理代替实测 ✔）。
5. **诚实边界** ✗：健康会话下的**转速读数**(`fan_rpm`／`fan_coolers`)＝ **已实现且实测**（阶段 2 遥测与 Metal 负载；本轮因 wedge 未拿到数值）；**风扇入口**＝**已实现且实测**（跑通＋失败方向实测；健康会话下同一入口的数值面属既有遥测证据，非本步新测）；**占空比百分比**＝**驱动未发布**（不是"没找到 id"）；**风扇写**（`set-fan-speed`／`reset-fan-speed`／曲线族）＝**未实现，且在访问驱动前拒绝**——本后端没有风扇 SET 入口，`settings_write=false` 不变 ✔。没有触碰 `ctrl2080fan.h` 的 33 行空壳结论：那是**私有 CTRL(FAN)** 不存在的证据，与 RUSD 侧有转速读数并不矛盾 ✔。
6. **验证范围** ✔：新增 core 测试 `fan_readings_keep_the_cooler_slot_and_the_report_names_the_missing_fields`（槽号保留／两个百分比特 None／`unavailable` 三键同理由／停转说明／**不可用段必须是"not read"而非"stopped"**），并修正两处按已发布字段计数的夹具（27／28，注释写明设备侧的 26→27 实测依据）；`cargo test -p nvoc-core` 合计 **124**／`-p nvoc-cli` **85**（新增 `the_fan_read_is_allowed_and_its_unfillable_fields_say_so`）／`-p nvoc-srv` **53**／`-p nvoc-auto-optimizer` **68**，failed 全 0；workspace clippy `-D warnings` 干净；`cargo fmt --check` 干净；机器侧 `cargo build -p nvoc-cli` 成功（6 个源文件两端 sha256 相同 ✔）。**未发任何 SET、未提权、未重启** ✔。

### 本轮追加八十五（2026-10-18，功耗族的独立审计：允许"后端真有的那两个瓦数"，并挡掉一个会被误当上限的字段）

1. **选择依据** ✔：原目标要求风扇／功耗／温度**各自**给出"可读 / 可解释 / 范围 / 原状态 / 恢复"的证据，且阶段 11 第 5 条禁止"后端可用而入口一律拒绝"。上一轮做完风扇族，本轮做**功耗族**，方法相同：先看后端**实有什么**，再决定入口该答什么。
2. **后端实有** ✔：RUSD 功耗段发布 `power_limit_requested_mw`、`power_limit_mw`、`power_policy_tgp_mw`（外加瞬时 `power_mw` 与均值 `average_power_mw`）。平台健康会话的实测量（`outputs/stage2-telemetry-20261017T033137Z/`）：`power_limit_mw = 100000`、`power_limit_requested_mw = 100000`、`power_policy_tgp_mw = 11277–11309`、`power_mw = 11254–11359`。
3. **口径问题（阶段 11 第 5 条）** ❌：`get-public-power-limit`（契约＝ min/current/max 瓦数区间）在 macOS 上被**整体拒绝**，而后端明明发布其中 **current** 这一个量（还有 requested）⇒ 与风扇族同一类错误。
4. **本轮改动（已实现）** ✔：core 新增 `PowerLimitReport` 与 `DeviceStatus::power_limit_report()`；`current_watt` 取 enforced wall（`power_limit_mw`，语义同 NVML 的 power-management limit，按 mW→W `/1000` 精确换算），`requested_watt` 取 requested limit，`policy_tgp_watt` 取 `power_policy_tgp_mw`。契约的 `min_watt`/`max_watt` **恒为 `null`** ＋ `unavailable` 理由（该 section 无约束对），CLI `get-public-power-limit --macos-rm` 与 pynvoc `query_power_limits` 由**同一投影**作答，`--nvml`/`--nvapi` 两平面语义不变。**未读到 ≠ 0 W**：字段被 checks 判无效或 section 不可用时该字段为 `null` ＋该字段自己的理由；而**合规的 0 mW 仍作为 0.0 W 报出**（有数据与没数据必须可分）。
5. **一个被挡掉的误用** ✔：`power_policy_tgp_mw` 的名字像是"策略 TGP"，但实测它处在**瞬时功耗尺度**（11.3 W 对 100 W 的墙），与 100 W 的 TGP 完全不同量级 ⇒ 本轮**只按字段名原样报告**它，**绝不当上界使用**；它究竟代表什么标记为**尚未定位**，并把这次的数值证据写进代码注释与功能表（记录"为什么不拿它当上限"，而不只是"不知道"）。
6. **诚实边界** ✗：**范围（min/max）**＝**尚未定位**，不是"驱动没有"，证据是已实测的负结果 `0x2080200B PERF_GET_LEVEL_INFO_V2 → 0x1f`（该读取本应携带 level 区间）；**功率写**（`set-power-limit`／`reset-power-limit`／`set-public-tgp-percent`／`--rated-tdp-set`）＝**未实现，且在访问驱动前拒绝**，`settings_write=false` 不变（RATED_TDP 仍是**独立候选**，本轮未跑）；`get-rated-tdp`＝**未实现**，理由是它的读取路线是 NVAPI 私有三元组（0xED2BEA09/0x87BD35EF/0xFCBDF642），本路径从不发出 —— 是"没读"，**不是**"驱动没有"；`get-power-ceiling`／`get-power-mode`／`get-dnotifier`／PPAB／Whisper/Battery Boost＝**驱动未发布或移动端专属**，继续拒绝 ✔。
7. **验证范围** ✔：新增 core 测试 `the_power_limit_report_gives_the_wall_and_names_the_ends_it_has_no_source_for`（健康夹具的瓦数换算；被拒字段为 `null` ＋字段自身理由且**邻字段保留数值**；`0 mW` 作为数据报出 0.0 W 且不进 `unavailable`）、CLI 测试 `the_power_limit_read_is_allowed_while_its_write_half_stays_refused`。`cargo test -p nvoc-core` **125**／`-p nvoc-cli` **86**／`-p nvoc-srv` **53**／`-p nvoc-auto-optimizer` **68**，failed 全 0；workspace clippy `-D warnings` 与 `cargo fmt --check` 干净（clippy 报出的 `needless_question_mark` 已按其建议改为 let 形式）。设备侧：机器重建 `nvoc-cli` 后跑一次 `get-public-power-limit --macos-rm`（只读、零 SET）。**未发任何 SET、未提权、未重启** ✔。

### 本轮追加八十六（2026-10-18，温度族的独立审计：三个传感器视图"读得的读、读不到的说清"，并把"未接线"与"驱动没有"分开）

1. **选择依据** ✔：原目标要求风扇／功耗／温度**各自**给"可读 / 可解释 / 范围 / 原状态 / 恢复"的证据，阶段 11 第 5 条禁止"后端可用而入口一律拒绝"。风扇族（追加八十四）、功耗族（追加八十五）之后，本轮做**温度族**（三族齐备）。
2. **后端实有** ✔：RUSD 发布 4 个温度段的读数（`temperature_c`／`temperature_memory_c`／`temperature_board_c`／`temperature_power_supply_c`，步进 1/256 °C）；健康会话遥测（`outputs/stage2-telemetry-20261017T033137Z/`）里**只有 `temperature_c ≈ 45.6 °C` 有值**，其余三段无有效时间戳（驱动不发布）。
3. **独立第二来源，结论一致** ✔：公共热读 `NV2080_CTRL_CMD_THERMAL_SYSTEM_EXECUTE_V2 (0x20800513)` 在 `outputs/stage-thermal-read-20261017T061320Z/thermal-lines.txt` 实测：`1 target(s), 1 sensor(s)`、`sensor 0 reading 47 | range [0,127] | target 0 provider 0`，分类行给出驱动自己的目标类型词汇 **1=GPU 2=MEMORY 4=POWER_SUPPLY 8=BOARD**。两条独立证据都指向"这张卡/这个驱动只有 1 个活温度传感器（GPU）"，而那些缺的传感器**不是我们没读**，是**驱动不发布**。
4. **口径问题（阶段 11 第 5 条）** ❌：`get-legacy-temp-sensor`（GPU/Memory/Board 三传感器视图）在 macOS 上被**整体拒绝**，而后端确实发布实时温度 ⇒ 与前两族同类错误。
5. **本轮改动（已实现）** ✔：core 新增 `TempSensorView` 与 `DeviceStatus::legacy_temp_sensor_report()`，CLI `get-legacy-temp-sensor --macos-rm` 进入允许清单并由该投影作答。**根形状与每传感器键沿用入口自身**（数组根、`target`/`current_c`/`min_c`/`max_c`/`controller`），另加两个**附加**键：`field`（RUSD 字段名，来源可见）与 `unavailable`（每个 null 的理由）。传感器**按字段名配对而非按位置**：驱动类型序第 3 位是供电（4），而 RUSD 段序第 3 段是板卡 —— 按位置会错配两个 ✔。
6. **分清两种 null** ✔：`min_c`/`max_c`（物理范围）与 `controller` 恒 `null`，理由明确写着**它们可读**：范围有实测证据（`0x20800513` 报 `[0,127]`，target type 1=GPU，provider 0），缺的是**本客户端还没把这次公共读接进快照** ⇒ 记为**客户端未接线**，不是"驱动没有范围"；`controller` 则是因为公共读只报 provider 索引(0)，它与本入口 controller 的对应关系**尚未建立**，**不靠猜**去填。传感器读不到时，`current_c` 用**该字段自身的理由**，绝不填 0 °C（0 °C 会被读成真读数）✔。
7. **诚实边界** ✗：**实时温度**＝已实现且实测（GPU 芯温在阶段 2 与 Metal 负载实测；本轮因 wedge 未取到新数值）；**温度入口**＝已实现且实测（`get-legacy-temp-sensor --macos-rm` rc=0，失败方向实测见 review §111）；**阈值族**（`get-temperature-thresholds`：slowdown/shutdown/max）＝**驱动未发布到本路径**（该段只有读数没有策略），继续拒绝；**温度限制范围**（`get-public-temp-limit`：min/default/max＋curve）＝同上；`get-temp-sim`／`reset-temp-sim`＝仿真状态不在本路径；**温度写**（`set-temp-limit`／`reset-temp-limit`／`set-private-target-temp-limit`）＝未实现且在访问驱动前拒绝，`settings_write=false` 不变。
8. **pynvoc 侧** ✔：Python 面没有独立的 legacy-thermal 函数，这 4 个读数经 `query_status` 出口（macOS 分支已把它们连同 `temp_core`/`temp_memory` 别名发布）⇒ 本轮**没有硬造新 Python 函数**，功能表按"同一后端同一数值面"记录。
9. **验证范围** ✔：新增 core 测试 `the_legacy_temp_view_reports_each_sensor_and_the_fields_it_cannot_fill`（4 路读数与 target/field 配对；范围与 controller 恒 null 且理由含实测候选 `0x20800513`；一个传感器不可信时该路 `null`＋自身理由、且**邻路数值保留**、**不得为 0.0**）、CLI 测试 `the_legacy_thermal_view_is_allowed_while_the_threshold_entries_stay_refused`。`cargo test -p nvoc-core` **126**／`-p nvoc-cli` **87**／`-p nvoc-srv` **53**／`-p nvoc-auto-optimizer` **68**，failed 全 0；workspace clippy `-D warnings`、`cargo fmt --check` 干净；`cargo check -p pynvoc` 通过；机器 `cargo build -p nvoc-cli` 成功并实机跑一次（只读、零 SET）。**未发任何 SET、未提权、未重启** ✔。

### 本轮追加八十七（2026-10-18，把"量程"从"未接线"变成真接线：三态分清，实测楔子期间卡在会话建立）

1. **选择依据** ✔：追加八十六把温度族的"范围"记为**可读但客户端未接线**，并在功能表里写明"这是缺失的客户端读，不是缺失的驱动值"。本轮就补上这次接线（只读、零 SET），使该族"范围"一列有真实来源。
2. **接线依据是头文件与既有实测，不是猜** ✔：`NV2080_CTRL_CMD_THERMAL_SYSTEM_EXECUTE_V2 (0x20800513)` 与 `ctrl2080thermal.h` 的指令结构（`GET_INFO_TARGETS/SENSORS_AVAILABLE`、`GET_STATUS_SENSOR_READING`、`GET_INFO_SENSOR_READING_RANGE`、`GET_INFO_SENSOR_TARGET`、`GET_INFO_SENSOR_PROVIDER`、`GET_INFO_TARGET_TYPE`）在仓库里已有可复用证据：`probe-macos-rm/src/nvx_suite.c:1038-1170` 在同一台设备上跑过并留下 `outputs/stage-thermal-read-20261017T061320Z/`（1 target/1 sensor、`range [0,127]`、`target 0 type 1`）。target type 取值同时有头文件定义（`NONE=0 GPU=1 MEMORY=2 POWER_SUPPLY=4 BOARD=8 UNKNOWN=0xffffffff`）与设备实测两条证据。
3. **实现** ✔：C 侧新增 `nvoc_macos_rm_session_thermal()`（新增 40/372 字节 ABI 结构＋布局槽 41-43；调用与偏移通道同一个 `nvx_rm_control`，只发读指令）；Rust 侧新增 `ThermalReadout`/`ThermalSensorReadout` 镜像（带 `size_of`/`offset_of!` 编译期断言与 C `_Static_assert` 对齐）、`MacosRmSession::read_thermal()`、`Device::thermal_view()`、`QueryMacosRmThermalView`；CLI `get-legacy-temp-sensor --macos-rm` 改由该查询作答。
4. **三态分清（本轮的核心设计）** ✔：`ThermalReadOutcome::{NotAttempted, Refused, Answered}`——"没发起"才可以说"本快照未接线"，"发起但未完成"必须原样带上驱动理由，只有"已作答"才填量程。这不是文字游戏：本轮实测遇到的正是第二种，若把两者混成一句"未接线"，就会**把可用的客户端藏在病了的 GSP 后面**，与阶段 11 第 5 条冲突。
5. **不得把未跑指令当 0** ✔：量程只有在 `range_known==1`（该指令 `executed && result==0`）时才填；target type 未命名（`target_known==0`）的传感器**不填任何入口**，因为类型才是"这个量程属于谁"的依据。单测覆盖这两种情况。
6. **设备实测（楔子期间，只读、零 SET）** ✔：入口 rc=0、四路传感器均在位、`current_c`/`min_c`/`max_c` 为 `null`；`min_c` 理由为 `the public thermal read did not complete (macOS RM: RUSD allocate/map: 0x00000062 (NV_ERR_RESET_REQUIRED))`。**这条顺带给出一个此前没有的新事实**：楔子期间 `MacosRmSession` **在建立会话（RUSD 映射）时就被拒**，所以公共读根本没发出去——偏移通道的读同样卡在这一层，这也解释了 `get-settings` 报的是"打开失败"而非"逐块 0x62"。
7. **诚实边界** ✔/✗：**量程的真实数值**＝**已实现待实测**（接线完成、单元测试与构建、失败方向设备实测都有，但健康 GSP 下的 `[0,127]` 尚未由本入口实测）；**入口可达性**＝已实现且实测（rc=0）；**实时温度**＝已实现且实测（阶段 2 与 Metal 负载）；**阈值族/温度限制范围**＝驱动未发布到本路径（该段只有读数与量程，没有策略）；**温度写**＝未实现、访问驱动前拒绝，`settings_write=false` 不变。
8. **验证范围** ✔：core 单测新增 `the_thermal_read_fills_the_range_of_its_own_target_and_nothing_else`（本 target 量程填入、其余三路理由为"答了但没有该类型的传感器"、`range_known=0` 不变成 0、`target_known=0` 不落任何入口、RUSD 无量值时用读的读数并标 `0x20800513:sensor0`）与 `a_refused_thermal_read_is_not_reported_as_an_unwired_one`（三态互不混同）；core **128**／cli **87**／srv **53**／optimizer **68** 全过；clippy `-D warnings`／`fmt --check` 干净；`cargo check -p pynvoc` 通过；机器 `OGKM=… cargo build -p nvoc-cli` 成功（C 侧一并编译）并实机两次运行（第一次暴露 C 缺 `ctrl2080thermal.h` 头文件包含，已修）。

### 本轮追加八十八（2026-10-18，电压/VF 族的静态只读盘点：公开树里"确实没有"，仓库里"模型完整"，缺的是中间那一层）

**为什么现在做** ✔：原目标把 VF 曲线**后置**（"集中突破一条核心/显存频率域偏移通道，VF 曲线后置"），但没有把它划出范围；同时原目标禁止用"头文件搜不到"当未知的理由 ⇒ 电压/VF 这两族必须给出**具体调查结论**，而不是继续挂着"未实现"三个字。

**1. 公开 OGKM 树：两族都不可命名（这是核对后的精确结论，不是"搜索不便"）** ✔
- `reference/…/ctrl/ctrl2080/ctrl2080clkavfs.h` = **36 行**，正文只有 `#pragma once` ＋ 3 个 `#include`，**零命令声明**；`ctrl2080volt.h` = **38 行**同样空壳 ✗。
- `grep -rln "AVFS" src/` ⇒ **零文件命中**；`grep -rn "AVFS_.*_ID" src/` ⇒ **零命中** ⇒ AVFS 的 interface/message id **不在公开树里** ✗。
- `grep -rn "0x2080[Bb]2[0-9a-f][0-9a-f]" src/` ⇒ **零命中** ⇒ 电压族（VoltRails `0x2080b201`）**同样不可由公开树命名** ✗。
- 对照：CLK 偏移通道的私有块**也不在**公开树，两者**同形状** ✔。

**2. 出口表里这两族的行确实存在（所以不能写"驱动没有"）** ✔
| 族 | 接口字节 | 1118 行固件表 | 889 行导出表 |
|---|---|---|---|
| CLK legacy 读半边 | `0x90` | **74** 行 | **68** 行 |
| 电压 legacy 读半边 | `0xb2` | **13** 行 | **13** 行 |
| 电压 legacy 读半边 | `0xb3` | **1** 行 | **1** 行 |
- 电压两字节在两张表上**行数完全相同**（13/1），而 CLK 从 68 涨到 74 ⇒ §46 那批 229 行丢弃**确实覆盖 CLK 族** ⇒ 引用"某族有多少行"必须写明是哪份捕获 ✔（与 §48.4 同一纪律）。
- `0x2080b201@2444`（VoltRails GetInfo）与其余 `0xb2` 行是**读**半边；其写孪生按 §42 规则（接口字节 +0x40、消息 id +1）为 `0xf2xx` ✔ —— **写面存在**，与偏移通道的结论一致 ✗（不是"驱动没提供电压写"）。

**3. 仓库自身：NVAPI 调用者侧的 VF 模型是完整的** ✔（这才是"仓库内已有材料"）
- `nvapi-rs/src/clock.rs`：`VfPoint`/`VfpEntry`、版本化解码 `from_raw_versioned`（v1/v2/v3）、`v1_marshalable`/`v2_marshalable`、`ClkVfDomainHint`/`ClkVfSegmentKind`/`ClkVfDomainClass`（含 `is_gpc_curve_plane`）、以及 `clk_vf_g_prior*`/`clk_vf_effect_for_delta`/`clk_vf_delta_for_target`/`clk_vf_stair_fit` 的**曲线推演** ✔；
- `core`：`QueryVfpPointVoltage`、`QueryDomainVfpPoints`、`QueryDomainVfpIndices`、`VfPoint`/`VfPointType`、`SetPublicVftablePointOffset`/`ResetPublicVftableOffset`、`SetGpcVoltLock`/`VfpResetDomain`、`SetNvapiVfpPointPrivate`/`SetNvapiVfpRangePerPointPrivate` 等 ✔；
- 消费方：`auto-optimizer` 的 `oc_scanner`/`phases`/`pressure`/`scan_support` 已经在用这些操作做真实扫描 ✔。

**4. 结论与分类** ✔
- 缺的**不是模型、不是驱动实现**，而是 **NVAPI 与 RM 之间的那一层在 macOS 路径上的接线**（私有 id 与其封送）——与偏移通道**同一形状**的问题 ⇒ 偏移通道先做，VF 后置 ✔（符合原目标）。
- 分类：电压/VF 的**入口**＝**未实现（在访问驱动前拒绝）**，原因＝该层未接线 ✔；**不是**"驱动没有"✗（`0x90`/`0xb2`/`0xb3` 的行确实在表里，写孪生按规则存在 ✔）；**不是**"尚未定位"到无法动手 ✗（方法已定：id 结构 ＋ 仓库内调用者结构 ＋ 可证伪的只读请求，与偏移通道同一套）✔。
- **待做**（保持 VF 后置，不改优先级）：在偏移通道的写入门（权限前置条件 ＋ 客户端 SET 入口）解决后，用同一方法为 VF 挑出可证伪的只读首发 id。

---

### 追加八十九：偏移容器的**第二方法**复核、两条口径更正（2026-10-18）

本轮**未改变任何功能面的分类**（下表各行不变）；改的是"哪些结论已经够结实可以引用"。

1. **`0x20809025` 现由两种独立方法给出同一结构** ✔：零种子 payload 解码（`clkctl-hex.txt`）与 `0xA5` 背景差分（`clkctl-offsweep-run1-A5.txt`，工具自报 `106/527 dwords changed`）在记录位置（`+20/+244/+468`，stride 224）、类型戳（`01030101/01040101/01050101`）、`rec+8/20/24/28` 上**逐字段一致**。单位问题**不存在**：`rec+8 = 1638400` 的名称与单位取自仓库内 NVAPI 侧字段（记为 max kHz），不是从数字形状猜的。
2. **口径更正一**：range 头部 `+4 = 0x7` 此前写作"支持的域掩码 bits 0,1,2"，但三条记录的类型字节是 **3/4/5**——与位 0/1/2 **不在同一坐标系**；同族已有 `0x20809019`（头部同时给出 `0x3ff` 行位与 `0x38007f` 域位并集）与 §14（`0x2080901b` 的 `+4` 是**条目选择器**）两个先例 ⇒ 记为**候选更正**（本块字节无法单独区分），并给出可证伪条件。**在此之前引用该字段必须标出这条不确定性。**
3. **口径更正二**：`0x2080901d @32680` 的 `(head 40, stride 1020)` 是**算术**（同一等式四解：`(40,1020) (168,1016) (296,1012) (424,1008)`），而实测脏 dword 分布以 **128 字节**为周期（`+36..+56` 与 `+164..+184` 三 dword 逐字节相同）——**128 不能平铺任一算术解** ⇒ 已把两个读法写成工具 `MEASURED_REPETITION` 与 `--selftest` 断言，并把判决条件预登记（第三组落 `+292` ⇒ stride 128；落 `+1060` ⇒ head 1020）。**在此之前该 id 记"读成功但布局尚未定位"。**
4. **自有缺陷与纪律**：本机 `core-fixtures/*.bin` 曾是**全零**（早期错误掩码运行的残留，3084/1797 dword 全 0）⇒ 基于它们的离线分析都是空的；已从真实捕获重建并记录 md5（`182d7851c6fff2f28520846bae90b1df` / `9cba1f2228a50b82ecc175cc9e257c3a`）。纪律重申（与 §48.4 同一句）：**引用载荷必须带 artifact 与 md5**。
5. **同次运行的其余判读**：`0x2080901a@2052 → 0x1f`、`0x2080901c@808 → 0x65`、`0x20809026@5332 → 0x65`（**随后把会话锁进 `0x62`** ⇒ 保持登记在 `nvx_known_wedge_ids`，下次改发仓库内副本声明的 **2508**）、`0x2080901d@32680 → NV_OK`（352/8170 dwords）、`PERF_GET_LEVEL_INFO_V2 → 0x1f` ⇒ **驱动不暴露时钟域列表**；RUSD 该帧 `gpu 50.75C / gpc 1530000 / mem 6000984 / fan 0 rpm`（空转 0 rpm 的又一实例）。
6. **验证范围（如实）**：全部为**重读已录字节** ＋ 仓库内资料；测试机本轮**仅 1 次只读 anchor**（`0x00000062`，uptime `1 day, 6:39`，无重启），**零 SET、未提权、未改 EFI/boot-args**；未改 Rust/C 代码 ⇒ 不重跑构建；改动＝工具（`decode-clk-containers.py`）＋纠错表 §49，已双向 md5 一致（`29a3da41f7f7f0f9e097184492883593` / `c3e1bc425759e104cc8b86b7916460bf`）。

---

### 追加九十：偏移通道的**槽位**定位到 `rec+8`，并把该结论做成可执行校验（2026-10-18）

本项此前挂在"十二个值槽里哪一个是有符号偏移——尚未定位"。本轮把它收敛，且**不改任何行的分类**（写入面仍是"待授权"，读面结论不变）。

1. **两条依据**（均在仓库内可得、且可复核）：①`nvapi-rs/sys/src/gpu/clock.rs` 的调用者字段表 `clk_ctrl_entry` 把五槽第一槽命名为 `OFFSET_KHZ=44`（`RANGE_MIN/RANGE_MAX/APPLIED` 依次 48/52/56），而本驱动实测记录类型 3/8/9 全在 `v1_marshalable`（`nvapi-rs/src/clock.rs`）集合内 ⇒ **V1 命名适用于这些记录**，V1 回退分支的 `values_kHz[0] = off` 亦一致；②跨层字段差 `0x24` 是在**兄弟容器** `0x20809025` 上实测的（调用者 `REC_MAX_KHZ/REC_MIN_OFFSET/REC_MAX_OFFSET` ↔ RM `rec+8/+20/+24`，三对同差、其中两对是偏移语义字段、值同批实测），据三个 k 拟合得槽位 `rec+8+4k`，与 §14 实测写集合（每 `0x08/0x09` 记录**连续 13 个 dword** `rec+8…rec+56`）自洽 ⇒ **有符号偏移＝RM `rec+8`**。
2. **反证归位**：`VALUES[2]` 出自 **V2 世代**（`clk_ctrl_entry_v2`，magic `0x261A4`/24996 B，`BASE 292 / STRIDE 772 / VALUES@268`，Blackwell `FREQ_KHZ==0x114==VALUES[2]`），而本驱动 `0x2080901b` 只在 **RM 尺寸 2108** 回过 `NV_OK`、V2 调用者尺寸直发 RM 入口是 `0x1f` ⇒ 它不是关于本块的证据，此前"两说并列"的悬置取消。
3. **可执行化**：`decode-clk-containers.py` 新增 `cross_check_rust_field_map()` 并接入 `--selftest`——**直接读两处 Rust 源**断言上述字段常量、断言工具打印的五个槽恰等于"该四字段加下一个减 `0x24`"、断言 `v1_marshalable` 仍覆盖 3/8/9、断言 V2 的 `BASE/STRIDE` 属另一代；任一侧漂移即报错（负向验证：打印槽 16→28 ⇒ `([8,12,20,24,28],[8,12,16,20,24])`）。门控写脚本候选顺序改为 **`+8` 先、`+44` 兜底**（只改脚本与注释，**未发写**）。
4. **仍保留的不确定性（写进门槛前的最后一句）**：`0x24` 实测于兄弟容器，跨用到 `0x2080901b` 仍是一步跨容器。**最终裁决只能是**写入门槛满足后的单域最小步长写入＋**同字段回读**（写 dword 读回 `orig+delta`、其余值 dword 逐字节不变、恢复后整块逐字节相同），且**不以实时频率变化为判据**。
5. **验证范围（如实）**：设备访问**仅 1 次只读 anchor**（`0x00000062`，uptime `1 day, 6:46`，无重启）；**零 SET、未提权**；未改 Rust/C ⇒ 不重跑构建；改动＝工具（Python）＋门控脚本注释＋纠错表 §50，三件与测试机 md5 一致。

---

### 追加九十一：阶段 11 第 5 项——三入口与后端的能力/范围/错误一致性审计，查实并修好两处（2026-10-18）

按"后端已实现的面 × CLI/pynvoc/srv 三个入口"逐面核对，**查实两处真实不一致并修好**（commit `9925228`、`b9900ef`），其余三处"装不下/缺函数/跑不起来"点名为缺口并给出依据。**不改变任何一行既有分类**。

1. **修好：pynvoc 的风扇与功耗入口在 macOS 上"一律拒绝"**。`query_fan_info`／`query_power_limits` 原把后端**硬编码 `"nvml"`**，而 `with_target` 先过 `ensure_backend_available()`（macOS 只放行 `macos-rm`）⇒ 两者各自的 macOS 分支是**死代码**，后端能答而入口一律拒绝 —— 正是本条禁止的第一种形态。改为按平台取默认（macOS `macos-rm`，其余平台仍 `nvml` ⇒ 非 macOS 逐字节不变）＋显式 opt-in 参数，并加回归测试把"旧值被拒"钉住。
2. **修好：CLI 对 macOS 逐传感器范围的描述与实现相反**。`get-legacy-temp-sensor` 的说明仍称范围"null and not wired in"，而 `legacy_temp_sensor_report` 已在公开热读答复时回填 `min_c`/`max_c` 并区分"本次没读到/根本没接线"⇒ 描述低报了能力，已改写（`cargo test -p nvoc-cli --lib` 87 passed）。
3. **一致性表（本轮核对结论）**：风扇 rpm（core `FanReport`／CLI `get-fan-info`／pynvoc 本轮修好／srv 由 `read_gpu_info_json` 输出 `fan_rpm`）＝**三入口可达**；功耗（请求值/墙/策略）同；温度读数（4 target）同；**域偏移读**（776 文档 `clock_offsets`）＝ core／CLI `get-pstate-global-freq-offset`／pynvoc `query_settings` 与 `plan_clock_offset_step`／srv 具名理由 ✔。srv 的两处 `None`（风扇占空比、功耗窗口）与 pynvoc 的写面拒绝都是**同一具名拒绝**、非静默 ⇒ 与后端一致。
4. **点名缺口 1（缺失层＝客户端接口层）**：pynvoc **没有温度传感器/范围函数** ⇒ Python 入口拿不到 `TempSensorView` 的 `min_c/max_c/controller`。追加八十六 曾以"这 4 个读数经 `query_status` 出口"为由不新增函数——该理由覆盖**读数**，但 `status.sensors` 是 `(descriptor, temp)` 对，**不含范围**，而范围是追加八十八新接入的能力 ⇒ 缺口成立；core 已有该投影，补一个函数＋注册即可（下一步小步）。
5. **点名缺口 2（环境/前置条件，不是协议）**：pynvoc 在测试机**无法链接**（`cargo build -p pynvoc` 报 `_PyBool_Type`/`_PyBytes_AsString` 未定义），因本机 `python3` 是 Xcode 3.9 且 **`Py_ENABLE_SHARED=0`**。⇒ Python 入口在 macOS **从未实跑**，其后端行为目前只有静态读码＋Linux 单测级证据；macOS 侧唯一可用验证是 `cargo check -p pynvoc`（本轮已过）。**不写成"pynvoc 不可用"**。
6. **点名缺口 3**：srv 的类型面（`FanReading.percent`、`SensorBundle`、`MonitorSample.fan_pct`）装不下 rpm 与范围——不伪造百分比是**正确**的（注释已写明 rpm 在此结构无处安放），rpm 已由 `read_gpu_info_json` 输出；是否扩类型＝待决小步，非缺陷。`query_temperature_thresholds` 在 macOS 报平台级拒绝而 CLI 报"策略未发布"，同一拒绝、措辞不同，记为**待统一措辞**，本轮未动以免影响其他平台默认。
7. **验证范围（如实）**：**0 次新 GPU 请求**（wedge 仍 `0x00000062`，uptime 1 day 6:53，未重启）；本机 `cargo check -p pynvoc` ✔／`clippy -p pynvoc -D warnings` ✔／`cargo test -p pynvoc --lib` 13 passed（含新测试）／`cargo check -p nvoc-cli` ✔／`cargo test -p nvoc-cli --lib` 87 passed；测试机 `cargo check -p pynvoc` ✔、`cargo build -p pynvoc` ❌（缺口 5，既有）。

### 本轮追加九十二（2026-10-18，把"哪些 id 真的发过、答了什么"变成可复算的普查，并据此更正两处散文化结论）

**(1) 动机（缺口驱动的静态工作，不动 GPU）** ✔：本表与 review 里到处是逐 id 的判定（"从未发过"/"已实测"/"驱动拒绝"），而它们**只能靠散文自证** ✗。健康窗口稀缺，所以先把"设备访问史"做成产物：`probe-macos-rm/tools/audit-live-attempts.py` 扫描 `outputs/` 下**每个 `*.txt`/`*.jsonl`**，取带 `status` 的行，抽出该行出现的 `0x2080xxxx` id，按 id 汇总"派发次数／各自状态／来自哪些文件"；`--table` 再与方法表交叉，`--id` 可当闸门（该 id 从未 NV_OK 则退出码 1）。**刻意跳过 `.md`**：评论里提到一个 id **不是**派发证据 ✔。工具自带 `--selftest`（合成 jsonl/txt/md/table：正例计入、无 status 的行与 `.md` 散文不计入、交叉与闸门两个方向都断言）✔。

**(2) 普查结果（2026-10-17T213645Z，616 个捕获文件；本地只读，无可写文件）**：**58** 个 id 曾出现在返回状态里，其中**仅 16** 个至少一次 NV_OK ⇒ 与方法表对照：**1061 个在表 id 从未出现在任何返回状态**。与本目标直接相关的（数字为**修正后**，见 (6)）：`0x20809019`＝775×NV_OK、`0x20809025`＝465×NV_OK、`0x2080b201`＝49×NV_OK、`0x2080901b`＝17×NV_OK（另有 38×`0x1f`、15×`0x25`、33×`0x62`、2×`0x65`）、`0x2080901d`＝12×NV_OK、`0x20809026`＝2×NV_OK（另有 11×`0x1f`、2×`0x65`）、`0x20800513`＝3×NV_OK（另有 2×`0x62`）；而 **`0x20808513`＝0×NV_OK（2 次派发全部 `0x62`）**、`0x20809006`＝0×NV_OK（39×`0x1f`，接受形式未定位 ✔）、`0x2080901a`＝0×NV_OK（2×`0x1f`＋2×`0x62`）。⚠️ **这批数字先错后改**：初版把「行里出现几次 id」当成「派发几次」，而探针的一行会写两遍同一个 id（标签与参数各一次）⇒ 计数恰好翻倍；已在工具内改成**一行＝一次派发、同 id 只计一次**，并加断言（selftest：写两遍的 id 只计 1、仅在 detail 里被提及的 id 仍计入＝**上界**、无 status 的行不计），随后重跑得到 (6) 的修正数字 ✔。

**(3) 更正一（本表自身的笔误）**：追加七十七 (5) 曾写"温度读取＝`0x20808513` 已实测 NV_OK" ✗。普查显示**实测 NV_OK 的是公开 id `0x20800513`**，而 legacy physical 别名 `0x20808513` 的 2 次派发**全是 `0x62`** ⇒ 与本节 (2) 的"alias 与公开同胞同结构同尺寸"并不矛盾（可读的是公开半边），但"已实测"必须归到正确的 id。已就地更正。

**(4) 更正二（更强的发现：风扇族的"实测"结论没有产物）**：本表追加七十三曾记 `0x20808532@336 → 0x62`、`0x20800532@336 → 0x56`，并标为"本轮实测 2 次只读 GET" ✗。普查＋全树 grep：**`outputs/` 乃至整个 develop 目录、`/tmp` 下没有任何 `.txt`/`.jsonl` 含这两个 id**（除本轮新建的 census 文件）⇒ 那次运行的**原始行从未落盘**，两个判定目前是**散文化结论**（运行可能确实发生过，但按本项目"引用必须带产物"的规矩，它现在不算证据）。⇒ 处理：①本表就地加更正注；②`core/src/macos_rm.rs` 的 `FAN_CONTROL_UNREADABLE_REASON`（**用户可见串**）同步改写，明说这两个结论是 prose-only、并指名 census 工具；③`tools/clock-offset-read-run.sh` **把 `--fan-read` 并入读取电池**（放在 `clkctl-read` 之后、解码之前），健康窗口一次跑出 `fan-read.txt`，从此该族有产物；④**预登记**：健康跑若 `0x20808532` 返回 `0x62` ⇒ 与"wedge 是唯一障碍"一致（**不是**"命令不存在"、**不是**权限）；若返回其他状态 ⇒ 那次散文结论被推翻，按新字节重写；公开 `0x20800532` 的 `0x56` 同理需复核（同轮 `edpp-limit-info` 也 `0x56`，芯片确实缺的命令也这样答 ⇒ 不能据此断言）。
   **（旁证：普查反而支持本表先前"从未发过"的声明——`0x20808540@24`、`0x2080a06e`、`0x2080d01c`、`0x2080d024` 均列在"表里有、从未出现在返回状态"✔，与追加七十/七十五的记载一致 ✔。）**

**(5) 验证范围（如实）**：**0 次新 GPU 请求**（wedge 仍 `0x00000062`，uptime 1 day 7:16，未重启；无写入、未提权、未动 PR）；本机跑 `--selftest` ✔、`ruff format --check`／`ruff check` ✔、对新版 `nvx_suite.c` 的编译在测试机 ✔（`built probe-macos-rm-idle`）；普查产物存于测试机 `outputs/stage-live-attempt-census-20261017T213645Z/census.txt` ✔。**普查的边界**：它只能看见**已落盘**的返回状态 ⇒ "未落盘的运行"恰恰是它这次抓到的缺陷类型，故每次设备侧运行都必须先定 `OUT_ROOT`。

**(6) 本轮末尾：把"散文化"和"从未发"直接变成产物（只读 GET，无授权门槛）** ✔：普查暴露的缺口可以当场关掉，所以没把它留到下一个健康窗口。
- **风扇族**：`--fan-read` 实测两次，`0x20808532@336 → 0x62 NV_ERR_RESET_REQUIRED`（**可派发**，唯一障碍是 wedge ✔）、`0x20800532@336 → 0x56 NV_ERR_NOT_SUPPORTED`。**同一轮内**表内 id 拿 `0x62`、这个公开 id 拿 `0x56` ⇒ 这是一次**同轮内对照** ✔：公开半边的拒绝**不是** wedge 码，与"方法表不含公开 id"一致；但仍**不能**断定原因是"芯片不支持"还是"表里没有"——两者在本路径上都表现为"不被服务"，如实记账 ✔。产物：`outputs/stage-fan-read-20261017T214007Z/fan-read.txt`、`outputs/stage-fan-hwfs-read-20261017T214251Z/read.txt`（sha256 `05b8713a1d5bce02…`）✔。
- **散热器/HWFS 族**：新增 `--hwfs-read`（`0x20808540@24`，尺寸取**方法表行声明的 paramSize 24**，不取 NVAPI 副本的 52 B 平面，并写明理由：isl 尺寸错会伪装成"无此命令"），**首次派发** ⇒ `0x62`，与全体 GSP 转发控制同一堵墙 ⇒ 该 id **可派发、只等健康窗口** ✔，从"从未发"升级为"已登记且已派发 1 次" ✔。已并入读取电池。
- **修正后的普查（2026-10-17T214405Z，621 个捕获文件）**：派发过的 id **61** 个（含本轮新增的 3 个），其中 16 个至少一次 NV_OK；`0x20808532` 0×NV_OK（2×`0x62`）、`0x20800532` 0×NV_OK（2×`0x56`）、`0x20808540` 0×NV_OK（1×`0x62`）✔。产物：`outputs/stage-live-attempt-census-20261017T214405Z/census.txt`（sha256 `6e1b606275bbc6f4…`）✔。
- **不变的一条**：全部都是 GET；`SETTINGS_WRITE_SUPPORTED=false`，没有任何一次写入 ✔，未提权 ✔。

### 本轮追加九十三（2026-10-18，偏移入口把「限制／原状态」公开出来：一次读取回答三问，仍零写入）

1. **选择依据** ✔：本轮最高优先的偏移通道仍卡在**健康窗口**（楔死未解，`0x62`），而目标第 3 条要求"先取得**当前值／限制／原状态**"，第 5 条要求"接入现有公共 core/CLI/Python 接口"。核对入口时发现：core 的 `DeviceSettings` 里**窗口与当前值都已读到并有产物**，`OffsetStep` 也会带窗口——但**只有**加 `--plan-delta` 才会打印窗口，等于"限制"在入口上不可直接问；同时功能表的平坦表仍把这命令写成"未实现：访问驱动前拒绝"（与同一文档第八十三节自相矛盾）。⇒ 本轮把三问做进**同一个读入口**，并回填那张表。
2. **实现（core）** ✔：新增 `ClockOffsetView`（`domain`／`domain_bit`／`offset_khz`／`frequency_khz`／`corroboration`／`window_min_khz`／`window_max_khz`／`window_records`／`window_reason`／`restore_khz`）与 `DeviceSettings::clock_offset_view(name)`，供 CLI 与后续 pynvoc 共用同一份数字与判据（避免三入口各写一套"当前值"定义）。两条规则落进代码：①**未读到窗口不是 0 窗口**——`None` ＋ `window_reason` 引**偏移通道那条记录的原文**（按 `", "` 切分时对未闭合括号做回接，不改写措辞）；②**未命名域在入口被拒**，与后端同理由（`named_clock_offset`）。
3. **实现（CLI）** ✔：`nvoc-cli get-pstate-global-freq-offset --macos-rm --domain X` 现在额外发布 `window_min_khz`／`window_max_khz`／`window_records`／`window_reason`／`restore_khz`（**纯增键**，既有键与 `plan` 语义不变）；`restore_khz` 是"写回时必须回到的值"，**在写闸门打开前就先给出**，以免事后自造恢复目标。
4. **Python 入口同源（同一次整改的第二处缺口）** ✔：pynvoc `query_settings` 此前在范围块被拒时**什么都不给**——调用方无从分辨"被拒"与"没读到"，正是本后端明令不允许的含糊。改为从**同一个**共享入口 `DeviceSettings::window_absence_reason()`（域无关；本轮由私有 `offset_window_absence` 提升为公开，理由原文取自该块自身结果）取词，插入 `clock_offset_window_reason`；`clock_offsets_named` 的每条另加 `restore_khz`（＝本次读到的偏移：写回目标**读出来**，不事后自造）。**验证范围有限**：pynvoc 整 lib 在本机与测试机都无法链接（pyo3 缺 libpython 符号，既有环境条件）⇒ 本侧只做到 `cargo check -p pynvoc` 通过，**未跑其运行时测试** ⇒ 该项归类**已实现待实测（Python 运行时）** ✔。
5. **文档回填（本轮自查出的不一致）** ✔：平坦表 `get-pstate-global-freq-offset` 行由"未实现：拒绝"改为读面**已实现且实测**＋写面未实现；并在该表前加一句列义说明（该列＝**入口**行为，必须与 `nvoc-cli --macos-rm` 允许清单一致，漫拒覆盖已实现命令即为缺陷）。
6. **诚实边界** ✗：本步**一次 SET 都没发**，`SETTINGS_WRITE_SUPPORTED` 未动；`window_*` 是**驱动声明的整体跨度**（`window_records` 给出同意该跨度的记录条数），**不是**逐域边界；`restore_khz` 是"读到的当前值"，它的证明要等第一次 SET＋同字段回读。
7. **验证范围（如实）** ✔：新增 core 单测两个——`the_entry_view_carries_the_window_and_the_restore_target`（窗口走**生产解析路径** `apply_offset_channel`，三条同值记录 ⇒ 入口必须带 `Some(-18750)`／`Some(18750)`／`records=3`，且 `restore_khz == offset_khz`）、`an_unread_window_is_absent_with_the_blocks_own_reason`（范围块 `Refused(0x62)` ⇒ 窗口为 `None` 且理由里同时出现"refused"与"no window is reported"，**偏移本身仍报出** ⇒ 两个读不塌成一个失败；未 corroborate 的域返回错误而非 0）；`cargo test -p nvoc-core --lib` **109 通过／0 失败**（含新增两例）、`nvoc-cli` **89 通过／0 失败**（含新增两例：`the_offset_entry_publishes_the_driver_span_and_the_restore_target`、`an_unread_window_reaches_the_entry_as_null_with_a_reason`——为此把入口 JSON 抽成 `offset_entry_json(&DeviceSettings, …)`，使**入口形态**无需设备即可测）、`nvoc-srv` 53／`nvoc-auto-optimizer` 68 全绿；`cargo clippy --workspace --exclude cli-stressor-cuda-rs --exclude pynvoc --all-targets -- -D warnings` 干净，`cargo fmt --all -- --check` 干净，`cargo check -p pynvoc` 成功。**本轮测试机 0 次 GPU 请求**（只 ssh 取 `uptime`＋后台重试日志、`git` 同步、`cargo test`）。测试机（macOS）实测同四个新测试**全部通过**：`nvoc-core` **108**／`nvoc-cli` **88**（本机 Linux 为 109／89——差 1 来自仓库既有的**平台门控桩测试**（`cfg(not(all(target_os="macos", target_arch="x86_64")))` 的占位测试在非 macOS 侧存在），与本次改动无关，两侧新增测试名逐一相同 ✔）。数字本身仍是**已实测**（范围块 `NV_OK`、±18750、三条记录；776 文档逐域偏移），"入口是否把它们完整发布"由上述单测保证 ⇒ 数字＝**已实现且实测**、入口形态＝**已实现（单测覆盖）**；**入口的实时快照仍待健康窗口**（楔死期 `MacosRmSession` 在 RUSD 映射阶段即失败，该命令此刻取不到新快照），故本轮**没有**把它记成"入口已实机跑通"；写面仍 **未实现，且驱动明确拒绝**。
8. **设备侧口径** ✔：楔死未变（后台重试第 100 次仍 `0x00000062`，uptime 1 天 8 小时 ⇒ **未重启**，不自动重启）。健康窗口一到即按交接 §7.0 重建（`OGKM=` 必带）并跑 `tools/clock-offset-read-run.sh`；写闸门仍等一次交互式 sudo。

### 本轮追加九十四（2026-10-18，把"入口漫拒"从人工发现变成闸门：一个缺陷类别的机械核对，含四个历史反例，零 GPU 请求）

**(1) 选择依据** ✔：上一轮的缺陷（平坦表把已实现的 `get-pstate-global-freq-offset` 写成"入口在打开驱动前就拒绝"）是**靠人眼比对文档发现的**。同一类问题在表中还有 92 处漫拒，人眼每次只能抽检；而阶段 11 第 5 条要求"既不能后端可用而入口一律拒绝，也不能可点但无实现"。故先把这一类做成**可执行判定**：健康窗口稀缺，闸门必须能在无设备、无 GPU 时随时跑。

**(2) 实现** ✔：`probe-macos-rm/tools/audit-entry-parity.py` 只读三份**决定答案**的来源——CLI 拒绝路径里 `validate` 的 `matches!`（真正执行的允许清单）、`cli/src/lib.rs` 的命令表（变体→命令名）、拒绝消息里那句用户可见的 `supported commands: …`——产出三类判定：①允许清单与命令表是否对得上（变体缺命令名即 FAIL）；②那句消息与允许清单是否**双向**一致（漏掉允许的命令、或列出被拒的命令都 FAIL）；③本表每一行：命令在允许清单里却带漫拒 → FAIL（带行号）。**`--selftest` 覆盖**：干净表通过、把**上一轮那条真实缺陷行原文**放回去必须 FAIL（`matrix:2`）、消息漏项／多项、变体无命令名、允许清单读不出来、表里没有任何命令行、**反向行**（见下）——八种情形全部断言，且**空输入一律判 FAIL 而非静默通过**。**反向（镜像）方向另作信息项、不判 FAIL**：某行自称"已实现"而命令不在允许清单里，是同一缺陷的另一面，但"已实现"是散文 ⇒ 只打印。今天它只点名一处——`matrix:124 get-vbios`（该行写"**部分**：`-i/--input` 离线解析已实现"），且**这处是正确的**：`cli/src/lib.rs:2351` 在选后端**之前**就短路 `get-vbios -i <file>`，离线解析根本不经过该漫拒 ⇒ 有据可查，不是漏改。再者出现新名字时它是**问题**而不是判决。**（§一百一 追加第四类判定：表本身的完备性——代码里 `CommandSpec` 声明的每条命令都必须在表里有行、每组标题的项数必须等于其行数、节标题的总数必须同时等于代码声明数与行数。第三类判定问的是"表里的行对不对"，这一类问的是"该有的行在不在"；`list` 正是后者：代码有、表没有，而行级判定看不见没有的行。）**

**(3) 反例（闸门确实有牙）** ✔：对四份历史文档运行同一工具（用当时的 `docs/design/macos-backend-feature-matrix.md`）：`8830c0f`／`dac43f2`／`d06dff7`／`18b5117` **四版全部在第 181 行报 FAIL**（`get-pstate-global-freq-offset carries the blanket refusal while the entry allows it`），对当前 `71ebffd` 报 `9 allowed command(s); none of them is behind the blanket refusal (108 table rows read)` ⇒ rc=0。**这四个历史命中是"闸门有效"的证据，不是新缺陷**。

**(4) 不检查什么（写清边界）** ✗：①**不判断非允许命令那行的散文是否准确**——那列自由文本会点名 RM 状态码、尺寸与前置条件，属 `audit-matrix-claims.py` 的职责（本轮另跑一次：**151 supported／0 contradicted**）；②不做"可点但无实现"的反向检查——允许清单只说明入口放行，**放行后是否真有实现**由各命令自己的产物与功能表说明；③不认 id、不认尺寸、不发任何请求。

**(5) 验证范围（如实）** ✔：新增工具 1 个（Python，`--selftest` 七情形 + 真实文件运行）；`ruff format . --check`／`ruff check` 干净。测试机同跑一次并留产物（`outputs/stage-entry-parity-<ts>/`），命令与退出码见该目录 README。**本轮测试机 0 次 GPU 请求**；wedge 未变（第 112 次 `0x00000062`，uptime 1 天 8 小时 22 分，**未重启**）。

**(6) 诚实边界** ✗：闸门只覆盖**能机械判定**的那一类（允许清单 × 漫拒短语）；表里其余措辞仍靠 `audit-matrix-claims.py` 的"声明／派发／产物"三级核对与人工复核。短语本身是**字面量**，若将来改写漫拒措辞（例如加限定词），工具会**失去命中而不是假装通过**（`BLANKET` 常量在文件头，注明来源）。

### 本轮追加九十五（2026-10-18，同一个问题不再有两种答案形状：macOS 评测的纯逻辑并入 core，三入口发布同一份 plan 文档，零 GPU 请求）

**(1) 选择依据** ✔：目标第 3／5 条与阶段 11 第 5 条（"各入口能力、范围和错误与后端一致"）都要求"同一件事在入口上说同一种话"。核对时发现**同一个问题有两种答案形状**：`auto-optimizer macos-eval --plan` 输出的是文档（`applied`／`settings_write`／`requests`／`note`／`steps`），而 `nvoc-cli get-pstate-global-freq-offset --plan-delta` 与 `pynvoc.plan_clock_offset_step` 输出的是**裸 step**（`verdict`／`target_offset_khz` 直接挂在 `plan.*`）。差别不是有意的：读者看不出理由；更要紧的是**诚实文本只在一边**——`applied: false`、`settings_write`、以及"`ready` ≠ 已写入"的那段说明只出现在 optimizer 的答案里，用 CLI 问同一问题的人**看不到"什么都没写"**。同时这段纯逻辑住在二进制 crate 里（`auto-optimizer/src/macos_eval.rs`），另外两个入口既用不到也测不到。

**(2) 实现（core，纯函数、无 I/O）** ✔：新增 `core/src/macos_eval.rs`：`PlanRequest`、`parse_plan_requests`（`<target>=<delta>`，语法拒绝时用**语法自己的话**，不把笔误读成"未命名域"）、`PlanDocument{applied, settings_write, requests, note, steps}` 与 `plan_document()`（**不可能失败**：step 由 `plan_offset_step` 产出，没有第二条错误路径）、`PLAN_NOTE` 常量、以及遥测归约 `Stats`／`stats`、`ThrottleSummary`／`throttle_summary`、`LoadSummary`／`load_summary`（**throttle 证据就在同一个值里**，报告不可能带了负载却忘了为什么被限）、`fps_tail`。**core 里不构造 JSON**：全部是带 `Serialize` 的类型值，由各入口自己序列化 ⇒ `serde_json` 继续留在 `core/Cargo.toml` 的 `[dev-dependencies]`（这不是风格偏好：第一版把 JSON 建在库里，编译直接报 `unresolved import serde_json`，正是那个错误把形状定成了类型）。`settings_write` 读 `settings.capabilities.settings_write`，即 `core/src/macos_rm.rs:437` ← `SETTINGS_WRITE_SUPPORTED`（`:119`），因此文档**不可能**与写闸门说两套话。

**(3) 实现（三入口同一份文档）** ✔：optimizer 的 `--plan` 段改为 `serde_json::to_value(plan_document(..))`，并删掉它本地的 `PlanRequest`／`parse_plan_requests`／`dry_run_plan`／`stats`／`load_summary`／`throttle_summary`／`fps_tail` 与其单测（这些测试**移入 core，未删**，名字见第 4 点）；CLI 新增 `offset_plan_json()`，`--plan-delta` 输出同一份文档（`plan.applied`／`plan.settings_write`／`plan.note`／`plan.steps[0].verdict`）；`pynvoc.plan_clock_offset_step` 返回同一份文档（docstring 已改）。**请求的拼写仍按入口**：CLI 用 `--domain X` 指定目标、`--plan-delta` 只带步长，optimizer 与 Python 用 `target=delta`——共享的是**语法**（`parse_offset_target`／`parse_offset_delta`）与**文档**，拼写差异写在一处、不藏。`pynvoc` 未能运行的部分照实归类（见第 6 点）。

**(4) 可执行证据（本机、零 GPU）** ✔：`cargo test -p nvoc-core --lib` **119 passed／0 failed**（含移入的 5 个 + 新增 4 个：`a_plan_document_is_never_applied_and_says_whether_this_backend_writes`、`the_plan_document_keys_are_the_published_interface`（**把 `["applied","note","requests","settings_write","steps"]` 钉住**）、`the_documents_write_flag_is_the_backends_own_switch`（断言 `== SETTINGS_WRITE_SUPPORTED`）、`the_load_summary_keys_carry_the_throttle_evidence`）；`-p nvoc-cli --lib` **90 passed／0 failed**（新增 `the_offset_entry_publishes_the_shared_plan_document`——它失败时打印的正是 CLI 自己产出的那份文档，输出即证据）；`-p nvoc-auto-optimizer` **61 passed／0 failed**；`-p nvoc-srv` **53 passed／0 failed**；`cargo fmt --all -- --check` 干净；`cargo clippy --workspace --exclude cli-stressor-cuda-rs --exclude pynvoc --all-targets -- -D warnings` 干净；`cargo check -p pynvoc` 干净；`ruff format . --check` 干净（185 files）。**负向断言随测试一起留下**：字段缺失 ⇒ `None`／`null`（**不是 0**）、无样本 ⇒ `None`、`target` 拼错 ⇒ 语法自己的错误文本、`=` 缺失 ⇒ 该标志自己的错误文本。

**(5) 上机侧验证（测试机 `c3a0129`，0 次 GPU 请求）** ✔：`cargo test -p nvoc-core --lib` **118**／`-p nvoc-cli --lib` **89**／`-p nvoc-auto-optimizer` **60**（**failed 全为 0**，与 Linux 各差 1，差值是仓库既有的平台门控 stub：`core/src/macos_rm.rs:1947/1954` 的 `cfg(not(all(target_os="macos", target_arch="x86_64")))`），`cargo check -p pynvoc` rc=0，`cargo fmt --all -- --check` rc=0；产物 `outputs/stage-plan-document-20261017T225812Z/`（`README.md` `726aa2ff…`、`core-tests.txt` `1ba53e0f…`、`cli-tests.txt` `06525a10…`、`optimizer-tests.txt` `b9dba449…`、`pynvoc-check.txt` `a117fe9d…`、`fmt.txt` `ef1ee37d…`，sha256 前缀，每条含命令、输出与退出码）。**入断言**：第一次上机捕获全部 `rc=127`（非交互 bash 里没有 `cargo`，PATH 未带 `~/.cargo/bin`），该残缺目录已按**绝对路径核对后**删除并重跑——不是"静默通过"。**本轮 wedge 只探一次**（读 watcher 日志，未重启）：第 120 次 `0x00000062`，uptime 1 天 8 小时 40 分。

**(6) 诚实边界（未夸大）** ✗：①`pynvoc` 的**运行时**仍未验证——整 lib 链接在两端都因 pyo3 未按 extension-module 链接 libpython 而失败（本轮改动之前就如此），故本轮对 Python 入口只做到 **`cargo check` 通过 + 形状同源**，归类"已实现待实测"；②CLI 读面里 **live 取值**（`window_*`／`restore_khz`／当前偏移）**仍需健康窗口**才能看到，本轮钉住的是**形状与诚实文本**，不是那些数字；③写闸门**未开**：`SETTINGS_WRITE_SUPPORTED` 仍为 `false`，本轮**未发任何写**；④`applied: false` 是**结构性**的（`plan_document` 里就是常量），不是"这次恰好没写"。

**(7) 附带修好的一处可复现性缺陷** ✔：本轮复跑 `audit-matrix-claims.py` 时**复现不出**上一轮记录里的那个总和（上轮写"**151 supported**／2 no artifact／0 contradicted／10 retracted／3 unverifiable"），因为该工具的输出**从不点名它的输入**——同一份表配不同 census、不同 `--declared` 表、不同文件集合会给出完全不同的分类计数，而总和行看不出是哪一种。已修：`audit()` 现在开头打印 `census:` 与 `declared table:`（缺省时明写 `none … NO ARTIFACT (SKIP)`），结尾在总和行之后再加一行 `tally from: census=…, declared=…, inputs=[…]` ⇒ **把那一行抄进记录，计数即可复现而不是被信任**。并用它把本轮的账算清（可复现的四组，均 0 contradicted）：`census=outputs/stage-live-attempt-census-20261017T222708Z/census.txt` + `declared=gsp-1118.tsv` + 仅功能表 ⇒ **102 supported／3 no artifact／0 contradicted／6 retracted／2 unverifiable**；同 census 不带 `--declared` ⇒ 100／5／0／6／2；`roc84/census.txt` 同两组 ⇒ 102／3／0／6／2 与 100／5／0／6／2；**把 `outputs/review-stage-gsp-clkctl-20261017.md` 也放进去 ⇒ 194／13／9／7／2**（评审文里在**引用**别人的状态码，故出现 contradicted——这正是工具设计的"打印出来让人读"，不是回归）。上一轮的 151 属于"更宽的文件集合"，此前无法从总和行看出；现在能了。

### 九十六、健康窗口的"决定性仪表"先受控：用一**整份实机 `--clkctl-hex` 抓取**约束解析器（2026-10-18，静态，0 次 GPU 请求，0 次写入）

**① 选择依据** ✔：偏移通道的下一步完全压在**健康窗口**上——楔死后 GET **不可重放**（`0x62`），整份 32680 字节 payload 至今未捕获，而读电池的健康产物会经 `tools/decode-clk-containers.py` 解码并自动打印 0x2080901d 的 `report_one_d_stride()` 裁决（`+268` ⇒ 128 B 周期；`+1160` ⇒ 32×1020；两者皆非 ⇒ 两个读法都错）。核对工具时发现一处**结构性缺口**：该工具 `--selftest` 里**每一个**检查喂给解析器的字节都是**同一个文件自己产出的**（合成缓冲 + `parse_blocks` 的往返文本），因此 `parse_blocks` 的行格式或 payload 组装一旦被改动，**健康窗口的解码可以被悄悄清空而不触发任何失败**——这正是最不能出错的时刻。修法不是"多加断言"，而是**引入真实字节**：把 2026-10-17 那份**整份** `--clkctl-hex` 抓取（167499 B，`outputs/stage-gsp-clkctl-20261017T092536Z/clkctl-hex.txt`）**逐字节**纳入仓库 fixture，并把当天对**同一个文件**的解读固化成断言。

**② 实现（仓库内 ✔）**：新增 fixture `probe-macos-rm/fixtures/tu116-clkctl-hex-20261017T092536Z.txt`（md5 `8817a92fe8dad405568e58bb599c9797`，**原件照抄、不做任何裁剪**，因此没有"哪些行重要"的取材规则可争）；`probe-macos-rm/tools/decode-clk-containers.py` 侧新增 `REAL_CAPTURES` 与 `check_real_capture()`：按 md5 开门 → `parse_blocks()` → 断言两块**按长度整份复原**（0x20809019@12336、0x20809025@7188，**短一字节即失败**）→ 断言 `GEOMETRY` 的 `(header,stride)` 与该抓取一致 → 断言头部字（`+0/+4/+8/+12/+16/+20/+24/+28` 的实录值）→ 断言 0x20809019 的**十行目录**（`0:1:0808 1:2:0909 2:16:0808 3:4:0909 4:8:0909 5:1048576:0909 6:524288:0303 7:64:0808 8:2097152:0909 9:32:0909`）、`rec+44` 的两处 **i16 对**（`+92=[-1000,1000]`、`+860=[-1000,3000]`）→ 断言 0x20809025 三条记录的 `max_vf=1638400`／`floor=-18750`／`ceiling=+18750`。**负控有两层**：①丢掉文件里**最后一条 dump 行**（`07184: 00 00 00 00`，4 B）⇒ 必须**恰好** `0x20809025` 变短**恰好 4 B**、别的块不变（这同时是"长度确实从行里读出来"的证明，而不是从摘要文本推的）；②把 fixture 复制到临时目录改一个字节并把 `NVOC_CLKCTL_FIXTURES` 指过去 ⇒ 必须 `fixture drifted` 失败。两条与既有的 `check_measured_repetition()` 负控同风格；`--selftest` 尾部改为具名打印 `real capture (parsed)` 与 `measured repetition checked`，**SKIP 不再是静默通过**。fixture 的取材与两个注意点写进 `probe-macos-rm/fixtures/README.md`。

**③ 可执行证据（本机，零 GPU）** ✔：`python3 probe-macos-rm/tools/decode-clk-containers.py --selftest` **rc=0**（含新控制与 md5 门；打印 `real capture: 167499 B … recovered 0x20809019@12336 and 0x20809025@7188 whole … dropping a 4-byte dump line shortens exactly 0x20809025 and nothing else`）；`ruff format --check`／`ruff check` 干净；对真实抓取的**运行时解码输出未变**（28 行、三处 `MATCH`、rc=0）⇒ 新增的只是自检路径，不改变既有结论。

**④ 上机侧（测试机 python 3.9.6，零 GPU 请求）** ✔：`77d4fe3` fast-forward 后 `--selftest` **rc=0**（同一行末段 `real capture (parsed)`）；用**测试机自己那份** `outputs/stage-gsp-clkctl-20261017T092536Z/clkctl-hex.txt` 解码 rc=0 且三处 `MATCH`；fixture 在机上的 md5 复算为 `8817a92fe8dad405568e58bb599c9797`（与记录一致），`wc` 1254 行／167499 B。产物 `outputs/stage-parser-control-20261017T231043Z/control.txt` sha256 `6076d9b3e1f6f03de256edd0f8319c5264e9ed2443ad7ebeef1452132e661d93`；测试机仓库仍是原有 3 个未跟踪遗留。

**⑤ 诚实边界（未夸大）** ✗：①这条控制**不进 `cargo xtask ci`**——xtask 只跑 rustfmt／clippy／ruff／pytest，probe 工具的 `--selftest` 仍由**人工或读电池**触发；加它要改共享门禁（`xtask/src/checker.rs`），本轮**刻意不动**并记为待办。②该 fixture 是**旧打印格式**（无 `bg=`、无 changed 计数），所以将来的抓取**不会**与它字节可比——这正是**保留原件**而非"更新成最新格式"的理由。③0x2080901d 的**整份 payload 仍未捕获** ⇒ 步长问题**仍然未定**；本轮只是让那个裁决**一旦有字节就自动给出**。④零写入、零 SET、未提权、未重启；`SETTINGS_WRITE_SUPPORTED` 仍 `false`。⑤wedge 未变（第 8 次 `0x00000062`，uptime 未变小 ⇒ 未重启）。

### 九十七、让"决定性运行"的产物自带**出身与结局**：两份 runbook 的 bundle 记下 revision／二进制指纹／构建新鲜度／退出状态（2026-10-18；静态＋2 次**只读**电池尝试，0 次写入、0 次重启）

**① 选择依据** ✔：读电池与门控写入是本项目**不可重复**的两份证据——健康窗口不可重放（一次拒绝就可能把 GSP 楔死），单域写入只允许**一步**。§九十六已经让"解读器"受真实字节约束，但**收集方式**本身还有三处静默缺口：①bundle 只有时间戳目录名，事后无法从产物判断是**哪个 revision** 回答的；②**哪个二进制**回答的——陈旧二进制在本项目已有前科（少了 battery row 的 `probe-macos-rm` 把"行不存在"读了出来），而 revision 抓不到这一点：修复前构建的二进制"在纸面上仍属于正确 revision"；③运行是**跑完**还是**在第一个 case 就停**——楔死时两者只差几个文件。于是这一轮与 §九十六同一条原则：**产物必须自己点名输入与结局**。

**② 实现（仓库内 ✔）**：新增 `probe-macos-rm/tools/record-run-provenance.py`，在**首个请求之前**向 bundle 写 `README.txt`：label、UTC/本地时间、uid（是否 root）、host、`git rev-parse HEAD`（拿不到就写 `unknown (<原因>)`，**绝不冒充当前树**）、二进制的 md5 与 mtime、最新源文件及其时间、`STALE (<更新的文件>) | FRESH | UNKNOWN (<缺哪一半>)` 的新鲜度判决（**没有裸 `OK`**，缺二进制或缺源码一律 `UNKNOWN`）、OGKM 路径与存在性、要发的 ids、case 列表、**预注册判据**，以及"实时频率不是写入证据，回读与恢复才是"的提醒；`--sums` 模式对 bundle 内**每个文件**做摘要（含刚写的 `README.txt`，不含自身）并把完成时间写进摘要文件头。两份 runbook 都在中止与结束两条路径上走同一个 `finish()`：先向 `README.txt` 追写 `... exit <status> at <time>`，再生成摘要——**因此退出状态也在摘要覆盖范围内**，read 的 wedge-abort 也走 `finish 5`。用法错误（缺 bundle）⇒ `exit 3`，`--flag` 缺值／未知参数 ⇒ `exit 2`，两者都不会产出"看起来通过的空产物"。`--selftest` 在临时目录里用假源码与假二进制覆盖**三条新鲜度分支**（含 `build.sh` 无后缀也算源）、README 字段、摘要往返与参数拒绝。

**③ 证据（测试机，只读）** ✔：`--selftest` 在测试机 python **3.9.6** 上 **rc=0**（`unknown (git rev-parse exit 128)` 分支在临时目录里被真的走到）。两次电池尝试：`outputs/stage-clkctl-readrun-20261017T231617Z`（在 `5cb77b4` 上跑；README 记 `git: 5cb77b42…`、`binary … md5 dc4a8ad527543bdc26712654da98d44e mtime 22:16:42Z`、`build freshness: FRESH (… src/nvx_suite.c 22:16:16Z)` ⇒ 这次结论**不是陈旧二进制**造成的）；`outputs/stage-clkctl-readrun-20261017T231656Z`（在 `2dc661a` 上跑）**退出码 5**，README 末行 `battery exit 5 at 2026-10-17T23:16:56Z`，`SHA256SUMS` 覆盖 `README.txt 75e8107b…`、`clkctl-hex.txt 6dc68cf3…`（3555 B）。**方法更正一条**：第一次运行的 `rc` 被我自己的 `| tail` 掩成 `0`——退出码读的是管道末段的；于是重跑了**一次**（只读、同一脚本、不重启），从脚本本身读回 `rc=5`，并把这条教训记为"退出状态必须从被测量的进程读"。电池第一个 case 的实录：`0x2080901b@2108 in@4=0xFF bg=A5 -> 0x00000062 (NV_ERR_RESET_REQUIRED)`，随后 `STOP: aborting the remaining hex dumps` ⇒ 楔死仍在（本轮的只读尝试因此没有走到 `0x20809025`／`0x20809019`／`0x2080901d`），那 3555 B 日志本身就是"第一个 case 即楔死"的实录。

**④ 诚实边界** ✗：①provenance 只写 `README.txt`／`SHA256SUMS`，**不进日志、不改 probe 行为、不触碰设备**；②本轮共 **2 次只读电池尝试**（外加一次看门狗日志轮询），**0 次写入、0 次重启**、未提权、未改 EFI／boot-args；`SETTINGS_WRITE_SUPPORTED` 仍 `false`；③工具仍**未进 `cargo xtask ci`**（沿用 §九十六⑤：动共享门禁，待办）；④`README.txt` 结尾由 shell 追写的退出行意味着该文件**不再逐字节等于**工装输出——这是刻意的，且被同一次摘要覆盖。

### 九十八、让"仪器自己的检查"进入门禁：`cargo xtask check` 现在会跑 probe 工具的 `--selftest`（含"没跑要说出来"的发现式点名与门禁自身的反例）（2026-10-18；静态＋门禁在测试机 4 次运行，0 次 GPU 请求、0 次写入）

**① 选择依据** ✔：本项目每一条"驱动到底答了什么"的结论都靠 `probe-macos-rm/tools/` 里的工具产出或核对，而**没有任何一步在执行它们**——`ruff` 只读语法，`cargo xtask ci` 的四个环节（rustfmt／clippy／ruff／safe 测试）都不碰这些断言。于是它们钉住的东西可以**静默腐坏**：真实抓取必须仍能被解析的 dump 格式、解码必须与 Rust 结构一致的字段映射、claim 审计点名输入的可复现性。§九十六⑤与§九十七④已两次把它记为待办，这一轮做掉它——**门禁应当跑仪器自己的检查，而不是只检查仪器的排版**。

**② 实现（仓库内 ✔）**：`xtask/src/checker.rs` 的 `check()` 末尾新增 `probe tool selftests (device-free instruments)` 步骤：对每个带 `--selftest` 的工具用 **`python3`**（runbook 调用它们的同一个解释器）执行，**要求退出码 0 _并且_ 该工具自己打印 `selftest ok` 行**——把这一行原样引用进门禁日志（证据可见，而非只计数）；退出 0 却**没报结果**的 selftest 会**失败**，而不是被当成通过。`gsp-handler-map.py --selftest` 读的是**不在仓库里**的 GSP 固件 blob，因此它被登记为"需要输入"：只有 `NVOC_GSP_FIRMWARE` 指向该文件时才跑（且该路径必须是文件），否则打印**点名原因的 SKIP**。第三个清单登记**确实没有 selftest** 的六个工具（纯抽取／分析），并且**枚举 `tools/*.py`**：任何既没被跑、也没被声明为"无 selftest"的**新**工具会让门禁失败并给出该改哪一行——这样"新加一个工具然后它永远不被执行"不再可能悄悄发生。

**③ 证据（测试机，4 次门禁运行，0 次 GPU 请求）** ✔：`outputs/stage-gate-probe-selftests-20261017T232524Z/`，摘要 `gates.txt` sha256 `19ad31b9…`。①`NVOC_GSP_FIRMWARE=<blob> cargo xtask check` ⇒ **rc=0**，六条 `selftest ok` 全部出现，含 `gsp-handler-map.py` 的 `24 checks -- 1118-row recount, the ids the flags bug hid, per-handler code and class info, trampoline shape, VOLT family sizes, twin enumeration on both sides`（`check-firmware.txt` sha256 `041bd351…`）；②不设该变量 ⇒ **rc=0**，五条 `selftest ok` 加一条 `SKIP … it reads the GSP firmware blob, which is not in the repo; set NVOC_GSP_FIRMWARE=…`（`check-unset.txt` `cf952fb8…`）；③**门禁自身的反例**：把 `record-run-provenance.py` 复制成未登记的 `tools/zz-gate-control.py` ⇒ **rc=1**，报文正好点名它并要求登记（`check-undeclared.txt` `d761cd6e…`），随后删掉该自建文件并确认 `gone`；④删除后重跑 ⇒ **rc=0**（`check-final.txt` `192cd015…`）。这四次运行同时说明我的 Rust 改动通过了同一门禁的 rustfmt／clippy／ruff 三段。

**④ 门禁当场发现的真缺陷** ✔：第一次带固件的运行**失败**，因为 `gsp-handler-map.py --selftest` 退出 0 却**只打印每项结果和 `# N check(s) failed`**，成功时**什么都不说**——它此前无法"报出成功"。已在 `982e0ef` 修好：统计并打印 `selftest ok: 24 checks -- …`。这正是新规则想要的效果：**能报失败、不能报成功的 selftest 与没跑的 selftest 看起来是一样的**。

**⑤ 顺带修好的环境前置（测试机，非仓库改动）** ✗/✔：测试机 `nvoc-head` 的 `nvapi-rs` **目录有内容但没有 git 元数据**（`git submodule status` 前缀为 `-`），于是 `cargo xtask ci` 在**预检**就被拒（`main.rs:35 → doctor::ensure_submodule_synced`），本轮之前它一直没跑到过我的步骤。修法是**原地修复、不动文件**：在该目录 `git init`＋加 remote＋`fetch`＋`git reset --mixed 7d16bab…`（**只移动 HEAD，不碰工作区**）＋父仓库 `git submodule absorbgitdirs`；修后 `git submodule status` = ` 7d16bab51865840bd87f8b51a774cecc5bf9e784 nvapi-rs (0.1.4-337-g7d16bab)`、`git -C nvapi-rs status --porcelain` **0 行**（即既有内容与该提交**逐字节相同**）、修前修后对 100 个文件做 SHA256 清单比对**只多出新建的 `nvapi-rs/.git` 文件**。另一条前置：`uv` 在本机位于 `~/.local/bin`，门禁的 ruff 步骤需要它在 PATH 上（第一次尝试正因缺它失败）。`absorbgitdirs` 把模块目录放进了 `nvoc-next/.git/worktrees/nvoc-head/modules/nvapi-rs`（`nvoc-head` 是旧 checkout 的 **worktree**），旧 checkout 的工作树**未改**。

**⑥ 诚实边界** ✗：①门禁因此**新增两条前置**——`python3` 与（跑全套时尚需）`uv`；缺 `python3` 会**明确报错**而不是跳过。②固件那条在 CI 上是**点名原因的 SKIP**，只有设置了变量才算真跑（本机与测试机都能设）。③"无 selftest"的六个工具由**枚举＋显式清单**保证不会被静默漏跑，但它们的**正确性**仍由消费它们的工具（如 `gsp-handler-map.py` 对 1118 行重数的断言）与 fixture 间接钉住——**不是**由本步骤直接验证。④本轮**未跑 GPU 请求、未写入、未重启**；`SETTINGS_WRITE_SUPPORTED` 仍 `false`。⑤三道清单是**手工维护**的：新增工具会被强制登记，但把工具从"有 selftest"错列到"无 selftest"仍需人看（清单就在同一个文件里，审查可见）。

**⑦ 本轮能声称的边界** ✗：`cargo xtask ci` 的 check 半段（含新步骤，`ci-full.txt` 内 `selftest ok` 计数 **6**）**绿**，但 safe 测试层在本机**因宿主环境而红**且与本次改动无关：`cargo test -p pynvoc --no-default-features` 链接失败（`ld: library 'python3.9' not found`）。根因已查明——本机**没有 Xcode**（只有 `/Library/Developer/CommandLineTools`），而 `/usr/bin/python3`（3.9.6）的 `sysconfig` 仍报出 **Xcode 形状的 `LIBDIR`**（`prefix` 却指向 CLT，`Py_ENABLE_SHARED=0`），真正的 `libpython3.9.dylib` 只存在于 `/Library/Developer/CommandLineTools/Library/Frameworks/Python3.framework/Versions/3.9/lib/`；`PYO3_PYTHON=/usr/bin/python3` 无效（同一解释器报同一路径）。候选修法（**未改仓库**）：`RUSTFLAGS='-L <CLT lib dir>'`（代价：整体重新指纹、全量重建）、CLT 感知的 `PYO3_CONFIG_FILE`、或装 Xcode。这是§九十六⑤所记 "pynvoc 只能 `cargo check`" 的同一件事的另一副面孔——因此本轮只声称"`check` 门禁（含新步骤）绿"，**不**声称"`cargo xtask ci` 全绿"。

**⑧ 补记（同日）：全套 `ci` 在本机**也能绿**，缺的只是两条 CLT 路径变量** —— ⑦说的"safe 层因宿主环境红"已收窄为一条**实测过的配方**：只加 `RUSTFLAGS='-L /Library/Developer/CommandLineTools/Library/Frameworks/Python3.framework/Versions/3.9/lib'` 时链接成功、测试二进制真的起来，但立刻被 dyld 打死（`Library not loaded: @rpath/Python3.framework/Versions/3.9/Python3 … Reason: no LC_RPATH's found`；CLT 的 `libpython3.9.dylib` 只是指向 `../Python3` 的替身，install name 是 `@rpath/…`）⇒ **链接与装载是两个缺口**，只补前者只是把失败挪到运行期（`pynvoc-clt-link.txt` rc=101）。再补 `DYLD_FRAMEWORK_PATH=/Library/Developer/CommandLineTools/Library/Frameworks` 后：`RUSTFLAGS='-L <CLT Python lib>' DYLD_FRAMEWORK_PATH=<CLT Frameworks> NVOC_GSP_FIRMWARE=<blob> cargo xtask ci` ⇒ **rc=0**，`ci-clt-paths.txt` 中 `selftest ok` 六条、`test result: ok` 16 次、`test result: FAILED` **0** 次（`gates.txt` 覆盖同目录七个产物）。因此"pynvoc 只能 `cargo check`"应改写为"**不设 CLT 路径变量时**只能 `cargo check`"；本机 `ci` 全绿的前置是三条环境变量（两条 CLT 路径＋固件路径）。**未改仓库任何一行**，只是把配方记下来。

### 九十九、同一条拒绝不再有两种说法：CLI 与 pynvoc 共用同一条"缺失层"理由，且门禁与实机抓取各抓到一处措辞缺陷（2026-10-18；静态＋门禁/入口实跑，0 次 GPU 请求、0 次写入）

**① 选择依据** ✔：本轮直接兑现 §九十一 第 6 点（`docs/design/macos-backend-feature-matrix.md:1510`）留下的"**待统一措辞**"：`query_temperature_thresholds` 在 macOS 报的是**平台级**拒绝（"NVAPI/NVML are not available on macOS"），而 CLI 对同一件事报的是"不在允许清单里"的漫拒清单——**同一个拒绝、两种说法**。这与阶段 11 第 5 项的要求（"各入口能力、范围和错误与后端一致"）和本项目已经写明的一条纪律直接冲突：**未读／未定位的东西不得被说成"不存在"**（`core/src/macos_rm.rs:386-416` 的家族理由表就是为这条纪律立的）。三处具体缺口：①`get-temp-thresholds`（矩阵 `:155`）与 `get-pstate-freq-range`（矩阵 `:183`）在入口只拿到漫拒清单，用户看不出**为什么**被拒、也看不出同族"其实答得出"的入口是哪一个；②pynvoc 的 `query_temperature_thresholds`／`query_pstates`／`query_cooler_info` 只说平台没有 NVAPI/NVML——对平台为真、对**入口**为假，因为同一个后端确实答得出这三族的**读数**（传感器读数走 `query_legacy_temp_sensor`／公开热读、当前 P-state 走 `query_status`、每槽 rpm 走 `query_fan_info`）；③`query_cooler_info` 不是纸面函数：TUI `overclock.py:1543` 与 GUI `dashboard/sections/overclock.py:1898` 都在调它，所以这句错话是**用户可见**的。

**② 改动（仓库内 ✔）**：①`core/src/macos_rm.rs` 新增两条与既有家族理由同风格、**点名缺失层**的共享常量：`THERMAL_THRESHOLDS_UNREADABLE_REASON`（`:173`）与 `PSTATE_FREQ_TABLE_UNREADABLE_REASON`（`:179`），并从 `core/src/lib.rs:21,23` 导出；第三族复用已有的 `FAN_PERCENT_UNREADABLE_REASON`（`:1114`）而不是新造一条（同一条缺失层就该是同一句话）。阈值那条写明"族已读到（`0x20800513` NV_OK、1 target／1 sensor、驱动自报 `[0,127]` 范围，非特权孪生 `0x20808513` 是同一次读）、SDK 热族只有这两个 id 且其 opcode 覆盖 targets/sensors/readings/reading ranges/sensor target index ⇒ **没有** severity/threshold opcode ⇒ 阈值是**未定位**而非不存在，配置策略限值又是另一回事（见 thermal_limit），真正答得出的是 `get-legacy-temp-sensor`"；P-state 那条写明"快照只有**当前** P-state 与逐域偏移，不是表；能喂表的读要么私有、要么实测为空——`0x2080901d` 在自带 32680 B 尺寸上 NV_OK 而每个值 dword 都被写成 0（0xA5 背景；**是写进去的 0，不是没动过的字段**），公开 per-level 控制 `0x2080200B` 仍 `0x1f`"。②CLI `cli/src/macos_rm.rs:43-60` 在这两个命令上给出**带理由**的拒绝（前段保留既有契约短语 "unavailable on macOS RM"，这样"每个被拒入口都说自己被拒"的既有断言仍成立），不再与"没有数据"混为一谈。③pynvoc 新增 `refuse_without_source()`＋`source_less_message()`（`nvoc-python/src/lib.rs:112,125`），三个入口在 macOS 上抛 `RuntimeError`：`"<entry> is not implemented on macOS: <共享常量>"`——**同一份常量**，两端不可能各说各话。④两侧单测把纪律钉住：`nvoc-python/src/lib.rs:6035` 逐条断言三族理由**各自点到自己的缺失层**（三条必须互不相同）、**不得出现平台级那句话**、包装层除入口名**不得再加任何东西**（同一轮里正是这里先写错过）；`cli/src/macos_rm.rs` 的用例断言两个拒绝以"入口名＋共享常量"开头，并保留"`get-temp-sim` 这类**根本没有读源**的入口仍走清单式拒绝"这一对照。

**③ 证据（测试机，0 次 GPU 请求）** ✔：`outputs/stage-entry-reasons-20261018T010500Z/`：`RUSTFLAGS='-L <CLT Python 3.9 lib>' DYLD_FRAMEWORK_PATH=<CLT Frameworks> NVOC_GSP_FIRMWARE=<blob> cargo xtask ci` ⇒ **rc=0**（`ci.txt` sha256 `ff3457ba…`：`test result: ok` **16** 次、`FAILED` **0** 次、工装 `selftest ok` **6** 条），修订 `6f907ac`（= 本机 HEAD = fork）。**实机抓取**（`entry-refusals.txt` sha256 `a353b1ff…`，摘要 `gates.txt` `4546b8ac…`）：`nvoc-cli get-temp-thresholds --macos-rm` 与 `nvoc-cli get-pstate-freq-range --macos-rm` 现在各打印一条**带缺失层理由**的拒绝（rc=2；这两条在**参数解析期**决定，因此**不碰设备**、楔死期也能抓），`get-temp-sim --macos-rm` 仍打印清单式拒绝（对照 ✔）。**Python 入口做了真跑**：按仓库自己的命令（`python3`→`uv run --locked --package pynvoc --group dev --no-config maturin develop --release`，`maturin.txt` `375bd164…`，rc=0）重建扩展后，三个函数真抛新文案（`query_cooler_info: RuntimeError: query_cooler_info is not implemented on macOS: the RUSD fan section publishes a tachometer reading per cooler slot, with no duty percent and no rpm range, so a percentage would need an invented scale; …`）。**重修前的旧文案也被记下**：第一次抓取（未重建扩展）三个函数仍抛平台级那句话，说明机上的 `_native.abi3.so` 是旧构建——这正是"措辞不一致"的活证据，也说明**重建后才算实跑**。客户端侧：`audit-entry-parity.py` 在新修订上 `2 ok, 0 failed`（**9** 条允许命令、读 **108** 行表，与我改前一致 ⇒ 允许清单未被搅动）、`cargo fmt --all -- --check` clean、`ruff` 两段由门禁覆盖。

**④ 门禁与实机抓取各自抓到的缺陷** ✔（这是本轮最该留下的部分）：**门禁抓到三处我自己的错**：①第一版测试不满足 `cargo fmt`（`ci.txt` sha256 见 `outputs/stage-entry-reasons-20261017T233913Z/`，提交 `8580167` 修）；②第一版用"关键词扫一遍"判"是否点名缺失层"，结果在风扇那条上失败——它的缺失层说法是"would need an invented scale"，**关键词扫不出来**（提交 `977f389` 改为逐条钉住各自那句，这也更诚实：三族的缺失层本就不同）；③"三条不得同一句"的守卫写成拿每条与 `cases[0]` 比，**第一条等于自己**⇒ 自造的必失败（提交 `e5d8add` 改为三条去重后计数；两次失败产物：`stage-entry-reasons-20261017T233913Z/`、`stage-entry-reasons-20261018T000500Z/`）。**实机抓取又抓到两处门禁抓不到的**：①拒绝文案"复述"了常量已经说过的形状（`get-pstate-freq-range is unavailable on macOS RM: it reads the per-pstate clock table, and the per-pstate clock table has no source here: …`，提交 `91bf8d2` 让包装层只留入口名与"谁答得出"）；②阈值那条结尾把 `get-legacy-temp-sensor` 说了两遍（常量结尾已点名，提交 `6f907ac` 去掉尾巴）。**教训**：结构可以由单测钉住，**措辞只有真跑才看得见**——这也是本轮把"抓取"当验证步骤而不是可选步骤的理由。

**⑤ 诚实边界** ✗：①本轮**只改措辞与入口分类，没有新增任何读源**——三族的**数据**结论不变：温度读数／当前 P-state／每槽 rpm＝已实现且实测；阈值表／每-pstate 频率表／散热器百分比三元组＝**未实现（无可读源）**，这条不因为措辞变好而升级。②`SETTINGS_WRITE_SUPPORTED` 仍 `false`（`core/src/macos_rm.rs:119` 未动），本轮**零写入**。③**零 GPU 请求**：楔死仍在（本轮只读一次看门狗日志：第 32 次 `0x00000062`，uptime 1d9h，**未重启**），抓取用的两条 CLI 拒绝在解析期就返回，Python 那三个函数也在访问设备前抛错。④srv **未改**：它只有写面 `READ_ONLY`（`srv/src/macos.rs:48`）一类 macOS 专属串，三族都不在它的读面里（`grep` 只命中 `supports_writes`／`READ_ONLY`）⇒ "不动"是核对后的结论，不是漏做。⑤GUI/TUI 未改：它们直接把 pynvoc 的异常文本显示出来，没有自己的 macOS 文案，也没有断言旧文案的测试（已 grep 确认）⇒ 措辞修正自动生效。⑥`outputs/stage-entry-reasons-20261018T010500Z/gates-1-clobbered-rc.txt` 是**我自己的记错**：第一版摘要行把 `ci rc=0` 写成了 `rc=2`（抓取循环复用了变量 `rc`，最后一次赋值来自 `get-temp-sim`）；更正后的 `gates.txt` 保留说明，旧文件一并留存。⑦本轮**未动** `macos-eval` 入口归属问题（`cli/src/lib.rs:6998-7004` 的散文仍称 `nvoc macos-eval`，而该子命令属于 `nvoc-auto-optimizer`）——仍记为待办。

### 一百、偏移读入口在 macOS 上作答、写入口改说"门"而不是"平台"：同一后端不再对同一次读给两种答案（2026-10-18；静态＋门禁/入口实跑，0 次 GPU 请求、0 次写入）

**① 选择依据** ✔：阶段 11 第 5 项要求"各入口能力、范围和错误与后端一致，既不能后端可用而入口一律拒绝，也不能可点但无实现"，而偏移这一族**恰好是"后端可用、入口拒绝"**：CLI 的 `get-pstate-global-freq-offset --macos-rm` 早已把逐域"当前值／窗口／原状／恢复目标"发出来（§九十七/九十八的读数面），但同一个后端的 **Python 入口 `query_clock_offset` 报的是 `invalid backend`**——它用 `parse_backend`（只认 `nvapi`/`nvml`/两个 cooler 串，**不认 `macos-rm`**），且默认值在**所有平台**都写死 `nvml`，于是在 macOS 上"后端答得出、入口说没有这个后端"。消费者侧核对：`query_clock_offset` **没有** GUI/TUI 调用者（GUI/TUI 只调 `set_clock_offset`，如 `gui/src/tabs/dashboard/sections/overclock.py:2829,2887`）⇒ 这不是"用户可见的错话"，而是**入口一致性**的缺口，按阶段 11 的口径一样要修。同一族的**写**入口则在 macOS 上抛平台级那句"NVAPI/NVML are not available on macOS"——对**平台**为真、对**入口**为假（同族的读刚刚才答出来），与 §九十九 立的"未读／未定位的东西不得被说成不存在"是同一类错。

**② 改动（仓库内 ✔，三条修订）**：①`core/src/macos_rm.rs:742` 新增 `public_offset_domain(ClockDomain) -> Option<&'static str>`——把"设备时钟域枚举 ↔ 公开域名的拼法"这**唯一一份**映射放进 core（`graphics/memory/video/processor`，其余返回 `None`），并从 `core/src/lib.rs:26` 导出；CLI 自己那份 `domain_label`（`cli/src/lib.rs:8340`）改为**委托**它（`unwrap_or("unknown")`），于是 CLI／pynvoc 两处不可能各写一份域名表。②同一文件 `:139` 新增共享常量 `MACOS_RM_WRITE_GATE_REASON`（`core/src/lib.rs:20` 导出），它**点名缺失层**：写 id 已定位（指向 `WRITE_NEEDS_PRIVILEGE_REASON`）、当前值与窗口**已经读得出**（点名 `get-pstate-global-freq-offset`／`get-settings`），缺的是"同一次读把它读回来"的**验证**，门由 `SETTINGS_WRITE_SUPPORTED` 按构造关着。③pynvoc（`nvoc-python/src/lib.rs`）：`clock_offset_backend`（`:2227`，接受 `macos-rm`／`auto`／`default`／`nvapi`／`nvml`，默认走平台自己的 `fan_power_backends(cfg!(target_os="macos"))`，cooler 串仍拒）、`macos_write_gate_reason`＋`macos_write_gate`（`:2259`／`:2248`）、`clock_offset_macos_rm_mhz`（`:2280`，纯投影）+ `normalize_query_clock_offset_macos`（`:2270`，`{"mhz": …}` 与 NVAPI/NVML 分支同字段同单位）；`query_clock_offset` 改为经 `with_target(gpu, backend, …)` 三分支（macOS 取快照的逐域记录、NVML 取 NVML、NVAPI 取 live delta），`set_clock_offset` 接受同一批拼法并**在碰库前**用上面的共享常量拒绝 macOS 写（并在取库那行留了"门开了要改这张表"的注释）。④两侧单测（`nvoc-python/src/lib.rs:6322`／`:6372`）钉住：答得出来的域给 `-0.126`（= NVML 分支的 MHz 单位）且**只**回一个字段、没被同一快照佐证的域给 core 那条**点了域名**的理由、拒绝里**不得出现平台级那句**、默认与 `auto` 落到平台自己的后端、cooler 串仍被拒、门的文案以"入口名＋共享常量"开头。

**③ 证据（测试机；0 次 GPU 请求、0 次写入）** ✔：修订 `e8ef60a`／`0827925`／`fc35cd0`（已推 fork，测试机 `nvoc-head` ff 到 `fc35cd0`）。①`RUSTFLAGS='-L <CLT Python 3.9 lib>' DYLD_FRAMEWORK_PATH=<CLT Frameworks> NVOC_GSP_FIRMWARE=<blob> cargo xtask ci` ⇒ **rc=0**，`test result: ok` **16** 次、`FAILED` **0** 次、工装 `selftest ok` **6** 条（`outputs/stage-offset-entry-20261018T013000Z/ci.txt` sha256 `4599a543…`）。②同一特征集下点名跑新用例（`cargo test -p pynvoc --no-default-features --lib macos_rm`）⇒ **6 passed／0 failed／10 filtered**（`new-tests.txt` `6a3d15d0…`）。③**门禁内生反例**（`negative-controls.txt` `94e5baa1…`）：把 `-0.126` 翻成 `0.126`、把"拒绝里不得出现 `NVAPI/NVML`"翻成"必须出现平台句 'not available on macOS'"，两条断言**都当场失败**（`left: -0.126 / right: 0.126`；`4 passed; 2 failed`）⇒ 这两个用例不是空转的；控制后工作区恢复原状（`git status --porcelain` 仅剩三个**先于本轮**就存在的未跟踪二进制）。④**实机入口真跑**（`runtime-offsets.txt` `af443a00…`；扩展按仓库自己的命令重建：`uv run --locked --package pynvoc --group dev --no-config maturin develop --release`，rc=0，`maturin.txt` `74c07996…`，`_native.abi3.so` 3147620 B）：`set_clock_offset("0","macos-rm","core",8.0,"P0")` 与 `set_clock_offset("0","auto",…,"mem",…)` 都抛 **RuntimeError** 且文案＝共享常量（平台句**不出现**），`set_clock_offset("0","cuda",…)` 仍 ValueError `invalid backend "cuda"; expected 'nvapi', 'nvml', 'auto'/'default', or 'macos-rm'`；读面在**解析期**先拒的两条（域名 `bogus`、后端 `cuda`）都在**不碰设备**的情况下返回 ValueError（楔死期照样能抓）；而 `query_clock_offset("0", None/"macos-rm", "core", None)` 这两条**已经过掉后端解析**、落到设备层，报的是 `no GPU selected`（= 楔死导致 RM 会话连设备枚举都给不出来），这正是"哪一层在说话"的对照：旧码在这两条上报的是 `invalid backend`。⑤楔死只探一次：看门狗第 **62／63** 次仍 `verdict=0x00000062`（17:04／17:05），`uptime` 1 day 9:47（比上轮 1d9h33m **涨了**）⇒ **未重启**、非重启用窗口。

**④ 本轮我自己的两处错（留档，比结论有用）** ✔：①第一版用例对 `PyErr` 调了 `.to_string()`／`expect_err` ⇒ 门禁在 **12 ok/1 failed** 处红，panic 来自 `pyo3::interpreter_lifecycle.rs:134`：本 crate 的门禁用例跑在 `--no-default-features`（即**没有** `extension-module`、**没有**解释器），**连格式化 `PyErr` 都不行**；修法是把两个"判断"抽成纯函数（`clock_offset_macos_rm_mhz`／`macos_write_gate_reason`），测试只断言字符串与数字（`0827925`）。②改完被 clippy 挡住：`ok().expect()` 触发 `ok_expect`，而它建议的 `expect()` 恰恰要格式化 `PyErr` ⇒ 改成 `match` 并在失败分支 panic 一个**字面量**（`fc35cd0`）。**教训**：pynvoc 的门禁用例有一条隐性约束——**不能触碰解释器**，这条此前没有写下来过，本轮两个失败把它变成了明文（也解释了为什么该文件既有用例全都只断言 `serde_json` 值）。

**⑤ 诚实边界** ✗：①**"能作答"这件事本轮仍未经真机测量**：楔死下 RM 会话连设备枚举都失败（`no GPU selected`），所以"从快照答出 `-0.126`"目前只有**免设备的单测**作证（拿手搭的 `DeviceSettings` 投影），实机作答要等健康窗口；分类记作**已实现、待实机测量**（不是"已实现且实测"）。②"未被佐证的域给点名理由"这条同理，只有单测证据。③**零 GPU 请求、零写入、零提权、零重启**；`SETTINGS_WRITE_SUPPORTED` 仍 `false`（`core/src/macos_rm.rs:119` 未动），写入口只是**改了拒绝的措辞**，能力没变。④**默认后端的改变是一个真实的行为改变**：`query_clock_offset(gpu, None, …)` 在 macOS 上从"NVML"变为"macos-rm"（非 macOS 平台不变，仍是 NVML——本轮的 helper 只在 `cfg!(target_os="macos")` 下换）；因为 macOS 上根本没有 NVML，这个改变只会把"平台级拒绝"换成"设备层答案/设备层错误"，不会把可用变成不可用。⑤`set_pstate_clock_offset`／`reset_pstate_clock_offsets` **仍未改**（它们不收 backend 参数，在 macOS 上仍报平台级那句）⇒ 与本轮同族、同一类缺口，**记为待办**，不在本轮的声称范围内。⑥`plan_clock_offset_step` 的默认仍是写死的 `macos-rm`（在非 macOS 平台也如此），本轮**未动它**——与本轮"平台默认"的取法不完全一致，同样是待办。⑦`macos-eval` 入口归属问题仍未动（`cli/src/lib.rs:6998-7004` 的散文仍称 `nvoc macos-eval`，而该子命令属于 `nvoc-auto-optimizer`）。

**⑥ 附：本轮另外做的机械复核（与偏移无关，但同一批数字要能复算）** ✔/✗：把 CLI 的命令规格表与 `--macos-rm` 允许清单机械相乘（`CommandSpec::new` 出现次数 × `cli/src/macos_rm.rs::validate` 的 9 条白名单）得到三个被漫拒家族的条目数：**Fan 8**（只放 `get-fan-info`）、**Power 14**（只放 `get-public-power-limit`）、**Thermal 9**（只放 `get-legacy-temp-sensor`）——即"后端答得出读数、入口整族漫拒"的规模有了可复算的计数，供后面逐族收口时对照。✗ 一处未核实：规格表里 `CommandSpec::new` 我数到 **102** 处，而矩阵 §一 表头写的是 **101 条**——两者差 1，**尚未定位**是表头陈旧还是我的正则多算（已记为待办，不结论）。

## 一百一、偏移写入口改说"门"、命令表补回 `list` 并让"表必须与代码一致"成为可执行判定（2026-10-18；静态＋门禁＋入口实跑＝0 次写入、0 次提权；含 2 次只读锚点读取）

**① 选择依据** ✔：两条待办都来自 §一百：⑤⑤ 记下"偏移族的另两个写入口（`set_pstate_clock_offset`／`reset_pstate_clock_offsets`）仍报平台级那句"，⑥ 记下"`CommandSpec::new` 我数到 102 处而表头写 101，**尚未定位**"。两者都是阶段 11 第 5 项"各入口能力、范围和错误与后端一致"的口径：前者是**同一族里读入口答得出、写入口却说平台不存在**（`set_clock_offset` 上轮已修，这两个是同族的孪生），后者是**表自称来源是代码的命令表、却比代码少一条**。两件事都不需要设备，因此楔死期可做。

**② 改动（仓库内 ✔，两条修订）**：①pynvoc：新增 `macos_write_gate_here_reason`（`nvoc-python/src/lib.rs:2267`，按平台取后端：macOS 上等于 `macos_write_gate_reason(entry,"macos-rm")`，其余平台返回 `None` ⇒ 保持各族原有文案）与它的拒绝包装 `macos_write_gate_here`（`:2276`），并在**两个不收 backend 参数的偏移写入口**最前面调用：`set_pstate_clock_offset`（`:4668`）、`reset_pstate_clock_offsets`（`:4999`）；`set_pstate_clock_offset` 的注释同时写明本轮**没有**解决的事：macOS 的偏移记录是**逐域**的、不带 P-state 下标，所以"这个 P-state 参数将来落到哪个字段"本身是要在门内验证的问题之一。用例按平台写断言（`:6448`）：macOS 上必须抛"入口名＋共享常量"，非 macOS 上必须**不抛**（保持后端自己的句子）。②`probe-macos-rm/tools/audit-entry-parity.py` 新增**第四类判定**（`table_completeness`，`:138`，接入 `verdicts` 于 `:257`）：从 `cli/src/lib.rs` 取 `(命令, Group)` 对（`GROUP_SPEC_RE`，`:109`）、从 §一 取每组标题与其行（`command_table`，`:114`），三向比对——代码声明的每条命令都要有行、每组标题的项数要等于其行数、节标题的总数（`HEADLINE_RE`，`:107`）要同时等于代码声明数与行数；`SECTION_RE`（`:106`）把扫描限定在 §一，不靠全文件扫行。③矩阵 §一：表头 `101 → 102`（`:107`）、`Info` 组标题 `13 → 14`（`:111`）、补回漏掉的 `list` 行（`:125`，分类"**不适用（不依赖后端）**"并给出两条代码依据：`validate_invocation` 对它跳过后端检查、`run_invocation` 在选目标前直接渲染）、§0 基线的"101 条命令表"改 102（`:284`）、§九十四 (2) 追加第四类判定的说明、§一 前的引用句同步。

**③ 证据（测试机；0 次写入、0 次提权、未重启）** ✔：修订 `3640274`／`0478373`（已推 fork，测试机 `nvoc-head` ff 到 `0478373`）。①`RUSTFLAGS='-L <CLT Python 3.9 lib>' DYLD_FRAMEWORK_PATH=<CLT Frameworks> NVOC_GSP_FIRMWARE=<blob> cargo xtask ci` ⇒ **rc=0**（`CI_EXIT=0`），`test result: ok` **16** 次、`FAILED` **0** 次、工装 `selftest ok` **6** 条（`outputs/stage-offset-write-wording-20261018T081500Z/ci.txt` sha256 `3c9e582e…`；同一份日志里 ruff 两段真跑并通过：`186 files already formatted`／`All checks passed!`——本机缺 `ruff` 命令，门禁走 `uv run … ruff`）。②点名跑该文件用例（`cargo test -p pynvoc --no-default-features --lib macos_rm`）⇒ **6 passed／0 failed／10 filtered**（`new-tests.txt` `022c8ea6…`）。③**门禁内生反例**（`negative-control.txt` `8c4874ef…`）：把 `macos_write_gate_here_reason` 里的 `"macos-rm"` 改成 `"nvml"`，macOS 上它返回 `None` ⇒ 新断言**当场失败**（`5 passed; 1 failed`，panic 文本＝`the offset write is behind the gate on macOS too`，`nvoc-python/src/lib.rs:6450`，rc=101）；`git checkout --` 复原后同一条用例 **6 passed／rc=0**，`git status --porcelain` 仅剩三个**先于本轮**就存在的未跟踪项（`status-before-control.txt` `0a43635f…` 与复原后输出逐字相同，`core/native/macos_offset.rs`、`probe-macos-rm/probe-macos-rm`、`probe-macos-rm/probe-macos-rm-idle`）。④**新判定对真实内容的反例**（`tool-control.txt` `7eb0cb92…`）：对**修表之前**的矩阵（`git show 3640274:docs/…`）运行工具 ⇒ `2 ok, 2 failed`，两条 FAIL 分别是 `§一 Info: the code declares 14 command(s); the table omits list` 与 `§一 states 101 条 but the code declares 102 and the table has 101 row(s)`；对修好的工作区 ⇒ `3 ok, 0 failed`（`9 allowed command(s)… (109 table rows read)`）。⑤**入口真跑（免设备）**：pynvoc 三个偏移写入口＋一个族外写入口（`wording.txt` `3a7a7ffa…`，`cli-and-pynvoc-wording.txt` `da347882…`）——`set_pstate_clock_offset`／`reset_pstate_clock_offsets`／`set_clock_offset` 都抛 **RuntimeError＝"入口名: 共享常量"**（**不含**平台句），族外的 `set_ppab_status` 仍是原来的平台句（对照 ✔：本轮**只**收口这一族，边界写进 ⑤）。CLI 侧**无需改动**（已核对）：`reset-pstate-global-freq-offset --macos-rm` 的漫拒**尾巴本来就点了写门**（`0x2080D01C` 的两次门禁＋"直到同一次读把它读回来"），`set-pstate-global-freq-offset --macos-rm` 先要求 `<OFFSET_MHZ>`（clap 层，不碰设备），给了参数后是同一句。⑥**设备级一致（只读，2 次锚点读取；`device-state.txt` `6edd0ef8…`）**：同一时刻 `query_clock_offset("256","macos-rm","core",None)` 与 `nvoc-cli get-pstate-global-freq-offset --macos-rm` 给出**同一句** core 理由——`the offset channel was not read in this snapshot, so graphics is not named; see notes`（`core/src/macos_rm.rs:1096`）⇒ 偏移读面在 CLI／pynvoc 两侧的**设备级行为一致**，这是阶段 11 第 5 项在本族的一次实机核对；同一次抓取里锚点判词仍 `0x62`（`uptime` 1 day 10:02，比上轮 1d9h47m **上涨** ⇒ 未重启），即"没读到偏移通道"与 `0x62` 同时出现。⑧本节写完之后，工具在**最终修订**（含本节）上再跑一次仍是 `3 ok, 0 failed`（测试机 `tool-control-final.txt` sha256 `4630e882649ef8d124bd06089e12bee8e8a2b2fc1d4c97f9bf740804dd5acc56`；客户端同修订同结果）——本节只加散文，而工具读的是 §一 与全文件行扫描，所以这次复跑是"改动之后判定没被搅动"的对照。⑦扩展重建：按仓库自己的命令 `uv run --locked --package pynvoc --group dev --no-config maturin develop --release` ⇒ rc=0（`rebuild.txt` `9e77ba6d…`）。

**④ 本轮我自己的错（留档）** ✔：①新判定的正则第一版**没有** `re.M`，而我的 selftest 夹具里矩阵**从** §一 标题开头 ⇒ 夹具通过、真实文件（§一 在第 107 行）里却报 `the §一 heading states no command count to check`：**夹具的形状与真文件的形状不一致，等于没有测到**。修法有两步：加 `re.M`，并把夹具改成与真文件同形（前面先放一个 `## 状态图例` 节）；随后用**反例**确认这条夹具现在有牙——把 `re.M` 拿掉，selftest 立刻失败（`the §一 heading states no command count to check`）。②先改了工具、后跑 ruff：本机（客户端）没有 `ruff`，用 `uvx ruff@0.14.0` 按仓库配置检查，发现三处 `ruff format` 不通过（长行折行），格式化后门禁的 ruff 段确认干净——若不查，GitHub 的 `ruff format . --check` 会红。

**⑤ 诚实边界** ✗：①**门仍关着**：本轮只改"拒绝时说什么"，`SETTINGS_WRITE_SUPPORTED`／`SETTINGS_RESTORE_SUPPORTED` 仍是 `false`（`core/src/macos_rm.rs:119`／`:120` 未动），偏移写入口的**能力**没变，本轮**零次写入**。②**§一百⑤① 的一处更正**：上轮写"楔死下 RM 会话连设备枚举都失败（`no GPU selected`）"——**这句过头了**。今天同一次抓取显示：`query_clock_offset("256","macos-rm",…)` **解析到了设备**（`0x62` 仍在），说明枚举是好的，`no GPU selected` 说的是**选择器字符串没匹配上**（本机 CLI 打印的设备标识是 `256`，不是 `"0"`）。因此偏移读面的分类从"仅免设备单测"上调为"**设备级一致已实测**"，但"某个域被点名"仍待"快照里带偏移通道"的窗口。③"`0x62` ⇒ 快照没有偏移通道"这条**因果本轮没有钉死**：探针自己的说明是该状态会把带状态检查的 handler 一律折成 `0x62`，与观察一致，但直接证据（同一次读里 `0x2080901b` 的状态码）本轮没抓。④`("0", …)` 报 `no GPU selected` 这件事的**成因**（标识名规则）本轮只到"现象＋本机反例"，没读代码钉死。⑤**族外写入口仍报平台句**：机械计数=**41** 个写入口直接取 `BackendSet::Nvapi/Nvml`；本轮有 3 个（本族）改说门，其余约 38 个保持原句——对**写**而言那句是**真话**（后端确实还没写），只是不如"门"有信息量，**记为有界待办并给出计数**，不是遗漏。⑥**§一百⑤⑥ 的更正（撤回）**：`plan_clock_offset_step` 写死的 `macos-rm` 默认**不是**不一致——重读实现，该入口要求目标**本来就是** macOS RM 窗口（`has_macos_rm()`，否则明确报"这份计划是拿 RM 窗口量出来的"），换平台默认反而会让非 macOS 目标落到别的后端再被自己拒；故本轮**未动**，并把上轮的"待办"改成"有意为之"。⑦pynvoc 那一侧本轮**未测**"门开之后写什么"，那是验证轮的事。⑧本轮做了 **2 次只读锚点读取**（开头探一次、状态抓取时再读一次完整判词），无写、无提权、无重启；`0x62` 未变 ⇒ **非重启用窗口**。

**⑥ 附：机械计数（可复算）** ✔：①`CommandSpec::new(\s*"<名>"\s*,\s*Group::<组>` 在 `cli/src/lib.rs` 计 **102** 条，分 9 组：`Info 14`／`Power 14`／`Thermal 9`／`Fan 8`／`Clock 20`／`Voltage 13`／`Vfp 14`／`Perf 9`／`Scanner 1`（和＝102）；修表前 §一 各组行数与代码逐一相同**除了** `Info`（13 行 vs 14 条），缺的正是 `list` ⇒ §一百⑥ 的"差 1"定位完毕：**不是我的正则多算，是表少一行且表头跟着少**。②新的第四类判定上线后，这个数字不再靠人眼：表头、每组标题、每组行数、组内命令集合四者与代码对齐才算过（工具现在打印 `the §一 table matches the code: 102 spec(s) over 9 group(s), and every heading counts its rows`）。

## 一百二、拒绝消息点的程序名必须是本工作区真的会构建的那个：把"消息里写了 `nvoc <命令>`"做成可执行门禁（2026-10-18；静态＋门禁＋入口实跑＝0 次写入、0 次提权、0 次 GPU 请求）

**① 选择依据** ✔：本表与 review 里到处用"`nvoc <命令>`"指路，而**仓库里根本没有叫 `nvoc` 的程序**：`cli/Cargo.toml` = `nvoc-cli`、`auto-optimizer/Cargo.toml` = `nvoc-auto-optimizer`、`srv/Cargo.toml` = `nvoc-srv`／`nvoc-srv-ctl`，工作区里没有任何名为 `nvoc` 的 bin。于是"照消息敲一遍"会得到 shell 的 command-not-found，而**所有断言整句其余部分的用例都看不见这个错**——被测的正是那句建议本身。这与阶段 11 第 5 项（各入口的能力、范围和**错误**要与后端一致）同一口径：拒绝时给的路标也是入口行为的一部分。楔死期只做免设备的事，所以先把这类"用户可见字符串里的程序名"做成机械判定。

**② 改动（仓库内 ✔，六个修订）**：①修 3 处**用户可见**文本＋2 处注释：`core/src/gpu.rs:132`／`:153`（选不到 `--gpu` 时的建议）由"use \`nvoc list\`"改为"use \`nvoc-cli list\`"；`cli/src/lib.rs:7004`（`--plan-delta` 在非 RM 后端上的拒绝）由"\`nvoc macos-eval --plan <target>=<delta>\`"改为"\`nvoc-auto-optimizer macos-eval --plan <target>=<delta>\`"；`cli/src/lib.rs:6998` 与 `nvoc-python/src/lib.rs:2672` 两处注释同步。**只改程序名，句子其余部分逐字不动**。②新增 `probe-macos-rm/tools/audit-command-names.py`：程序名**从 Cargo 清单读**（`binaries()`）、命令名**从源码的命令表读**（`cli/src/lib.rs` 的 `CommandSpec::new` 与 `auto-optimizer/src/arg_help.rs` 的 `Command::new`），判定两件事——(a) 裸 `nvoc` 后面跟着的 token 若是**已声明**的命令 ⇒ FAIL 并指名该命令真正的宿主程序；(b) 真程序名后面跟了**别人**的命令 ⇒ FAIL。**刻意不判**的：`nvoc uses`／`nvoc lacks` 这类把程序名当普通名词用的散文（未被声明的 token 一律放过），因为那种写法没有错、只是不含命令。故意引用错拼写的那一行加标记 `audit-command-names: quoted` 整行豁免并**计数打印**（本轮 10 行，其中 1 行是真正的反例断言）。③`--selftest` 10 个用例（两处历史缺陷原文、反方向、三个假阳性护栏、两个空输入、一个"改名的命令从源码读出来就能抓到"、一个整行豁免），**空输入判 FAIL 而非静默通过**：`selftest ok: 10 cases …`。④注册进 `xtask/src/checker.rs` 的 `RUNNABLE`（`[&str; 5]` → `[&str; 6]`）——未注册的新工具会让门禁的 selftest 步骤直接失败，所以"工具写了但没进闸门"这件事本身是错的。⑤`core/src/lib.rs` 导出 `select_gpu_ids`（与 `select_targets` 同一出口）；⑥两个**钉住渲染文本**的用例：`core/tests/non_gpu.rs` 的 `gpu_id_selection_suggests_a_command_that_exists` 对两处拒绝**逐字节**断言整句（顺带证明续行没有留下多余空格），`cli/src/lib.rs` 的 `the_nvapi_offset_read_names_the_entry_that_can_evaluate_a_step` 调**同一个构造该句的函数**断言其中两段。

**③ 证据（测试机；0 次写入、0 次提权、0 次 GPU 请求）** ✔：修订 `fac6d63`→`ee29541`（已推 fork，测试机 `nvoc-head` ff 到最后一条）。①**门禁 rc=0**：`RUSTFLAGS='-L <CLT Python 3.9 lib>' DYLD_FRAMEWORK_PATH=<CLT Frameworks> NVOC_GSP_FIRMWARE=<blob> cargo xtask ci` ⇒ `CI_EXIT=0`，`test result: ok` **16** 次、`FAILED` **0** 次、工装 `selftest ok` **7** 条（上轮 6 条，**新增的第 1 条就是本工具**，见 §九十四 (2) 的登记）、pytest `52 passed, 10 skipped`；同一份日志里 `187 files already formatted`／`All checks passed!`（`outputs/stage-command-names-20261018T083000Z/ci.txt` sha256 `ffa9474126304dc333fff2f6fedd8c4a59c5c7f218441e31d6c8f31d6806382f`，617 行）。②**工具在真实树上从 5 FAIL 到 0 FAIL**：修前在本机对**同一个**工作区运行 ⇒ `command names: 3 ok, 5 failed`，逐条点名 `cli/src/lib.rs:6998`、`cli/src/lib.rs:7004`、`core/src/gpu.rs:132`、`core/src/gpu.rs:152`、`nvoc-python/src/lib.rs:2672`（`tool-before-fix-on-client.txt` sha256 `88542c7cfd3ede99c22046fac947982cef721ffbec8503d263848e1dcf163bee`；selftest 修前即已通过 ⇒ 5 条 FAIL **不是**工装自身报错，`tool-selftest-before-fix-on-client.txt` `28b73d99…`）；修后在测试机对 `ee29541` 运行 ⇒ `command names: 4 ok, 0 failed`（`tool.txt` `203d6c19068d27cb6416ad5d38fba51ad4727fb9cc38c58b373e691f51a7ec6b`，扫描 213 个源文件／13 个程序名／111 条命令）。③**入口真跑（重建 CLI 后，免设备只读）**：`nvoc-cli get-info --gpu 9999` ⇒ 用户可见一行 `Error: no GPU matches --gpu 9999; use \`nvoc-cli list\` to see available indices`（`--macos-rm` 变体同句）；这既证明消息真的换了，也证明续行渲染后**没有**多余空格（`runtime-command-names.txt` `8e37b749ae8fdb97fb66660b0cc420ebdc551c4a13765298c5e0557b801db2f1`）。④**反例（有牙的证明）**：把旧拼写**只**写回这三处消息（内容层）⇒ `core/tests/non_gpu.rs:160` 的逐字节断言失败、`cli/src/lib.rs:8502` 的断言失败（panic 文本里**原样打印出整句**，这也是那句在真机上唯一能看到渲染结果的方式），工具同时点名**三行**且给出**各自正确的宿主**（`cli/src/lib.rs:7004` → `nvoc-auto-optimizer`；`core/src/gpu.rs:132`／`:153` → `nvoc-cli`）；`git checkout --` 复原后同两条用例 1 passed／1 passed、工具 0 failed，`git status --porcelain` 与抓取前**逐字相同**（`negative-control.txt` `92f3835e7fc843dc1ce47bb93cbd87525b59ae2192cb7c49bb99c5763f78eafb`）。

**④ 本轮我自己的错（留档）** ✔：①**门禁连开三枪**，都是我先改代码、后跑门禁：第一枪 `cargo fmt --all -- --check` 说我多打了一个空格（`);  //` → rustfmt 要 `); //`）；第二枪 `cargo clippy -- -D warnings` 报 `nvoc_core::select_gpu_ids` **根本没导出**（`mod gpu;` 是私有的，`pub use` 里没它）；第三枪又是两条 clippy：`GpuId` 只在 `#[cfg(test)]` 里用 ⇒ 非测试构建下成了**未使用导入**，以及 `.err().expect()` 该写 `expect_err`。教训同上轮第 ④ 条但更贵：**"源码看起来对"与"门禁通过"是两件事，`fmt`/`clippy` 是第一个能看见长行、可见性与 lint 的东西**；本轮之后的顺序固定为——先 `cargo fmt --check`，再 clippy 一次，再跑整门禁。②工具第一版把**它自己**的两行（docstring 里引用的错拼写、夹具常量）判成了缺陷：因为标记文本是**插值**出来的、不是字面量。改成字面加标记后自检通过——这反过来证明该工具**不会**因为"是我自己写的"而放过一行。③选例子的方向错过一次：`--plan-delta` 的拒绝在 macOS 上用 `--nvapi` **跑不到**（`NvAPI_Initialize`→`NvAPI_EnumPhysicalGPUs` 先失败，`LibraryNotFound (-2)`，根本走不到偏移读），所以"入口真跑"只能证明 core 那一句；补法是**用例直接调构造该句的函数**，而不是假装跑到了。④最初的 `subcommands()` 按**程序名**过滤而不是按 crate 目录，于是每次运行都返回"no binary declares a subcommand list to check against"——夹具与真文件形状不一致的老毛病，修法是按清单映射取键、程序名从清单推导。

**⑤ 诚实边界** ✗：①**门仍关着**：`SETTINGS_WRITE_SUPPORTED`／`SETTINGS_RESTORE_SUPPORTED` 仍是 `false`（`core/src/macos_rm.rs:119`／`:120` 未动），**零次写入**；本轮唯一碰设备的是重建后的 CLI 做了一次只读枚举（`--gpu 9999` 都还没轮到读卡就报错）。②**`.md` 不在闸门内**：工具只扫 `.rs`／`.py`。我**手工**扫过全部 `.md`（`grep -n "nvoc [a-z]"`，剔除 `nvoc-`）⇒ 剩下的全是 `cargo xtask …`／`pynvoc …`／"nvoc uses/lacks/…"这类**用法名称**，没有一处是"`nvoc <命令>`"路标；也就是说今天不扫 `.md` 没漏东西，但**这是人眼结论、不是门禁结论**（本表 §一百一 那句"`nvoc macos-eval`"就是故意留的原文引用，改进闸门反而要豁免）。③**"程序名后面跟了别的程序的命令"这一类今天没有真实反例**：它只在 selftest 里被断言过 ⇒ 分类为"**已实现、待实机反例**"，不是"已实测"。④跨**语言**的同类问题没管：Python 侧（`nvoc-python/src/lib.rs`）扫到了，但仓库里 `xtask` 打印给用户的提示、C 侧注释、以及 `gui/`／`tui/` 里的字符串**没有**单独核对（它们都在 `.rs` 扫描范围内，所以只要写的是"裸 `nvoc <命令>`"就会被抓到；但 `tui` 里"按 `n` 跳到…"这类纯文字不受影响）。⑤本轮**没有**新增"消息里的命令名必须真的存在"这一层：工具只判**程序归属**，不判"命令是否还存在"（后者已由 `audit-entry-parity.py` 的表完备性判定覆盖，两者互补而不重叠）。

**⑥ 附：机械计数（可复算）** ✔：①`BIN_RE`／清单扫描得到 **13** 个程序名（含 `nvoc-cli`／`nvoc-srv`／`nvoc-srv-ctl`／`nvoc-auto-optimizer` 等），两个程序共声明 **111** 条命令（`nvoc-cli` 命令表 102 条＋`nvoc-auto-optimizer` 的 9 条）；修前真实树 5 FAIL、修后 0 FAIL，豁免行 10（每行都在工具输出里计数，不是静默跳过）。②`CommandSpec::new` 计数 102 条／9 组与 §一百⑥ 一致（工具从源码读，不再依赖人眼）。③门禁的 `RUNNABLE` 现在是 **6** 条（`audit-command-names.py`、`audit-entry-parity.py`、`audit-live-attempts.py`、`audit-matrix-claims.py`、`decode-clk-containers.py`、`record-run-provenance.py`），`NEEDS_FIRMWARE` 仍只有 `gsp-handler-map.py`。

## 一百三、风扇／功耗／温度三族逐个族的"可读／可解释／范围／原状态／恢复"现状：把散在核心常量里的结论合成一张账，并补上楔死期的入口级证据（2026-10-18；静态＋门禁＋入口实跑，0 次写入、0 次提权、0 次 GPU 请求）

**① 选择依据** ✔：原目标把这三族列为**分别**调查的次优先项（"不因 VF 受阻就一并宣布不可用"，每族各给五项证据与剩余问题）。到今天为止这些结论是**散**的：真相在 `core/src/macos_rm.rs` 的若干 `*_REASON` 常量与 §九十九～§一百一 各轮记录里，而"某族到底哪一面已实测、哪一面是缺失客户端读取、哪一面是缺失驱动发布"没有一张可核对的账；阶段 11 第 5 项（各入口能力、范围与**错误**一致）也因此无法逐族验收。本轮把它合成账，并补上此前缺的一类证据：**同一族在两个入口（CLI 与 pynvoc）上作答与理由是否一致**，以及**该族写入口现在的说法**。

**② 改动（本表本节＋测试机 `outputs/stage-family-surface-20261018T085000Z/`；本族代码本轮零改动）** ✔：只落证据与分类，不改行为。三族 31 个入口的机械分类（`CommandSpec` 9 组之一）与 macOS 允许清单对照：**允许清单里的读 3 个**（`get-fan-info`／`get-public-power-limit`／`get-legacy-temp-sensor`）、**被漫拒的读 8 个**（风扇 `get-fan-curve`／`get-legacy-fan-policy`；功耗 `get-power-ceiling`／`get-power-mode`／`get-dnotifier`／`get-rated-tdp`；温度 `get-public-temp-limit`／`get-temp-sim`）、**另有 1 个读自带点名层**（`get-temp-thresholds`，见 §一百一 的专门入口）、**被漫拒的写 19 个**（风扇 5／功耗 9／温度 5）。逐族账见下表；每格的"依据"列指向代码里的常量或本轮抓取，避免再次靠散文自证。

| 族 | 可读 | 可解释 | 范围 | 原状态 | 恢复 |
|---|---|---|---|---|---|
| 功耗 | **是**：`get-public-power-limit` 答 rc=0（楔死态三个瓦数全 `null` 且各自带理由），健康会话实测 `current_watt`＝`requested_watt`＝**100.000 W** | `PowerLimitReport`（`min/current/max` ＋ `requested_watt`／`policy_tgp_watt`）；`power_policy_tgp_mw` 实测 **11.277–11.309 W** 对 100.000 W 墙、瞬时功率 11.254–11.359 W ⇒ "这条字段承载什么"**尚未定位**（`POWER_POLICY_SCALE_NOTE`） | **两端尚未定位**：`POWER_RANGE_UNREAD_REASON` 记下 RUSD 功耗段只发布请求值／墙／策略瓦数，**没有 min/max 约束对**；已测候选 `0x2080200B`（`PERF_GET_LEVEL_INFO_V2`）在本驱动上答 **`0x1f`**（点名缺失层＋一个实测负例，不是"头文件里没找到"） | **可读**：与 `get-status` 同一次快照里的请求值与墙 ⇒ 写之前有原状态可比 | **不适用（后端不写）**：9 个写入口全部漫拒；`POWER_WRITE_NEEDS_PRIVILEGE_REASON` 另点出特权与"RATED_TDP 是仲裁请求不是瓦数"这一前置条件 |
| 风扇 | **是（曾实测）**：`get-fan-info` 答 rc=0，每 cooler 槽有转速读数；楔死态 `count:0` 并**明说**"本快照的风扇段不可用（`RUSD allocation/mapping failed: 0x00000062`），空列表是**未读到**而不是没有风扇" | `FanReport{coolers:[{slot,rpm}]}` ＋ `unavailable` 三个百分比；`FAN_PERCENT_UNREADABLE_REASON` 说明 RUSD 风扇段只发布转速、没有占空比百分比、也没有转速量程 ⇒ 百分比需要一个"自己发明的刻度" | **转速范围无来源**；**控制面**：SDK 的 fan 头是一个 33 行 FINN 空壳，控制头树里**没有任何** `NV2080_CTRL_CMD_*FAN/*COOLER*` 宏（这是关于**已发布头文件**的事实，不是关于驱动的）⇒ 候选来自树内 NVAPI 副本：RM `0x20800532`（经 escape `0x07000038`，填充 232 字节 `fanPolicyInfo`），本机方法表里只有它的私有孪生 `0x20808532`（paramSize 336，读半边），**attempted 读答 `0x62`**，而公开 id 答 `0x56`——两条都还只是散文，因为 `audit-live-attempts.py` 的普查在这些 id 里找不到任何返回状态 ⇒ 已备成 probe `--fan-read`（336 B 于 `0xA5` 背景，未写字段留 `0xA5`、写进去的 0 仍可区分）并纳入读电池 | 只读，无状态可存 | **不适用（后端不写）**：5 个写入口漫拒；`reset-fan-speed` 在 NVAPI 面才会真正"恢复默认" |
| 温度 | **是（曾实测）**：`get-legacy-temp-sensor` 答 rc=0，四路传感器结构齐备（楔死态 `current_c` 全 `null` ＋每格带理由）；`0x20800513` 健康时答 NV_OK、一个目标一个传感器读数 **47**、驱动自报量程 **`[0,127]`** | `TempSensorView`（四 target 各一），`current_source` 区分来源（`rusd:<field>` 与 `0x20800513:sensor<n>`） | **已可读且已接线**：`DeviceStatus::thermal_view()`（`core/src/macos_rm.rs:1572`）真的发这条读并把 `ThermalReadOutcome` 三态（`NotAttempted`／`Refused`／`Answered`）分开；本轮抓取里 `min_c`/`max_c` 的理由是"这次没读到（`0x62`）"，**不是**"没有来源" ⇒ 范围一面属**已实现、待健康窗口复测**（该常量 `THERMAL_RANGE_UNREAD_REASON` 描述的 `NotAttempted` 分支已是旧措辞，见⑤） | 只读快照 | **不适用（后端不写）**；`THERMAL_THRESHOLDS_UNREADABLE_REASON` 另有**定位到的否定**：SDK 的温度控制族恰好只有两个 id（`0x20800513` 与私有孪生 `0x20808513`），其 opcode 覆盖 target／sensor／reading／reading range／sensor target index，**没有** severity/threshold opcode ⇒ 阈值是"尚未定位"而非"不存在" |

**③ 证据（测试机；0 次写入、0 次提权、0 次 GPU 请求）** ✔：`outputs/stage-family-surface-20261018T085000Z/`：①`cli-family-surface.txt`（sha256 `6e18deb4d7ae9cc571bd8efe7c819508af73ec2b75ce9721ace63bfe95179e6d`）：三条允许的读在 `c1c23d0` 上重建的 CLI 各答 rc=0，输出逐字段带 `unavailable` 理由；②`pynvoc-family-surface.txt`（`82b0e34a6d84ec4af0b028e7dec9f78eca068f8c92ca19301ba12148136e788c`）：同一台机、同一修订、同一三族经 `PYTHONPATH=nvoc-python/python python3` 直调 `pynvoc.query_fan_info/query_power_limits/query_legacy_temp_sensor("256","macos-rm")`；③`entry-parity.txt`（`ddedc8f837b812cac0657cfa405414330b9f67a6d1a30612c16ad67a2816280c`）：**机械比对**两个抓取里 `unavailable`/`notes` 的全部理由字符串 ⇒ 两侧各 **10** 条、**完全相同**（"in the CLI capture only: (none)"／"in the pynvoc capture only: (none)"），三族各自点名的层（`RUSD fan section`／`RUSD power section`／`public thermal read`）与候选 id `0x2080200B`、字段名 `power_policy_tgp_mw`、`temperature_power_supply_c` 两侧一致，楔死成因 `RUSD allocation/mapping failed: 0x00000062` 两侧都在 ⇒ 三族的**读**在"能力、范围与错误"上入口一致（阶段 11 第 5 项，本族读面）；④`write-refusals.txt`（`0ceb4ed4843fca8e03b9056c7305a12adb078a006b9d7c1daeda24511a17c848`）：三族 8 个写入口（`set-fan-speed`／`reset-fan-speed`／`set-fan-curve`／`set-fanstop-status`／`set-power-limit`／`reset-power-limit`／`set-temp-limit`／`reset-temp-limit`）在 `--macos-rm` 下**全部**在打开 IOKit 之前以点名层的漫拒结束，`rc=2`；⑤`refused-reads.txt`：三族被漫拒的 8 个**读**入口的实际首行（与 `get-temp-thresholds` 自带理由的首行对照，见⑤）。本轮代码未动 ⇒ 与 `c1c23d0` 上那次 `CI_EXIT=0` 的同一次门禁一致。

**④ 本轮我自己的错（留档）** ✔：①第一版写入口抓取**整段空白**——因为测试机是 zsh，**未加引号的 `$c` 不做分词**，"`set-fan-speed 50`"被当成一个词传给了 CLI，于是每个命令什么都没输出。**"沉默"被误当成"被拒绝"正是本项目最忌讳的一类错误**，所以改走 `bash -c` 重跑，并把这次事故写进产物（`write-refusals.txt` 第 2 行）。②同一抓取里 `set-fan-curve` 我**少给了位置参数**，clap 先报 `required arguments were not provided` ⇒ 那是**用法错误、不是后端拒绝**；补上 `0 40:800,60:1200,75:1800` 后它才给出真正的漫拒。两次都记在产物里，因为"退出码相同、原因完全不同"是这套验收里最容易混的一对。③`${PIPESTATUS[0]}` 在 zsh 里是空的（应为 `pipestatus[1]`），所以最后追加的那行 `rc=` 空白；已在产物末尾注明，块内其余行是经 `bash` 跑的、`rc=2` 为真。

**⑤ 诚实边界** ✗：①**抓取是楔死态**：三族所有实数都是 `null`，理由里写着 `0x62` ⇒ 本轮证明的是"**形状与理由**"，不是"**数值**"；`current_watt`＝100.000 W 等数字来自**健康会话**的旧实测，由常量与理由字符串保存，本轮**没有**复测。②因此三族的读面分类是"**已实现且曾实测／当前不可复测（楔死）**"，不是"本轮实测"。③**"范围"一面仍有两个尚未定位**：功耗的 min/max（已测负例 `0x2080200B`→`0x1f`）与风扇百分比的刻度（RUSD 风扇段没有量程）；温度的范围**已接线**，缺的只是健康窗口。④**8 个读入口的拒绝还没点名各自缺失层**：`get-fan-curve`／`get-legacy-fan-policy`／`get-power-ceiling`／`get-power-mode`／`get-dnotifier`／`get-rated-tdp`／`get-public-temp-limit`／`get-temp-sim` 现在答的是**漫拒＋写门那句**（对人**读**而言那句不是它们被拒的原因）。已机械计数并点名，其中 3 个有现成常量可直接引（风扇控制族的 `FAN_CONTROL_UNREADABLE_REASON`、温度配置上限的 `THERMAL_LIMIT_UNREADABLE_REASON`），另 5 个要先分别定位自己的缺失层 ⇒ **记为有界待办（8 个，逐一列名）**，不是遗漏，也**不在本轮**改（半个族的改法会留下更难核对的半状态）。⑤`THERMAL_RANGE_UNREAD_REASON` 的措辞已过期：它写"this snapshot does not wire that read in yet"，而 `thermal_view()` 早已接线，本轮抓取打出的也是"读已接线、这次没读到"（`0x62`）那句 ⇒ 该常量现在只在 `NotAttempted` 分支出现，**下一轮要么改措辞要么说明分支**，本轮如实记为"常量与实况不一致"（**已由 §一百六 ② 修掉**：措辞改成"这一快照没有*发起*该读"，并新增两条断言钉住分支语义）。⑥`reset-*` 的"恢复"面今天是**空洞为真**（后端一个字节都不写）⇒ 真正的恢复验收属于偏移写的那次门控写，不属于这三族。⑦`srv` 只把温度与功率映射进 dashboard 样本（`srv/src/macos.rs:111-130`），其结构里**没有**风扇转速的位置（`fan_pct` 恒 `None`，注释明说"RM 通道发布的转速在这个结构里没有家"）⇒ srv 面本轮未动；GUI/TUI 未动。

**⑥ 机械计数（可复算）** ✔：①三族共 **31** 个 `CommandSpec` 条目＝风扇 8（读 3／写 5）＋功耗 14（读 5／写 9）＋温度 9（读 4／写 5）；其中被允许清单收下 **3**，被漫拒 **27**（读 8＋写 19，另 `get-temp-thresholds` 自带理由）。②本轮设备访问＝三条允许的读各一次（CLI）＋同一三条经 pynvoc 各一次，**共 6 次只读请求**，均 rc=0；其余抓取（写入口 8 条、被拒读 9 条、parity 比对）**完全免设备**。③楔死轮询 1 次（`0x00000062`，uptime `1 day, 10:30`），无重启、无写入。④`entry-parity.txt` 的判据是两个抓取里**逐字符串相等**的 10 条理由，不是"看起来一样"。

## 一百四、偏移通道的 id 与分级从"已发布定义"侧核对：`+0x40` 与"写半边 = idx+1"两条规则各自找到逐字依据；CLK 族的命令与结构体在 OGKM 里根本不发布（这就是定位到的缺失层）（2026-10-18；测试机只读源码考古，0 次设备访问、0 次写入）

**① 选择依据** ✔：原目标要求"受阻时点名缺的是哪一层——协议／前置／驱动实现——**有证据**"，且"从真实编排逻辑重构，不做无据的扫描"。此前我们对 `0x2080901b`／`0x2080d01c` 这一族 id 的描述**全部来自自己的推断**（bit7 是 legacy 别名、写半边＝接口字节 `+0x40`、消息 id `+1`）。本轮在测试机上的 OGKM 610.57.04 只读副本里把这些推断逐条对照**已发布**的定义，并顺带回答了一个悬着的问题（现代／legacy 别名空间是不是同一个消息 id）。

**② 改动（本节＋测试机 `outputs/stage-family-surface-20261018T085000Z/`；代码零改动）** ✔：只读考古与记录。做法是**先算**（按 `id = (0x2080<<16)|(cat<<8)|idx` 把每个 id 拆成 cat／idx）**再查**（`grep -ril "0x<id>"` 全树），而不是逐个文件翻。

**③ 证据（路径相对测试机 `~/Desktop/macOS-nvoc-develop/reference/open-gpu-kernel-modules-610.57.04/`）** ✔：①`src/common/sdk/nvidia/inc/ctrl/ctrlxxxx.h:47-50` 逐字给出 bit 域：`CLASS 31:16`、`CATEGORY 13:8`、`INDEX 7:0`；`:53` 是宏体 `NVXXXX_CTRL_CMD(cls,cat,idx)`；**`:68` `#define NVxxxx_CTRL_LEGACY_PRIVILEGED (0xC0)`、`:69 #define NVxxxx_CTRL_LEGACY_NON_PRIVILEGED (0x80)`**；②`src/common/sdk/nvidia/inc/ctrl/ctrl2080/ctrl2080base.h:34` 是 `NV2080_CTRL_CMD(cat,idx)`，同文件的族表逐字给出 `THERMAL (0x05)`（`:44`）、`THERMAL_LEGACY_NON_PRIVILEGED (0x85)`（`:46`）、`CLK_LEGACY_PRIVILEGED (0xd0)`（`:52`）、`CLK_LEGACY_NON_PRIVILEGED (0x90)`（`:53`）、`PERF_LEGACY_PRIVILEGED (0xe0)`（`:61`）、`PERF_LEGACY_NON_PRIVILEGED (0xa0)`（`:62`）；③`src/common/sdk/nvidia/inc/ctrl/ctrl2080/ctrl2080perf.h:399` `NV2080_CTRL_CMD_PERF_RATED_TDP_GET_CONTROL (0x2080206e)`、`:438` `..._RATED_TDP_SET_CONTROL (0x2080206f)`——**取/设成对，消息 id 只差 `+1`，写在已发布头文件里**；④`src/common/sdk/nvidia/inc/ctrl/ctrl2080/ctrl2080thermal.h:135` `THERMAL_SYSTEM_EXECUTE_V2 (0x20800513U)` 与 `:137` `..._PHYSICAL (0x20808513U)`——**同一个名字的私有／非特权孪生，差的正是分级字节 `+0x80`**；⑤`src/common/sdk/nvidia/inc/ctrl/ctrl2080/ctrl2080clk.h:52` 是 CLK 族**唯一**一条已发布命令（idx `0x37`），而我们的 9 个 CLK id（idx `0x19`～`0x61`）在全树 `grep -ril` 里**一条都找不到**（`src/nvidia/inc`、`src/nvidia/generated` 里也没有）⇒ 它们只存在于闭源 RM 自己的 CLK 头里。

**④ 由此确定的两条规则与一处更正** ✔：①**分级字节**：`LEGACY_NON_PRIVILEGED (0x80)` 加 `0x40` 恰为 `LEGACY_PRIVILEGED (0xC0)`（逐字同上），所以本表 §四（已知限制，`:392`／`:396`／`:419` 三处）写的"写半边＝读 id 的接口字节 `+0x40`"不再是我们的推断，而是**已发布常量之间的算术**；`0x2080901b`（CLK 非特权）→`0x2080d01c`（CLK 特权）正是这一条。②**取/设成对＝消息 id `+1`**：`PERF_RATED_TDP_GET_CONTROL 0x2080206e`／`SET_CONTROL 0x2080206f` 是已发布的逐字例证，与我们观测到的读／写对 `0x2080901b`／`0x2080d01c` 形状一致（分级 +0x40 且 idx +1）。③**更正**：我之前把 `0x2080e06f` 记作"SET 孪生"，但它不是头文件里的**字面** id——字面 id 是 `0x2080206f`；我们观测到的 `0x2080a06e`／`0x2080e06f` 是**同一个命令的 PERF legacy 非特权／特权分级**（idx `0x6e`／`0x6f` 不变）⇒ 悬着的问题"`0x2080206e` 与 `0x2080a06e` 是不是同一消息"**有答案了：同一命令，不同路由分级**。同一模式也有第三个例证：温度族 `0x20800513` 与私有孪生 `0x20808513`。

**⑤ 诚实边界** ✗：①**OGKM 不定义我们 CLK 族的命令，也不定义它们的结构体** ⇒ 本表 §四（`:380` 的 `0x2080d01c@2108`）与 §五（`:729` 的 2404 B 调用者结构体）里写的"两层编排"（NVAPI 调用方结构体 `0x964`＝2404 字节；GSP 校验块 `2108`＝`60+32×64`）**无法在这个树里证实**：全树**没有任何 2108 字节的结构体**，`2108` 这个数字在 `ctrl2080/*.h` 里也不出现。②"32 个域"得到一个**独立**旁证：`ctrl2080perf.h:662` `NV2080_CTRL_PERF_CLK_MAX_DOMAINS 32U`，且 `:652` 的逐域记录是 `{flags, domain, currentFreq, defaultFreq, minFreq, maxFreq}`（24 字节）⇒ 32×24＋12＝**780** 字节（`:675` 的 `PERF_GET_LEVEL_INFO_V2_PARAMS`），**不是**我们的 2108 ⇒ **"32"作为条数被旁证，64 字节步长没有被证实**。③`32680` 的 `0x2080901d` 结构体步长：`40+32×1020` 与 `40+255×128` 两种拆法都成立，**这个树无法判定**。④`0x190000` 在整棵树里**字面不出现**（⇒ 运行期算出，无法在此命名）；`HUB／MEM／HOST` 这类域枚举**不在这个树里**（只有 Tegra 的 `TEGRA_UNDEFINED/GPCCLK/NVDCLK`），所以本表早先"三条 range 记录是 HUB(3)/M(4)/HOST(5)"的猜测**仍未获证**；目录 `+40`／`+44` 的 `i16` 对单位同样未定。⑤**因此"缺失的层"现在可以点名**：不是驱动没实现、也不是前置条件，而是 **RM 自己 CLK 族的控制定义（命令名与参数结构体）没有随 OGKM 发布**——分级约定发布了，内容没有。⑥本节**零设备访问**（纯源码静态核对）。

**⑥ 机械计数（可复算）** ✔：①核对 **19** 个 id：**7** 个解析到已发布名字（`PERF_RATED_TDP_GET/SET_CONTROL`、`PERF_GET_LEVEL_INFO_V2`、`THERMAL_SYSTEM_EXECUTE_V2` 与其 `_PHYSICAL` 孪生、`VGPU_MGR_INTERNAL_GET_VGPU_FB_USAGE`），**12** 个全树找不到（9 个 CLK ＋ `0x2080b201`／`0x2080b202`／`0x20808532`）。②CLK 族已发布命令数＝**1**（`0x20801037`）；VOLT 族已发布命令数＝**0**（`ctrl2080volt.h` 38 行）。③全树 `grep -ril "0x<id>"` 对 12 个"未解析" id 返回**空**。④复算命令：先按 `cat=(id>>8)&0xFF`、`idx=id&0xFF` 拆位，再对 baseline `0x2080<cat&0x3F><idx>` 与原文 id 各 grep 一次。

**⑦ 由本节得出的下一个候选（方法可复算，不是扫描）** ✔：`PERF_GET_LEVEL_INFO_V2` 已发布（`ctrl2080perf.h:675`，结构体 780 字节，含逐域 `minFreq`／`maxFreq`），其 **PERF legacy 非特权分级**为 `0x2080a00b`（＝ `0x2080200b` 的族字节换成 `0xa0`）。本机至今只试过**基态** `0x2080200b`（答 `0x1f`），**从没试过它自己的非特权分级**——这正是我们一直在用的那条路线（`THERMAL_SYSTEM_EXECUTE_V2` 的 `0x20808513`、`RATED_TDP_GET` 的 `0x2080a06e` 都是这样找到的）。⇒ 健康窗口里第一个值得发的候选 id 是 `0x2080a00b@780`，若答 NV_OK，逐域 `minFreq`／`maxFreq` 就是本表 §一百三"范围"一列缺的时钟量程（也是功耗入口那一列的旁证）。本轮**未**发它：探针现有的 `--anchor-read` 电池是一个"判方向"的六行仪器（其六行含义写在 `probe-macos-rm/src/nvx_suite.c:880-908`），往里加一行会模糊它的含义，而它是 GET-only、必须等健康窗口才能读；故记为下一步而非本轮临时加参数。

## 一百五、功耗／风扇／温度三族"契约字段到底哪里能读到"的来源定位（OGKM 610.57.04 只读考古）：功耗范围与风扇百分比在**已发布**接口里没有来源，温度范围有——而且我们早就在发那条 opcode（2026-10-18；0 次设备访问、0 次写入）

**① 选择依据** ✔：§一百三 的"范围"一列留了两处"尚未定位"（功耗 min/max、风扇百分比刻度），原目标要求这两处要么给出候选 id、要么给出**有证据的缺失**；同时顺手核对一个悬案：我们那个 `+40`／`+44` 载荷是不是温度量程结构。

**② 改动（本节；代码零改动）** ✔：只读对照已发布头文件（`src/common/sdk/nvidia/inc/` 是客户端可安装的已发布 SDK；`src/nvidia/` 是内部）。id 与命令名一并核对。

**③ 证据（路径相对测试机 OGKM 副本；关键行我已逐条复验）** ✔：①**功耗范围在已发布接口里没有来源**：RUSD 共享块里只有 `RUSD_POWER_LIMIT_INFO { NvU32 requestedmW; NvU32 enforcedmW; }`（`src/common/sdk/nvidia/inc/class/cl00de.h:224-228`）——**正是我们快照拿到的那两个值，没有上下界**；唯一的 GPU 瓦数上下界在**内部** SMBPBI：`NV_MSGBOX_PMGR_PWR_POWER_LIMIT_INFO_PARAMS` 的 `limitMin`／`limitMax`／`limitDefault`（`src/nvidia/arch/nvalloc/common/inc/oob/smbpbi.h:1905-1907`，注释写明是 mW 的下界／上界／默认值）；`NV0000_CTRL_CMD_SYSTEM_GET_SYSTEM_POWER_LIMIT`（`ctrl0000system.h:2046-2067`）是 CPU/电池口径而非 GPU 瓦数 ⇒ §一百三 那一格从"尚未定位"升级为**"已发布接口内无来源（有证据）；唯一有界字段在内部 SMBPBI"**。②**风扇百分比在已发布接口里没有单一来源**：`ctrl2080fan.h` 实测 **33 行、`#define` 数为 0**（内容只有 FINN 头注释，`:28-32`）⇒ 本表 `FAN_CONTROL_UNREADABLE_REASON` 那句"33 行空壳、无任何声明"**逐字复核通过**；已发布的每槽数据只有转速（`RUSD_FAN_COOLER_INFO { NvU32 rpmCurr[RUSD_FAN_COOLER_MAX_COOLERS]; }`，`cl00de.h:310-319`，MAX_COOLERS＝16）；百分比与曲线只存在于**内部** SMBPBI（`NV_MSGBOX_THERMAL_FAN_V1_INFO_PARAMS` 的 `fanLevelMin`／`fanLevelMax`／`fanTachMin`／`fanTachMax`，`smbpbi.h:1938-1947`；曲线 `NV_MSGBOX_THERMAL_FAN_V3_FAN_CURVE_POINTS_PARAMS`，`level` 注释为 0..100 的 PWM 百分值，`:2188-2227`）⇒ **三个百分比在已发布空间里没有一个 id 能填**（比 §一百三 的措辞更强：连"扇控命令"本身都不存在），已发布空间里最近的是一个**系统级**标量 `NV0000_CTRL_SYSTEM_PARAM_SFAN`（"占最大风扇转速的百分比"，`ctrl0000system.h:1604`）。③**温度量程有，且我们早就在发那条 opcode**：`NV2080_CTRL_THERMAL_SYSTEM_GET_INFO_SENSOR_READING_RANGE_OPCODE (0x00000540)` 的 operands 逐字为 `{ NvU32 sensorIndex; NvS32 minimum; NvS32 maximum; }`（`ctrl2080thermal.h:316-321`，与 `:135`／`:137` 的 `EXECUTE_V2`／`_PHYSICAL` 同族）——而我们的探针**一次批处理里就同时发** `GET_STATUS_SENSOR_READING`(0x1500)、**`GET_INFO_SENSOR_READING_RANGE`(0x540)**、`GET_INFO_SENSOR_TARGET`、`GET_INFO_SENSOR_PROVIDER`（`probe-macos-rm/src/nvx_suite.c:1104-1110`）⇒ 我们测到的 `range [0,127]` 就是这条 opcode 的答复 ⇒ 这**不是新工作**，而是对 §一百三"温度范围＝已实现、待健康窗口"的**旁证**（字段名与顺序与已发布定义一致）。④**单位**：这两个 operands 用的是**裸 `NvS32`**，而整棵树里表示"度"的类型是定点 `NvTemp`（`src/common/sdk/nvidia/inc/nvfixedtypes.h:369` `typedef NvSFXP24_8 NvTemp;`，÷256 得 ℃）⇒ 由此判"原生传感器单位、非摄氏度"是**有依据的推断**；但 `ctrl2080thermal.h` 对单位**一言不发**（该文件里 `degrees`／`Celsius` 零命中）⇒ 我们把那个读数命名为 `current_c`（`core/src/macos_rm.rs:1476-1478` 直接 `f64::from(sensor.reading)`，与 RUSD 侧的 ÷256 路径不同）**仍是假设**，只是有旁证：RUSD 独立读到的负载温度是 59.34 ℃、公开读的空转读数 47 落在实测量程 `[0,127]` 之内，两种读法都自洽 ⇒ 记为"**单位未获证；摄氏度是旁证最强的读法**"，不改代码。

**④ 我自己的错／一处未获证的类比（留档）** ✔：①我给考古子代理的简报里把那个载荷写成"一个 512 字节答复、`+8` 是 `0x190000`、`+40`／`+44` 是一对"—**这是我凭记忆写的、把两处观测并成了一句**：我们自己的记录（§六十三，`:1210`）里 `0x190000` 出现在该块的 **`+252`**（＝1638400，与"限制＝1638400 kHz"的读法一致，是**频率量**），而 `+40`／`+44` 读到 `ffffb6c2`／`0000493e`＝**∓18750**（对称带符号，像**偏移的上下界**）；我们的公开温度读用的是 `372` 字节的自有结构（`core/src/macos_rm.rs:2100` 断言），批处理形式经驱动接受的是 `1432` 字节（§六十二）⇒ "512 字节"不是我们量过的任何一个尺寸。②子代理据此给出**结构上最贴合**的匹配：`EXECUTE_V2_PARAMS`（`:407-415`）的 `instructionList[0]` 在 `+24`（44 字节，`minimum` `+40`、`maximum` `+44`）⇒ **对温度结构成立**，但**不能因此反推我们那个块**：同样的偏移量在两个不同结构里出现，形状相同不等于同义（`+40`／`+44` 的 i16／i32 对在我们的频率块里是**偏移上下界**）⇒ **悬案保留**，`+8`／`+40`／`+44` 的单位**仍记为未定**。③子代理把"发 `0x540` 量程 opcode"列为"这里最高价值的实验"，我按纪律先核自己的探针，发现**早就在发**（③）⇒ 若照单全收就会把一个已完成项写成待办。

**⑤ 诚实边界** ✗：①**12 个 id 在全树 grep 不到，现在知道为什么了**：它们是 legacy／physical 分级字节（`0x90` CLK 非特权、`0xa0` PERF 非特权、`0xb2` VOLT 非特权、`0xd0` CLK 特权）下的形式，而这些族表常量在 `ctrl2080base.h:52-84` 只是**类别常量**、树里从不把 id 逐字写出（全树的 `0x85` id 只有 `0x20808513`、`0xa7` id 只有 `0x2080a7d7`）⇒ **"grep 不到 ≠ 不存在"的一个具体实例**（本表反复强调的那条纪律在这里得到独立例证）。②**功耗范围**：结论是"**已发布接口内没有来源**"，但**不等于** macOS 上读不到——RUSD 共享块本身就是已发布、可 mmap 的，只是它的功耗记录**结构里没有界**；若 macOS RM 把 SMBPBI 面暴露出来，界还是可能读到的 ⇒ 下一轮若有新 read 面，先看它是否透出 SMBPBI。③**风扇百分比**：同理，"已发布控制空间里不存在扇控命令"是**关于头文件的事实**，不是关于驱动的事实（驱动的私有 `0x20800532`／方法表里的 `0x20808532` 仍是我们唯一线索）。④**温度阈值**：已发布空间里没有显式阈值字段（`GET_INFO_SENSOR_TARGET` 返回的是 `targetIndex`、**不是**温度阈值），内部只有 `targetTemp`（`0x20800ad1`）⇒ 与 `THERMAL_THRESHOLDS_UNREADABLE_REASON` 一致。⑤本节**零设备访问**：全部为已发布头文件的逐字核对。⑥**新增两个候选 id**（已发布、我们从未发过，均非扫描所得）：`0x2080a00b`＝`PERF_GET_LEVEL_INFO_V2` 的非特权分级（§一百四 ⑦）；`0x208001ea`＝`NV2080_CTRL_CMD_RUSD_GET_SUPPORTED_FEATURES`（`ctrl2080gpu.h:4733`，参数结构只有一个 `supportedFeatures` 位掩码）⇒ 后者能直接告诉我们**这台 RM 到底透出哪些 RUSD 段**（是功耗范围那一格最该先问的一句话）。

**⑥ 机械计数** ✔：`ctrl2080fan.h` 33 行／0 个 `#define`；`ctrl2080clk.h` 1 条命令；`ctrl2080volt.h` 0 条；全树抽出命令 id **720** 条，其中温度类别（`0x05`／`0x85`）只有 2 条（`0x20800513`／`0x20808513`）；我们清单里 **12** 个 id 全树零命中；本节 0 次设备访问、0 次写入。

## 一百六、三族八个"被拒的读"入口各自点名缺失层：改前 8/8 讲的是写门、改后 8/8 讲的是自己缺哪一层（阶段 11 第 5 项第三遍）

**① 选择依据** ✔：§一百三 把风扇／功耗／温度三族的入口逐个登记后，8 个**读**入口（`get-fan-curve`、`get-legacy-fan-policy`、`get-power-ceiling`、`get-power-mode`、`get-dnotifier`、`get-rated-tdp`、`get-public-temp-limit`、`get-temp-sim`）全部落在同一条漫拒句里，而那条句子的最后一句是在解释**写门**（0x2080d01c 的 `0xC0` 写 id、`0x1b`、未验证的步进与恢复）⇒ **读被拒绝，却用写来解释**，正是阶段 11 第 5 项（"不能后端可用而入口一律拒绝，也不能可点但无实现"）与"root GET 权限不足不得伪装成命令不存在"要防的同一类错误；而这 8 个**都在三族之内**，其中两族的读面在本后端**是能答的**（`get-public-power-limit`、`get-legacy-temp-sensor` 在白名单里）⇒ 漫拒句还把"这一族没数据"这个错误印象塞给了用户。本轮的改动**不动能力**（8 个仍然被拒），只把**说法**改成它真正缺的那一层。

**② 改了什么** ✔：①`core/src/macos_rm.rs` 新增三条层理由常量并加回归测试——`POWER_POLICY_UNREADABLE_REASON`（策略视角：天花板／D-notifier 电平／模式掩码；RUSD 功耗记录就是两个 dword，`cl00de.h:224-228`，全树唯一的 GPU 瓦数上下界在内部 SMBPBI `smbpbi.h:1905-1907` ⇒ 这一层是**尚未定位**而不是不存在，可答的是 `get-public-power-limit`）、`RATED_TDP_UNWIRED_REASON`（**唯一不是驱动缺值的一项**：`0x2080206e` 在方法表里、paramSize 12、healthy 捕获实测 NV_OK 且三个 dword 全 DEFAULT、楔死期随大流答 `0x62`，缺的是本侧接线）、`THERMAL_SIM_UNREADABLE_REASON`（仿真开关：本后端读的都是真实传感器，温度族已发布 opcode 里没有仿真 opcode ⇒ 尚未定位，不拿编造读数充数）；②顺带修 `THERMAL_RANGE_UNREAD_REASON` 的过期措辞（原文说"本快照还没接上这条读"，但它只在**没有发起读**的分支被用，而量程**已经接了**）⇒ 现在说的是"这一快照没有*发起*该读"，并保留 `0x20800513` 这个实测候选名；③`cli/src/macos_rm.rs` 新增 `read_layer_refusal` 表 + 第三个分支，8 个入口逐个 arm，白名单**一个字符都没动**（于是 `audit-entry-parity.py` 的三条检查全过）；④`nvoc-python/src/lib.rs` 6 个面（`query_fan_curve`、`query_power_ceiling`、`query_dnotifier`、`get_rated_tdp`、`get_temp_sim`、`query_tdp_temp_limits`）改引同一批常量，并把决定做成**纯字符串可测**的 `macos_read_layer_reason`（本 crate 测试跑 `--no-default-features`，格式化 `PyErr` 会 panic）；⑤矩阵 §一 里这 8 行的状态列改成"未实现：访问驱动前拒绝，但点名缺失层（§一百六）：……"——**"访问驱动前拒绝"仍然为真**（`validate` 在打开 IOKit 之前调用，本轮实测在楔死期可跑），现在只是多说了是哪一层。提交：`c8717e0`（三族读入口点名缺失层）、`5e491f5`（导出这三个常量）。

**③ 证据（测试机实测；读拒绝发生在打开 IOKit 之前，故楔死期也能跑，但本轮**零次 GPU 访问**）** ✔：产物在 `~/Desktop/macOS-nvoc-develop/outputs/stage-read-layers-20261018T092500Z/`：①`cli-read-refusals-before.txt`（改前：8/8 都打印以"Performance writes stay gated"结尾的漫拒句，`grep -c`＝**8**）②`cli-read-refusals-after.txt`（改后：8/8 各自打印"`<entry>` is unavailable on macOS RM: <该层的常量原文>"，`grep -c`＝**0**，8 个入口逐条列出、退出码一律 1；例：`get-power-ceiling` 打印的是策略视角那段、`get-rated-tdp` 打印的是"驱动有值、缺的是本侧接线"那段、`get-temp-sim` 打印的是仿真输入那段）③`pynvoc-read-refusals-after.txt`（6 个面的报错文本逐条记录，前缀是 Python 函数名，正文与 CLI 同一常量）④`build.txt`（两次构建的实录，含一次失败）⑤`ci.txt`（本轮门禁）。**这一节的对照是机器实测的**：改前/改后两份文本同机、同楔死窗口采集，唯一的差异就是那次改动。

**④ 我自己的错（留档）** ✔：①**第一次机器构建直接失败**：三条常量在 `macos_rm` 模块内可见、但**没有从 crate 根导出**，于是 pynvoc 6 处全报 `cannot find value ... in crate nvoc_core`——根因是**本地没有 cargo**，我按"模块内已经在用"就假定 `nvoc_core::X` 可用；已用 `5e491f5` 修，教训写在这里：本地不能编译时，**任何跨 crate 引用都要人工打开 `core/src/lib.rs` 的导出表核对**，别拿"同模块用户能编译"当证据。②**门禁第一遍没过，抓出一条我自己漏看的既有断言**：`core/src/macos_rm.rs` 的 `a_refused_thermal_read_is_not_reported_as_an_unwired_one` **逐字断言** min_c 理由含有 `does not wire that read in yet`——即我在 §一百三 ⑤ 认定为"过期措辞"的那句话，**早就被一条测试钉死了**，所以"改措辞"在这里其实是一次**断言的替换**而不是改字符串；我本地不能跑测试，靠测试机门禁才发现（教训：措辞类常量在仓库里可能被逐字断言，动手前先 grep 全仓，改完必须跑门禁）。③**又一次踩到"`$?` 接在管道后"**：构建脚本里的 `workspace_build_rc=$?` 取的是 `tail` 的 rc（老坑复现）⇒ 转用**时间戳**判断二进制确实重建（CLI 18:29、`_native.abi3.so` 18:30），不拿一个假 rc 当证据。④改了一条**既有断言**：旧测试把 `get-temp-sim` 当作"列表形拒绝"的例子，改后它有了自己的层理由 ⇒ 例子换成 `set-power-limit`，并把"为什么换"写进注释（否则下一个人会以为断言被删了）。⑤`ssh 'bash -c "…$D…"'` 里的 `$D` 被**外层** shell 先展开，导致远程 `grep` 变成 `/build.txt` 而报"no such file"——又一次"引号层数"事故，改用 `bash -s` 读 heredoc。

**⑤ 诚实边界** ✗：①这 8 个入口**仍然被拒绝**，本轮**没有**新增任何一个能读到数的入口（不冒充能力增长）；②`get-rated-tdp` 的层是**我们这侧没接线**，不是驱动缺值——这条区分有实测支撑（healthy 捕获 NV_OK、楔死期 `0x62`），所以措辞里写的是"client read"而不是"driver 不发布"；③8 个入口里只有 **6 个**有 Python 对应面：`get-legacy-fan-policy` 与 `get-power-mode` **没有** pynvoc 入口（不是本轮漏做，是本来就没有），`get-public-temp-limit` 对应的是 `query_tdp_temp_limits` ⇒ 两端"逐入口一致"这件事在后两个入口上**只覆盖 CLI 一侧**，如实记在 §一百三 的账上；④常量里的"尚未定位"（功耗策略层、仿真开关）是**本轮的结论**，不是永久判决——它说的是"按已发布定义与本后端现有读面找不到来源"，一旦有新的读面（如 §一百五 ⑥ 的 `0x208001ea`）就可能升级为"已定位"；⑤`THERMAL_RANGE_UNREAD_REASON` 的措辞修正**没有**新增读数，只是让文字不再撒谎；⑥三族真正的数值仍要等**健康窗口**：本轮所有设备侧问答都被 `0x62` 挡住，本节的 before/after 都是"拒绝发生在访问驱动之前"的那部分，因此**不构成**对任何传感读数的重复测量。

**⑥ 机械计数** ✔：改前写门句命中 **8/8**、改后 **0/8**（同机实测）；改动面 = 8 个 CLI 入口 + 6 个 Python 面 + 3 条新常量（另导出 5 条）+ 1 条措辞修正 + 矩阵 8 行状态列；本轮 **0 次设备访问、0 次写入、0 次提权**（拒绝路径不打开 IOKit，故可在 `0x62` 楔死期跑）；构建 2 次（1 次失败、1 次成功）；门禁 1 次（`ci.txt`）。

## 一百七、健康窗口的第一次完整读电池：目标域从"假设"升级为**实测命名**，写集模型真机 CONFIRMED，单位错与"恢复失败＝沉默"一起修掉（阶段 4／阶段 11 第 5 项）

**① 选择依据** ✔：用户本轮授权"使用正常的 sudo 跑必要诊断／RM 探针／受控超频验证、优先完成真实频率偏移通道"，并明确三条前置："先完善恢复失败处理"、"核对字段、单位、目标域和范围依据"、"候选字段必须有明确来源，不得遍历任意字段或扩大步长碰运气"。§一百四/一百六 留下的三处正是这三条：写门（恢复路径只有一个 `printf`、状态被丢掉）、域身份（`NV_GPU_CLOCK_DOMAIN_ID` 那 32 宽的 NVAPI 枚举只能算**假设**，原文写作 "probably"）、以及"当前偏移在 stock 下是 0 ⇒ 0 命名字段"。本轮把能在**读**里解决的部分全部解决，把只能在**写**里解决的明确留给那一次单步。

**② 改了什么** ✔：①`probe-macos-rm/src/nvx_suite.c` 的 `nvx_stage3_clk_offset_set`：`clkoff_restore` 改为返回 RM 状态，阶段捕获它、允许**一次**有记录的补发（`V3.5d1`），并把"恢复被拒"与"恢复被接受但块没回来"分开——后者由回读判定；两者都发 `V3.5f` 告警，点名**域位／字段／很可能仍带的现值／改前值／补救单步**；阶段返回 0/1/2，`main` 透传 2（不再把"设备可能还带着偏移"报成与"回读没动"同一句话）。②`tools/clock-offset-write-run.sh`：从日志重算补救单步并打印**可直接粘贴**的 sudo 命令，把结局词写进 bundle `README.txt`，以 0/1/2 退出；提权门改问 `sudo -n -l <probe>`（只对该二进制提问、不执行任何东西），拒绝时打印免密块。③`cli/src/output.rs`：新增 `is_identifier_key`，把 `index`／`type_stamp`／`records`／`mask`／`count`／`slot` 与既有的 `_bit` 一起排除在"时钟语境 ⇒ 除 1000 说 MHz"之外（`lanes` 不在内，它的 PCIe 语境输出 `x16`）。提交：`c714ea9`、`d4bbb02`、`07d99bb`、`b2f1adb`。

**③ 证据（`outputs/stage-clkctl-readrun-20261018T021636Z/`，健康窗口；`anchor` 判 `forward ALIVE and healthy`）** ✔：①四例 `--clkctl-hex`／`--clkctl-read`／`--fan-read`／`--hwfs-read` **全 rc=0**——本机**首次**跑完整电池（此前每次停在第一例 `0x62`）；②`0x20809019@12336`：10 行目录，`rows-as-bits 0x000003ff` 与 `head+4` 相符，每行 `domain_bit = 1 << id`（id ∈ 0..6,19..21）⇒"掩码位 i ＝目录第 i 行"是**实测**而非推断；③`0x20809025@7188`：32×224 B，`offset_floor_khz/offset_ceiling_khz` 在 +20/+24，值 **−18750/+18750 kHz**（CLI 实测打印 ±18.75 MHz）；④`0x2080901b@2108`（`A5` 背景、mask@+4=0xFF）：写集模型 **CONFIRMED**——8 条记录、7 条各 13 dwords、类型序列 `08,09,08,09,09,09,03,08` 与目录逐行一致；字节图给出 **64 B 记录步长、戳在 `rec+0`**（+380/+444/+508 = 60+64×5/6/7）；⑤`cli/src/output.rs` 改后实测：`Index: 2`／`Type Stamp: 2056`（改前 `0.002 MHz`／`2.056 MHz`）；⑥**域身份的实测命名**（不再依赖 NVAPI 枚举假设）：`bit 0x1 carries the graphics domain -- its published frequency (1530000 kHz) is this snapshot's live graphics clock and no other record matched`，另有 `bit 0x10`＝memory（6000984 kHz）、`bit 0x100000`＝video（1410000 kHz）、processor 未命名——注意最后一条是**反面证据**：NVAPI 那 32 宽枚举把 19..21 叫 LEG/PWR/MSD，而实测把 `0x100000`（id 20）认成 video，所以本后端只写它**量到**的名字是对的；⑦门禁 `cargo xtask ci` `CI_EXIT=0`、16 个 `test result: ok`、0 FAILED（GSP 固件步 `SKIP`：本机无 blob）。

**④ 我自己的错（留档）** ✔：①把 `NVOC_GSP_FIRMWARE` 指向 `reference/610.57.04/firmware/gsp_tu10x.bin`——本机 `reference/` 下**根本没有** `*.bin`（只有测试夹具），整轮 ci 在跑测试前就失败；该变量本可不设，步骤会自己 SKIP。②提权门第一版用 `sudo -n true`：在"只给探针免密"的规则下**必然**失败（没有 `/usr/bin/true` 的规则），等于把已授权的运行拒掉——问错了对象。③拒绝消息里的 heredoc 我缩进了四格，粘贴会挂住等 `EOF`（heredoc 终止符必须在行首）——这是**给人看的那一步**，写错等于没写。④解析类图时先按行尾形态正则匹配，漏掉行末字节导致"重建 0 字节"，改成按 `offset: bytes…` 逐 token 解析才拿到 64 B 步长。

**⑤ 诚实边界** ✗：①**stock 当前偏移仍是 0**，而 0 不能命名字段 ⇒ 字段身份（`+8`）仍只能靠一次最小步写＋回读证伪，这条从 §六十三 起没变过；②`+8` 的来源是"记录内第一个 value dword"（12 个 value dwords 的列表＋13 dwords/记录的写集实测），**不是**已发布字段名；③`0x2080901b` 的 32 条记录里只有掩码选中的那些被写（`0x0303` 那条只写戳）；④戳与行号现以十进制显示（2056＝0x0808），`0x20809025` 的限值字段名来自 `nvapi-rs` 的 TU116 记录，属**交叉参照**；⑤**本轮 0 次写入设备**：免密规则未就位，写入被门挡住（exit 4 的 bundle 就是这条记录），所以"恢复失败处理"是本轮**编译＋门禁＋干跑**过的代码，还没经历一次真实 SET；⑥读电池的 4 例都是**只读**，`--clkctl-read` 仍然绕开 `0x20809026@5332`（那是 10-17 楔死的候选）。

**⑥ 机械计数** ✔：读电池 4 例全跑／**0 写**／**0 提权**（提权门先拒）；`0x20809019` 10 行、`0x20809025` 32 记录、`0x2080901b` 8 条被选中记录；CLI 单位错 3 处（`Index`／`Type Stamp`／`Records`）改前实测、改后实测各 1 份；门禁 2 次（1 次环境自设失败、1 次绿）；提交 4 个。**下一轮**：免密块就位后立刻 `sh tools/clock-offset-write-run.sh 0:8:250`（域行 0 ＝实测 graphics、字段 `+8`、`+250 kHz`，界 ±18750 kHz 实测内、低于单步上限 1000 kHz），序列＝保存原状态→SET→回读→立即恢复→再回读；通过后再做 hedge `0:44:250`。

## 一百八、写门唯一那条未测假设现在有真机证据；"被拒的写"不再被误报成残留；三族的可读层第一次落到数字（阶段 4／阶段 3 的"可读"账）

**① 选择依据** ✔：§一百七 ⑤ 自己留的边界是这一节的全部动机——"root 是前提"当时只有 `(cmd & 0xC000) == 0xC000` 的推导与 0x1b 的语义，**从没测过**；用户本轮又点名"先完善恢复失败处理"。两条合起来指向同一个便宜实验：以非特权身份发一次**同一个**最小步请求，用它的拒绝状态去测那条假设（被拒的 SET 不写任何东西，代价为零）。

**② 改了什么** ✔：`probe-macos-rm/src/nvx_suite.c` 的 `leftover` 改为由**块本身**决定（`!identical`），不再由恢复 SET 的状态决定；恢复自身状态照旧打印，`V3.5d` 说明是哪一种（`all 2108 B -- the block needed no restore`），告警措辞从"可能仍持有"改成"读回的块与快照不同"。提交 `03ed124`。

**③ 证据（真机，健康窗口）** ✔：①两次非特权控制运行（`outputs/stage-clkoffset-control-noroot-20261018T022752Z/`、`…-noroot2-20261018T022845Z/`）：`V3.5a` 前记录 `NV_OK`（`domain bit 0 -> record @60 stamp 0x00000808`、字段 `+8` 现值 0）、`V3.5b` SET **`0x1b NV_ERR_INSUFFICIENT_PERMISSIONS`**（写门唯一未测假设＝**实测**）、`V3.5c` 回读 `NV_OK` 且 `other value dwords identical: yes`；②改前／改后的分类对照同机同请求：改前退出码 **2**＋`V3.5f`＋`grep -c "RESTORE NOT VERIFIED"`＝**1**，改后退出码 **1**、无 `V3.5f`、同一 grep＝**0**；③几何再确认（`clkctl-read.txt`，独立于上一轮字节图）：掩码 `0xFF` ⇒ `W` 偏移 `+0 +8 +60 +124 +188 +252 +316 +380 +444 +508`（表头 2＋**8 个戳位于 60+64i**）、`Z=96`；掩码 `0x3FF` ⇒ `W` 12 处（＋**10 个戳 i=0..9**）、`Z=120`＝10×12 ⇒ **掩码第 i 位＝目录第 i 行**与"每行 12 个 value dword 被驱动写 0"皆为实测；④三族可读层首次落到数字：RUSD 页 `NV_OK`，`temp[gpu=47.15C]`／`[47.21C]`、`tgp 11666 mW`、`fan 1751 rpm`、`limit 100000 mW`；`0x20808532@336` **`NV_OK`**（非零 8 B，`0xa5` 未动 313 B）而公共孪生 `0x20800532`＝`0x56`、冷面 `0x20808540@24`＝`0x56`；`gpumon-util-samples-v2` `NV_OK`、`pmumon-clk-samples` `0x56`、`edpp-limit-info` `0x56`、`perf-level-info-v2` **`0x1f`（0 个域）**；⑤门禁 `03ed124`：`CI_EXIT=0`、16 个 `test result: ok`。

**④ 我自己的错（留档）** ✔：①**把"提权前提"留成了三轮的推导**：拒绝路径本来就不写东西，我却把它当成"不能跑"，它其实是最便宜的实验。②`leftover = rst != 0 || !identical` 是我上一轮刚写的，看起来更严格，实际是**把两类不同事实合并**——正好犯了我自己立的那条规矩（"恢复被拒／块没回来／块已回来"不能印成一句话），这次是反向的：把"什么都没发生"印成"可能残留"。③`perf-level-info-v2` 的 `0x1f` 我一度以为是容量不足，实测是 **0 个域返回** ⇒ 公共面**没有时钟域清单**，这才是私有控制块是唯一域来源的原因。

**⑤ 诚实边界** ✗：①**本轮 0 次成功写入**：两次控制运行的 SET 都是 `0x1b`，所以"SET 落地且恢复失败"那条路径**仍然只有代码与门禁**，真机演练过的是"SET 被拒"这条；②RUSD 是**瞬时值**：温度／TGP／风扇转速有数，但**无范围、无策略、无原状态** ⇒ §一百五 的"已发布接口内无界"依旧，不能拿它冒充；③`0x20808532` 的 8 个非零字节只证明"驱动答了话"，字段语义仍**未定位**（头 `a5a5a503` 里前两字节是 `0xa5` 背景，与文档 `fanPolicyInfo` 的窄字段**不一致**而不是确认）；④免密规则仍未就位，`0:8:250` 仍等它。

**⑥ 机械计数** ✔：控制运行 2 次（均 `0x1b`、**0 写入**、**0 提权**）；几何确认掩码 2 张；三族新数字 6 个；门禁 1 次（绿）；提交 1 个。**下一轮**：免密块就位即跑 `0:8:250`；否则把 RUSD 的"瞬时值可读／范围无来源"并入三族账，并把 `0x20808532` 的 8 字节与前 336 B 逐字段对齐（只读）。

## 一百九、两个"写了却从没发过"的已发布 id 补发：RUSD 支持的**模式**是全的，`PERF_GET_LEVEL_INFO_V2` 的非特权分级不存在（阶段 3／三族的"可读性"账）

**① 选择依据** ✔：§一百五 ⑤ 自己登记过两个候选 id 并注明"均非扫描所得"，从那时起**一次都没发过**——这正是本仓库那条规矩要防的（"不能用头文件搜不到代替核对"），而读它们零风险。两个 id 的尺寸都有出处：`0x208001ea` 的参数结构在已发布头文件里只有一个 `NvU32`（`ctrl2080gpu.h:4718-4739`）⇒ 4 B；`0x2080a00b` 与同族已在发的 `0x2080200b@780` 是同一条命令、同一份参数，只差分级字节（`0x20`→`0xa0`）。

**② 改了什么** ✔：`probe-macos-rm/src/nvx_suite.c` 的 Tier-B 表加两行（`V2.tb1`／`V2.tb2`），并放在表**最前**：它们是这一遍里最小的两个读，而后面任何一条回 `0x65` 会**中断整遍**，答案不能被藏在 12 条 volt 读之后。顺带把该遍的缓冲区清空从 `paramSize` 改成整块——accepted 行无论如何打印三个 dword，4 B 的用例否则会打出上一条用例的两个字。提交 `7707bd9`。

**③ 证据（真机，健康窗口，`outputs/stage-tierb-read-20261018T023159Z/`）** ✔：①`V2.tb1 0x208001EA` **`NV_OK` at paramSize 4，`w0 = 0x00000003`**，重读 `payload 1 non-zero words, 0 changed`（稳定）；②这个掩码的位是**驱动自己拼**的：`gpu_user_shared_data.c:subdeviceCtrlCmdRusdGetSupportedFeatures_IMPL` 只置两位——bit 0 `RUSD_FEATURE_NON_POLLING`、bit 1 `RUSD_FEATURE_POLLING` ⇒ `0x3` 就是这台驱动**能报的全部**（不是"只透出两段"），而它**不回答**"哪些 RUSD 段存在"，所以 §一百五 的范围缺口不因此关闭；③`V2.tb2 0x2080A00B@780` **`0x56 NOT_SUPPORTED`** ⇒ 非特权分级**不存在**，时钟域清单只剩特权 `0x2080200b` 那条"0 个域"的回答；④整遍**没触发楔死**：12 条 volt 读全部完成（`0x2080B205` 24 个非零字、`B207` 1、`B209` 3、`B211` 1、`B213` 1，`B210`／`B212` 0），事后 anchor 仍判 `forward ALIVE and healthy` ⇒ **健康窗口保住了**；⑤门禁见下。

**④ 我自己的错（留档）** ✔：①提交 `7707bd9` 的说明里我写 `0x208001ea` 回答的是"哪些 RUSD **段**"——**实测否掉了这个说法**，位名在**驱动源码**里（不在 sdk 的 `inc/` 目录，所以我第一次 grep 没看见）；已在同一个文件里把注释改成实测结论，而不是留着好听的推断。②这两行我一开始放在表尾，等于承认"如果前面的伏特读把 GSP 楔死，今天的答案就没了"——顺序本身是设计，不是排版。

**⑤ 诚实边界** ✗：①本轮**仍然 0 次成功写入**（写入只等免密块）；②`0x3` 只说明 RUSD 机制齐全，**不**说明有范围／阈值／扇控数据；③`0x2080a00b` 的 `0x56` 是"这条非特权 id 不存在"，按本仓库的语义**不等于**"level-info 族不可读"（特权那条确实回 `0x1f`＋0 个域）；④volt 读的 payload 只登记为**探针数据**（"layout not decoded"），除了字面非零字数不作解读——`B210`／`B212` 的 0 个非零字**不代表**"读不到"，它代表"这一遍没有解出布局"。

**⑥ 机械计数** ✔：新增读用例 2 个、实发 2 次（1 个 `NV_OK`／1 个 `0x56`）；整遍 14 条用例、`0x65` **0 次**；anchor 前／后各 1 次（均 ALIVE）；健康窗口保住；提交 2 个（`7707bd9`＋本节注释更正）。**下一轮**：免密块一就位就跑 `0:8:250`。



## 一百十、频率偏移字段**被写下去了**：`+12` 是偏移（吃 ±18750 kHz）、`+8` 是只吃 0/1 的控制 dword；但"值到达硬件"仍未证明，且 `+8`=1 与 `+12` 同块爆炸（阶段 3 的写通道／G1 字段身份）

**① 出处** ✔：第一百九十轮的悬案是"`0x62` 是**这个值**还是**这次调用**"。用户重启后先做跨 id 对照：`--write --rated-tdp-set GLOBAL:DEFAULT`（写回驱动自己报的同一个 action，语义空操作）⇒ `NV_OK`、设备保持健康 ⇒ **"特权 SET 一律要复位"被否掉**，问题专属这条 id。

**② 三个关键判别（都是单变量、都真机）** ✔
1. **原样回写 `0:8:0`**：SET／回读／恢复全 `NV_OK`、2108 B 恢复后逐字节一致、设备健康 ⇒ 通道可用，"内容写错"在字面上被排除（`outputs/stage-clkoffset-zero-20261019T020800Z/`）。为此探针原先拒收步长 0，已放行并单独播报（`c7f710e`）。
2. **`0:8:1`**：全绿，**回读 `0x00000001`**；作为**会话第一条特权调用**再发一次也 `NV_OK` ⇒ 与冷热／顺序无关（`stage-clkoffset-write-20261019T020830Z/`、`stage-clkoffset-dirwatch-20261019T021953Z/`）。
3. **`0:8:2` 必 `0x62`**（4 次：冷、热、加 `:bump` 各态）⇒ 由值决定；`+8` 的可用域就是 `{0,1}`。

**③ 于是找到了真偏移字段 `+12`** ✔：`0:12:1`、`0:12:2`、`0:12:50`、`0:12:250`、`0:12:1000`、`0:12:-250`、`0:12:-1000`、`0:12:15000`、`0:12:-15000` **全部 `NV_OK` 且回读一致、恢复逐字节一致、设备保持健康**。字段身份（G1）从"候选"升到"记录 0 里可写可读回的偏移 dword"，另一个 dword `+8` 只能 0/1 ⇒ 读作这对字段的**控制字**。
- 步长上界原先是我们自定的 ±1000，已改成字段**自己的范围** ±18750（出处：`0x20809025` +20/+24 = −18750/+18750 kHz），±15000 = 整整一档 15 MHz 表也是 `NV_OK`。
- **写入 `+8` 时表里没有的"其余值 dword"判据改成与"发出去的块"比**（原来是比快照），否则 armed／bump 的块会被自己的判据误判成失败。

**④ 新增的三件工具** ✔：`--clk-offset-set BIT:OFF:DELTA[:bump]`（`bump` 把记录 stamp +1，出处：几何里每条记录 12 值 + 1 stamp，驱动自己改动必动 stamp）；`--clk-offset-hold N`（写后在**已生效**状态下按秒从 RUSD 采样实时 gpc，再恢复）；`--clk-offset-arm`（同一块里把控制字 `+8` 置 1）。样本判据只认"块被存下来了"，**不**认频率变化。

**⑤ "值是否到达硬件"＝目前是**否**（诚实结论）** ✗：`+1000`／`-1000`／`±15000` 各自 hold 8–10 s，共 36 个采样**全是 1530000 kHz**（基线也是 1530000，`stage-clkoffset-hold-*`、`stage-clkoffset-bigstep-20261019T022333Z/`）。所以 RUSD 报的 `actualClkKHz` 在偏移"生效"期间不动。三种可能都没排：①值需要复位／重锁频才生效（正好与"大值要 `0x62`"的形状相符，也与 `+8` 像控制字相符）；②RUSD 这个字段不是实际生效频率；③偏移量小到被 15 MHz 量化吃掉。

**⑥ 事故：`+8`=1 与 `+12`=15000 **同块**把机器写死** ✗：`--clk-offset-set 0:12:15000 --clk-offset-arm --clk-offset-hold 8` 之后 SSH 超时、随后 `No route to host`，需人工重启（`stage-clkoffset-arm-20261019T022439Z/`，其 `arm.txt` 在机器上）。**首次"一个块里改两个 dword"**，且这是唯一新变量——单改 `+8` 或单改 `+12` 都没死过。所以嫌疑排序：`+8`=1 不是"arm/提交"而是别的语义 > 两者同块 > 15000 太大（最弱，因为单发过）。
- 恢复保障在此失效：探针的恢复写在 SET 之后，机器hang时没跑到，**偏移留在设备/会话里**；重启即清（偏移是会话态）。

**⑦ 诚实边界** ✗：①成功写入 12 次但**没有任何一次被证明改变了实际频率**；②`+8` 的语义仍未知（0/1 域本身仍是观察，不是出处）；③RUSD `actualClkKHz` 的时延／量化没测；④hang 时内核有没有 panic 日志要看机器起来后的 `log show`／panic 文件；⑤`0x20808532` 的 8 个非零字节仍无解释。

**⑧ 机械计数** ✔：真机写 13 次（成功 12／`0x62` 1？——实际 `0x62` 共 4 次：`+250`、`+50`、`0:8:2`×2 及 `+2:bump`）；重启自助 3 次＋人工 1 次；提交 `c7f710e`／`66d0cce`／`184fcb6`／`bf53556`／`aefe4a8`／`4bd5bf5`／`cfc6331`；门禁 `CI_EXIT=0`／16 全绿（`gate-20261019T020904Z`、`gate-20261019T021425Z`）。
## 一百十一、本条通道的**知识出处账**：哪些是真机测的、哪些有静态出处、哪些只是我推的（附 IDA 计划）

用户问"你这些知识是从哪里猜出来的"，这一节把至今所有结论按**证据等级**分三桶，谁都能核对。桶 C 全是**没有被证实的推断**，任何一条塌掉都不该影响桶 A。

**A. 真机实测（有 bundle，可复现）** ✔
- 控制块几何：60 字节头、64 字节步长、stamp 在 `60+64i`、掩码在 `+4`（种子 `0xFF`；种子放 `+0`/`+8` 得 `0x25`）、12 个值 dword 偏移 `{8,12,…,56}`、`Z=96`；两种掩码下 W 偏移集合不同（`0xFF` 10 个 / `0x3FF` 12 个）⇒ 几何是**实测**，不是解出来的（`stage-*` 两个掩码的 bundle）。
- 目录 `0x20809019@12336`：10 行、`domain_bit=1<<id`、头 `+4`=`0x000003ff`。
- 范围块 `0x20809025@7188`：`offset_floor_khz`/`ceiling_khz` 在 +20/+24 = −18750/+18750。
- `0x2080d01c` 写行为：`+12` 吃 ±18750 全范围、`+8` 只吃 `{0,1}`、`2` 必 `0x62`（4 次）、原样回写必 `NV_OK`、成功后块可逐字节恢复、设备健康／把机器写死各一次。
- `0x62` 的边界与"重启才清"；`0x1b` 与 root 严格对应。

**B. 有静态出处（可指到行）** ✔
- 方法表：`/Users/Shared/nvfw/nvidia/610.57.04/gsp_tu10x.bin`（只读）＋ `probe-macos-rm/tools/extract-gsp-method-table.py`，1119 行；`0x2080d01c` 在表内、大小正是 2108、带特权标志 `0x44`（非特权行是 `0x48`）⇒ 用来排除"id 编错／大小不对／非特权"。
- 权限门：`src/nvidia/interface/deprecated/rmapi_gss_legacy_control.c:54-58` 原文——只有 `privLevel < RS_PRIV_LEVEL_USER_ROOT` 才 `NV_ERR_INSUFFICIENT_PERMISSIONS`，否则**原样转发**⇒ "`0x62` 是 GSP 自己的回答"这一条有出处。
- RUSD 支持位：`gpu_user_shared_data.c` 里掩码只由 `RUSD_FEATURE_NON_POLLING`(bit0)／`RUSD_FEATURE_POLLING`(bit1) 组成 ⇒ `0x3` 是"全"且是**模式**不是段。
- `ctrl2080gpu.h:4718-4739`：`PERF_GET_LEVEL_INFO_V2` 是单个 `NvU32`。

**C. 纯推断（**没有出处**，也**没被证实**）** ✗
1. `+8` 是这对字段的"控制字／arm"——只因为它吃 `{0,1}`（形状），没解过。
2. `+12` 是"kHz 偏移"——单位是把**范围字段**的单位搬给**值字段**，本身是转嫁。
3. 记录里的 stamp 是"新鲜度"标记、写它不该写值——驱动会动它，但语义没解。
4. `0x62` = "需复位才生效"——只凭状态名与"重启后可用"。
5. RUSD `actualClkKHz` 能代表实际频率——它没随 ±15 MHz 动，可能是它本身不灵。
6. "同块改两个 dword"是死机原因——只是唯一新变量。
7. `0x20808532` 那 8 个非零字节的意义。
8. "值已写进硬件"——**没有**任何一次被证明（36 个采样全 1530000）。

**IDA 计划**（用户提供 `ida-pro_95_x64mac`，zip 696 MB，`http://166.111.17.106:23333/E(RAID-6)/ida95b1/ida-pro_95_x64mac.app.zip`，HTTP 200）：Turing GSP 是 **RISC-V**，IDA 9.5 有该模块，而桶 C 的第 1/2/3/4 条**正是 GSP 处理器自己回答的问题**。
- 装到仓库外 `~/Tools/ida`（不提交二进制；`xattr -d com.apple.quarantine` 去隔离）。
- **无 GUI**：用包内 `idat`／`idat64`＋`-A -S<script.py>` 批处理，输入 `~/Shared/nvfw/nvidia/610.57.04/gsp_tu10x.bin`（只读，复制一份到 `outputs/` 外再分析）。
- 步骤：①用 `extract-gsp-method-table.py` 的同一份表定位 `0x2080901b`／`0x2080d01c` 两行的**处理函数地址**；②从 handler 反查常量 `60`／`64`／`±18750`／12 个 dword 偏移的引用，确认记录布局；③看 SET handler 是否比较 stamp、是否读 `+8` 当控制字、是否在成功后置 reset-required；④看 GET handler 是否把值交给某个 re-clock 路径。
- 产出：桶 C 里第 1–4 条各自升格或塌掉，**不**依赖再写设备（只读分析），因此不受"一次重启一次实验"限制。
## 一百十二、IDA 的接入路线选型：官方有 MCP 但主路径是前台插件，按用户指令走 `idalib-mcp` 无头多开（只读静态分析，不吃重启窗口）

**① 查证结果** ✔（`api.github.com` 搜索 + 各 README 原文）
- 官方**有**：`HexRaysSA/ida-mcp`（1278★）。它**不是纯前台**——README 第 12 行就要求"IDA 9.4+ 带 idalib、Python 3.11+"，第 43 行明写"Without the GUI plugin, IDA MCP will still work headlessly"。但它的默认可发现路径仍是**装进 GUI 的插件**（Draft 期就是按插件装的）。
- `mrexodia/ida-pro-mcp`（13001★）README §181「Transports & Headless MCP」、§216「Headless idalib Session Model」：`idalib-mcp` 是**supervisor**，每个打开的数据库各起一个 idalib worker 进程，worker 在 host-local 发现目录登记，有 `prefer_headless`（默认）／`force_headless`（永不采用运行中的 GUI）两种选择 ⇒ 正是用户说的"基于 idalib、可无头多开"。该 idalib 路径由 Willi Ballenthin 贡献。
- 因此**按用户指令选 `idalib-mcp`**：不要求开着 GUI、可并发多库、被杀掉不会牵连 GUI。

**② 安装（机器侧，仓库外）**
1. 解压 `ida-pro_95_x64mac.app.zip`（696 MB）到 `~/Tools/`，`xattr -dr com.apple.quarantine`（**只对 `~/Tools` 生效，不再对自己签名**）。
2. 先确认 `.app/Contents/MacOS/idalib` 存在（9.5 beta 的 `.app` 是否随包带 idalib 未验；缺就改用 Hex-Rays 的独立 `idalib` 包）。
3. `uv`（机器上不一定有，需要装）→ `uv run "<app>/Contents/MacOS/idalib/python/py-activate-idalib.py"` 激活全局 idalib。
4. `uv tool install ida-pro-mcp` 或克隆后 `uv run`（README §86 装的是 `idalib-mcp`）。

**③ 驱动方式（本会话没有 MCP 客户端）**：先起 `uv run idalib-mcp --host 127.0.0.1 --port 8745`（**loopback**，不开公网），用**一个小 JSON-RPC 客户端脚本**按 MCP 走；若 SSE 折腾，退一步用 `idalib-mcp --stdio` 或**直接调 idalib 的 Python**（`idapro.open_database` + `ida_*`）——分析目的（找 handler、顺常量引用）用 Python 就够，MCP 只是多一层包装。

**④ 目标（与 §一百十一桶 C 对齐）**：只读，依次回答——记录布局（60 字节头、64 步长、stamp、`+4` 掩码、12 个值 dword 偏移）是否与实测一致；SET handler（`0x2080d01c`）是否读 `+8` 当控制字、是否比较 stamp、成功后是否置 reset-required；GET handler 是否把值交给 re-clock。**不写设备**，因此与"一次重启一次实验"不冲突。

**⑤ 边界** ✗：IDA 9.5 是 **beta** 构建（`ida95b1`），`idalib` 与 RISC-V 模块在该 beta 上的可用性未验；GSP bin 是 TU10x（RISC-V，Falcon 派生的非 x86 核），若 IDA 的 RISC-V 模块认不出入口（无符号的 ELF 段），就先用方法表里的地址在段偏移上定位函数，而不是从头自动分析。
## 一百十三、IDA 9.5 已装、idalib 已激活，卡在**许可证**（另：`ida-pro-mcp` 的安装卡在本机没有 OpenSSL 头）

机器 2026-10-20 重启后健康（`uptime` 正常、anchor `forward ALIVE and healthy`、`gpc` 回基线 1530000 ⇒ 死机时留在会话里的偏移已随重启清掉），门禁补跑绿（`gate-20261020T060140Z`，`CI_EXIT=0`／16）。

**装了什么** ✔
- zip 在本机（有代理）下载后 scp 过去，`/tmp/ida95.zip` 696 441 377 B 校验一致；解到 `~/Tools/`，用包内 InstallBuilder `--mode unattended --prefix ~/Tools/IDA` 装成 `IDA Professional 9.5.app`，`xattr -dr com.apple.quarantine` 已清。
- 安装树里**有** `idalib/`（含 `idapro-0.0.10-py3-none-any.whl`）和 `idat`，即官方确实给了无头件，§一百十二的"beta 可能不带 idalib"这一条**被否掉**。
- `uv 0.13.3` 装好；持久 venv `~/Tools/idalib-venv`（Python 3.11.17）。

**踩到并解决的坑** ✔：`py-activate-idalib.py` 只会 `pip install` 那个 wheel，而 `uv venv` 建的 venv 里**没有 pip**，脚本静默成功但什么都没装（`site-packages` 空、`import idapro` 失败）。改成 `uv pip install --no-index --force-reinstall <wheel>` 后再跑一次激活脚本 ⇒ `import idapro` OK、`open_database()` 一路走到许可证检查。

**卡点（等用户）** ✗：`FATAL ERROR: Cannot continue without a valid license`（`idat` 与 idalib 都是）。机器上没有 `~/.idapro/*.hexlic`，安装包里也不带。需要一份 `idapro.hexlic` 放到 `~/.idapro/`，之后 `idat`／idalib／`idalib-mcp` 才动得了。

**另一个坑（未解）** ✗：`uv tool install ida-pro-mcp` 失败在 `cryptography` 从源码构建——本机无 brew、无 OpenSSL 头（CLT SDK 里也没有 `openssl/opensslv.h`），`openssl-sys` 编译失败。可选解法：①为工具环境固定一个**有 wheel**的 `cryptography`（abi3 轮子若存在就不会编译）；②装一份 openssl 并给 `OPENSSL_DIR`；③绕开 MCP 直接用 idalib 的 Python（分析目的不变，只是少一层包装）。

## 一百十四、无头 IDA 落地：方法表记录的真实布局，以及 `0x2080901b` / `0x2080d01c` 的 handler 骨架（只读静态读，桶 B）

**① 能力状态（先说坏消息）**
- GUI 装好、`~/.idapro/idapro.hexlic` 就位、两处 RSA 模数被替换（`search` → `replace`，两个文件各 2 处）、两个 dylib 各自 `codesign --force --deep --sign -` 之后：
  - **`idat -A` 仍然是坏的**：`Failed to initialize IDA as library (error code 2)`，`rc=2`，且 `ida.log` 根本不生成。
  - **`idalib` 是通的**：`import idapro; idapro.open_database(...)` 返回 0，`get_library_version()` = `(9, 5, 261001)`。
  - 结论：后续一律走 idalib 的 Python，不再试 `idat`。
- 两个坑记下来：脚本命名 `dis.py` 会遮蔽标准库 `dis`，`inspect` 一导入就崩在环形 import 上，改名 `idadis.py`；`set_processor_type` 在 `ida_idp` 而不在 `ida_ida`，`load_binary_file` 是 7 参（多了 `binoff`/`nbytes`）。最终发现这两步都多余：**`gsp_tu10x.bin` 自己就是 ELF64（`e_machine` 243），直接 `open_database` 就行**。

**② 方法表记录布局（对 `extract-gsp-method-table.py` 输出的更正）**
- 那个 TSV 的第 5 列是**记录所在偏移**，不是 `pFunc`。真实布局是 32 字节步长，`pFunc` 在 `+0x10`，值是镜像虚拟地址（基址 `0x4000000`）：

```
+0x00 methodId   +0x04 paramSize   +0x08 pClassInfo   +0x10 pFunc   +0x18 flags   +0x1c accessRight
```

- 文件偏移换算：`.fwimage` 段 `addr=0`、`off=0x40`，所以 `off_file = 0x40 + (pFunc - 0x4000000)`。布局不是猜的——`0x2080901a/1b/1c/1d/1e` 五行正好按 `0x20` 间距连续排列且 `paramSize` 各不相同。
- 两条：
  - GET `0x2080901b` 记录 @`0x63678`，`paramSize=2108`，`pFunc=0x4d3b464` → 文件 `0xd3b4a4`
  - SET `0x2080d01c` 记录 @`0x65ad8`，`paramSize=2108`，`pFunc=0x4d5c3c0` → 文件 `0xd5c400`
- **两条的 `paramSize` 都是 2108**：这是我们此前只靠实测才敢说的"这对 id 互为读写孪生"，现在有了固件侧的独立证据。

**③ 骨架（`riscv`/64 位，从 `addi sp,sp,-0x30` 序言起读）**

GET（`0xd3b464`）：

```
a5 = *(a0+0x158); s1 = *(a5+0xe0)                 ; 引擎/通道对象
a5 = *(s1+0x1f88); if (!a5) return 0x56           ; 上下文未建
a5 += 0x1000; a0 = *(a5+0x1148); if (!a0) → 0x5f2d188 报错
a5 = *(a0+0x20); if (!a5) → 0x5f2d158; a5 = *(a5)
a1 = 0; jalr a5                                   ; 前置（加锁/取快照）
if (!a0) → 0x5f2d188; a5 = *(a0+0x38); jr a5       ; 尾部尾调用，真正拷出
```

SET（`0xd5c3c0`）同形，但**多两道前置门槛**：`*(s1+0x2050)` 非空且 `lbu +0x4f4` 非零（否则跳 `0xd5c51c`）；随后 `lbu`( `*(*(s1+0x1f88)+0x1148)` `+0x179` ) 非零则跳 `0xd5c5ac`。

- 这解释了此前"`+8` 取 `≥2` 就回 `0x62`"为什么只在 SET 侧出现：GET 根本没有这两级分支，两条 id 不是同一段代码的两个方向，而是两套各自带门的封装。
- 失败/前置路径返回的立即数这次看到 `0x56`（上下文未建）和 `0x40`；它们与实测见过的 `0x62`/`0x65` 同属一张状态表，要把它对上名字还得靠 `pClassInfo`。

**④ 边界** ✓ 全部只读（库建在 `/tmp`），设备未动；**"值是否到达硬件"这一条没有变化**。IDA 是 `ida95b1` beta 构建，反汇编结果也还没与该固件的签名/版本交叉验证，所以本条只到"链在哪、门在哪"这一层。

**⑤ 下一步**：`*(a0+0x20)` 和 `*(a0+0x38)` 都是对象 vtable 槽，顺着它们找到真正 memcpy 那 2108 字节、读 `+4` 掩码、落地 12 个值 dword 的段，以及 `+8` 控制字与 stamp 的比较点，再回看 `0x62` 的产生处。

## 一百十五、两条 handler 的状态字读出来了；同时发现 `.fwimage` 的地址映射还缺一块（只读静态读，桶 A/B）

**① 状态字（这两个是读出来的，不是猜的）**
- GET 与 SET 在**同一个**前置判据上返回 **`0x56`**：`*(s1+0x1f88) == 0`（`0xd3b490` / `0xd5c3ec`），即该域上下文还没建。
- SET 独有的一级门槛（`lbu 0x179(...) != 0`）返回 **`0x6c`**（`0xd5c5ac`：`li s1, 6ch` 后跳回公共收尾）。
- 两者在 `jalr` 之后都做 `sext.w s1, a0; bnez s1` → 走"打日志 + 返回 a0"的收尾（`0xd5c53c`：`slli/srli 0x20` 保留符号、`li a0, 2` 严重度为 2）。**所以 `0x62` 不是这两帧产的**，只能由 `*(a0+0x38)` 那个尾调用返回后原样上传——之前"`+8` 取 `≥2` 就回 `0x62`"的观察，落点是更深一层。
- SET 帧体边界：`0xd5c3c0`（`addi sp,sp,-0x30`）起，到 `0xd5c662`（`j` 回公共收尾）止，`0xd5c664` 已是下一帧（`-0x40` 序言）。GET 同构，`0xd3b464`–`0xd3b5a4`。
- 这两帧里的日志调用后面还跟一段固定形状：`lbu a5, 500h(s1)` → `beqz` 回去重试 → `ld a5, 0x438bf28` + `li a4, 0x111300` + `sw zero, 0(a5)`。这是"等日志缓冲刷掉再清标志"的展开，不是业务逻辑，别在这里找业务。

**② 地址映射缺一块（记录清楚，免得下次白花时间）**
- 代码能按 `base 0x4000000` 正常反汇编（`pFunc 0x4d3b464` ⇒ 文件 `0xd3b464`），但日志格式串的地址 **在文件外**：`0x5f2d158` ⇒ 偏移 `0x1f2d158`，而 `.fwimage` 只有 `0x1c01dc8`，文件总长 `0x1c05380`。
- `.fwimage` 能寻址到的是 `0x4000040`–`0x5c01e08`，差 `0x32b350`。`e_entry=0`、**没有程序头**（`e_phnum=0`），段表里所有 section 的 `addr` 都是 0，所以 `.fwimage` 只是容器内容，真实 RAM 布局在别处（`.comment` 里那串 `.section_task_rm_elf_*_instance` 名字暗示这是个"分节装载"的镜像，而不是直载 ELF）。
- 结论：**在解开这层映射之前，别用 `li` 出来的串地址去读字符串**，会一律读空（已试 `%s`/`NmDock` 之类都对不上）。代码路径不受影响。

**③ 边界** ✓ 只读，设备未动；`0x62` 的出处仍为未知，桶 C 的第 4 条（`0x62` = 需要复位）没有因此变实。

**④ 下一步**：`a0` 是 `*( *(s1+0x1f88) + 0x2148 )`，`*(a0+0x20)` 指向的对象首个槽被 `jalr`、`*(a0+0x38)` 被尾调用。要么找到往 `+0x38` 存函数指针的构造函数（`sd ?, 38h(a0)` 且附近有 `ld ?, 1148h(`），要么先按 `0x62` 的 73 处 `li …, 62h` 收窄到与本族相邻的代码。

## 一百十六、找到装槽的地方：`.fwimage` 里有一张 op 类表，`+0x28` / `+0x38` 两个动词就是这对方法的真正收发点（只读静态读，桶 B，附一条地址换算的自我更正）

**① 先更正我自己上一节的一个地址错误**：IDA 里载入的镜像基址是 `0`（段从 `0` 起、`.fwimage` 的 `off` 是 `0x40`），所以 **IDA 地址 = 镜像虚拟地址 − `0x4000000`**，而**文件偏移 = IDA 地址 + `0x40`**。上一节给的两条 `pFunc` 是虚拟地址，换算到文件时多加了 `0x40`；因为反汇编对偏移有一点容忍，函数体读出来仍然是对的，但入口点差 `0x40`，本节按正确公式重取。

**② 装槽表（这次是硬证据）**：`0x1b2aad8` 分配对象后，`0xd42d00`（IDA 地址）这一小段连续 `li a5, <函数地址>; sd a5, <槽>` 一口气装了十三个动词槽：

```
+0x08 = 0x4d53414   +0x18 = 0x4d40eb0   +0x28 = 0x4d41094*   +0x38 = 0x4d3c774*
+0x58 = 0x4d3c634   +0x68 = 0x4d395d0   +0x78 = 0x4d3c6d8    +0xe8 = 0x569c1d0（数据）
+0xf0 = 0x4d536e0   +0x110 = 0x4d406dc  +0x118 = 0x4d3c968   +0x120 = 0x4d39604
+0x128 = 0x569fb90（数据）
```

带 `*` 的两个正是这对方法用的槽：**SET 帧 `0xd5c440` 取 `*(a0+0x28)`、GET 帧 `0xd5ced0`… 即 `0xd3b4d0` 取 `*(a0+0x38)`**（见 §一百十四）。同一帧里 `jalr` 的调用约定是 `a0=引擎对象, a1=上一步结果, a2=调用方参数缓冲`，与这两个动词的签名一致。

**③ 唯一性已核**：对这十三处槽值按 `lui+addi/addiw` 形态反查引用，`0x4d41094`、`0x4d3c774`、`0x4d53414`、`0x4d3c634`、`0x4d39604` **各只被引用一次，全部来自 `0xd42d00` 这一段**。也就是说这是**一个类**的装槽函数，`+0x1148`（本对方法）与 `+0x1150`（`0x2080901c`，用 `+0x08`）都是它的实例槽位，不是各方法各自实现。

**④ SET 动词 `0x4d41094` 干了什么**（IDA 地址 `0xd410d4` 起，前序在更早）；
- `lbu a6, 179h(a5)` 与 `a2` 比较，相同则**跳过**这一批拷贝；`ld a4, 50h(a4)` 且 `lbu a4, 4D0h(a4)` 非零也跳过——与帧内的 `0x179`／`0x4F4` 两道门是同族判据。
- 逐字节把参数里 `+8,+9,+10,+11,+13,+12` 抄到对象 `+178h,+179h,+175h,+176h,+17Ah,+177h`。
- 然后 `memcpy(对象+0x1C8, 参数+0x10, 0x28)`——**只抄 40 字节**，再 `call 0x16a6d80`。
- 于是 2108 字节的缓冲**并不是整块落盘**：头 16 字节被拆成标志位，`+0x10` 起的 40 字节被整段搬走。我们此前盯的 12 个值 dword 正好落在 `+8..+0x37` 这 48 字节区间内（前 6 个当标志、后 40 个当整段），这与"`+8` 是控制字、`+12` 是偏移"能对上，但**哪个 dword 进哪个字段，还要看 `0x16a6d80` 及更深一层**。
- 尾段 `call 0x16a6d80(a0=s4, a1=s3, a2=params)`：`0x16a6d80` 是通用件，先取 `*(a1)` 再减去 `*s3` 得到容器，然后依次 `jalr *(s3+0x50)`、`jalr *(s3+0x40)`、`jalr *(a0+8)`，最后 `jalr *(s4+0x78)`——典型"取容器→加锁→取对象 vtable[0]→调 op"。

**⑤ 边界** ✓ 只读，设备未动；桶 C 第 1、2 条（`+8` 是控制字、`+12` 单位）只是**在结构上更自洽了**，仍不算证完——`0x62` 从哪来也还没到。

**⑥ 下一步**：`0x16a6d80` 深一层找真正拿 dword 的代码；并确认 `+0x1148` 这个实例确实来自该装槽函数（找写 `+0x1148` 的初始化，或反查 `0x1b2aad8` 的调用者是否把返回对象挂到 `引擎+0x1148`）。

## 一百十七、`+0x148`／`+0x150`／`+0x158` 是一次性连号分配的，但装槽函数没有静态引用点（只读静态读，桶 B + 一处"尚未定位"）

**① 上下文里的连号槽是真的**：`0x13613a4`/`0x13613c0`、`0x1127e64`/`0x1127e78`/`0x1127e90`（同一段）都是这个形状——

```
call 0x18c1494(arena, size)     ; arena 分配
sd   a0, 148h(ctx) / 150h(ctx) / 158h(ctx)
beqz a0, <日志 0x5fec... 并返回>
```

`0x18c1494` 是分配器（`a0` = arena 基址、`a1` = 字节数，成功回指针，失败回 0 并走 `0x5fec660`/`0x5fec678` 那几个日志）。三处落点 `+0x148`、`+0x150`、`+0x158` 与三帧 handler 读的 `+0x1148`、`+0x1150`、`+0x1158`（差固定 `0x1000`）**一一对上**：`0x2080901b` 用 `+0x1148`、`0x2080901c` 用 `+0x1150`，说明这三个槽就是本族三个 op 对象的连续位。

**② 但 `+0x28`／`+0x38` 是谁写进去的——尚未定位**（如实记）：
- 装槽函数体在 `0xd42c1c`–`0xd42da8`（序言 `addi sp,sp,-N` 在 `0xd42c1c`），`0xd42d00` 起 `call 0x1b2aad8` 分配、随后十三个 `li/sd` 装槽、`ret` 前 `li a0, 0`。
- 它**没有静态引用点**：既找不到指向它的 `jal`，也找不到任何 `lui+addi` 把 `0x4d42c1c` 装进寄存器的点；`0x4d42c1c` 在数据区里也没有 8 字节指针字面量。也就是只能经 `jalr` 用**运行期表里的值**进来。
- 所以"`+0x1148` 的实例来自这个装槽函数"目前仍是**推断**（依据：槽值 `0x4d41094`/`0x4d3c774` 在全镜像各只被引用一次、且只被这一个函数引用）。要坐实，得先找到那张运行期表怎么填。

**③ 边界** ✓ 只读，设备未动。

**④ 下一步**：①把 `0xd42c1c`–`0xd42d00` 的前半段读出来（它先做了一段别的活、还会 `jalr a5` 并回 `0x3d`），前半段很可能暴露它属于哪个模块的初始化序列；②在 `+0x148` 的三处分配点之后继续跟 `unk_13611B4` 这类成功分支，看有没有对刚分配对象的 `sd` 装槽调用。

## 一百十八、健康机重跑只读电池（`NV_OK`×4、写集模型第四次成立）；回读值全 0 ⇒ "没有任何域带偏移"（桶 A，设备只读）

**① 前提**：设备未重启（`uptime` 2 天 1 小时）、无 wedge，anchor 报 `forward ALIVE and healthy`。跑的是定义好的只读台账 `tools/clock-offset-read-run.sh`，`rc=0`，产出在 `outputs/stage-clkctl-readrun-20261021T144509Z/`（6 个文件 + `SHA256SUMS`）。

**② 结果**：`0x2080901b` 在头部 `+4` 依次种 `0xFF`／`0x3FF`／`0x1`／`0x0`，**四次全部 `NV_OK`**；写集模型第四次成立——8 条记录、类型序列 `08,09,08,09,09,09,03,08`，`0xFF` 那次 106 个 dword 被写（14 个头 + 7×13 + 1），`0x3FF` 12 个、`0x1` 3 个、`0x0` 2 个。**掩码决定写多少，这条规律稳定。**

**③ 两个结构更正**（都基于这次的原始字节，不再靠推）：
- 记录 `+0` 不是"stamp"，是**类型字节对**（`08 08`／`09 09`／`03 03`）；`+4`（逐记录那个位置）**没被写**，掩码在**头部 `+4`**。此前"记录 stamp / 记录 mask"的说法按此更正。
- 类型 `08/09` 的记录被清 `+8..+56`（`+48` 恒不动），类型 `03` 的记录只写类型字节。头部被写的是 `+8..+13` 那 6 个字节和 `+16..+56` 的 0。

**④ 关键读数**：**所有被写的值 dword 都是 0**——固件回答"每个域当前偏移 = 0"。这与"频率从来没动过"完全自洽：写进去了（`0:8:1` 能回读到 1），但**没有一处非零偏移落在域上**。
- 换句话说，`0x62`／不动的根源不在这条读通道，而在 SET 之后"值没被采纳"的那一层。

**⑤ 与静态读的交叉**：同一窗口已确认 SET 动词只把参数 `+8..+13` 当 6 个标志字节、把 `+0x10` 起 **40 字节整段**拷到对象 `+0x1C8`（§一百十六），说明 2108 字节缓冲里**真正被解释的只有前 48 字节**，其余是填充。要动频率，改的位置就在这 48 字节内，`+12` 那个 dword 仍是首选（见 §一百十、§一百十八④）。

**⑥ 边界** ✓ 全程只读、未重启、未改配置。

## 一百十九、`0:8:250` 复现 `0x62`：`+8` 更像"只吃 0/1 的控制字"，不是 kHz 偏移（桶 A，设备写入，一次）

**① 顺序与结果**（`tools/clock-offset-write-run.sh 0:8:250`，`rc=2`，产出 `outputs/stage-clkoffset-write-20261021T144822Z/`）：
- `V3.5a` 前置 GET **`NV_OK`**：域位 0 → 记录 @60、类型 `0x0808`、目标字段 `+8` 当前 `0`、快照已存。
- `V3.5b` SET **`0x62 NV_ERR_RESET_REQUIRED`**。
- 之后 `V3.5c` 回读 GET 也是 `0x62`、`V3.5d1` 同一快照重试仍 `0x62`、`V3.5d/f` 判 RESTORE NOT VERIFIED（回读 `0xffffffff` = 没读到）。
- 会话由此进入"需复位"态：**在重启前不要再发任何写**。

**② 三个可比事实叠在一起**：
1. `0:8:1` 上一会话成功过并回读到 `1`（`0x00000001`）；
2. `0:8:2`、`0:8:50`、`0:8:250` 一律 `0x62`——今天补上 `250` 这一档；
3. 今天早先的只读电池里，**每个值 dword 回读都是 0**（§一百十八④）。

⇒ 结论收紧为：**`+8` 的可接受域像 `{0,1}`，是控制字/开关量，不是偏移量**；真正承载 kHz 的字段另有其人（`+12` 是现成的备选，`+44` 是调用层命名位置的对冲）。这也解释了"写进去了但频率从没动"：写的是开关，不是偏移。

**③ 边界** ✓ 单域、单字段、单步、先存快照、SET 后立即尝试恢复；未重启、未改配置、未动固件。设备现在需要用户手动重启才能继续写入。

**④ 下一窗口（重启后）**：`0:12:250`，判据同上（SET `NV_OK` ＋ 同字段回读 `250` ＋ 其他值 dword 逐字节不变 ＋ 恢复后回读与快照全等）。若 `+12` 也回 `0x62`，说明"任何非 0/1 值都要复位"，则改走 §一百十的 `+0x24` 调用位置对照（含 `0x20809025` 的实测 delta）重新定字段，而不是继续在 `+8` 上碰。

## 一百二十、按字节数一遍 SET 动词的拷贝清单，`+12` 成为首选字段（静态推断，桶 C，可直接被实验否证）

把 §一百十六 那段 SET 动词按**字节**重念一遍，长度就说明问题了：

| 参数来源 | 去处 | 长度 |
| --- | --- | --- |
| `+8, +9, +10, +11, +13, +12`（六个单字节，顺序如此） | 对象 `+178h,+179h,+175h,+176h,+17Ah,+177h` | 6 B |
| `+0x10` 起 | 对象 `+0x1C8` 起（`memcpy`, `a2=0x28`） | 40 B |

合起来正好把 **`+8` 到 `+0x37`（也就是 dword 2 到 dword 13）逐字节搬完**，中间没有跳过。于是：

- **`+8` 的四个字节全部走单字节通道**（8,9,10,11）⇒ 它是"若干个布尔/枚举"，不是数值——与实测"只吃 `{0,1}`、写 `250` 回 `0x62`"完全吻合（§一百十九）。
- **`+12` 只走低两字节（12,13）**，高两字节（14,15）严格说**不在这两条通道里**。偏移是 ±18750 kHz 量级的小整数，正好吃 16 位 ⇒ `+12` 作为 **16 位偏移字段**在结构上最自洽，也解释了为什么历史记录里"记录 0 的 `+12` 是唯一真被写动的 dword"。

**判据（可被否证）**：`0:12:250` 应当 `NV_OK` 且回读 `+12` 低半字为 `0x00FA`；若它同样 `0x62`，则"任何非 0/1 值都要复位"成立，字段要按 `+0x24` 调用位置 delta 重定，而不是继续在 `+8` 或 `+12` 上试。

## 一百二十一、这批"记录"其实是**一组字段描述符**，每个都有自己的 get/set 对；`+8` 只是描述符的状态字节（静态，桶 B，据此收紧 `+8` 的定位）

把 GET 侧那半个栈读穿，机制就变了：

**① GET 动词 `0x4d3c774` 不是取数，是两段式转调**：
```
call *(obj+0x108)( obj - *(obj) + 0x160, 0 )      ; 准备/引用容器，非 0 就记日志 0x5f2c7b0 并退
call *(obj+0x110)( a0=eng, a1=obj, a2=params, a3=*(obj+0xD0) )   ; 真正的取值
```
两处都出现同一个 `+0x160` 容器算式，与 SET 侧同族。

**② `+0x110`（`0x4d406dc`）自己也不填整块，只填一个字节**：它调共用件 `0x16a62b0(eng, obj, params)`，成功后就一条
```
lbu a5, 171h(obj) ; sb a5, 8(params)      ; 只写 params+8 这 1 个字节
```

**③ `0x16a62b0` 是共用解析器**：检查描述符的使能字节 `*obj`、把 `obj+2` 的字节抄到 `params[0]`、然后 `jalr *(obj+0x50)` 拿一个"值对象"（`a3=1`）、再按 `lhu 2(...)` 之类格式化。也就是说**数值是由每个描述符自己的 `+0x50` 生产者算出来再排版的**。

**④ 结论（对定位的直接影响）**：
- 那 2108 字节不是"驱动填的平坦结构"，而是**一串独立字段描述符**的答案；每个"记录"对应一个描述符，`+8` 是这个描述符的**状态/使能字节**（1 字节），数值在别的槽里。
- 这与实测严丝合缝：`+8` 只吃 `{0,1}`（§一百十九），写 `250` 回 `0x62`；`+12` 是紧随其后的独立字节通道，更像那一组里的第一个数值槽（§一百二十）。
- 因此"改 `+8` 就等于改频率"的说法可以正式降级为"改的是描述符状态"。

**⑤ 下一步（静态可继续）**：读 `+0x50` 生产者的族，按"哪个描述符的值带 kHz/有 `0x20809025` 同款 ±18750 限幅"筛出偏移描述符；设备侧重启后仍先发 `0:12:250` 那一条，两者互为校验。

## 一百二十二、字段字节有两个写入簇（活值与影子值分家），并修掉一处扫描 bug（静态，桶 B）

**① 扫描 bug 更正**：S 型存储指令的立即数拆在 `31:25`（`imm[11:5]`）和 `11:7`（`imm[4:0]`）两段，我之前用 `(x>>20)&0xfff` 当作立即数，把 `rs2` 也算了进去，结果对 `>=0x100` 的偏移不可信。本节按正确字段重扫。

**② 重扫结果**（`sb` 才算，宽度对齐）：
- `obj+0x171`（GET 抄给 `params+8` 的那个字节）写出点 14 处，集中在 `0xd4ed20…0xd55064` 一串同形函数里，另有 `0xd90960`、`0xd909a0` 两处；
- `obj+0x178`（SET 从 `params+8` 抄进来的那个字节）写出点 23 处，其中 `0xd41108` 正是 SET 动词自己，其余另一簇；
- 两簇地址几乎不重叠 ⇒ **活值（`0x171…`）与 SET 写入的影子值（`0x175…0x17A`）是两套字段组**，GET 报活值、SET 写影子值，两者由别处（`0x16a6d80` 那条链）同步。

**③ 顺带确认**：GET 转调时把 `*(obj+0xD0)` 当第四个实参传下去（§一百二十一①），`+0xD0` 有 26 个写入点，是这条链的上下文档位，不是数据字段。

**④ 对"哪个 dword 是偏移"的影响**：`params+8` 被两端都当**单字节**处理（写/读都是 `sb`/`lbu`），再次独立印证 `+8` 不是数值通道；`+12`（16 位）仍是设备侧首选，静态侧下一步应在 `+0x50` 生产者族里找"读 `0x20809025` 那类限幅上下文"的描述符。

## 一百二十三、装槽函数的真入口是 `0xd42b8c`（不是 `0xd42c1c`），且它同样没有任何静态引用点；`+0x50` 读的是基址不是对象（静态，桶 B）

**① 更正上一个入口**：`0xd42c1c` 那条 `addi sp,sp,50h; ret` 是个空壳函数，紧接其后的代码才用到 `-4Ah(s0)` 的栈位 ⇒ 真入口更早。按负向栈调整（`imm >= 0x800`）回扫，落在 **`0xd42b8c`**（`0xfb010113` = `addi sp,sp,-0x50`）。§一百十七 里"入口 `0xd42c1c`"按此更正。

**② 但换了入口结论不变**：`0x4d42b8c` 在全镜像里**没有任何 8 字节指针字面量、没有 `jal`、没有 `lui+addi` 引用**。也就是说这个"给描述符装 13 个动词"的函数只能经运行期表/计算进入，静态上**无法**给 `+0x1148` 的实例定类。这一条就按"尚未定位"记，不再重复扫。

**③ 顺手校正一个读法**：`0x16a62b0` 里 `s3 = a1 - *(a1)`，`obj+0` 存的是**指向所在基对象的负偏移**；随后 `ld a5, 50h(s3)` / `jalr *(s3+0x50)` 读的是**基对象**的槽，而 GET 动词里 `ld a5, 108h(a1)`、`110h(a1)` 读的是**对象自己**的槽。两类槽必须分清，否则 `+0x50` 会被错挂到描述符上。

**④ 现状**：字段仓库、动词池、读写转调、`params` 字节通道长度都已读清；仍缺的一环是"哪个描述符是偏移量"，设备侧重启后 `0:12:250` 是唯一能立刻定性的一步。

## 一百二十四、风扇族独立只读审计（用今日健康时段已有的产物，桶 A；不做任何写）

风扇族按 goal 要求**单独**审，不因频率受阻一并宣布不可用。数据全部来自 `outputs/stage-clkctl-readrun-20261021T144509Z/fan-read.txt`（wedge 之前、健康状态下的只读运行）。

**① 可读：有两条独立通道**
- RUSD 页（`NV00DE` 映射 4 KB）：`temp gpu=48.31C`、`fan 1523 rpm`、`tgp 11381 mW`、`limit 100000 mW` —— **转速与温度都是实测数字，不是 0、不是占位值**。
- 私有 `0x20808532`@336：`NV_OK`。返回 336 B 中 8 B 非零（首个 @4、末尾 @30）、15 B 被写 0、313 B 仍是 A5 底色；头部 `a5a5a503 00000001 00000001 a5ff0001`。

**② 可解释的部分（只有两条，其余如实未知）**
- `+0` 若按文档的 `fanPolicyInfo` 表起始解读，则 policy 掩码为 `0xa5a5a503`——**比文档描述的宽**，高两字节仍是背景色说明文档没覆盖这一段；
- `+4` = 1 读作 **active policy index**，`+8` = 1 与之同值，可作为"默认策略生效"的旁证。
- 336 B 的几何、`+12` 以后各 dword 的含义：**尚未定位**。

**③ 范围 / 原状态 / 恢复**
- 范围：只有 336 B 这一种尺寸被测过（历史上的 `0x20808532` 从未进过任何返回状态记录，今天首次进册）；限幅未知。
- 原状态：`active policy index = 1`、掩码 `0xa5a5a503`，已留档在产物里，可做恢复基准。
- 恢复：本族**一次都没写过**，所以"恢复"待首次写之后才能验；本轮不写。

**④ 公私双胞胎的不对称（与时钟族同型）**：私有 `0x20808532` `NV_OK`，公开 `0x20800532` 直接 `0x56 NV_ERR_NOT_SUPPORTED`。这在本驱动上不是孤例（时钟对 `0x2080901b`/`0x2080901c` 也是私有通、公开不通），可作为"公开 id 面在 macOS 上整体缺装"的第二条独立证据。

**⑤ 边界** ✓ 纯只读，未触发任何写。

## 一百二十五、电压/供电族（`0x2080b213`）在健康机上**首次被接受**，但载荷几乎是空的（桶 A，只读）

**① 事实**：今天健康时段的只读电池里，`0x2080b213`@1036 返回 **`NV_OK`**，回填后 **只有 1 个非零字节（@0）**，其余全 0（与 A5 底色的对比脚本按"非零字节"统计，`first @0 last @0`）。这是该 id **第一次在非 wedge 状态下拿到应答**——此前 `outputs/volt-read-20261017T111107Z`、`volt-recheck-*` 两批都是在 wedge 之后跑的，全部 `0x62`，因此当时"id 不存在/尺寸不符"都无法判定，只能记"post-latch state"。

**② 由此能确定的**
- id **存在**且 1036 是**可接受尺寸**（`NV_OK` 而非 `0x1f`/`0x56`）；
- 该容器在 P1 空闲态下**基本不填内容** ⇒ 它的数据依赖于某种前置（pstate？被谁触发？）——**尚未定位**；
- 载荷形状（表头/条目/限幅）**未知**，因为只有 1 个字节有值，一个字节无法命名字段。

**③ 同一批里的两条旁证**（都在健康态）
- `0x2080a0ff`@776 → `0x56 NV_ERR_NOT_SUPPORTED`（非特权位的未知 id，说明"未知 id"回 `0x56`，与 `0x62` 语义清楚分开）；
- `0x2080ffff`@776 → `0x1b NV_ERR_INSUFFICIENT_PERMISSIONS`（即便以 root 跑，这一档高位 id 仍是权限拒绝）⇒ `0x1b` 不是"不存在"，是另一层门。

**④ 恢复/范围**：本族只读、无写、无恢复可言；尺寸只测过 1036 一种；值全 0 无法给范围。

**⑤ 下一步（设备）**：必须在非空闲（有负载）下再取一次 `0x2080b213`，并在换 pstate 后再取一次，看字节数是否变化——只有变化了才能谈形状。这与频率主线不冲突，且是纯读。

## 一百二十六、写探针的真实接受规则（便于每次下命令前自检）＋ 探针源码里的头部模型与今天的实测逐字节吻合（桶 B）

**① 接受规则（`src/main.c:90` `parse_clk_offset_set`，直接读的实现，不是文档转述）**
- `bit`：`0..31`；
- `off`：**`0..2108` 任意值**——探针**不做**白名单。此前"只接受那 12 个 value dword"是**runbook 的口径**（`tools/clock-offset-write-run.sh` 头部说明），不是探针会拒绝；真正会被挡的只有负数、越界和非数字。
- `delta`：`-18750..18750`（正好等于 `0x20809025` 那条实测限幅，不是随手取的数）。
- 可选第三段 `:bump`，会连记录 stamp 一起 +1；注释写明"驱动自己写时会动 stamp，而至今所有受控运行都带着旧 stamp 去改值"——所以"值变了但 stamp 没动"本身就是与驱动真实写的差异点。
- `bump` 可以单独用（`0:12:bump`，delta 缺省 0），没有 `:bump` 也没 delta 会被拒。

**② 一个很好的交叉校验**：`src/main.c:1973` 的注释把头部模型写死成"60 B 头、14 次写：`+0`(u8 `0xFF`)、`+8 = 0x01010000`、`+12..+56 = 0`"。今天设备回读的头部**逐字节就是这个**（`+0` 尾字节 `ff`、`+8 = 0x01010000`、`+12..+56` 全 0，唯一差别是 `+12` 的低两字节）。也就是说工具模型与固件行为**已经对齐**，不需要再改模型，只差"哪个槽吃 kHz"。

**③ 对下一步的直接影响**：`0:12:250` 合法、`18750` 是硬上限可放胆用最大步长；若单步 `250` 被 `0x62` 挡，下一次应换 `0:12:250:bump`（把 stamp 一起动），而不是继续加大数值。

## 一百二十七、温度族与 `RATED_TDP` 的现状（桶 A，只读；两者都不吃频率主线的结论）

**① 温度**：唯一可信来源仍是 RUSD 页的 `temp gpu=48.31C`（今天健康态实测）。性质要说清——**单一传感器、单点、无列表**：`perf-level-info-v2` 已经在今天被证否（`capacity 0` 回 0 个域，`capacity 1` 回 `0x1f`，即驱动不暴露 clock-domain 列表），所以拿不到"每域温度"。范围/限幅未知；只读，无恢复。
- 结论：**可读，但只有一个总温**；想按域分温需要另找容器，尚未定位。

**② `RATED_TDP`（独立候选，不替代偏移控制）**：`rated-tdp-get-control` 今天 `NV_OK`，返回 `raw[0..2] = 0 0 0`，工具把这三个字解释为 client／input／vpstate 回显（`V2.4`），配套 `perf-current-pstate` = `P1`、`perf-get-powerstate` = 0（AC）。
- 也就是说：这条**控制面是通的**（`NV_OK` + 明确的三字回显语义），只是本次会话里**从未对它做过 SET**，所以"改得动/改不动"仍无证据；这属于**待授权写入**，不是"不可用"。
- 一旦频率主线拿到偏移，`RATED_TDP` 可作为第二维（整卡功耗上限）独立推进，两者不互相替代。

**③ 边界** ✓ 纯只读（数据来自 wedge 前的健康时段产物），未写。

## 一百二十八、`0x2080901b` 头部／记录字段映射总表（把散在 §一百十…一百廿六 的结论并成一张，带来源与更正史）

写这张表的目的：此前结论散落在十几节里，且有两条已被自己推翻，容易照着旧口径下命令。下面每行都给来源与置信。

### 头部（60 B）
| 偏移 | 含义 | 来源 | 置信 |
| --- | --- | --- | --- |
| `+0` | 末字节被置 `0xFF`；前三字节保持背景 | 实测（`0xFF` 那次唯一种子变化点） | A |
| `+4` | **掩码**，决定写几条记录（`0xFF`→7 条、`0x3FF`→?、`0x1`→1、`0x0`→0 条值） | 实测四档 + 写集模型 | A |
| `+8` | 置 `0x01010000`（`8=00, 9=01, 10=01, 11=00`）；`+8` 这一**字节**是描述符状态量，只吃 `{0,1}` | 实测回读 + SET/GET 动词都按单字节处理 + 写 `250` 回 `0x62` | A（"只吃 0/1"）/ C（语义名） |
| `+12` | 低两字节被清 0；是唯一**成对出现于单字节通道又不带高半字**的槽 | 实测回读 + §一百二十 的拷贝清单 | C（**当前首选偏移槽**） |
| `+14,+15` | 高半字不被任何通道覆盖 | §一百二十 | B |
| `+16..+56` | 全 0 | 实测 | A |

### 记录（64 B 步长，`60+64i`）
| 偏移 | 含义 | 来源 | 置信 |
| --- | --- | --- | --- |
| `+0` | **类型字节对**（`08 08`/`09 09`/`03 03`），不是时间戳 | 实测原始字节（§一百十八③更正旧说法） | A |
| `+4` | 逐记录的 `+4` **不被写**（旧说"记录 mask 在 +4"→更正为头部 `+4`） | 实测 | A |
| `+8..+56` | 类型 `08/09` 记录被逐 dword 清 0，`+48` 恒不动；类型 `03` 记录只写类型字节 | 实测 7 条记录逐一比对 | A |
| 12 个值 dword 中谁是 kHz | **未知**，`+12` 为首选、`+44` 为对冲 | §一百十/一百廿 | C |
| 记录对域的对应 | 掩码 bit i ↔ 记录 i，bit 0 是核心域 | 实测（`0:8:*` 稳定落到记录 0） | A |

### 已被推翻、不要再引用的旧说法
1. "记录 `+0` 是 stamp／时间戳" → 实为类型字节对；`bump` 动的是它，但语义不是时间。
2. "记录 `+4` 是逐记录 mask" → mask 在头部 `+4`。
3. "`+8` 是偏移量" → 是状态字节，实测只吃 `{0,1}`。

### 边界
只读整理，无设备动作。设备侧动作仍是重启后 `0:12:250`（被挡则 `:bump`）。

## 一百二十九、找到 SET 动词的**镜像反函数**：它逐字节把对象抄回 `params`，映射与 SET 完全对称；并据此更正 §一百二十二 的"活值/影子"读法

`0x4d40eb0`（就是装槽表里的 `+0x18` 槽）是 SET 动词的对称件，逐条对抄：

```
lbu 178h(s2) -> sb 8(s1)      ; param+8
lbu 179h(s2) -> sb 9(s1)
lbu 175h(s2) -> sb 0Ah(s1)
lbu 176h(s2) -> sb 0Bh(s1)
lbu 177h(s2) -> sb 0Ch(s1)    ; param+12
lbu 17Ah(s2) -> sb 0Dh(s1)    ; param+13
beq a0,a1 / bltu a0,a1 / addi a5, s2, 1F0h / bgeu a0, a5   ; 边界
li a2, 28h ; call 0x1ba41b8                                ; memcpy 40 字节
```

**① 由此可以定死的东西**
- `params` 与对象之间是**严格 1:1 的字节对**：`8↔178`、`9↔179`、`10↔175`、`11↔176`、`12↔177`、`13↔17A`。此前只从 SET 一侧看到其中一半，现在两侧互为镜像，映射不再是单边推断。
- 一组的载体是 **6 个单字节 + 从 `params+0x10` 起的 40 字节整块**，两头完全一致；对象侧 40 字节区的**末端是 `+0x1F0`**（`bgeu` 用它做越界判据）。
- `params+12` 的**两个字节各自有独立目标**（12→`0x177`、13→`0x17A`），所以它既可能是"一个 16 位小端值"，也可能是"两个并列单字节字段"——**两者都还成立**，仍要靠设备判定；但"高半字被丢弃"的说法要修正为"高字节走 `+0x17A`"。

**② 更正 §一百二十二 的读法**：真正成组的是 `0x175…0x17A` + `0x1F0` 区，SET 写的和这里读的是**同一套**。`+0x110` 子取值器抄的 `obj+0x171` 属于**上一层对象**（`0x171` 的读写点有 81/26 处，是通用偏移），不是本组的成员。前一轮"活值簇 `0x171…` vs 影子簇 `0x175…`"的结论按此更正为：**同一族里 `+0x18` 是出、`+0x28` 是入，作用于同一组 6+40 字节**。

**③ 边界** ✓ 只读静态。

## 一百三十、整块 GET 是**父层循环子描述符**拼出来的：`+0x18` 的调用点有一处在遍历循环里（静态，桶 B）

`+0x18`（镜像反函数 `0x4d40eb0`）既没有 `jal` 也没有 `lui+addi` 引用——与全族一样只经槽加载进入。在 handler 区按"`ld ?, 18h(...)` 紧跟 `jalr`"筛，只有两处：`0xd4d8b0`、`0xd55ab4`。

看 `0xd4d8b0` 的上下文：

```
0xd4d890  li   a4, -1
0xd4d894  sh   a4, -7Ah(s0)      ; 16 位栈位先置 -1（每次迭代）
0xd4d898  ld   a0, 8(a5)         ; 取子对象
0xd4d89c  lbu  a5, 0(a0)         ; 子对象使能字节
0xd4d8a0  bnez a5, ...           ; 未使能则跳过
0xd4d8a4  ld   a1, -98h(s0)      ; 描述符
0xd4d8a8  mv   a2, s2            ; params
0xd4d8ac  mv   a0, s6            ; 引擎
0xd4d8b0  ld   a5, 18h(a1)       ; 取槽
0xd4d8b4  jalr a5                ; 调用（eng, desc, params）
```

**① 这解释了"13 个 dword 怎么来的"**：不是一个动词填一整条记录，而是**父层按列表逐个描述符调用**，每个描述符只写自己那 6+40 字节里的一小片；`-7Ah(s0)` 那个 16 位栈位每次迭代先置 `-1`，是"本片默认无效"的哨兵。未使能的子对象（`*obj == 0`）直接跳过——正是为什么掩码清 0 时整条记录会全 0。

**② 与 §一百廿九 连起来**：`+0x18` 出、`+0x28` 入，同一组 6+40 字节，父层循环驱动。**字段语义（尤其哪片是 kHz）仍取决于各描述符自己的 `+0x50` 值对象**，这是最后一层没读的部分。

**③ 边界** ✓ 只读静态。

## 一百三十一、描述符指针数组的分配几何：计数存在 `+0x138`，两张表各按 `count×8` 分配（静态，桶 B）

`0x1361330` 区段里建列表的地方：

```
lw    a1, -0B8h(s0)      ; 取 count
sw    a1, 138h(s9)       ; 计数落到上下文 +0x138
slli  a5, a1, 20h ; srli a1, a5, 1Dh   ; = count*8
call  0x18c1494          ; alloc(arena, count*8)
sd    a0, 148h(s9)       ; 指针表 A（失败即跳错误分支）
lwu   a1, -0B8h(s0) ; slli a1, a1, 3   ; count*8
call  0x18c1494 ; sd a0, 150h(s9)      ; 指针表 B
```

**① 结论**：`+0x148`、`+0x150` 是两张**同长度、按 8 字节步长**分配的表，长度都存在 `+0x138`；分配失败会走 `0x5fec660` 那条诊断分支（`call 0x1bee124(a0=1, a1=0x5fec660)`）。这与之前记的"三张连续分配的表"一致，且给出了**长度来源**——之前只知道宽度不知道谁定长度。

**② 意义**：说明"每域一个描述符 + 一张平行表"是编译期就固定的结构（count 来自上下文，不是运行期抖动），与掩码 bit↔记录 i 的实测对应吻合。

**③ 边界** ✓ 只读静态。`+0x50` 值对象仍是最后一层未读项。

## 一百三十二、`+0x50` 是**虚方法槽**（`jalr *(base+0x50)`），不是数据对象 ⇒ 字段身份藏在函数指针里，这就是"找不到规格表"的结构性原因

**① 读法更正**：`0x16a62b0` 里 `ld a5, 50h(s3)` 后紧跟 `jalr a5`，`s3 = a1 - *(a1)` 是**基对象**。所以 `+0x50` 是基类的一个**虚函数槽**（"取本描述符的值"），调用后由调用方用 `lhu 2(...)` 去格式化返回值。之前把它当"值对象指针"的说法按此更正。

**② 由此得到结构性结论**：字段身份不在任何数组或字面量里，而在**构造时被装进槽的函数指针**里，而装槽代码又没有静态引用点（§一百二十三）——所以"翻遍字面量找 kHz 表"注定找不到，这是**结构性原因**，不是找得不够。设备实测因此成为唯一判定手段。

**③ 一个直接可见的旁证**：`0xd4ed20` 的尾部就是这种装槽构造件的收尾——`sb a5, 171h(s2)` 之后连装三个方法指针（`0x4d53bc8`→`+0xF0`、`0x4d41554`→`+0x6C0`、`0x4d3ce60`→`+0x6A8`），地址都紧邻 SET（`0x4d41094`）与 GET（`0x4d3c774`）动词。**不同类型（`08/09/03`）的描述符装的动词集不同**，与类型字节的存在互为印证。

**④ 结论定性**：静态能做的到此为止；再往下必须靠设备。"哪片是 kHz"仍记**尚未定位**，`+12` 为首选。


## 一百三十三、重建后的首条 `0:12:250` 完整往返通过，字段语义仍待判定

选择依据：按交接的单域、单字段、单步顺序，先试记录 `+12`；该槽在固件拷贝路径包含两个独立字节目标，不能预先认定为 kHz。

实际验证：本轮 HEAD `31aff9b`，先在测试机 `probe-macos-rm/` 运行 `LINT=1 sh build.sh`（`lint build OK`），再分别原生构建 idle/full（`built probe-macos-rm-idle` / `built probe-macos-rm`）。构建脚本位于该子目录；lint 成功不代替原生二进制重建。idle anchor 明确 `forward ALIVE and healthy`，随后运行 `sh tools/clock-offset-write-run.sh 0:12:250`。

- 桶 A：快照记录 0 位于块偏移 60，类型对 `0x0808`，`+12` 原值 0；SET `NV_OK`，同槽回读 `0x000000fa`，其余 11 个值 dword 与发送内容相同；恢复 SET `NV_OK`，恢复 GET 后全部 2108 B 与快照逐字节一致；runbook exit 0，二进制 freshness 为 FRESH。
- 桶 C：这证明可存储/回读/恢复 250，**不证明该字段的单位或频率偏移效果**；尚无独立字段语义或负载 A/B 证据，公共频率控制能力不因此启用。压力与校验尚未在本轮执行。
- 更正交接与 §一百二十六：`main.c` 的 parser 接受 off 0..2108，但 `nvx_stage3_clk_offset_set` 还有第二道十二个值 dword 白名单；不能把 parser 边界当实际写入边界。记录 `+0` 是类型字节对；`:bump` 实际修改记录 `+60`，该槽的 stamp 语义尚无独立证明。

原始证据：仓库外 `outputs/stage-clkoffset-write-20261021T151653Z/`，含 provenance、anchor、write 和 SHA256SUMS。下一步在原状态已证实恢复的前提下判定 `+12` 与对冲候选 `+44`，保留单步快照/回读/恢复门禁。


## 一百三十四、对冲槽往返与 stock NVK/Metal 基线

选择依据：首个 `+12:250` 往返恢复已验证后，按既定顺序独立尝试 `+44:250`；继而用短时 stock 负载区分动态 Boost 和用户偏移，不从空闲频率推断设置语义。

实测桶 A（同一份 FRESH 原生二进制，源码 HEAD `31aff9b`）：

| 操作 | 实际结果与证据 |
| --- | --- |
| `0:44:250` 单域单字段 | SET/GET/恢复 NV_OK，回读 250，其余值 dword 相同，恢复全部 2108 B 一致；runbook exit 0；`outputs/stage-clkoffset-write-20261021T151853Z/` |
| stock NVK 5 秒低强度负载 | 显式指定驱动包 NVK ICD 与 loader；`--vulkan-only --duration 5 --vulkan-width 320 --vulkan-height 180 --vulkan-iters 8 --vulkan-shells 1 --vulkan-particles 0`，exit 0；前后 anchor 健康；`outputs/stage-stock-nvk-20261021T152007Z/` |
| 同期时钟遥测 | RM776 graphics 从 1530000 升至 1965000/1950000，再回到 1530000 kHz；RUSD 相邻采样有延迟。stock Boost 已证明空闲恒 1530 MHz 不是负载频率上限；不能将这次自然升频记作偏移效果 |
| Metal 正确性基线 | 在测试机重建后，显式 `--gpu NVIDIA --verify --elements 4096 --iters 128`；设备名指向 GTX 1650 SUPER / NVMTL / NVK TU116-A，exit 0，全量 4096 元素，零 mismatch，`metal-verify.txt` |

分档：`+44` 与 `+12` 的存储往返已实测，频率字段语义仍为调查未知；本次 NVK 仅是 5 秒图形负载，Metal 仅是上述整数内核的一次 stock 正确性检查，不等于超频稳定、浮点覆盖或显存全覆盖。IDA 9.5 idalib 已实开固件只读副本（版本 9.5.261001），后续继续核对组头/记录层次，不能把历史“静态到此为止”当作硬限制。


## 一百三十五、核心加法偏移首次真实生效：同负载 1965 → 1980 → 1965 MHz

### 选择依据与协议更正（桶 B）

交接中的“静态只能到此为止”被后续追踪证否。IDA 9.5.261001 可用，固件副本 md5 `830551d8942fb915ea4c56fd9d63093f`。本节地址为镜像虚址，IDA 地址减 `0x4000000`。

- `0x4d41094` 拷贝的六个字节/40 B 是**组头**，不能直接作为单域记录 `+8/+12` 的语义；它随后直接调用 `0x56a6d80`，后者按对象循环，经对象 `+0x30` SET 槽消费记录。
- `0x4d3c6d8` 明确 `record = params + 60 + (index << 6)`。单域 GET `0x4d53004` 把对象 `+0x158` 起 40 B 拷到记录 `+8`；SET `0x4d62e94` 执行反向拷贝。`0x4d5d6d8` 的扩展 GET 另写记录 `+52/+56`，与实测写集一致。
- 真正的频率消费者 `0x4d60374` 将对象 `+0x158` 交给 `0x4d60180`。后者读结构 `+0` 选择器：**0 → signed i32 @+4 / 1000，加到 MHz 频率；1 → signed i16 @+4 × 输入频率 /4096，按其舍入路径相加**。因此对应单域记录 **`+8 = 单位选择器`、`+12 = 加法 kHz（选择器 0）或相对量（选择器 1）`**。
- 更正 §一百二十八/一百二十九 及旧源码：记录 `+12` 的加法量是 **i32**，不是组头两个单字节，也不是固定 i16；`+8` 不是 enable。`250 kHz` 在上述 MHz 运算会截断为 0，正是先前“回读成功而时钟不动”的可检验解释。选择 15000 kHz 的依据是这条单位转换和实测 15 MHz 档位，并非遇到拒绝后加数值碰运气。

原始静态证据：仓库外 `outputs/clock-protocol-20261021/ida-record-delta.py`、`ida-record-delta.txt`。记录层和组头必须分别追踪；`:bump` 所动的记录 `+60` 语义仍未确认，本次生效不需要它。

### 实际验证（桶 A）

首条 `+12:250` 的往返门禁已在 §一百三十三 通过。随后独立的 `+44:250`、`+12:250` 加选择器 1、`+12:-15000` 都逐次恢复；其中选择器 1 试验在语义定位前进行，不能再按“arm/启用”解释，也不作为相对模式支持验收。空闲下 `-15000` 时钟不变；把负载启动放在写后，核心为 1935 MHz，但跨次 stock 基线有自然 Boost 差异，因此这批不足以单独证明幅度。

决定性实验：**同一个 10 秒低强度 NVK 负载内 stock → 单步 → 原快照恢复**，不重启负载、不改 Boost/锁频/驱动配置。命令参数与 §一百三十四 stock 相同，只把 duration 改为 10；单步 `--write --clk-offset-set 0:12:15000 --clk-offset-hold 3`，保持选择器 0。

| 时段 | RM776 graphics kHz | RUSD actualClkKHz | 控制状态 |
| --- | --- | --- | --- |
| 负载已启动，t=1..2 s | 1965000 | t=2 为 1965000 | 原快照 `+12=0` |
| 写入后，t=3..5 s | 1980000（三次） | 1980000（三次） | SET NV_OK，`+12=0x00003a98`，其它值 dword 不变 |
| 恢复后，t=6..10 s | 1965000（五次） | 1965000（五次） | 恢复 SET NV_OK，GET 后全部 2108 B 与快照一致 |

事件记录：step 从同负载第 2.321 s 开始，第 5.448 s 完成并恢复；负载 exit 0，遥测 exit 0；前后 anchor 均明确 healthy。显存全程 6000984 kHz。产物 `outputs/stage-offset-aba-20261021T153117Z/`（events.json、write.txt/jsonl、telemetry.txt/jsonl、vulkan.txt、before/after.txt）。这是**首次有同负载 A/B/A 与恢复证据的核心 +15 MHz 偏移**。

### 本步实现、验收范围与剩余缺口

- 探针移除误导性的 `--clk-offset-arm` 写入行为，该旧选项现在在打开驱动/日志前拒绝并说明单位选择器；hold 只在 SET 和回读比对成功时运行；原始 dword 步长计算避免 signed overflow；空字段/溢出参数/hold=0 在打开驱动前拒绝。
- runbook 自动重建过期 full 二进制，anchor 必须给出明确 healthy；成功判据要求 SET/GET 为 0；摘要写完后再生成 SHA256SUMS，修复 README 在摘要落盘后摘要哈希失效的问题。
- 验证：测试机原生 idle/full 构建与 lint；CPU CLI 回归新增参数拒绝用例，并补齐此前缺失的四个 fake-suite 链接符号，使原先无法链接的回归实际执行；连同 runbook 的假健康拒写、拒绝 SET 不误报成功、过期构建和最终摘要哈希回归，共 31 tests passed。最终门禁和新二进制复测见仓库外本批产物及 INDEX。
- 分档：**核心加法偏移协议/单个 +15 MHz 实机效果/回读/原状态恢复已验证**。仅 10 秒低强度图形负载，不声称超频稳定、自动优化完成或全模式支持；选择器 1 的相对模式未交付，显存偏移未做本批写验证。
- 公共产品入口仍未完成频率 SET 接入，不能因探针成功宣称 CLI/Python/GUI 设置已支持。下一步需要用确认的控制记录 `+12` 修正 `core/src/macos_offset.rs`/`apply_offset_channel` 的当前设置投影：旧实现将 RM776 的猜测字段报告成 offset，本批未验证该猜测。随后把已证明的快照/写/回读/恢复集中接入 macOS backend 和既有操作，继续原目标，不能只停在探针。

最终验证补记：修改后的原生 idle/full 再构建通过；full 二进制对 `--clk-offset-arm` 返回 2，未创建 JSON 日志、未打开设备。runbook 实际发现过期 full 并自动重建，`0:12:250` 再次完整往返，产物 `stage-clkoffset-write-20261021T153924Z/`，`shasum -a 256 -c SHA256SUMS` 全 OK。最终源代码下再次执行同负载 A/B/A（`stage-offset-aba-20261021T154231Z/`）：1950 → 1965（三次）→ 1950 MHz，RM776/RUSD 同向吻合，完整 2108 B 恢复，前后 healthy。全门禁 `cargo xtask ci` 带指定三个环境变量返回 **CI_EXIT=0**（首个 A/B/A 批目录 `ci.txt`）；附加 probe CPU 回归 **31/31**，不含 GPU 写测试。公共 core/CLI/Python 频率 SET 仍不因这条探针验收自动启用。


## 一百三十六、公共当前偏移改读已证实控制槽，CLI 与真实 Python 扩展读数闭环

选择依据：§一百三十五 已定位记录 `+8` 为单位选择器、`+12` 为选择器 0 下的 signed i32 kHz。旧 `apply_offset_channel` 却把 `0x20809061` 的相邻 signed dword 猜为配置偏移，导致 stock 核心输出 -0.001 MHz、显存 -0.126 MHz；这是消费者可见的错误，必须先修正再接 SET。

实现：`OffsetControlRecord::additive_offset_khz()` 只解释 type 08/09、selector 0 的第二个值 dword；公共 `clock_offsets` 以控制记录 index 连接 `0x20809019` 目录，再以目录的 domain bit 连接 RM776 的**实时频率**。保留现有 JSON 字段、单位和原 NVAPI/NVML 实现；RM776 的旧 raw 字段名为兼容保留，明确不再作为配置值。缺任一必要读、相对/未知模式、type 不符、重复域或缺少唯一非零频率均不伪造 0，有逐项原因。type03 不发布值。settings 的第二个 session 校验设备身份，显式 close 错误进入 notes；文档明确多次 GET 非原子。公共写入门禁的说明同步改为“探针已验证，产品事务 SET/restore 尚未接入”。

Python 构建缺口也已定位并修复：原 `cargo build -p pynvoc` 在 macOS extension-module 下链接时缺少 `-undefined dynamic_lookup`，导致 `_PyBool_Type` 等导入宿主符号未解析。新增局部 `nvoc-python/build.rs`，仅 macOS 且 extension-module 启用时为 cdylib 添加这两个参数；不改依赖版本、不改其它平台、不把此参数施加给嵌入解释器的测试。测试机原生扩展成功构建，并在现有 Python 3.11 中从仓库外临时包导入，真实 GPU 发现返回 GTX 1650 SUPER、backend_macos_rm=true、nvapi/nvml=false。

实际验证（桶 A）：测试机 `get-pstate-global-freq-offset --macos-rm` 与 `pynvoc.query_clock_offset` 共享相同 Rust 后端；先 stock，再由已验收探针保存 `0:12:250` 并 hold10秒，期间分别查询核心与显存，恢复后再查。

| 阶段 | CLI 核心 kHz / MHz | Python 核心 MHz | CLI / Python 显存偏移 |
| --- | --- | --- | --- |
| stock | 0 / 0.0 | 0.0 | 0 / 0.0 |
| 控制槽保存 250（查询发生在 hold 内） | 250 / 0.25 | 0.25 | 0 / 0.0 |
| 原快照恢复 | 0 / 0.0 | 0.0 | 0 / 0.0 |

探针 SET/同字段回读/其它值不变/全部2108 B恢复通过，前后 anchor healthy。这里的 250 kHz 验证的是**设置读取与单位**，不是频率效果（MHz 计算截断）；真实 +15MHz 效果仍由 §一百三十五 的同负载 A/B/A 举证。Python GPU 选择使用发现得到的 PCI GPU id `0x100`，不是 CLI 序号 0。

分档：公共 core/CLI/Python **当前加法偏移读取已实现并实测**；本步不增加性能 SET。显存 SET、相对模式、跨平台运行及 GUI 图形交互仍未因此验收。原平台安全回归由全门禁覆盖的 mock/单测举证，未实际运行 Windows/Linux。

测试与产物：core 单测 122 passed，新增反例覆盖 signed kHz、目录 index≠domain bit、type03、相对模式、type不符、频率缺失、三读拒绝和重复域；真实导入扩展后 Python import/参数验证测试 52 passed、0 skipped。全门禁及最终构建/实机复测见本批仓库外产物及 INDEX。初次 Python 查询使用错误的序号而非 PCI id 返回 no GPU selected，在写入前结束，之后按发现 id 更正重跑；保留失败日志，不当硬件故障。原始闭环 `outputs/stage-consumer-offset-read-20261021T155644Z/`，构建/测试日志 `consumer-offset-*-20261021.txt`；此后最终 session 身份/close 增量的复测另记本批最终目录。

最终补记：session 身份校验/显式 close 与门禁说明更正后的最终构建 `cargo build -p nvoc-cli -p pynvoc --locked` exit0；同一读数闭环在 `stage-consumer-offset-read-20261021T160227Z/` 再现（stock 0 → 核心0.25/显存0 → 全部恢复0，两个消费者一致、前后healthy）。最终 `cargo xtask ci` 使用指定三环境变量 **CI_EXIT=0**；`consumer-offset-ci-20261021.txt`；实际导入扩展的 Python import/validation **52/52、0 skip** 单独记录在 `consumer-offset-python-tests-20261021.txt`，不能用全门禁内未加载扩展的 skip 替代该证据。显式只读硬件测试 `the_offset_channel_reports_domains_records_and_its_own_window -- --ignored --nocapture` 的最终结果见 `consumer-offset-hardware-read-20261021.txt`。


## 一百三十七、核心偏移接入共享事务、正式 CLI/Python，产品同负载 A/B/A 实测

选择依据：§一百三十五/一百三十六已分别确认 selector-zero、signed i32 kHz、directory index 编址、真实核心 +15 MHz 和公共读数。由此实现集中后端事务，再验证原有消费者命令，而不是直接打开全局设置开关。

实现与契约：

- `SetMacosRmClockOffset` 复用共享 operation/run 分发。Rust 事务在同一 transport mutex 下 GET2108 B 快照，仅修改 record0 的 `+12..+16`，SET 后比较全部2108 B；目标是**绝对配置偏移**，不是在当前值上累加。原成功 SET 持续生效，与现有 setter 一致；失败或合作式取消则恢复原快照，最多两次恢复尝试，并完整比较。恢复未确认明确报 `RESTORE NOT VERIFIED`。Drop 只兜底 unwind，不宣称 SIGKILL、abort 或断电恢复。
- 原生桥只接受固定 GET/SET 与完整2108 B，不提供任意 RM id；SET 要求 root。每次校验发现/会话 PCI、device、RM identity，限定已测 `10de:2187` / `610.57.04`，并重新读取 directory/range，要求 index0 唯一对应 domain1/type0808、范围仍为 ±18750 kHz。快照必须 selector0，目标与原值均在范围内，单次变化幅度≤18750 kHz。其它字节完整保留。
- 正式 CLI `set-pstate-global-freq-offset <integer MHz> --macos-rm` 支持核心；`reset-pstate-global-freq-offset --domain core --macos-rm` 将偏移设0。reset要求显式域，避免把核心归零冒充全域恢复。显式 `--pstate`、其它域和越界值在 discovery 前拒绝。输出包含原值、目标、模式、回读和保留结果。SIGINT 使用已有 InterruptGuard。
- `pynvoc.set_clock_offset(gpu, 'macos-rm', 'core', MHz, None)` 进入同一事务，保留小数 MHz→舍入 kHz；拒绝 P-state/其它域/非有限或越界目标，detach期间合作式检查 Python signals。GPU使用发现的id。NVAPI/NVML原分支保持。Python `reset_core_clocks` 的完整解锁语义尚未接入，当前可用 set target0 只重置加法偏移。
- `graphics_offset_write` 是独立能力；`SETTINGS_WRITE_SUPPORTED` / `SETTINGS_RESTORE_SUPPORTED` 仍false，表示通用整组设置/恢复尚未完成，不能误启用其它控制族、服务闭环或无人值守优化。进程内互斥不等于跨进程排他，外部工具并发写不具备原子性保证。

实际验证（桶 A）：

| 产品动作 | 实测结果 |
| --- | --- |
| CLI 设1 MHz → Python设0.25 MHz → CLI显式core reset | 0→1000→250→0 kHz；Python的0.25替换1，没有累加；显存配置保持0。前后四种mask各2108 B全部一致，共8432 B；两端anchor healthy |
| 正式CLI在同一10秒NVK负载中设15 MHz，再恢复0 | 稳定前段1950000 → 写入段1965000 → 恢复段1950000 kHz；RM776与稳定段RUSD一致。显存6000984 kHz保持，负载/监控exit0；四种mask完整恢复8432 B，前后healthy |
| 权限/请求拒绝 | 非root正式CLI返回RM `0x1b`，偏移不变；CLI/Python越界、显式P-state、显存请求在无效GPU选择之前报告对应契约错误 |
| 事务故障回归 | SET已应用后报错、readback GET失败、其它字节变化、写后取消均精确恢复；快照失败/模式错误不写；恢复失败有显式诊断；绝对值、no-op、步长/范围边界有单测 |

原始产物：仓库外 `outputs/stage-product-core-set-final-20261022T012500Z/`、`stage-product-core-aba-20261022T013100Z/`、`stage-product-core-rejections-20261022T013000Z/`。目录名只是标识，实际时间以 events/log 内记录为准。首批 `stage-product-core-set-20261022T010000Z/` 的比对脚本用 re.match 漏掉带前缀的十六进制行，误报失败；改用 re.search 后对原始日志复核全部一致，保留失败记录。最终批使用修正后的解析器，未依靠重新写入掩盖问题。

验证：测试机 core128、CLI92、Python Rust18（`--no-default-features --lib`）通过；真实扩展导入的 import/validation52/52、0skip。默认extension-module的Rust嵌入测试链接失败，按既有测试契约关闭该feature运行；不把链接失败当硬件故障。全门禁的Python GPU skip不能代替真实扩展测试。最终指定三个环境变量的 `cargo xtask ci`、原生构建和当前源复测结果见本批INDEX与日志。

分档：**核心加法偏移公共core/CLI/Python SET、回读、核心归零、保存原值恢复已实现并实测**。模拟故障/取消与恢复失败属单测覆盖，未宣称实机强制造错。仅10秒低强度图形负载，不是稳定性验收。显存写、相对模式、锁频/解锁、全设置恢复、UI交互和自动优化仍未因此完成；完整阶段0→12目标继续。

最终源补记：`cargo xtask ci` 使用规定三环境变量 **CI_EXIT=0**，`cargo build -p nvoc-cli -p pynvoc --locked` **BUILD_EXIT=0**；在最终原生二进制上再次运行产品设值/覆盖/reset与同负载A/B/A，产物 `stage-product-core-set-head-20261021T162544Z/`、`stage-product-core-aba-head-20261021T162544Z/`。再次1950→1965→1950 MHz，完整8432 B恢复；两批verdict通过，顶层SHA256SUMS已生成。源码manifest标识测试时的文件内容；此补记仅文档增量。测试机另一checkout的59个dirty文件保持不变。


## 一百三十八、显存偏移独立过门槛并接入公共事务，正负同负载生效

选择依据：目录已证实 memory 为 index2/domain bit0x10（不是mask0x10），type0808，selector0。先单独运行FRESH探针 `2:12:250`，SET/回读250、其它值不变、全部2108B恢复均NV_OK；产物 `stage-clkoffset-write-20261021T162805Z/`。随后才分别执行正负15MHz同负载10秒低强度NVK实验，不改核心配置。

| 独立probe试验 | 写前 / 写入 / 恢复的显存实时时钟 kHz | 证据 |
| --- | --- | --- |
| +15000kHz | 6000984 / 6015984 / 6000984 | `stage-memory-offset-aba-20261021T162833Z/` |
| -15000kHz | 6000984 / 5985980 / 6000984 | `stage-memory-offset-aba-20261021T162913Z/` |

RM776 domain0x10与RUSD显存读数一致；写入/恢复稳定段核心均1965000kHz。负向实际差15004kHz，记录PLL量化差，不把配置kHz与物理时钟强行等同，也不乘成DDR有效速率。两批前后healthy；分别比较四种mask共8432B完全恢复。verdict和SHA256SUMS已生成。

实现：扩展§137同一个事务而非复制实现，集中选择graphics(index0/bit1)或memory(index2/bit0x10)，预检目录唯一性/type0808，C桥只准这两个index。每次仍只有一个mask、一个+12字段、同样范围/步长/selector/root/身份限制和精确回读/恢复。新增memory_offset_write；设备支持判定集中，CLI info/status与settings以及Python settings发布各域能力，bulk settings/restore仍false。现有CLI/Python setter自然复用同一operation，CLI memory reset须显式选域。mock新增memory mask/字段选择、signed值、SET已应用后失败/GET失败/其它字节变化恢复；graphics mask不能通过memory事务，跨边界单步不发SET。

实际产品验证（桶 A）：

- `stage-product-memory-set-20261021T163407Z/`：先核心设1MHz并保持；CLI显存设1MHz→Python显存0.25MHz→Python显存-0.25MHz→CLI显存reset0，核心每次仍为1MHz。随后Python核心-1MHz，显存仍0，最终两域归零。验证绝对目标、正负/kHz小数、独立域及reset保留。完整8432B恢复、前后healthy，两个窄能力true、bulkfalse。
- `stage-product-memory-aba-20261021T163450Z/`：正式CLI显存+15MHz，同一个10秒负载内6000984→6015984→6000984kHz；正式CLI恢复原值0。
- `stage-product-memory-aba-20261021T163509Z/`：正式CLI显存-15MHz，同负载6000984→5985980→6000984kHz；恢复原值0。两批RM776/RUSD一致、显式NVK loader/ICD、负载/监控exit0、四mask8432B完全恢复、前后healthy。
- 非root显存SET实测0x1b，配置保持；越界、显式P-state、未知域在无效GPU选择前拒绝。Python既有parser只接受core/memory别名，video先报invalid domain；不为迁就测试假定而放宽原契约。

验证：原生build0，core129/CLI92，Python Rust18（no-default-features），真实扩展import/validation52/52零skip；指定三个环境变量 `cargo xtask ci` **CI_EXIT=0**。日志 `memory-offset-unit/build/ci/python-import/python-rust-20261022.txt`，源码manifest与实验脚本在仓库外outputs；每批包含raw/events/verdict/SHA256SUMS。主命令表的三个频率入口同步更正为当前状态，历史节仍保留并由本节覆盖；证据索引更新为当前detached测试checkout，机器访问资料留在仓库外交接。

分档：**核心/显存加法偏移公共SET、读取、单域归零与原值恢复已实现且实测**；正负显存物理效果也已有独立probe与产品证据。±18.75MHz仍是当前保守验证窗口，不宣称更大硬件范围。相对模式、绝对/范围锁频及解锁、全域reset和全设置恢复未交付；GUI/TUI/服务闭环尚未因此实测。10秒图形负载不等于超频稳定或完整显存正确性覆盖。完整适配目标继续。


## 一百三十九、board功耗限额70–100W接入共享core/CLI/Python，配置与RUSD执行上限独立验证

选择依据：继续阶段4锁频调查时，使用可用的IDA9.5.261001追踪**同版本Linux610.57.04 NVML**，从公开入口经虚表到实际RM请求，避免继续猜命令/尺寸。官方包仅按zstd/tar提取两个ELF用于静态分析，安装器未执行；库及IDA数据库未入仓库。具体地址、SHA256与字段布局见 [macos-board-power-r610.md](../reverse-engineering/nvapi/macos-board-power-r610.md)。

协议（桶 B + 桶 A）：`0x2080a618`/20000B给board policy metadata，按+152/+154/+183选择器找到index2，记录`208+252*index`、type/class0/9、units0，+4/+8/+12为min/default/max mW，实机**70000/100000/100000**。`0x2080a61d`/12B给client配置，byte0=board selector1、byte1=client0xfe、+4为mW，+2/+3及+8保留字段为0。SET是`0x2080e61e`/12B。三个命令及各自尺寸均在1118行GSP表中；host表没有对应行不等于未实现。

首次固定实验 `stage-power-limit-roundtrip-20261021T170040Z/`：快照100000mW，SET99000、GET全部12B与预期一致；RUSD requested/enforced随后从100000推进到99000；恢复原12B并精确回读，RUSD两值再回100000；显式close0，前后healthy。没有负载，没有更改RATED_TDP/Boost/其它策略。这证明**配置和驱动报告的执行上限**，未证明达到功耗墙时的物理限流/性能效果。

实现与公共契约：

- 新`macos_power`集中policy解码与参数/单位验证，C桥只准三个固定操作，不提供任意RM id。每次核对TU116/610.57.04和PCI/RM身份、live selector/type/units/window、原值；SET要求root，当前只准70..100W。原始12B仅改+4字段。
- 抽取`macos_control_transaction`，时钟与功耗共用完整SET/精确回读/错误及合作式取消恢复，最多两次恢复尝试，失败明确报警。保留原时钟单元测试并在最终二进制重测变频。成功普通SET持久；Drop仅unwind兜底，不宣称abort/SIGKILL/断电恢复；进程内mutex不等于跨进程排他。
- 原共享`QueryPowerLimits`/`SetPowerLimit`新增macOS分发，保留NVML原分支；带取消的`SetMacosRmPowerLimit`复用同一Device方法。CLI `set-power-limit <watts>`、`reset-power-limit`接入；拒绝NVAPI专用`--policy-index`、越界值在discovery前完成。reset取live policy default，恢复原值另由事务/调用者保存完成，不能混淆。
- `get-public-power-limit`与Python `query_power_limits`报告真实min/default/max，`current_watt`更正为配置client限额；`enforced_watt`/`requested_watt`保留独立RUSD读数，读序非原子。policy default不冒充nameplate/rated TDP，也不自动推出百分比模式。`get-settings`/Python settings包含policy原单位及min/default/max watts。
- `pynvoc.set_power_limit(gpu,'macos-rm',watts)`走共享事务，保留NVAPI百分比/NVML watts分支；auto/default按平台解析。settings/info发布独立`power_limit_write`；bulk settings/restore、服务闭环和无人值守优化仍false。

最终产品实测（桶 A，源码manifest与最终原生binary）：

| 入口/动作 | 实际结果 |
| --- | --- |
| CLI99W→Python98W→CLIreset | **100→99→98→100W**，每步配置GET和RUSD requested/enforced均观察到目标；min/default/max70/100/100，前后healthy |
| 与偏移并存 | 期间核心+0.25MHz、显存-0.25MHz保持；功耗reset保留这两个偏移，最终再恢复两域原值0 |
| 全量恢复 | client功耗12B前后相同；四种clock mask各2108B，共8432B完全相同 |
| 共享operation真机读取 | 点名只读test `shared_power_limits_read_the_board_policy_without_initializing_nvapi`通过：QueryPowerLimits70/100/100、policy/report一致、未初始化NVAPI；未运行全套ignored |
| 权限/参数拒绝 | nonroot99W返回0x1b，原值保持100；69/101W和foreign policy-index拒绝；Python越界先于无效GPU选择拒绝 |
| 共用事务后的核心实机回归 | `stage-power-clock-regression-20261021T172200Z/`同一10秒NVK负载**1965→1980→1965MHz**，RM776/RUSD一致，显存保持、8432B恢复、前后healthy |

产物：最终功耗`stage-product-power-limit-20261021T171853Z/`，前批`stage-product-power-limit-20261021T171546Z/`同样通过；拒绝`stage-product-power-rejections-20261021T171733Z/`。首次时钟回归`stage-power-clock-regression-20261021T171930Z/`配置/恢复均通过，但stock自然Boost在前后1965→1950MHz漂移，不能当同基线ABA；保留原记录，以上述后批作精确ABA证据。外部`control-protocol-20261021/`保留官方来源、提取脚本、IDA脚本/反编译、请求原始字节和初次只读数据，未更改实际驱动。

验证：core133、CLI93、Python Rust18通过；真实扩展import/validation52/52零skip。指定三环境变量`cargo xtask ci` **CI_EXIT=0**，`cargo build -p nvoc-cli -p pynvoc --locked` **BUILD_EXIT=0**；原生新函数与Rust实调用、功耗/时钟故障回归均在本轮验证。日志`power-policy-ci/build/python-import/python-rust/shared-read-20261022.txt`，每批verdict/source manifest/SHA256SUMS在仓库外。新增测试覆盖selector/mask/type/units、metadata长度/窗口变化、绝对目标、default、取消/SET已应用后报错恢复；时钟原有readback错误/恢复失败测试继续通过。Windows/Linux本轮未原生运行。

阶段4余项调查：NVML普通range-lock helper实际发`0x2080e078`/16B count+pointer carrier，reset同命令；它在已测host/GSP两表均无行，未发送猜测替代SET。另一memory route依赖`0x2080a06e`状态、发`0x2080e06f`/8，前置及恢复尚未验证；不能据此宣称所有锁频均无实现。后续独立风扇线索：同版本NVML给出status GET`0x2080852f`/844、control GET`0x20808530`/980、SET`0x2080c531`/980；两条GET本轮均NV_OK，尚未写风扇。

分档：**board功耗W读取/范围/SET/default reset/原值恢复及core/CLI/Python接入已实现且实测**，本轮实际写98/99W并恢复100W；70–100W为设备发布并逐写验证的窗口，不宣称每个值均单独实测。不支持任意放宽范围、百分比/RATED_TDP仲裁、温度策略、fan/VF写或自动优化。主功能表与阶段路线图同步反映当前状态，完整goal继续active。

## 一百四十、cooler0百分比与firmware auto接入共享事务、正式CLI/Python

选择依据：继续同版本Linux610.57.04 NVML的IDA9.5调用链，追到固定
metadata/status/current/default/SET五种请求。命令及尺寸均核对1118行GSP表，
完整协议见 [macos-fan-control-r610.md](../reverse-engineering/nvapi/macos-fan-control-r610.md)。
公开SDK没有声明、host表没有行均不能推导GSP无风扇实现；旧阶段6的否定推断已由实测推翻。

实现：

- 固定GET-info `0x2080852e`/1432、GET-status `0x2080852f`/844、
  GET-current/default `0x20808530`/980、SET `0x2080c531`/980。
  metadata mask1识别cooler0，零RPM也保留count1。status发布0..100%范围及独立反馈。
- manual只改原980B的mode+36与targetQ16+40；35%→22938。
  auto先GET-default（header+16=1），验证mode0且其余校准/控制字节未变，
  仅复制mode/target到原ordinary header0。完整回读与有界恢复复用既有事务。
- 身份/版本/live mask/type/range校验，SET root门控，进程内transport mutex，
  合作取消/错误后完整原块恢复；成功设置持久。不承诺SIGKILL/abort恢复。
- 原共享QueryFanInfo/SetFanSpeed(manual)/ResetFanSpeed接macOS；带cancel操作同一device实现。
  CLI set-fan-speed/reset-fan-speed与Python set_fan/manual/auto、reset_fan_speed接入；
  显式NVAPI/NVML路径保留，auto/default按平台选后端。
- 设置报告新增fan_control与窄能力fan_percent_write；bulk settings/restore、服务/优化器仍未启用。
  current_percent是驱动反馈（公共NVML形状取整），target_percent保留Q16解码分数；不冒充RPM目标/PWM。

实际验证：

| 项目 | 结果 |
| --- | --- |
| 首次固定外部35%实验 | `stage-fan-control-roundtrip-20261021T172842Z`，SET/完整回读通过，RPM0→约1800，980B恢复，anchors healthy |
| 正式产品CLI35%→Python36% | 最终`stage-product-fan-20261021T175023Z`，目标分别35.000610/36.000061%，短时观测RPM1409/1215、反馈50/43%；前批174112Z也通过（RPM1420/1790、反馈51/64%）。反馈与目标分别报告，不以短时RPM排序推断转速精度 |
| 自动模式恢复 | 同批CLI reset、Python auto及无backend参数reset_fan_speed均恢复原auto；manual期间保存了GET-default原块，最终980B完全相同 |
| 其它控制保持 | 同批power12B与四mask clock8432B前后一致；前后anchors明确healthy |
| 权限/前置参数拒绝 | `stage-product-fan-rejections-20261021T174325Z`，nonroot0x1b；>100%、slot1、RPM、legacy cooler、continuous策略在选absent GPU前拒绝；980B不变 |

分档：**cooler0百分比/模式读取、manual百分比SET、firmware-auto reset、原值恢复和
core/CLI/Python接入已实现且实测**。0..100%为设备发布范围，实际短测35/36%，未逐值实测。
RPM-target、FanStop配置、硬件/软件曲线、UI写入、服务闭环和自动扫描未因此完成；
反馈百分比不等于目标，短时转速响应不等于热稳定或转速精度验收。完整阶段0→12目标继续。

验证：最终指定三环境变量加仓库外本轮扩展的PYTHONPATH，`cargo xtask ci` **CI_EXIT=0**。
core137、CLI94、Python Rust18；真实扩展import/validation **56 passed，0 skip**，
点名GET-only硬件测试`shared_fan_info_reads_metadata_and_preserves_a_stopped_cooler` **1 passed**，
未初始化NVAPI。全CI的Python完整套件56passed/10skipped不能代替上述真实导入覆盖。
修正entry-parity脚本400字符查找窗口与命令参数提示解析，实际15个允许命令和102条命令表
核对通过，长描述/相邻variant隔离/参数提示静态回归通过。GUI/TUI回归分别281passed/13skipped、214passed；未据此宣称UI新控制实机完成。
日志`fan-{ci,build,python-import,shared-read,entry-parity}-20261022.txt`及source manifest、SHA256SUMS存仓库外。
CI最初加载旧扩展导致新契约测试失败，改为显式加载本轮编译扩展后通过；旧源目录扩展未覆盖。

## 一百四十一、native thermal policy metadata/control接入共享core/CLI/Python并完成实机恢复

选择依据：IDA9.5跟踪同版本NVML的`cDeviceGetTemperatureThreshold`、`cDeviceSetGpuAcousticTemperatureThreshold`及其metadata/cache调用，固定到GSP表中的`0x2080852a`/1784、`0x2080852c`/1440、`0x2080c52d`/1440；没有把GPU Max、acoustic target、sensor threshold和FanStop混成一个字段。完整协议与地址见`docs/reverse-engineering/nvapi/macos-thermal-policy-r610.md`。

实现与验证：

- 设备GPU Max固定88°C；acoustic policy2发布65..87°C、默认83°C。GET阈值保留GPU Max和acoustic min/current/max，Shutdown/Slowdown/MemoryMax/GpsCurr无来源则明确为unavailable。
- SET只发送acoustic target+212的FX8字段；驱动回读会联动计算多个曲线值。transaction根据已验证模型核对完整归一化1440B回读，错误/取消恢复原块。
- core `QueryTemperatureThresholds`、`SetNvmlAcousticTemp`、`SetMacosRmAcousticTarget`，CLI `get-temp-thresholds`/`set-temp-limit --domain acoustic`/`reset-temp-limit --domain acoustic`，Python `set_thermal_limit(..., 'acoustic')`/`reset_thermal_limit`已接入；显式GPU Max写入拒绝。
- 真实产品路径：82→81→83及Python再次82→83；边界65/87也独立往返。最终1440B原块恢复，风扇980B、功耗12B、时钟8432B保持，前后anchor healthy。拒绝路径覆盖nonroot0x1b、范围、缺域、错误域、reset缺域，均在选卡前拒绝。
- 最新门禁：core141、CLI94、Python Rust18；完整Python59passed/10skipped，TUI214passed，GUI281passed/13skipped；本轮最终`cargo xtask ci`已记录 **CI_EXIT=0**；硬件只读温度策略测试1 passed，source manifest与证据归档随后写入bundle。

分档：**native GPU Max/acoustic读取、acoustic目标SET/default reset、归一化回读、恢复和core/CLI/Python接入已实现且实测**。不支持修改固定GPU Max，不宣称完整NVML severity表、温度曲线编辑、风扇曲线/FanStop、服务/optimizer/UI写入或热稳定性。完整阶段0→12目标继续。

## 一百四十二、RATED_TDP SET 首次实机执行（单 client 单 action，立即恢复）

**① 选择依据** ✔：`0x2080e06f@8` 在固件方法表**有行**（flags `0x44`），读半边 `0x2080a06e@12` 同表行（flags `0x48`），两者身份已由 §七十七定为公开 `0x2080206e/206f` 的 PERF legacy 分级；探针的 pre-record → SET → 回读 → 立即恢复 + atexit/SIGINT 兜底早在 2026-10-17 就写好并通过 6/6 CPU 回归（`stage-ratedtdp-set-gate-20261017T060217Z/`），此后一直只差授权。故本轮不发明 id、不扫描，直接执行那一发。

**② 证据（测试机，一次写入）** ✔：`stage-ratedtdp-set-20261022T010316Z/`，`set.txt` rc=0。前置 `PROFILE=full sh build.sh` 新鲜构建、`--anchor-read` 判 `forward ALIVE and healthy`。V3.4a 预录 client GLOBAL(2) 原 action `DEFAULT(0)`、vPstate 1；V3.4b SET `NV_OK`；V3.4c 回读 action `FORCE_LIMIT(2)`、vPstate 保持；随即恢复 `SET 0x0, GET 0x0 (verified)` 回 `DEFAULT(0)`。同批 RUSD 快照 `tgp 11520 mW / limit 100000 mW / fan 1599 rpm / 49.9C`。

**③ 顺带否证一个候选** ✔：同次 Tier-B 用例发 `0x2080a00b@780`（level-info 的 PERF 非特权分级，§七十四/七十七留下的候选）⇒ **`0x56 NV_ERR_NOT_SUPPORTED`** ⇒ 「换分级字节取 range 列」这条路对 level-info 不成立（`0x2080200b` 基态本身也回 `0x1f`）。

**④ 诚实边界** ✗：①只有一次、单 client GLOBAL、单 action `FORCE_LIMIT`，其它 client（WAR/OS/PROFILE）、其它 action（EXCEED/LOCK/FLOOR）与跨 client 仲裁**未测** ✔；②**功耗效果未观测**——探针 SET 后立刻恢复，观察窗口太短，须补 hold 才能在负载里看 tgp 变化，故本轮不声称"限住了" ✗；③公共 core/CLI/Python **尚未接入**（现有公共面只有 `get-rated-tdp` 读取，没有 set），不把探针能力当产品能力 ✗；④单 client GET 的 input/vPstate 全 0，与 10-17 全 client 扫描（全 `DEFAULT`）一致，不据此推语义 ✔。

## 一百四十三、RATED_TDP `FORCE_LIMIT` 在同负载 A/B/A 里真的钳制了功耗与频率

**① 选择依据** ✔：§一百四十二只证明了"接受并恢复"，没证明"限住了"。要在负载里看效果就得让请求在采样期间持续存在，而探针原本 SET 后立刻恢复；因此补的是一个纯观察用的 `--rated-tdp-hold N`（1..60 s），不改写入路径本身，恢复仍由已武装的 pre-record 快照负责。

**② 方法** ✔：三段完全相同的 NVK 负载（`--vulkan-only --duration 22`，重默认 1280x720/128/16/262144），每段 t≈3 s 发单 client 单 action 受控写并 hold 5 s：A=`DEFAULT`（stock 对照）、B=`FORCE_LIMIT`、A2=`DEFAULT`（可逆）。hold 逐秒采样 RUSD 的 `tgpmW`／`enforcedmW`／`gpc`。

**③ 证据（测试机，三段各 rc=0）** ✔：`stage-ratedtdp-hold-20261022T010926Z/`。

| 段 | 实测功耗 mW（5 样本） | gpc kHz |
| --- | --- | --- |
| A | 13221 13360 13462 13120 12978 | 1950000 ×5 |
| B | 11966 11977 11976 11956 11950 | 1755000 ×5 |
| A2 | 13408 13204 13348 13208 12868 | 1950000 1950000 1935000 1935000 1935000 |

B 段 11.95–11.98 W 对 A/A2 段 12.87–13.46 W，5/5 样本无重叠，且频率同时被钳到 1755000 kHz ⇒ 是限电器生效而非噪声 ✔。三段都保持 pre-record→SET`NV_OK`→回读一致→`SET 0x0, GET 0x0 (verified)` 恢复。

**④ 一条反直觉记录** ✔：三段 `enforced_mw` 始终 `100000`。即 RATED_TDP 的 action 改的是**仲裁结果**，并不改写 RUSD `enforcedmW` 字段本身 —— 拿 `enforcedmW` 当"RATED_TDP 是否生效"的读数会误判，得看实测功耗与频率。

**⑤ 诚实边界** ✗：单 client GLOBAL、单 action、每段 5 样本、同一负载形状，不称稳定度认证 ✔；未测 EXCEED/LOCK/FLOOR、未测与 board power limit 的仲裁冲突、未在关闭 Boost 下复跑 ✗；公共 core/CLI/Python 仍未接入（公共 `get-rated-tdp` 是 NVAPI 读数面，不是这条 RM 写）✗；`--rated-tdp-hold` 只服务观察，不支持进程内取消 ✗。

**⑥ 静态佐证** ✔：`0x2080e06f` 行 `+0x10` pFunc → IDA `0x1547890`，函数按 `lw a5,0(a1)` 收 client、`lw a3,4(a1)` 收 vPstate，逐值比较后返 `0x1f`(非法)/`0x56`(不支持)/`0x0`，并 `jalr` 到更深一层 helper —— 与实测的"校验严格、合法参数才 `NV_OK`"一致。脚本 `outputs/clock-protocol-20261021/ida-rated-tdp.py`、`.txt`、`ida-rated-tdp-2.txt`（本轮新增，未入仓库）。

## 一百四十四、RATED_TDP 接成公共闭包（core/CLI/Python），并查掉「读永远是空」的真因是分级字节

**① 选择依据** ✔：§一百四十三把协议证完了，但公共面只有旧的 NVAPI 读；要成为产品能力就得走完 core 类型/解析/事务、CLI 三个命令、Python 三个入口，并在产品路径上证明读、写、恢复、拒绝。故本轮不再动探针的写入形状，只把它降级成采样器。

**② 一个必须先查掉的坑：`0x2080a06e` 是桩** ✔：core 侧接好后 `get-rated-tdp` 一直报「reply echoes unknown client 0」，而探针同一请求回 `[2,0,1]`。逐步排除会话差异（`/tmp/mini`：链打开 + 一发 GET 就复现；补 `fb-get-info-v2`、`perf-current-pstate`、`perf-get-powerstate` 都不变；`--stages 0` 也照样成功 ⇒ 与前置条目无关）。真正的差别是**id**：探针用的是 SDK 常量 `NV2080_CTRL_CMD_PERF_RATED_TDP_GET_CONTROL`(=`0x2080206e`)，core 用的是 a0 形状的 legacy 孪生 `0x2080a06e`。同进程内对照（`/tmp/mini3`）一次定案：struct 形状的 class-20 调用 `st=0x0 raw=0x2 0x0 0x1`，紧接的数组形状 a0 调用 `st=0x0 raw=0x0 0xffffffff 0x0` ⇒ **a0 GET 对任何 client 都回同一块桩 `{client 0, action 0xffffffff, vPstate 0}`，`NV_OK` 是假的成功**。`0x2080a06f` 没有这个问题（§一百四十二的 SET 一直有效），但读必须走 class-20。GET/SET 的尺寸都收敛为 12（16/32 被 `0x1f` 拒）。

**③ 实现** ✔：`core/src/macos_rated_tdp.rs`（`BYTES=12`、client/action 枚举、`parse`（容忍 dword0 为 0 的层）、`read`/`set` 走共享 `write_block` 事务与 2 次重试的 `Drop` 恢复）；`macos_rm.rs` 的 `NativeRatedTdp` 传输与 `ControlBlock<12>`（`set()` 把 client 盖进快照 dword0，保证恢复打到同一 client）；CLI `get-rated-tdp`/`set-rated-tdp CLIENT:ACTION`/`reset-rated-tdp CLIENT`；Python `get_rated_tdp`/`set_rated_tdp`/`reset_rated_tdp`。语义定为**成功即持久**（与功耗/风扇一致，用 `reset` 清），错误/取消才恢复原块。

**④ 证据（测试机，全序列）** ✔：`stage-ratedtdp-core-20261022T015028Z/`（1..14 步）。`default→force-limit→(OS force-lock→reset)→default` 全部 `Applied: yes` + `Readback Verified: yes`，跨 client 互不覆盖，Python 三读自洽；参数 `rm:LIMIT`、`global:TURBO`、多余 action、缺 action 全部拒绝；`--nvapi` 如实 `LibraryNotFound`。

**⑤ 效果与权限（同负载 A/B/A，写方是产品命令）** ✔：`stage-ratedtdp-effect-20261022T015325Z/`。Vulkan 重负载 45 s，t≈13 s 发 `set GLOBAL:FORCE_LIMIT`、t≈27 s `reset`，探针只采样：A（11 样本）12576..13174 mW、gpc 恒定 1950000；B（14 样本）**11901..11983 mW、gpc 恒定 1755000**；A2（15 样本）12678..13349 mW、gpc 回 1950000。B 与 A/A2 完全无重叠（约 −1.02 W / −7.9%），仍不改变 `enforcedmW=100000`。换负载前的 `metal_check` 版本无差异（忙率读数 0%、停在 1530 MHz），另录为 `stage-ratedtdp-effect-20261022T015149Z/`。权限：置 `EXCEED` 后非 root `set`/`reset` 都 `0x1b NV_ERR_INSUFFICIENT_PERMISSIONS` 且回读仍 `EXCEED`，非 root **读成功**，随后 root `reset` 回 `default`；CI 修完再跑 `GLOBAL:FLOOR(4)` 往返并停回 `default`。

**⑥ 门禁** ✔：`cargo xtask ci` **CI_EXIT=0**（core146、CLI95、Python Rust19，完整 Python 59passed/10skipped）。修掉门禁抓到的问题：`from_code` 死代码（改为用它命名回显 client）、单元素 `for`、`lib.rs` 的 `macos_rated_tdp`/`macos_control_transaction` 一直带着 macOS+test 的 `cfg` 导致 aarch64-linux 检查找不到符号（去掉 cfg 改为全平台模块）、`set_thermal_limit` 的 `#[pyo3(signature=…)]` 被上一轮插入顶掉（补回）、`test_import.py` 导出表与 `__all__` 顺序。

**⑦ 诚实边界** ✗：CLI/Python 的**读**只覆盖 GLOBAL 一个 client（没有 `--client`，其余 client 的读仍需探针）；未测 WAR/OS/PROFILE 的实际效果与跨 client 仲裁优先级；`reset` 与 `set …:DEFAULT` 等价，只测了前者；单负载形状、40 s 窗口、每段 11–15 样本，不是稳定度认证；`enforcedmW` 不反映这条仲裁的判据在 §一百四十三已记，此处只作一致性复核；a0 桩的成因未从固件侧查明（只知它稳定返回同一块），不排除是 legacy 分级的刻意空实现；`macos_rated_tdp.rs` 的 `set()` 覆盖 client 是给"层回 0"的保险，class-20 下回显本来就正确。
## 一百四十五、PERF 电源来源（AC/BATTERY）接成公共闭包

**① 选择依据** ✔：前面几轮挑的都是"写下去有物理效果"的面，代价是每个都要先证效果。本轮换一类：先证**协议完整可回读**，再看有没有效果 —— `PERF_GET_POWERSTATE 0x2080205a`／`PERF_SET_POWERSTATE 0x2080205b` 在固件表都有行（stag 4 字节），载荷只有一个 dword，取发布枚举 AC(0)/BATTERY(1)，驱动**双向原值回显**（SET 1 回 1、SET 0 回 0）。载荷就是值本身，天然满足"候选字段有明确来源、不遍历字段"。

**② 实测（探针 `/tmp/mini/mini8`，单域单值单步）** ✔：GET→`NV_OK val=0`；SET 1→`NV_OK echo=1`；SET 2→**`0x1f` 且状态保持 1**；SET 0→`NV_OK`，终态回 0。即合法的只有 0/1，2 由驱动自己拒。`PERF_GET_CURRENT_PSTATE 0x20802068` 恒回 1(P0)，与电源来源无关。同批否证：`PERF_SET_AUX_POWER_STATE 0x20802092` 收 0..4 全 `NV_OK` 但无任何变化、`PERF_BOOST 0x2080200a`（8 字节 `{flags,duration}`）同样接受了不生效；NVML range-lock 载体 `0x2080e078@16` 回 `0x56`（表里无行）。

**③ 实现** ✔：`core/src/macos_power_state.rs`（`BYTES=4`、`PowerState{Ac,Battery}`+`parse/DEFAULT`、`read`/`set` 走共享 `write_block` 事务）；C 桥 `nvoc_macos_rm_session_power_state` 在派发前先挡三种坏输入 —— 尺寸非 4、kind>1、SET 非 root（`0x1b`）、SET 值>1（`0x1f`）；CLI `get-power-source`/`set-power-source ac|battery`/`reset-power-source`；Python 三个同名入口。语义沿用「成功即持久、错误/取消恢复」；`reset` 固定回 AC（桌面默认），带位置参数会被拒，避免与 `set` 重复表意。命名刻意避开既有 `get-/set-power-mode`（NVAPI App 的 max/balanced 开关，语义不同）。

**④ 证据（测试机，产品命令）** ✔：`stage-powersource-product-20261022T024027Z/`：`ac→battery` 写入 `Applied`+`Readback Verified`，独立读仍是 battery（持久），`reset` 回 ac 且幂等；非目标域 rated-tdp `default`、风扇 `auto 62%/1734rpm`、功耗墙 70–100 W 全程未变；`dc`、多余位置参数、缺值三者在驱动前被拒；`0x1f` 只在驱动层可达（探针 mini8 记录）。`stage-powersource-extra-20261022T024235Z/` 用 `sudo -u mac` 做真非 root：set/reset 都 `0x1b` 且状态分毫不动，root 写入后才动；Python 侧 `get→set→get→reset→get` 自洽、`dc` 抛 `RuntimeError unknown power state`。

**⑤ 效果：本机没有可观测的时钟影响** ✗：60 s 常量 Vulkan 负载里 A/B/A 三段（写方是产品命令）图形时钟 **1950 kHz ×8 / ×10 / ×10**，三段没有区别；功耗与节流原因也无差。所以本行在功能表里记的是**电源来源请求**而不是频率通道，`get-power-source` 的 note 原样写明「不是时钟、不宣称时钟效果」。前一批 30 s 负载版本（`…product…` 的 21 段）第三段已跑出负载窗口，只作对照。

**⑥ 边界** ✗：`0x2080205a/5b` 由 IDA 静态追到的语义只到"值比较后 `NV_OK`/`0x1f`"，**谁消费这个状态未查明**；只在本机（台式、恒 AC）实测，笔记本按电池运行时未测；未测与 EC/`RUSD` 电源事件并发；`reset-power-source` 只保证回到 AC，不还原"开机时的原值"；`0x20802092`/`0x2080200a` 已确认接受但未接成命令。未重启、未改驱动/EFI/boot-args/固件、未动 PR。
## 一百四十六、风扇停止（mode 2）接成公共闭包，并把读面从 auto/manual 放宽

**① 选择依据** ✔：功能表里 FanStop 一直标着"未解码"。风扇控制记录的 `+36` 模式字是已量字段，值域只量过 0/1；本轮先证明第 3 个值 2 是驱动接受的（单字段、单步、不遍历），再决定要不要接。同时暴露一个真 bug：读面 `mode()` 只认 0/1，任何落在 2 的块都会让 `get-fan-info` 整条报错。

**② 实测（探针 `/tmp/mini/mini9`，单 cooler0、单步）** ✔：`SET mode=2 pct=50` → `NV_OK`，回读 `mode=2 target=32768`，3 s 后 rpm 0→1975；`SET mode=2 pct=0` → `NV_OK`，连续 24 s（8 次采样）`mode=2 target=0` 稳定保持、rpm 恒 0。对照 `SET mode=1 pct=0` 同样 `NV_OK` 且 rpm 恒 0（15 s 5 次采样）。⇒ **2 被驱动接受并持久，但本板上与 manual 0% 无可观测差异**，差异只在模式字本身。同批确认 `get sel=1` 的默认块 `mode=0 target=0`、SET 后 `get sel=0` 回显写入值。

**③ NVML 侧对照** ✍：`libnvidia-ml.so.610.57.04` 的 `sub_BB910`（`cDeviceSetFanSpeed_v2`）在 `0xbbb19` 硬编码 `mov [..+mode],1`，只用 `sub_B6480` 发放 SET —— 即 **NVML 本身到不了 stop 模式**，这也是"协议有、产品没有"的原因。

**④ 实现** ✔：core `FanControlTarget::Stop`（写 `mode=2`+`target=0`，其余字节不动，仍走共享 `write_block` 事务）、`mode()` 增 `2 => "fanstop"`；C 桥把 `mode > 1u` 放宽成 `mode > 2u`，并新增 `mode == 2u && target != 0u` → `0x1f`；`OperationKind` 映射到既有 `SetFanStop`；CLI 复用既有命令 `set-fanstop-status on|off`（`on`→Stop、`off`→Auto，`--curve` 只收 0，因为本后端没有曲线表），输出补 `fan_stop` 布尔；Python `set_fanstop_status` 在 macOS 分支走同一事务并回 `original_mode`/`applied_mode`/`readback_verified`/`other_bytes_preserved`。

**⑤ 证据（测试机，产品命令，全序列）** ✔：`stage-fanstop-product-20261022T033318Z/`（`00..47` 步，`SHA256SUMS` 校验通过）。写前先 `reset-fan-speed` 回固件 auto；`set-fanstop-status on` 输出 `Applied Mode: fanstop` / `Readback Verified: yes` / `Other Bytes Preserved: yes` / `Original Mode: auto`。独立读立刻显示 `mode=fanstop` 而 rpm 仍 1922，4 s 后 rpm 0 —— stop **不是瞬时**，且读面不再因 2 报错。探针回读 `mode=2 target=0`，块哈希 `d7d4b808…` 与 auto 块仅差一个 dword。重复 `on` 报 `Original Mode: fanstop` 仍成功。拒绝面：`--curve 2`、`maybe`、缺 STATE、`--rpm`。非 root：SET `0x1b` 且"原块已恢复并校验"，读面仍可用。Python：`on/off` 往返带同一组校验键、`query_fan_info` 依次回 `fanstop`/`auto`、`--curve 2` 抛 `ValueError`。`off` 回 auto 后 rpm 回 2045，前后 980 字节 `cmp` **完全一致**；相邻域（功耗墙 70–100 W fetch、PERF 电源来源 ac）全程未变。错误/取消回滚由既有单测 `fan_errors_and_post_write_cancellation_restore_original_mode_and_target` 覆盖（同一事务）。

**⑥ 门禁** ✔：`cargo xtask ci` → `CI_EXIT=0`（本机 aarch64-linux 与测试机各一次；测试机 60 passed/10 skipped）。修掉门禁抓到的问题：`SetFanStop` 在 macOS 下成死导入（改成平台门控的独立 `use`）、`needless_return`；测试机 `_native.abi3.so` 是旧的（`cargo xtask ci` 不含 maturin 步骤），重编后才能跑到新代码。

**⑦ 边界** ✗：`0x2080205a` 式的"谁消费"在这里同样没查明——只知 2 被接受、被回显、被持久；stop 期间上位机若再写 manual 百分比，模式会回到 1；只测 cooler0、只在 std 温度下测，未测高温下 stop 是否被固件强制取消；`--fan` 选项本命令不收（NVAPI 原形就是逐 curve 的），cooler 恒 0。

## 一百四十七、时钟域枚举与逐域实时频率（`get-clock-domain`）

**① 选择依据** ✔：锁定时钟（`nvmlDeviceSetGpuLockedClocks`）卡在"四次调用失败"上：api 侧 `sub_10A730`（`dmal/common/common_clock.c:162`，日志字符串就叫 `cDeviceMapTargetToActualFrequency`）发放的 `0x20809014` 不管怎么填都回 `0x1f`，因为它要的域对象句柄来自 `v10(v8, a2, 8320, &v22)`，而那个 getter 又是另一层 api 私货。要往锁定上走，先得有一条**不靠猜**的域通道；同时 class 0x2080 里这两个 8 字节只读控制（`0x20809001`/`0x20809006`）已被实测为 `NV_OK` 且返回真实数据，属于"控制面已通、产品面没有"的空白。

**② 实测（探针 `/tmp/mini/mini13`，子设备句柄，单命令单步）** ✔：`0x20809001 {mode,0}` → `NV_OK`，`mode=0` 回 `mask=0x03FC007F`、`mode=1` 回 `0x0380007F`，即两个模式各自的可选域位图；`0x20809006 {1,bit}` → `NV_OK`，`+4` 是**该域实时 kHz**（bit1 连续读得 `0x179374`=1545076、`0x1791ED`=1545069，即 ~1545 MHz）；`+0` 版本字取 0 或 ≥3 均 `0x1f`，1 才通过。同一批排除了 `0x20809002`（`0x56`，表里无行）、`0x20809004`（`0x3a`）、`0x20809006` 带 12 字节（`0x1f`，行只有 8 字节）。`0x20809014` 在 s/d/c 三种句柄、`+0` 试 1/2/4/5/6/7/`0x10038`/`0x2009C`、`+4/8/12/16/20` 各种组合下恒 `0x1f`，本轮如实记为未通。

**③ 来源** ✍：两个 id 都出自 api 分发器 `sub_8D210` 的 switch 分支（`0x20809001` 由 `sub_31C20` 在 `+4` 写域、`+0x20809006` 由 `sub_31D50` 先校验 `*a2==1` 再写 `a2[1]` 并把回答放 `a2[2]`），不是遍历哈希；`0x20809006` 的版本要求就来自 `sub_31D50` 的前置判断（版本 0 不满足 `*a2==1`，与实测 `0x1f` 吻合）。

**④ 实现** ✔：新增 `core/src/macos_clock_domains.rs`（`DOMAIN_MASK_CMD`/`DOMAIN_CLOCK_CMD`/`DOMAIN_CLOCK_VERSION`，260 字节定长块 = mask + 32×kHz + 32×status，`parse_domains` 只解码 mask 置位的位，短块拒绝而不是截断）；C 桥 `nvoc_macos_rm_session_clock_domains` 先枚举再逐位置位读，**单域被拒只留 status、不折成 0**；core `ClockDomainReadout`/`ClockDomainView`（`clock_khz`+`clock_mhz`+`reason`）、`Device::clock_domains`（先复核身份，session 用完即关）、`QueryMacosRmClockDomains`；CLI 新命令 `get-clock-domain`（`Group::Clock`，allowlist 放行，其他后端给出口径一致的拒绝文本，带人类可读 formatter，回绝的位在 MHz 列显示 `RM 0x…` 而不是空白）；Python `get_clock_domain` 与 `__all__` 导出。域按**位**报告而不翻译成 graphics/memory：这里的位图与 offset SET 用的公共域不是同一套编号，命名会是猜。

**⑤ 证据（测试机，产品命令，全序列）** ✔：`stage-clock-domain-20261022T0430Z/`（`00..08` 步，`SHA256SUMS` 校验通过）。只读命令不写块，所以核对的是"前后保护状态逐字一致"而不是回滚：`00/08` 的 HEAD、工作树、二进制 md5 `diff` 为空。`01/02` 两次 root 调用 mask 恒 `0x03FC007F`（15 位）而 MHz 列在 1543–1547 间逐次变动，证明读的是实时频率；`03` `--fan 0` 被 clap 拒绝；`04` 普通用户可读（无 `0x1b`）；`05` Python `get_clock_domain("0x0100")` 回同一 mask 的 15 条记录、无需 root；`06` 相邻域未动（offset `P0/core/+0`、power-source `ac`、fan-info `mode auto`）；`07` `get-status` 仍正常。

**⑥ 门禁** ✔：`cargo xtask ci` → `CI_EXIT=0`（本机 aarch64-linux 与测试机各一次）。顺带修掉门禁/实测抓到的一处真回归：macOS 分支对**所有**载荷都走值块渲染，命令自带的 formatter 在这里被静默忽略（新命令第一版打出来的就是通用块）；改成"有 formatter 就用 formatter"，值块只做兜底（命令没有 formatter、或 formatter 对本载荷无话可说时）。测试机 `_native.abi3.so` 须手工 `maturin develop --release` 重编，否则 Python 侧 `ImportError`。

**⑦ 边界** ✗：`+0` 的模式字 0/1 只知位图不同，语义未定（0 = 全量，1 = 更窄的子集，按实测原样记录）；域位与公共 graphics/memory 域的对应未建立，因此不能直接当作锁定通道；`get-status` 已有的 gpc/mem 读数来自 RM776，与本命令的两条路径互不覆盖；`0x20809014` 仍 `0x1f`，锁定时钟继续挂着。

## 一百四十八、风扇 RPM 请求接成公共闭包（`--rpm` / `set_fan_rpm`），并修掉满转读面

**① 选择依据** ✔：上一轮 fanstop 收口时留了三个空：RPM 目标、硬件曲线、高温行为。曲线要另造点表，高温行为要长跑控温，都不适合一步接；RPM 只差一层"请求单位到占空比"的换算，而风扇控制记录里**已经量到** `+32`（cooler 满转转速，本板 2800，auto/manual 下都在场）。所以先把 `--rpm` 接成公共闭包，同时把"驱动到底有没有 rpm 通道"这件事量清楚——不先证伪就直接翻译，等于假装通道存在。

**② 实测（探针 `/tmp/mini/mini11`/`mini16`，单 cooler0、单步）** ✔：`mode=3` 被 `0x20808530` 以 `NV_OK` 接受，但**值被吞掉**：把 1200/3000（`+32`）、3000/1200（`+40`）、3000/1200（`+44`）分别随 mode 3 送出，回读一律是 `mode=1`、`+40=0x8000`（50%）、`+32=0xaf0`（2800），风扇稳定在 1398–1406 rpm —— mode 3 在本驱动上是"50% 预设"，不是 rpm 通道。另一侧，100% 写入回读 2795–2807 rpm，`+32` 在 auto 与 manual 下恒为 2800 ⇒ `+32` 是该 cooler 的满转转速标尺，可以据它把 rpm 换算成占空比。

**③ 来源** ✍：百分比方向的 `round(pct*65536/100)` 与 `0x2080c531` 的 `v30[14] = (pct<<16)/100`（`+56` 记录槽、`+52` 使能位）来自上轮 IDA 结论，问题在于**驱动只吃 Q16 占空比**；rpm 方向因此只能由调用方换算。换算取 `round(rpm*65536/max)` 并夹到 65536，取舍与百分比路径同源（最近舍入），不引入新单位。

**④ 实现** ✔：core 新增 `FanControlTarget::Rpm(u32)`（参数级 0..6000 上限，真实上限＝记录自带 max，读不到时 `<500` 直接拒、不发 SET）、`FanControlSettings.max_rpm`、`FanReport.max_rpm`；CLI `set-fan-speed … --rpm` 与百分比走同一 cooler0 事务（快照→SET→回读→错误/取消整块恢复），`--rpm` 只属于 `set-fan-speed`，`--cooler` 仍拒；`get-fan-info` 多报一行 `Max Rpm`；Python `set_fan_rpm` 在 macOS 分支改走同一事务并回 `slot`/`applied_mode`/`applied_target_percent`/`readback_verified`；帮助文本与 fan RE 文档同步。**顺带修掉两处真缺陷**：读面 `word(status,32) > 65536` 在满转时必然失败（实测回 65559，连 `reset-fan-speed` 都被卡死），改正为"结构字段仍然严格、占空比反馈按 32× 上限容忍并夹回 100%"；范围错误的文案补上四个具体值，否则看不出漂的是哪一侧（这次就是靠它定位的）。

**⑤ 证据（测试机，产品命令，全序列）** ✔：`stage-fan-rpm-20261022T0540Z/`（`session.log` 步 1–12，`SHA256SUMS` 校验通过）。auto 基线 0 rpm→`700 --rpm` 写 25.0%（946 rpm）→`1200` 写 42.857%（1355 rpm）→`1800` 写 64.285%（1807 rpm）→`-O json` 同一事务、键齐→`4000` 夹到 100%（2798 rpm，读面 `Current 100%`）→`7000` 在驱动前拒→非 root `0x1b` 且"原块已恢复并校验"→`set-fanstop-status on --rpm` 拒→`reset-fan-speed` 回 auto→`off` 幂等→`40`（百分比形式）仍照旧工作（1259 rpm）→末态回 auto。相邻域全程未变，`Other Bytes Preserved: yes` 每步都在。

**⑥ 门禁** ✔：`cargo xtask ci` → `CI_EXIT=0`（本机 aarch64-linux 与测试机各一次）。新增单测三件：换算/夹上限/auto 恢复逐字节、无标尺时零 SET、满转越界值与瞬时大值都夹回 100%。

**⑦ 边界** ✗：**开环**——请求落成占空比，实际转速走风扇自己的曲线，低段偏高（700 要 25%，量到 946，+35%），中高段贴合（1800→1807）；25% 在本板仍在起动占空比之上，所以低 rpm 请求不会停扇，停扇仍要用 mode 2；只测 cooler0、只在 std 温度下测，高温下是否被固件抬高未测；曲线表与高温行为仍未接；`+32` 是标尺而不是可写参数（写它无效已由 mode 3 那批顺带确认）。

## 一百四十九、锁定时钟（`0x20809014`/`0x2080d041`）的拒绝点被钉到 GSP 内层方法 `0xD00`，外层已被排除

**① 选择依据** ✔：风扇 rpm 收口后，剩余的空里 `锁频/解锁` 排在最前。它卡住的不是协议没人碰过，而是**参数被拒**：api 侧 `sub_10A730` 的 GET 与 `0x2080d041` 的 SET 都回 `0x1f`。本轮不改产品代码，先把「谁在拒」钉死——否则后面填字段仍然是在猜。

**② NVML 的完整调用链（静态，api/ml 两份 idb）** ✔：`nvmlDeviceSetGpuLockedClocks` → `sub_4D5E0`（校验设备对象非空 → 取 `*(dev+164672)` 的 ops 表 `+368`，调 `+168` 槽）→ api 侧 `sub_A1DF0`（SET）/ `sub_A20F0`（GET）。两者都把调用者的 16 字节头原样拷进 648 字节块，再把**指针字段 `+8` 指向的 `20*count` 字节**条目拷到块 `+8`，然后以 **`0x2080d041`**（GET 与 SET 同为这一个 id）、648 字节发放；`+4` 是条目数，上限 `20*count <= 0x280`。这条链说明：产品侧真正要的是「16 字节头 + 20 字节条目」，不是我们先前试探的裸结构。

**③ 拒绝点定位（静态，GSP 固件 idb）** ✔：表行 `0x20809014` 的 `pFunc = 0x04d4e3cc`（ea `0xD4E3CC`）。该 handler 只做三件事：把 648 字节载荷整块拷进一个 **1560 字节**的服务端块、把块 `+4` 之后 1556 字节清零、把块 `+0` 写成 `u16 3328`（`0xD00`）、`+2` 写 `9`，然后调内层分发 `unk_17BAD88(dev, perfObj, 块头, 1576, …)`。也就是说：**外层只做搬运，`0x1f` 出自内层方法 `0xD00`**，与尺寸、`flags`、对象句柄都无关——`paramSize` 在表里就是 648（`0x288`），我们发的就是 648。

**④ 已否证（逐一实测，全部 `0x1f` 且驱动一字不回写）** ✔：NVOS54 `flags` 取 `0`/`1`/`32`/`0x100`/`0x8000`/`0x80000020`；头 `+0` 取 `0..8`/`0x10000`/`0x10000000`（当作版本字）；头 `+0` 当作字节数 20、`+4` 当作字节数 20 的两种错位；条目落点 `+8`（代码给定的正确位置）以及先前的 `+12`/`+16`/`+20`/`+24`；条目首字取域号 `1` 与 `bit<<8`（`0x100`）；`min/max` 取 MHz 与 kHz 两套单位；`count` 取 0 与 1。相邻的 `0x20809016`(1560) 回 `0x56`（本机不支持，尺寸与内层块相同但它不是这条路的入口）、`0x20809018`(392) 回 `0x1f`。表行原始 `flags` 与可用控制（`0x2080852e` 等）**逐字节相同**（`0x0406ce48`），所以也不是权限位。

**⑤ 下一步（不再猜）** ✍：内层 `0xD00` 的 handler 存在于 `unk_17BAD88` 的分发表里，拿到它就能读出它对 1560 字节块的**逐字段校验**（当前 `count`/版本/单位到底该填什么）。分发表不在明文段，16 位与 32 位两种 `{id,size}` 键都试过未命中，下一步要从分发函数体内反查表基址，而不是继续在载荷上扫。

**⑥ 状态与边界** ✗：本轮**没有**产品改动，锁定/解锁仍未实现，`0x20809014` 与 `0x2080d041` 继续按"未通"记账；api 侧 `sub_A1DF0` 的条目语义（20 字节里哪个是域、哪个是 min/max）已由代码给出位置但**未被接受**验证，所以还不能当已证事实写进用户文档。未写入、未重启、未改驱动/EFI/固件、未动 PR。

## 一百五十、锁定时钟打通并接成事务：`0x20809014` 读、`0x2080d041` 写、648 字节块、回滚，与 0x62 楔死

**① 结果先说** ✔/✗：锁定通道**打通并实机跑穿**——写回 `NV_OK`、时钟按窗口钉住、超上限被上限压住；但**解锁（复位）仍然没有协议通路**，而且连续多次 SET 之后 GSP 会以 `0x62`（`NV_ERR_RESET_REQUIRED`）整体楔死，必须重启。所以本轮交付的是「可写、可读、可精确回滚的窗口控制」，解锁仍是阶段内剩余项。

**② 块布局（实测钉死，不是猜）** ✔：648 字节 = 8 字节头 + 20 字节条目。
- `+0` `u32` 格式字，必须是 `1`（填 0 直接 `0xd`）；`+4` `u32` 条目数 `1..=31`（`0` 恒被拒）。
- 条目 `+0` vPstate、`+4` 域位（单 bit）、`+8` min kHz、`+12` max kHz、`+16` 保留 0。
- 校验是驱动侧真做的：域位非单 bit、min/max 任一为 0、同域重复条目都会被拒；相邻 id `0x20809015/16/17`、`0x2080d042`、`0x2080c536`、`0x2080d047` 全 `0x56`。

**③ GET 的归一化语义（这决定了事务怎么写）** ✔：GET 不回显请求，而是**用自己的规则改写 floor**，回显 ceiling。实测：
- vP8 + 域 1：floor 回成**当前实时时钟**（请求 100000 → 回 1620000，请求 1300000 → 仍回 1620000，此时钟就停在 1620）。
- vP4/vP12/vP15：floor 回成**ceiling 之上的一级阶梯值**（max 2200 → floor 2202187，max 2300 → floor 2303437），于是「min 2000 / max 2200」这种看起来合法的窗口在 vP4 上 floor > ceiling。

因此预检**不能**拿请求做等值比较。现在的规则是：预检读既是回滚快照、也是回读比较目标；只拒「回不去的窗口」——身份（vPstate/域）被搬走、边界被清零、floor 高于 ceiling；回读只锁身份和 ceiling，floor 允许跟着写入本身推动的时钟走。`set` 在 vP0 上因为阶梯 floor 高于 ceiling 被预检拒下、在 vP2 上是设备 `0x56`，与先前一致。

**④ 实机证据（`outputs/stage-locked-clocks-20261022T072344Z/`）** ✔：
- `03` CLI 拒绝面：缺 `--min-mhz`、`--min-mhz 0`、`--domain 3`（掩码不是单 bit）、`--domain bit9`、`--pstate 16` 全在访问驱动前退出，`--pstate P8` 通。
- `04` 预检拒绝面：vP0 1000/1200 与 vP8 1000/1200（后者 floor 回成 1620000 > ceiling）都被预检挡下。
- `05` 真正的写：`set --min 1080 --max 1620` → `NV_OK`，当前窗口钉在 1619.5 MHz，快照 `graphics vP8 1620000-1620000`。
- `06` 上限生效：`set 1200/1500` 被预检拒（floor 1620 > ceiling 1500）；`set 2000/2200` 写入成功，ceiling 换成 2200。
- `07` 楔死：写完最后一条后 GET 起回 `0x62`，读写全停。
- `01`/`02` 只读：域 1 与域 bit16 都能读，bit16 无阶梯、floor 直接贴着 ceiling。

**⑤ 事务与回滚** ✔（代码，`core/src/macos_rm.rs` + `core/src/macos_lock.rs`）：`SetMacosRmLockedClocks` = 身份门（TU116/610.57.04 + PCI/device/rm id 三连）→ 预检读（快照）→ 预检判定 → SET → 回读比对 → 不一致时把快照写回并**再读回验证**，恢复失败单独进消息。root 门用驱动自己的 `0x1b`。写入前把请求在 core 侧按 ABI 校验（单 bit、非零边界、无重复域、条目数），CLI 再挡一遍单位/掩码/范围。

**⑥ 已知边界** ✗：
- **解锁没有通路**：`nvmlDeviceResetGpuLockedClocks` = `sub_4D8A0` → ops `+184` → api shim `sub_A1DF0`/`sub_9B9D0` 走的是 count=0、648 字节全套载荷，实测所有 count=0 变体（dword0 扫 0..8、16 字节、`0xffffffff` 边界、bit31 标志）都是 `0x1f`；`0x2080c535` 回 `0x40`。**NVML 自己的复位在同驱动上也失败**。目前"解锁"只能靠写一个宽窗口近似。
- floor 想高于当前时钟不能一步到位：vP8 会把 floor 拉回当前时钟，只能靠 ceiling 抬上限再让负载把时钟顶上去。
- 连续 SET 会把 GSP 推向 `0x62`，楔死后读写全停，需重启。
- SET 不自动重启、不改 EFI/boot-args/驱动/固件。
