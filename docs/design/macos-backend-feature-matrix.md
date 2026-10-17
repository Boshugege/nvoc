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
| `get-public-power-limit` | Read the power-limit range: --nvml (auto default) reports the NVML power-management limits in watts (min/current/max); --nvapi reports the ClientPo… | **部分已实现且实测**：入口已允许，并由同一个 core 投影作答（CLI 与 pynvoc 同一函数）。后端**真有**契约里的两个变量：enforced wall（快照 `power_limit_mw`，语义同 NVML 的 power-management limit，健康会话实测 **100.000 W**）与 requested limit（`power_limit_requested_mw`，同一次实测 **100.000 W**）；另有驱动自己的 `power_policy_tgp_mw`，**按字段名原样报告**（实测 11.277–11.309 W，与瞬时功耗 11.254–11.359 W 同尺度 ⇒ 它**不是** 100 W 的 TGP 上界，因此不作 bound 使用；该字段究竟代表什么，**尚未定位**）。契约的 min/max **两端无来源**（该 section 只发布瓦数、没有约束对），返回 `null` ＋ 理由，并附**已实测的下一候选负结果**：`0x2080200B PERF_GET_LEVEL_INFO_V2` 在本驱动答 `0x1f`。**未读到 ≠ 0 W**：section 不可用时三个瓦数全为 `null` ＋各自理由（wedge 中实测如此），而不是"驱动把墙设成 0 W" |
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
| `get-legacy-temp-sensor` | Read NVAPI legacy 3-sensor thermal view (GPU/Memory/Board, live + physical range) | **部分已实现且实测（范围面：已实现待实测）**：入口由 core 投影作答（数组根与每传感器键沿用入口自身 `target`/`current_c`/`min_c`/`max_c`/`controller`，另加 `field`、`current_source`（每个数值注明来源：`rusd:<字段>` 或 `0x20800513:sensor<n>`）与 `unavailable`（每个 null 的理由））。**读数来源**：实时值来自 RUSD 温度段；**每传感器量程来自公共热读 `NV2080_CTRL_CMD_THERMAL_SYSTEM_EXECUTE_V2 (0x20800513)`**，本轮已按 ctrl2080thermal.h 的真实结构**接线**（同一 `nvx_rm_control` 通道，只发 GET 指令，`EXECUTE_FLAGS_IGNORE_FAIL`，逐指令回报 `executed`/`result`）。传感器**按驱动自己的 target type 位掩码配对**（ctrl2080thermal.h：1=GPU 2=MEMORY 4=POWER_SUPPLY 8=BOARD，设备实测 target 0 type 1）**而非按位置**。**三态必须分开**：未发起 / 发起但未完成（驱动拒绝，含 0x62）/ 已作答——本轮实测楔子期间为**后者中的"未完成"**：`MacosRmSession` 打开即在 RUSD 映射处被拒（`RUSD allocate/map: 0x00000062 NV_ERR_RESET_REQUIRED`），故量程留 `null` 且理由写明"did not complete"（这条同时说明：楔子期间**连会话建立都失败**，偏移通道的读同样卡在这一层）。**指令未跑不得当作 0**（`range_known`/`executed` 标志决定，实测单测覆盖）；**驱动未命名 target type 的传感器不填任何入口**；本快照该字段无量值时用公共读的读数并标明来源，绝不填 0 °C。失败方向实测：入口 rc=0、四路 `null`＋各字段理由 |
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
| `get-fan-info` | Read fan/cooler info (NVML: count + min/max percent; NVAPI: per-cooler info via private FanCoolerGetInfo) | **部分已实现且实测**：入口已允许并实机跑通（`get-fan-info --macos-rm` rc=0），读数取**同一个 RUSD 快照**（逐槽转速 ＋ **驱动自己的 cooler slot**），契约里本后端没有来源的三个百分比字段返回 `null` 并附理由（RUSD 风扇段只发布转速，既无占空比也无转速区间，百分比需要自造标度）；不可用时**区分"未读到"与"没有风扇"**——设备实测（2026-10-18，wedge 中）返回 `count:0` ＋ "the fan section of this snapshot was not usable (RUSD allocation/mapping failed: 0x00000062) … 空列表意味着 not read，而不是 no fan"；健康会话下的转速读数来自阶段 2 全量遥测（Metal 负载 4243 rpm、`metal-recheck` 2517 rpm） |
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

**三、避免张冠李戴**：NVAPI 副本引用的 `0x2080D024`（paramSize **66060**）是 **`0x20809024`** 的孪生，**不是**我们控制块的 SET ✔；`0x2080f031`/`0x20801acf`/`0x2080206d` **均不在方法表** ⇒ 调用它们的路径依赖**派生** ✔。

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
(3) 热族状态**按 id 分列** ✔：`0x20800513`（ThermalSystemExecuteV2）＝已实现待实测；`0x20808513`@1432（GSP-only、从未发）＝未实现待实测；`0x20808540` HWFS＝**已实现待实测（本机从未发）**，副本另有本代实测值 ✔；**热限/热策略 opcode**＝调查未知（驱动自命名的表里仅一条热命令 ✔，属**有名有姓的层次**：客户端/驱动实现缺口 ✔）。
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
