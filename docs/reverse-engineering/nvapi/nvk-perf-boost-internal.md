# NVK 内部 PERF_BOOST：谁在发、发什么、何时清

状态：静态证据已闭环（客户端 + host），实测日志已交叉确认。**未执行任何新的 GPU 写入**。
适用对象：macOS 上作为 Metal 插件底层的 NVK（`libvulkan_nouveau.dylib`）。

## 0. 为什么要单列这一条

安全记录里必须区分两件事：

- **agent 不发用户性能 SET**（本项目所有 gate 保证的）；
- **底层驱动自己发的性能请求**（不在 agent 控制下，但会改变 GPU 行为）。

此前只从 Vulkan 初始化日志看到 `PERF_BOOST` 字样。本文把它落到**具体指令、具体参数、具体生命周期**，
以免在后续偏移实验里把「驱动内建 boost 引起的频率变化」误当成「offset 写成功」的证据。

## 1. 证据来源

| 项 | 值 |
| --- | --- |
| 文件 | `/Library/GPUBundles/nvmtl/libvulkan_nouveau.dylib` |
| size | 21621168 B |
| sha256 | `cfa7e20f46af3b34365d11beca9b79ab83dae9fc07c9ec72ee8581b9d19a51c7` |
| Mach-O | 64-bit x86_64；`__TEXT` `fileoff=0` ⇒ **文件偏移 == 虚拟地址**（本机副本：`nvoc-analysis/libvulkan_nouveau.dylib`） |
| 符号 | 内嵌 `LC_SYMTAB`（43306 条），另有同目录 `libvulkan_nouveau.dylib.syms` |
| host 对照 | `open-gpu-kernel-modules-610.57.04`：`kern_perf_boost.c`、`ctrl2080perf.h` |

`0x2080200a` 该 dword 字面量在整份 `__TEXT` 里**只出现 2 次**：文件偏移 `0x12d24b`、`0x12d4f2`。
⇒ 客户端只可能在这两处发 `NV2080_CTRL_CMD_PERF_BOOST`，没有第三个发送点。

## 2. 封装层：`nvRmApiControl`（`0x12f6c0`）

prologue 直接给出 ABI：

```
mov  eax, [rdi+4]      ; 参数：struct { int fd; int _pad; }
mov  [rbp-0x30], eax   ;  → 请求头 +0
mov  [rbp-0x2c], esi   ;  → +4  = hObject
mov  [rbp-0x28], edx   ;  → +8  = command
mov  dword ptr [rbp-0x24], 0
mov  [rbp-0x20], rcx   ;  → +16 = params 指针
mov  [rbp-0x18], r8d   ;  → +24 = size
mov  dword ptr [rbp-0x14], 0
mov  ebx, [rdi]        ; fd
...
mov  esi, 0x2a         ; NV_ESC_RM_CONTROL = 42
call 0x130ad0          ; ioctl(fd, 0x2a, &req, 0x20)
```

⇒ 语义等价于 `ioctl(fd, NV_ESC_RM_CONTROL, {hObject, cmd, params, size})`。**与本项目 probe 走的
`NV_ESC_RM_CONTROL` 是同一条通路**，因此 §3 的两条命令会走 host NVOC → 私有 id → `RmGssLegacyRpcCmd`
转发到 GSP（见 `gsp-method-table-and-legacy-forwarding.md`）。

## 3. 两个发送点

### 3.1 设备创建：`_nvkmd_nvrm_enum_pdev`（函数起点 `0x12c420`），发送点 `0x12d24b`

```
0x12d21d  lea  rdi, [rip+0x7336e9]  ; rdi = "BOOST"                → 0x86090d
0x12d224  call 0x12dce0             ; nvrm610_on("BOOST")
0x12d229  test al, al
0x12d22b  je   0x12d281             ; 门关 ⇒ 完全不发
0x12d22d  movabs rax, 0xffffffff00000012
0x12d237  mov  [rbp-0xb90], rax     ; { flags = 0x12, duration = 0xffffffff }
0x12d23e  mov  rax, [rbp-0xa08]
0x12d245  mov  esi, [rax]           ; hObject
0x12d247  mov  rdi, rbx
0x12d24a  mov  edx, 0x2080200a      ; NV2080_CTRL_CMD_PERF_BOOST
0x12d24f  lea  rcx, [rbp-0xb90]     ; &params
0x12d256  mov  r8d, 8               ; sizeof
0x12d25c  call 0x12f6c0             ; nvRmApiControl
0x12d261  ...
0x12d271  lea  rsi, [rip+0x73369b]  ; "nvrm: b81 PERF_BOOST flags 0x%x INFINITE rc 0x%x\n"
0x12d278  mov  ecx, eax             ; rc
0x12d27c  call 0x7ff9b0             ; printf
```

按 `ctrl2080perf.h` 解码：`flags = 0x12` = `FLAGS_CMD_BOOST_TO_MAX(0x2) | FLAGS_CUDA_YES(1<<4 = 0x10)`；
`duration = 0xFFFFFFFF = DURATION_INFINITE`（头文件原文：*"the boost will last until cleared"*）。
`sizeof = 8` 与 `struct { NvU32 flags; NvU32 duration; }` 精确一致。

### 3.2 设备销毁：`_nvkmd_nvrm_pdev_destroy`（函数起点 `0x12d480`），发送点 `0x12d4f2`

```
0x12d4cb  lea  rdi, [rip+0x73343b]  ; rdi = "BOOST"
0x12d4d2  call 0x12dce0             ; nvrm610_on("BOOST")  —— 与创建点同一个门
0x12d4d9  je   0x12d501             ; 门关 ⇒ 跳过
0x12d4db  mov  qword ptr [rbp-0x28], 0   ; { flags = 0, duration = 0 }
0x12d4e3  mov  esi, [rbx+0x1c8]     ; hObject
0x12d4e9  lea  rdi, [rbp-0x20]
0x12d4ed  lea  rcx, [rbp-0x28]
0x12d4f1  mov  edx, 0x2080200a
0x12d4f6  mov  r8d, 8
0x12d4fc  call 0x12f6c0            ; nvRmApiControl
```

`flags = 0` = **`FLAGS_CMD_CLEAR`**，`duration = 0`。**该分支没有日志打印**（整份文件里只有 §3.1
那一条 `PERF_BOOST` 字符串）⇒ 不能用日志确认 clear 是否已发，只能用「同一门 + 同一函数被调」来推断。

发送 clear 之后，函数依次 `nvRmApiFree` 掉 `[rbx+0x1e8] / [rbx+0x1cc] / [rbx+0x1c8] / [rbx+0x1c4]`
几个 handle，再释放 `[rbx+0x218]`、`[rbx+0x1b0]` 与对象本身 ⇒ 这是**正常 teardown 路径**。

### 3.3 门的语义：`nvrm610_on`（`0x12dce0`）

```
snprintf(buf, 0x40, "NVRM_610_%s", name)   ; → "NVRM_610_BOOST"
v = getenv(buf)
if (v)  return v[0] != '0';
/* 否则读一次配置文件并缓存 */
fopen("/Library/GPUBundles/nvmtl/nvrm610.conf", "r")
snprintf(pat, 0x50, "%s=", name)           ; → "BOOST="
p = strstr(confbuf, pat)
if (!p) return true;                       /* 键不存在 ⇒ 默认 ON */
return p[len(pat)] != '0';
```

⇒ 优先级：环境变量 `NVRM_610_BOOST` > `/Library/GPUBundles/nvmtl/nvrm610.conf` 的 `BOOST=` 行 > **默认 ON**。
本机实测：env 未设；该 conf 只有 `NVMTL_*` 五行，**没有 `BOOST=`** ⇒ 门为 **ON**。

## 4. 实测交叉确认

`outputs/` 下 grep：**50 条**同一格式行

```
nvrm: b81 PERF_BOOST flags 0x12 INFINITE rc 0x0
```

出现在 `stage-vulkan-recheck-*/run-{bare,gpu0,badidx}.txt`、`stage2-telemetry-*/load-app.txt`、
`stage12-recheck-*/metal-verify.txt`。`rc 0x0 = NV_OK` ⇒ GSP 当时确实接受了这条
`BOOST_TO_MAX | CUDA_YES` 无限期请求。**即：只要 NVK 设备被创建（Metal 枚举 / Vulkan 离屏都算），
GPU 就进入过被请求的最大 boost 态**，与 agent 是否发 SET 无关。

## 5. host 侧：是否有独立释放

`src/nvidia/src/kernel/gpu/perf/kern_perf_boost.c`（R610.57.04）：

- `subdeviceCtrlCmdKPerfBoost_IMPL`：非 GB10Y（TU116 属此类）直接 `kperfBoostSet(...)`
  → `NV2080_CTRL_CMD_INTERNAL_PERF_BOOST_SET_2X (0x20800a9a)`。**没有 clear/teardown 分支**。
- 文件注释原文承认：「当 app 退出时**没有** follow-up `CMD_CLEAR` 调用……会让 GPU 长时间停在
  perfboost 模式」，并只为 **Tegra 专属** 打了个把 `INFINITE` 覆盖成 1 s 的 WAR —— **不适用于 dGPU**。
- ⇒ 释放责任完全在**客户端**（§3.2 的 clear）。

**推论（须标注为推论）**：若持有 NVK 设备的进程被 `SIGKILL` 或崩溃，`_nvkmd_nvrm_pdev_destroy`
可能不执行，clear 就不会发。此时 GPU 可能停在 BOOST_TO_MAX。属于**待验证项**，不要写成已确认。

## 6. 安全记录口径（对写入门槛的影响）

1. NVK 在设备创建时发的是 **BOOST_TO_MAX（不是 BOOST_1LEVEL）** + INFINITE。这属于驱动自身的运行行为，
   **不是** agent 的实验写入，两者必须在证据里分开写。
2. 因此在本项目的偏移实验里：
   - **禁止**把运行期频率升高当作 offset 写成功的判据（`WRITE-VERIFICATION-PLAN.md` §7.3-⑤ 已同步）；
   - 任何「稳定/成功」结论都必须来自**同字段回读 + 其它 dword 逐字节不变 + restore 后整块一致**，
     而不是来自 RUSD/实时 kHz；
   - 若实验窗口里同时存在 NVK 设备，必须先把 boost 的 `CMD_CLEAR` 路径与其门状态记录在案，
     以免混淆两套频率来源。
3. 本机默认门为 ON，且实测有 50 条成功 boost；文档与 review 记录中不得再出现
   「底层没有性能请求」这类表述。

## 7. 未决

- §5 的 kill/crash 场景是否真的让 GPU 停留在 boost：需要一次「创建 NVK 设备 → SIGKILL → 读 P-state/频率」
  的只读对照，且要在 GSP 解除 `0x62` 之后做，避免把 wedge 与 boost 残留混为一谈。
- `0x20800a9a`（`INTERNAL_PERF_BOOST_SET_2X`）在 host 侧的 refcount/超时语义未核对；它决定
  「未发 clear」时 boost 是否自行衰减。
