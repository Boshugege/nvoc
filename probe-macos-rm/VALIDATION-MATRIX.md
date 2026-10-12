# macOS RM 控制面验证矩阵（probe-macos-rm v1）

逐项验证矩阵：每项给出 **层级**（常量出处等级）、**命令/结构依据**、**期望与判读**。工具实现见 `src/`；本文同时收录 v1 代码**故意不发送**的后续探针（下一梯队）。

层级标记：
- **A-tree**：常量/结构直接来自 OGKM 610.57.04 源树（编译期权威）。
- **A-proven**：A-tree 且已被 NullMoth 自家用户态（NVK shim / kms.inc）在该驱动上跑通过。
- **B-campaign**：开源树不含的 GSP-RM legacy ID，来自 NVOC 逆向战役文档（MODS / NVAPI RE，见出处）；接受度本身待实测。

判读词汇：`OK`=NV_OK 且数值合理；`REJECTED`=调用完成但 RM 返回错误状态（记录状态码——本身就是实验结果）；`HARD`=对象链断裂；`GATED`=需 `--write`。

## 阶段 0 — 传输层

| 项 | 内容 | 依据 | 期望 |
|---|---|---|---|
| V0.1 | `IOServiceOpen` NVAccel(`0x4E560000\|0`)→NVRM(0) | nvrm_xnu.c（A-proven） | 打开成功；两者都没有 ⇒ 驱动未加载 |
| V0.2 | 设备连接（type 1） | 同上 | 打开成功（RUSD 映射需要） |
| V0.3 | `NV_ESC_CARD_INFO`(200) | nv-ioctl-numbers.h（A-proven，kms.inc:21） | `valid=1` + gpu_id/BAR 地址；**失败=适配器未过 pass 2** → 提示 `nvrmctl go 2` |
| V0.4 | `NV_ESC_REGISTER_FD`(201) | 同上（kms.inc:93） | 设备连接绑定 ctl fd |
| V0.5 | `NV_ESC_CHECK_VERSION_STR`(210) | nv-ioctl.h `nv_ioctl_rm_api_version_t`，QUERY 语义（A-tree） | reply=RECOGNIZED 且回带 "610.57.04"；Linux nv.c 同款握手 |

## 阶段 1 — RM 对象链

| 项 | 内容 | 依据 | 期望 |
|---|---|---|---|
| V1.1 | alloc `NV01_ROOT_CLIENT`(0x41) | g_allclasses.h:289（A-tree）；kms.inc:94 | NV_OK + 句柄。**这是"M0 第一关"：root 分配被拒 ⇒ 整条 OC 路线降级** |
| V1.2 | `GPU_GET_ATTACHED_IDS`(0x201) | ctrl0000gpu.h（A-tree） | 返回附着的 gpuId 列表 |
| V1.3 | `GPU_ATTACH_IDS`(0x215) | 同上（kms.inc:96） | NV_OK |
| V1.4 | alloc `NV01_DEVICE_0`(0x80) | cl0080.h:36（A-proven，kms.inc:98） | NV_OK |
| V1.5 | alloc `NV20_SUBDEVICE_0`(0x2080) | cl2080.h:36（A-proven，kms.inc:100） | NV_OK |
| V1.6 | `GPU_GET_CLASSLIST_V2`(0x800292) | ctrl0080gpu.h（A-proven，push.inc:46） | 类数量>0（NVK 已证设备级控制可达） |
| V1.7 | `CLIENT_GET_ACCESS_RIGHTS`(0xD03) | ctrl0000client.h（A-tree） | 本私有客户端拿到的 RS_ACCESS_MASK 原始位图——特权面内省（os_check_access 恒真在 RM 面的实际效果） |
| V1.8 | `GPU_GET_ID_INFO_V2`(0x205) | ctrl0000gpu.h（A-tree） | gpuFlags/boardId/deviceInstance/numaId |
| V1.9a/b | 内存冒烟：NVOS32 alloc(sysmem/vidmem 64KB)→`NV_ESC_RM_MAP_MEMORY`→mmap→写读回 | nvos.h + push.inc gbuf 路径（A-proven） | BAR1/VRAM 两条映射路都通——通道冒烟与未来 pushbuffer 工作的前提 |

## 阶段 2 — 只读遥测（M0 核心）

| 项 | 命令 | 依据 | 判读要点 |
|---|---|---|---|
| V2.1 | `FB_GET_INFO_V2` 0x20801303 | ctrl2080fb.h（A-proven，NVK 用于 VRAM 尺寸） | RAM/BAR1/L2 数值应与卡相符；失败 ⇒ subdevice 级 GSP ctrl-RPC 有问题 |
| V2.2 | `FB_STATS_GET` 0x2080132F | ctrl2080fb.h（A-tree） | 接受度探针（v1 不解码内容） |
| V2.3 | `PERF_GET_CURRENT_PSTATE` 0x20802068 | ctrl2080perf.h（A-tree） | 空闲应 P8 附近，负载 P0；**性能状态域活着的直接证据** |
| V2.4 | `PERF_RATED_TDP_GET_CONTROL` 0x2080206E | ctrl2080perf.h（A-tree；12B 参数） | rated TDP 仲裁控制读面；REJECTED 也记录（客户机枚举差异） |
| V2.5 | `GET_EDPP_LIMIT_INFO` 0x20800AFD | ctrl2080internal.h:3984（A-tree） | **功耗墙 min/rated/max/curr mW** —— M3 功耗墙写入的基准；GB207 台式卡期望 max≈额定 TGP |
| V2.6 | `GET_GPUMON_UTIL_SAMPLES_V2` 0x20802096 | ctrl2080perf.h + ctrl2080gpumon.h（A-tree，V2 无内嵌指针） | 引擎利用率环形队列；tracker 推进 ⇒ GPUMON 采样管线活着 |
| V2.7 | `CLK_PMUMON_CLK_DOMAINS_GET_SAMPLES` 0x20801037 | ctrl2080clk.h（A-tree） | **gpcClkFreqKHz / dramClkFreqKHz** —— 核心/显存频率读数（M0 频率遥测主通道） |
| V2.8 | `PERF_GET_POWERSTATE` 0x2080205A | ctrl2080perf.h（A-tree） | AC/电池电源状态语义（macOS 上 ACPI 电源源被桩掉，此读数有对照意义） |
| V2.9–11 | legacy rails/VfPoints GET：0x2080B201(76B)/0x2080B202(100B)/0x208090A2(10×68B) | B-campaign（voltrails-family-full-layout-r610.md；blackwell audit T3-1/T3-10） | 只读探针。`OK` ⇒ GSP 接受 legacy ID，B 层全部升级；`NOT_SUPPORTED` ⇒ 610 固件裁剪了该族；其他状态码逐个记录 |
| V2.12 | **RUSD 页**：alloc `NV00DE`(0xDE, poll mask CLOCK\|PERF\|POWER\|THERMAL\|FAN) + `NV_ESC_RM_MAP_MEMORY` + mmap | cl00de.h（A-tree；blackwell audit T3-6） | **整页遥测：温度(5 传感器型)/瞬时+平均功耗/功耗墙 enforced/风扇转速/P-state/时钟域/节流原因**。两个可能失败点分别判定：alloc 被拒 vs 映射被拒。成功 ⇒ M0 遥测一步到位 |
| V2.12b | RUSD 全字段转储：5 温度型/P-state/时钟域(GPC/MEM/SM)/瞬时+平均功耗/enforced 功耗墙/TGP 策略/风扇 | cl00de.h（A-tree） | 数值合理性即 RUSD 管线活性证明；全部失败（INVALID 时间戳）⇒ 轮询段未被固件填充 |
| V2.13 | `BIOS_GET_SKU_INFO` 0x20800808 | ctrl2080bios.h（A-tree） | BoardID/chipSKU/project 字符串；VBIOS 域可达性 |
| V2.14 | `BUS_GET_PCI_INFO` 0x20801801 | ctrl2080bus.h（A-tree） | deviceId/subsys/rev 应与 PCI 枚举一致 |
| V2.15 | `PERF_GET_LEVEL_INFO_V2` 0x2080200B | ctrl2080perf.h（A-tree，32 域×{cur/default/min/max}） | **每级性能状态的频率域表**——M2 锁频的合法范围参照 |
| V2.16 | `I2C_READ_BUFFER` 0x20800601（零参数探针） | ctrl2080i2c.h（A-tree） | FINN I2C 族分派可达性；返回良构 NV_ERR_* ⇒ GSP I2C 在（风扇/传感器路线活），IRQL/权限类拒绝则记录 |

> 阶段 2 无温度控制命令的原因：r610 开源树**不含** legacy `THERMAL_GET_INFO` 头，其 ID 未在本战役内核级核实——温度读数首选 RUSD 页（T3-6），命令式温度读数列为后续 IDA 任务（见"后续探针"）。

## 阶段 3 — 写入测试（`--write` 门控，M1 核心）

| 项 | 命令 | 依据 | 设计 | 判读 |
|---|---|---|---|---|
| V3.1a | `PERF_BOOST` 0x2080200A `BOOST_1LEVEL\|CUDA_YES` + 采样 + `CMD_CLEAR` | ctrl2080perf.h（A-proven 族） | 单级提升先行，2 s 采样后清除 | 细粒度提升接受度 |
| V3.1b | `PERF_BOOST` 0x2080200A `BOOST_TO_MAX\|CUDA_YES` + 5 s 采样 + `CMD_CLEAR` | ctrl2080perf.h（**A-proven**，nvk-macos.patch:10701——全仓库唯一已在用户态发过的 GSP perf 写入） | 采样 pre/boost×5/clear | `OK` ⇒ 报告 §3.4"OS 层特权全开是否足以过固件检查"**判定为是**；boost 期间 pstate/频率抬升 ⇒ 性能写入面全通 |
| V3.2 | `PERF_SET_POWERSTATE` 0x2080205B 往返 | ctrl2080perf.h（A-tree） | 读当前值→回写同值→回读验证 | 零位移；接受 ⇒ perf 电源状态写面通 |
| V3.3 | `UPDATE_EDPP_LIMIT` 0x20800AD0 往返 | ctrl2080internal.h:3234（A-tree） | GET curr→`bEnable=1, clientLimit=limitCurr`→回读验证 | **功耗墙写面探针**（M3 前置）；桌面卡可能 NOT_SUPPORTED（eDPp 是移动/平台语义）——本身即结论 |

恢复：`atexit`/SIGINT/SIGTERM 时对 BOOST 发 `CMD_CLEAR`（best-effort）。V3.2/V3.3 按构造自恢复。

## 阶段 4 — 遥测采样流
1 Hz 输出 pstate / gpc / dram / GPU 温度 / 瞬时功耗 / enforced 功耗墙 / 风扇转速 / GPU busy（来源：ctrl pstate + PMUMON + RUSD）。用途：
- boost 前后对照（M1 判据：BOOST_TO_MAX 期间 idle 频率锁顶、功耗抬升）；
- 与 Linux 实机 NVML、Windows NVAPI 采样做三方对照；
- 后续 M2/M3 写入实验的安全监控源。

## 阶段 5 — 通道冒烟（`--stages …5`，GPU 执行证明）

NullMoth 自家 push_test（`kexts/NVRM/push.inc`）的移植。不含任何调频/调压写入——是**引擎提交路径**的端到端验证，也是未来在 macOS 上做压测/推流工作的前提。

| 项 | 内容 | 依据 | 判读 |
|---|---|---|---|
| V5.0 | 从 classlist 选最新 GPFIFO 通道类（0x??6f 取最大） | push.inc:48（A-proven） | Turing+ 期望 ≥0xA06F（KEPLER_CHANNEL_GPFIFO_A 族） |
| V5.1 | alloc+map `TURING_USERMODE_A`(0xC461) 门铃页 | g_allclasses.h:587 + push.inc:52-60（A-proven） | 门铃页可映射 |
| V5.2 | alloc `FERMI_VASPACE_A`(0x90F1)，RETRY_PTE_ALLOC_IN_SYS | g_allclasses.h:895 + push.inc:63（A-proven） | 私有地址空间 |
| V5.3 | 5 缓冲：notifier(4K sys)/userd(512K **vidmem**)/gpfifo(256K)/cmdbuf(64K)/sem(4K)，各走 NVOS32→MAP_MEMORY→NV50_MEMORY_VIRTUAL→MAP_MEMORY_DMA | push.inc gbuf_alloc（A-proven） | VRAM+sysmem 双路 DMA 映射 |
| V5.4 | `NV01_CONTEXT_DMA`(0x02) 指向 notifier | nvos.h NV_CONTEXT_DMA_ALLOCATION_PARAMS + push.inc:74-78 | 通知路径 |
| V5.5 | alloc 通道（NV_CHANNEL_ALLOC_PARAMS，engineType=GRAPHICS=1） | alloc/alloc_channel.h + cl2080_notification.h:284 + push.inc:80-84 | 通道对象创建 |
| V5.6 | `NVA06F_CTRL_CMD_BIND`(0xA06F0104) | ctrla06fgpfifo.h（A-tree） | 引擎绑定 |
| V5.7 | `NVA06F_CTRL_CMD_GPFIFO_SCHEDULE`(0xA06F0103) | 同上 | GPFIFO 使能 |
| V5.8 | `GET_WORK_SUBMIT_TOKEN`(0xC36F0108)（+SET_NOTIF_INDEX 0xC36F010A） | ctrlc36f.h（A-tree） | 用户态门铃 token |
| V5.9 | 推信号量释放（method 0x5c）→ USERD GPPut(0x8C)=1 → 门铃(NVC361_NOTIFY_CHANNEL_PENDING=0x90) → 等 sem==1 ≤5 s | push.inc:95-114 + clc46f.h USERD 布局（A-proven 全套） | **PASS = GPU 在 macOS 上执行了我们提交的 pushbuffer**——整条执行链（RM 对象→通道→HOST→引擎）活体证明 |

## 后续探针（v1 代码不发送，按风险递增排序）

1. **legacy 温度读数 ID 核实**：MODS 4060 IDA 库（`reverse/mods_analysis/a3_work/mods4060.i64`）找 THERMAL 族（0x2080 11xx）分发点，取 `THERMAL_GET_INFO` 精确 ID + 参数尺寸，做成 B 层探针。（RUSD 已覆盖温度，此项为冗余备份。）
2. **VF 偏移 SET**：`nvmlDeviceSetGpcClkVfOffset` → RM `0x2080D031`（580）/`0x2080F031`（610 文档，参数 0x608 B，blackwell audit T2-10）。**M2 的核心写面**。先 GET 对应面（`0x208090A2` 已在 V2.11 探路），再小步幅（±15 MHz）+ V2.7 频率回读。
3. **电压轨 SET**：`0x20801ACF`（7 轨 ×316 B，uV；MODS REPORT.md）。前置：确认 kext 侧 RM registry key 通道（`RmVFPointCheckIgnore=1` 的 macOS 等价物，Linux 是 `NVreg_RegistryDwords`）——否则越限写会被 OV/reliability 预检拦下。**风险最高档**，必须在 M0 遥测闭环建立后进行。
4. **perf-point 提交族**：MODS `SetPerfPoint`/`InjectPerfPoint` 的精确 cmd 未定（audit §GSP 分派表："待 MODS 二进制 assert 桥接确认"）。锁定后即为"行为化频率写入"的正式路径（NVAPI record 面 0x2080D01C/D02D 已证不行为化，勿用）。
5. **风扇写面**：RUSD 只读 fan rpm；写面走 GSP boardobj/cooler 族（0x2080 26xx? 未核实）或 FINN I2C 控制（0x208006xx，在树）。属 M4，报告判定中低可行性。

## 与可行性报告的映射

- M0（遥测）= 阶段 0–2 + V2.12/12b RUSD 全字段
- M1（boost 复现）= V3.1a/1b
- M2 前置探路 = V2.11 + V2.15（合法频率范围）+ 后续探针 2
- M3 前置探路 = V2.5 + V3.3 + 后续探针 3
- 引擎提交路径 = V1.9 + 阶段 5（macOS 压测/推流的前提，对应报告 §8 stressor 落点）
- 报告 §3.4 特权面问题 = V1.1–V1.8 + V3.1 直接回答
