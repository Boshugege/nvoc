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
