# macOS RM 最低风险写入验证方案（供授权前审阅）

本文件把**唯一已知可达的公开写对**的实机验证步骤、停止/恢复方案与失败判据固定下来，
供用户在授权前审阅。**在明确授权前不要执行其中任何写入命令**；工具本身仍保持门控
（idle-only 构建与缺 `--write` 都在打开驱动前拒绝）。

## 0. 依据与范围

- 公开 610.57.04 RM 控制面的完整普查（`docs/design/macos-backend-feature-matrix.md` §四）
  只找到**一对**可达的公开读写：`PERF_RATED_TDP_GET_CONTROL` 0x2080206e（读，实测 `NV_OK`）
  与 `PERF_RATED_TDP_SET_CONTROL` 0x2080206f（写）。
- 内部 PM/PFM 族（`ctrl2080internal.h`）与其写候选（`UPDATE_TGPU_LIMIT`/`SET_VPSTATE`/
  `CONFIGURE_TGP_MODE`/`CONFIGURE_TURBO_V2` 等）实测整块 `0x56 NV_ERR_NOT_SUPPORTED`；
  `GET_EDPP_LIMIT_INFO`(0x20800AFD) 亦 `0x56`。**没有**时钟/电压偏移、VF、风扇或温度墙的公开写命令。
- 因此本方案只覆盖：**RATED_TDP 仲裁 SET（0x2080206f）**。它只切换"是否允许越过 RATED_TDP 点"
  的策略，**无偏移、无电压、可逆**（回 `DEFAULT(0)` 即恢复），文档标注其预期使用方为 EVGA Precision
  一类终端工具。它**不是**超频偏移，`NV_OK` 也不等于稳定。

另一条独立通道（**不在本方案的授权范围内**，见 §7）是私有 ClockClient 的时钟域偏移写：
`0x2080d01c @2108`（`0x2080901b @2108` 的 SET 孪生，GSP 侧实现）。它的门槛比 RATED_TDP 高得多，
且当前**尚未满足**（拿不到当前偏移与写步长）。**不得**用「渲染成功/未崩溃/实时频率变了」当作
偏移写成功的证据；唯一可接受的证据是**同一偏移字段的 GET 回读**。

## 1. 前置条件（缺一不可）

1. 已核对身份/版本：目标卡（GTX 1650 SUPER / `10de:2187` / PCI `0000:01:00.0`）、NullMoth 2.0.4、
   RM 610.57.04；桌面可见、核显显示未受影响。
2. 使用 **full profile** 构建（idle-only 构建直接拒绝写）：`PROFILE=full sh build.sh`。
3. 明确授权与降级路径：出现异常时**不重启**，用工具自身的恢复路径即可（见 §3）。
4. 进程级：不改全局驱动配置、EFI、boot-args。
5. 已知越点限制的**实际效果不可观测**：本机没有可读的 VF/电压面，无法证明"越点后频率/功耗真的变化"；
   本验证只证明**写面可达 + 回读一致 + 可恢复**。

## 2. 验证序列（工具已实现，逐条打印 `V3.4a/b/c`）

```sh
PROFILE=full sh build.sh
./probe-macos-rm --write --rated-tdp-set GLOBAL:EXCEED
```

工具按固定顺序执行（任何一步失败即中止且不写）：

| 步 | 动作 | 失败判据 |
| --- | --- | --- |
| V3.4a | **pre-record**：`GET_CONTROL(client)` 读原 `input`/`vPstateType`；读不到就**拒绝写** | GET 非 `NV_OK` ⇒ 拒绝 SET，退出 |
| — | **arm restore**：把原值存入进程级恢复槽，`atexit`/`SIGINT`/`SIGTERM` 均走同一恢复 | — |
| V3.4b | **SET**：`SET_CONTROL{client, input=EXCEED, vPstateType=原值}` | 非 `NV_OK` ⇒ 记录状态码并恢复 |
| V3.4c | **readback**：`GET_CONTROL(client)` 回读，要求 `input == EXCEED` | 回读不一致 ⇒ 记录并恢复 |
| — | **restore**：立即把 `input` 写回原值并 disarm | 恢复失败 ⇒ 见 §4 |

参数：client 取 `GLOBAL(2)`（`RM(0)` 按头文件不可 SET，工具直接拒绝；OS/PROFILE/WAR 同理可选）。
"EXCEED" 对应 `NV2080_CTRL_PERF_RATED_TDP_ACTION_FORCE_EXCEED`。

## 3. 停止与恢复

- **正常退出**：`V3.4c` 后立即 restore。
- **信号**：`SIGINT`/`SIGTERM` 触发已安装的处理器 → restore 后退出；`atexit` 兜底。
- **崩溃/SIGKILL**：进程内恢复不会执行（文档不声称 Rust 恢复在 SIGKILL 下有效）；此时该 client 的
  `input` 可能停在 `EXCEED`。恢复方法：重启后由系统/驱动把 RM 客户端状态复位，或用只读 `GET_CONTROL`
  确认后再以工具恢复原值（若原值为 `DEFAULT(0)`，一次 `--rated-tdp-set GLOBAL:DEFAULT` 即可）。
- 全程无需重启，不改 EFI/boot-args/全局配置。

## 4. 中止与恢复失败判据

出现下列任一情况即判定验证失败并停止后续实验：
- `V3.4a` GET 失败（不允许在无法保证恢复时写入）；
- `V3.4b` SET 返回非 `NV_OK`；
- `V3.4c` 回读不等 `EXCEED`（写面不接受该值）；
- **restore 后回读不等于原值**（恢复失败，最高优先级；此时立即只读记录 5 个 client 的全部状态并人工处理）。

## 5. 记录要求

- 落盘完整 `V3.4a/b/c` 输出、返回码、SET 前后 5 个 client 的 `GET_CONTROL` 结果；
- 同步记录同期只读遥测（RUSD：温度/功耗/频率）以证明写入期间无异常副作用；
- 输出/review 只落到测试机 `outputs/`（仓库外），仓库内不写私网地址或原始日志。

## 6. 本方案不能证明什么

- **不证明**超频稳定性（无偏移、无负载搜索）；
- **不证明**越点对性能/功耗的实际影响（无可读 VF/电压面）；
- 不覆盖内部 PM/PFM 写族（实测不可达）。

## 7. 时钟域偏移通道（`0x2080d01c`）——尚未授权，门槛未满足

### 7.1 为什么它是候选

- 该 id 与 `0x2080901b @2108` **paramSize 相同**，且在本机 GSP 自带导出表里成对出现（GET 低字节 +1、
  类别字节 `0x90`→`0xd0`）；同族有 13 对这样的配对。这是表事实，不是命名猜测
  （`docs/reverse-engineering/nvapi/gsp-method-table-and-legacy-forwarding.md` §5.2）。
- 同族的读面已经给出**逐域偏移范围**：`0x20809025 @7188` = 3 条记录、类型 3/4/5、
  `rec+8 = 1638400 kHz`（最高 VF 频率）、`rec+20/24 = -18750/+18750 kHz`（±18.75 MHz）。
  该三值与本仓 `nvapi-rs` 记录的 TU116（另一条 Windows 路径、独立逆向）**逐字吻合**。
- `0x20809019/0x20809025/0x20809026` **没有 SET 孪生** ⇒ 它们是只读视图，写面只能是 `0x2080d01c`
  （或 VF 族的 `0x2080d024`，按用户优先级后置）。

### 7.2 门槛（缺一不可，当前全部未满足）

| 门槛 | 现状 |
| --- | --- |
| **G0 该 id 对本客户端可达（特权门槛）** | **未满足，且不是软件问题**。写半区的 id 都带 `0xC000`（读 id `\|0x4000`），`RmGssLegacyRpcCmd` 在转发前要求 `privLevel >= RS_PRIV_LEVEL_USER_ROOT`，否则回 `0x1b`。本会话是非 root 客户端 ⇒ **GET 读面可达、SET 不可达**；提权不在授权边界内（`gsp-method-table-and-legacy-forwarding.md` §2.1/§9）。**§11.4 更新（源码级，未运行）**：macOS 端 `kexts/NVRM/os-xnu.cpp:174` 把 `os_is_administrator()` 定义为 `(kauth_getuid() == 0)`，`entry_points.c:185`/`escape.c:381` 再把它映射成 `RS_PRIV_LEVEL_USER_ROOT`，而 `nvstatuscodes.h:56` 确认 `0x1b` 就是这道门的返回码 ⇒ **uid 0 的客户端应当能过 host 门**；若 root 仍回 `0x1b`，那才是 **GSP 侧第二道权限**（`ctrl2080gpu.h:1222/2427/2556` 也把 `0x1b` 当控制返回值），两者必须分开记录 |
| G1 能读到**当前偏移**（不是实时频率） | **未满足**。`0x2080901b@2108` 的写图已实测（`A5` 值图：head 14 次写、每条记录 1 个 u16 类型戳 + 12 个 dword），但 stock 下**所有值槽都被写成 0**，所以无法区分「当前偏移恰好是 0」与「该 GET 不填值槽」⇒ 字段归属仍未确定；`0x20809026@5332` 是已知 wedge id，被代码拒绝。**§11.3 更新**：候选值槽有两个而不是一个 —— `rec+8`（上一层 article 说 slot 0 = signed freq offset）与 `rec+16`（同层 Blackwell 字段表说 `VALUES[2]`，经实测 `0x24` 位移折算）；stock 全 0 无法区分，只能靠逐位 counter 图 + 单步 SET 同字段回读定案。**§14 更新（候选顺序收紧，仍非证明）**：两条**互相独立**的论证都落在 `rec+8` ——（a）本仓 CLI 的代别表写明 `--freq/--volt` 在 **10~40 系**解析到 **slot 0/1**、在 50 系才解析到 slot 2/3，而 `rec+8` 正是该记录**值数组的第一个 dword**（A5 证据：值 dword 在 `+8..+44` 连续 10 个、`+52/+56` 另两个、`+4/+48/+60` 从不写）；（b）`0x24` 位移（在 `0x20809025` 家族上实测）把上一层 V1 记录的 `slot 0 = +44` 折到 RM `rec+8`。`rec+16` 来自 `VALUES[2]`，那是 **Blackwell 代**（50 系）的频率平面槽位，套到 Turing 属代别错配 ⇒ 降为次选。另：`0x2080901b` 的值 dword 里**没有**范围字段（范围在 `0x20809025` 的 ±18750），12 个值 dword 都是「当前控制状态」，stock 全 0 与「未施加偏移」自洽 |
| G2 能读到**范围** | 已满足：`0x20809025@7188` 的 `±18750 kHz`（单域） |
| G3 能记录**原状态**并把同一字段写回 | 未满足（依赖 G1 的字段位置） |
| G4 写步长有依据（最小步长） | **未满足**：`18750` 是范围端点，不是步长；步长可能来自同一容器的 steps 字段或驱动文档，**不允许猜**。**§14 更新（提案，需批准，未执行）**：本机现有证据给出的单位是 **kHz 整数**（`0x20809025` 的 ±18750 kHz；`nvapi-rs` 的 R610.74 活体往返记 `90000 kHz → 恰好 +90 MHz`），用户面单位是 MHz（一位小数 = 100 kHz）。因此首个最小步长提案取 **+1000 kHz（1 MHz，即 kHz 字段上可表达、又比单域范围小 18.75 倍）**；并对「驱动量化」设一条判据：若 SET 回 `NV_OK` 但同字段回读**等于原值**，判为「被量化/被钳位或字段不对」，**不得**当作成功、不得加大步长继续试，直接 restore 并按候选顺序考虑下一个字段。任何步长在获批前都只是提案 |
| G5 GSP 私有路径可用 | **未满足**：当前整条 legacy 转发处 `0x62 NV_ERR_RESET_REQUIRED`（需用户重启；reload 需 root） |
| G6 SET 侧的前置门 | **已枚举，运行时取值未验证**（`0x2080d01c` 的 pFunc `0x4d5c3c0` 反汇编，`docs/reverse-engineering/nvapi/gsp-method-table-and-legacy-forwarding.md` §4.4）：G1 `*(s1+0x2050)!=NULL`、G2 其 `+0x4f4` 字节非 0、G4 `*(*(s1+0x1f88)+0x1150)!=NULL`、G5 该对象 `+0x20` 访问器非空 —— 四处回 `0x56`；G3 `x+0x179 != 0` 回 **`0x6c NV_ERR_GPU_IN_DEBUG_MODE`**。GET thunk 没有这些门。这些字节在 GSP 对象里、本工具读不到 ⇒「GET 成功」**不能**推出「SET 可用」；第一次写入必须把 `0x56`/`0x6c` 列为预期失败层次，并**逐码**记录（`0x6c` 归因到 G3、`0x56` 归因到 G1/G2/G4/G5，`0x1b` 归因到 G0 特权层） |

### 7.3 满足门槛后的单域最小验证（设计，未执行）

1. 只读基线：`0x20809025@7188`（范围）+ 当前偏移读（G1 的容器）+ RUSD 遥测快照；记录**原值**。
   同时按 G6 列出「SET 可能被 thunk 前置门拒绝」的假设与预期返回码——它不改变别的判据，只是让第一次
   非 `NV_OK` 有归属。
2. 单域：只对一个域（例如显存域或域 0），并且只改**一个最小步长**；不整组、不 Boost、不搜索。
3. （仅在 G0 被满足、即调用者处于特权级时）pre-record → arm restore → SET `0x2080d01c{domain, offset=原值±1 step}` → **同一字段 GET 回读**。
   必须是新值（实时频率变化**不算**证据）。
4. 立即 restore 原值 → 再 GET 回读必须等于原值；记录两步的回读字节。
5. 中止判据：SET 非 `NV_OK`；回读不等于预期；restore 后回读不等于原值（最高优先级，立即停止并只读记录）；
   出现 `0x65`/`0x62` 立即停止（会整场打断 GSP 私有路径）。
   **SET 的唯一 mask 纪律（§11.1）**：`0x2080901b` 是**按 mask 播种**的发现接口，所以宽 seed 只属于 GET；对 `0x2080d01c` 只允许提交「GET 已证明被填过的那一条域记录」的掩码，**绝不宽 seed** —— 否则等于要求驱动套用 32 条记录、其中大多数驱动从未填过。另：V1 只 marshal 类型 `{1,3..9}`（类型 `2` 被静默丢弃），目标记录的类型必须在 SET 前先读到（`nvapi-rs/src/clock.rs:384-410`）。
6. 全程进程级，不改 EFI/boot-args/驱动配置；不自动重启。
7. **「成功」的定义（本方案口径，防止把弱证据当突破）**：必须**同时**满足 ① `0x2080d01c` 回 `NV_OK`；
   ② **同一字段**回读 == 原值 ± 步长（逐字节打印该 dword）；③ 该记录内**其它**值 dword 与快照逐字节相同
   （若不同，说明字段归属判断错了 → 按 §4 中止并 restore）；④ restore 后整块 2108 B 与写前快照**逐字节相同**；
   ⑤ RUSD gpc/mem kHz **不作为**判据。缺任何一条即失败，按 §4 恢复并如实记录失败层次。
8. **字段候选顺序（§14）**：`rec+8`（首选，两条独立论证）→ 若回读不符且无副作用，再考虑 `rec+16`；
   每次只动一个字段、一个域、一个步长，且每次都从新的快照出发。**绝不同时试多个字段**。

### 7.4 与 RATED_TDP 的关键差别

RATED_TDP 只是「是否允许越过额定点」的策略开关；偏移写**直接改时钟**。因此 §2 的 RATED_TDP 序列
（即使被授权并通过）**不能**作为偏移写的先例，反之亦然。两者都要各自的 pre-record/回读/恢复证据。

### 7.5 解锁条件与轮询判据（2026-10-17T12:15Z 实测：仍未解锁）

G5 的判据不是 `uptime`，而是一次**已知安全**的只读锚定：

```sh
cd probe-macos-rm && ./probe-macos-rm-idle --anchor-read
```

判读（`0x2080b201` 同时给出 payload ⇒ 它只可能由 GSP 应答）：

| `0x2080b201` 小尺寸档（88/164/620/2440 B） | 含义 |
| --- | --- |
| `0x1f NV_ERR_INVALID_ARGUMENT` | GSP 正常（尺寸门在转发之后、不随 fault state 移动）⇒ G5 满足，可跑只读电池 |
| `0x62 NV_ERR_RESET_REQUIRED` | GSP 仍处于 reset-required 锁存态 ⇒ **G5 未满足**，任何时钟域只读/写入都不可推进 |

**2026-10-17T12:15:17Z 实测**：`verdict 0x62`；`0x2080b201@2444 = NV_OK`（16 个非零 dword）；
`0x2080b202 = 0x62`；`0x2080a0ff = 0x62`；`0x2080ffff = 0x1b`（host 特权门照旧）；`0x20800102 = 0x1f`；
`0x2080122a@20 = 0x1f`（host 尺寸门照旧）。同一次里 stage-2 的 `fb-*`/`perf-*`/`rusd` 也全 `0x62`
（`NV00DE alloc rejected`），而 `bios-sku-info`/`bus-get-pci-info` 仍 `NV_OK` ⇒ **wedge 仍然是「GSP 派发面整体不可用、
host 应答面正常」**，与 §6/§12 的描述一致。轮询继续按此判据，不自行重启。

### 7.6 底层驱动自带的 PERF_BOOST（与 agent 写入分开记账）

`/Library/GPUBundles/nvmtl/libvulkan_nouveau.dylib` 里只有两处 `0x2080200a`，均为 NVK 自发：

| 时机 | 函数 | flags | duration |
| --- | --- | --- | --- |
| 设备创建 | `_nvkmd_nvrm_enum_pdev`（`0x12d24b`） | `0x12` = `BOOST_TO_MAX｜CUDA_YES` | `0xffffffff` = INFINITE |
| 设备销毁 | `_nvkmd_nvrm_pdev_destroy`（`0x12d4f2`） | `0` = `CMD_CLEAR` | `0` |

两处同一个门 `nvrm610_on("BOOST")`（env `NVRM_610_BOOST` → `/Library/GPUBundles/nvmtl/nvrm610.conf`
的 `BOOST=` 行 → 默认 **ON**；本机实测为 ON）。`outputs/` 里有 50 条
`nvrm: b81 PERF_BOOST flags 0x12 INFINITE rc 0x0`，即 boost 确已成功下发。
host 侧 `kern_perf_boost.c` **没有** clear/teardown 分支 ⇒ 释放完全依赖 §3.2 那条客户端 clear。
详见 `docs/reverse-engineering/nvapi/nvk-perf-boost-internal.md`。

对 §7.2/§7.3 的影响（已并入门槛）：

- 运行期频率升高**可能来自 NVK 的 BOOST_TO_MAX，而不是本次 offset 写入**；因此 §7.3-⑤ 的
  「RUSD 实时 kHz 不作判据」是硬要求，不能放宽。
- 只要实验窗口内创建过 NVK 设备，就必须在记录里写明该 boost 存在，禁止用
  「底层没有性能请求」来解释频率变化。
