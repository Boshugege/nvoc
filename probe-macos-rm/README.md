# probe-macos-rm — NullMoth NVRM kext RM 控制面验证套件

针对 [nullmoth/nvidia-macos-driver](https://github.com/nullmoth/nvidia-macos-driver)（本地 `reverse/nvidia-macos-driver-main`）的用户态 RM 控制面**分阶段验证工具**。配套分析：`reverse/nvidia-macos-driver-oc-feasibility.md`（可行性报告）；逐项矩阵与判读见 [VALIDATION-MATRIX.md](./VALIDATION-MATRIX.md)。

**目标**：在一台装好 NullMoth 驱动的黑苹果（macOS 15, x86_64, NVIDIA Turing+）上，实证回答可行性报告的 M0–M1 问题，并为 M2/M3 收集基线：

1. 用户态 → `NVRMUserClient` selector 1 → `rm_ioctl` 直通是否可达（报告 §3.3）；
2. RM 对象链（root client → attach → device → subdevice）是否完整可用；
3. GSP-RM 对**遥测类** FINN 控制与 **RUSD 遥测页**的接受度（M0）；
4. GSP-RM 对**写入类**控制（PERF_BOOST / SET_POWERSTATE / UPDATE_EDPP_LIMIT）的接受度与可逆性（M1）；每次写入均为"回读值回写"的零位移往返，退出时恢复。

## 前置条件

- macOS 15 (x86_64) + NullMoth 驱动四件套已加载（`kmutil showloaded --list-only | grep nullmoth` 有 4 行）；桌面出画面说明适配器已过 pass 2。
- Xcode 命令行工具（clang）。
- NVIDIA open-gpu-kernel-modules **610.57.04** 源码树（仅头文件用途，与驱动构建同源）：

```bash
git clone --depth 1 --branch 610.57.04 \
    https://github.com/NVIDIA/open-gpu-kernel-modules ~/ogkm610
```

## 构建

```bash
./build.sh                 # 产物 ./probe-macos-rm（默认 OGKM 路径 ~/ogkm610）
OGKM=/path/to/ogkm ./build.sh
```

非 macOS 机器上可以做纯语法/类型检查（IOKit 层走桩，其余用真实 OGKM 头）：

```bash
OGKM=/path/to/ogkm LINT=1 ./build.sh
```

## 运行

```bash
./probe-macos-rm --jsonl run1.jsonl                    # 默认阶段 0+1+2，全只读
./probe-macos-rm --stages 0123 --write --jsonl run2.jsonl   # 含写入测试
./probe-macos-rm --stages 4 --sample 30 --jsonl load.jsonl  # 30 s 遥测采样
```

- 退出码：`0` 全部通过；`1` 传输/对象链断裂；`3` 有 --write 门控项未授权（跑通了只读部分）；`2` 用法错误。
- 每个条目输出 `[OK|REJECTED|SKIP|GATED|HARD]` 判定 + `NV_STATUS` 名；`--jsonl` 同步落盘（结果 + 采样流），供与 Linux/Windows 实机三方对照。

### 安全须知（务必读）

- 阶段 0–2 只读；阶段 3 是**写入**（尽管设计为零位移往返 + 退出恢复），在有显示输出的机器上、远离重要工作的时间点执行；驱动层无任何联锁（报告 §6-2），误用直接进固件。
- 若用 `-nvrmnogo` 手动分阶段启动，需先 `nvrmctl go 2`，否则阶段 0 会在 card-info 处 HARD 失败并给出提示。
- **不要在生产/多用户机器上跑**：该驱动栈任意进程可发任意 RM 控制（报告 §6-7），本工具只应在自有的实验黑苹果上使用。

## 代码结构

```
src/nvx_xnu.c    IOKit 传输：IOServiceOpen(NVAccel 0x4E560000|n → NVRM n) + BINDFD + esc/MAP_MEMORY
                 （移植自 nvk-macos.patch 的 nvrm_xnu.c，即报告 §3.5 的参考实现）
src/nvx_rm.c     rm_alloc/rm_control/rm_free + NV01_ROOT_CLIENT→DEVICE_0→SUBDEVICE_0 链
                 （kms.inc:93-100 同款 dance）
src/nvx_suite.c  阶段 0–4 实现；全部 Tier-A 常量直接 #include OGKM 头，零转写
src/nvx_defs.h   仅收录开源树不含的 Tier-B legacy ID（逐条标注出处）
src/main.c       CLI / 阶段调度 / JSONL
```

常量出处纪律：**能进 OGKM 树的不进 nvx_defs.h**（编译期即权威）；树外 ID 必须带 campaign 文档出处注释；SET 面的 legacy 命令（`0x20801ACF`、`0x2080D031` 等）v1 只在矩阵文档记录、代码不发送。
