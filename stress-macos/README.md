# stress-macos — Metal 计算负载与正确性校验器

macOS 侧**正确性校验**补充件。主力压力负载沿用原 NVOC 的 Vulkan 渲染路径
（`cli-stressor-cuda-rs --vulkan-only`，见 [../cli-stressor-cuda-rs/README.md]），
本目录只补齐它缺的那一半：**计算结果比对**。

分工与依据：

- `--vulkan-only` 是渲染负载，只 report FPS，不做任何读回比对（见
  `cli-stressor-cuda-rs/src/vulkan_render.rs`，"渲染成功/没崩溃"不等于算对）；
  它同时依赖本机 Vulkan loader + NVK ICD，属图形域激励。
- 本工具走 Metal 计算管线（NullMoth 的 NVMTL 插件底层也是 NVK），把结果与
  **同一公式的 CPU 参考实现**逐元素比对，是可直接判定的计算正确性证据。
- 本机无可用 CUDA/OpenCL 运行时，原 CUDA GEMM 校验路径（`--verify` 的 ride-on
  detector）在 macOS 上无法充当验证器，因此计算域校验落在本工具。

## 内核与判定

`xorshift64* + splitmix` 终混的**整数**混合函数，GPU 与 CPU 用同一定义。
整数运算只有一个正确答案，比对是 bit-exact 的，没有浮点容差可以藏错。

- 每次 dispatch 后逐元素比对；默认全量比对（`--verify-sample 0`），
  可指定抽样数以拉长负载。
- **验证器自检**：启动时先对一份故意写坏一位的参考数组跑比对，必须报出 1 处不符，
  否则工具自己 `exit 2`（"检测器坏了却全绿"比没有检测器更糟）。
- `--inject-mismatch` 仅供测试：dispatch 后故意翻转一个元素，用于端到端证明
  "错了会被抓住"，任何时候都不用于真实跑分。
- 两 buffer 流水线（先入队下一次、再校验上一次），两个 buffer 都校验，
  不为了拉高占用率跳过任何数据。

## 构建与运行

```sh
./build.sh                                  # 仅 macOS；链接 Metal + Foundation
./metal_check --verify                      # 单次全量校验（自检 + 1 次 dispatch）
./metal_check --verify --inject-mismatch    # 负例：必须 exit 1
./metal_check --stress 60 --iters 2000      # 限时循环，截止或 SIGINT 停止
```

参数：`--gpu <名称子串>`（默认系统默认设备）、`--elements <n>`（默认 65536）、
`--iters <n>`（每元素内层循环，默认 2000）、`--verify-sample <n>`
（默认 0 = 全量）、`--json`（摘要始终打印）。

退出码：`0` 通过；`1` 校验失败；`2` 参数/环境错误。

结果 JSON（schema `nvoc/macos-metal-check/1`）含 `device`/`mode`/`elements`/
`iters_per_thread`/`verify_sample`/`verified_full_buffer`/`iterations`/`verified`/
`failures`/`checked_elements`/`mismatched_elements`/`first_bad_*`/`elapsed_ms`/
`stopped_by_signal`，可直接被上层消费者解析。

## `--stream`：内存受限负载（负载发生器，不是校验器）

`nvoc_mix` 内核把状态全留在寄存器里，功耗与 SM 占用都上不去。`--stream` 走另一个内核
`nvoc_stream`：每线程以 grid stride 走遍整个缓冲，每趟读+写全缓冲，功耗来自内存流量。

```sh
./metal_check --stream 60                                        # 默认 4M 元素 / 8 趟 / 8192 线程 / private
./metal_check --stream 60 --stream-storage shared --stream-lanes 4
```

- 参数：`--stream <sec>`、`--stream-elements <n>`（默认 4194304 = 32 MB）、`--stream-threads <n>`
  （默认 8192）、`--stream-passes <n>`（默认 8）、`--stream-lanes <n>`（每线程独立累加器数，
  默认 1，上限 16）、`--stream-storage <private|shared>`（默认 **private**）。
- **存储模式决定一切**：`shared` 是**主机内存**，GPU 过总线访问，实测封顶 **14.2 GB/s**；
  `private` 是设备内存，实测 **161–163 GB/s**（TU116 192 GB/s 的约 85 %）。校验器必须用
  `shared`（要逐元素读回），所以**不要**用校验器的数字判断卡的带宽。
- **不能**与 `--verify`/`--stress` 同用（exit 2 并说明理由）：缓冲被每个线程反复改写，没有可比的
  CPU 参考，所以只报 dispatch 数、字节数、GB/s，JSON 显式 `"verification":"none"`，并给一个廉价
  的 `state_changed`（shared 读 host 映射的首元素；private 用一次 8 字节 blit 读回比较）。
  **正确性仍由 `--verify` / `--stress` 逐元素比对承担。**

结果 JSON schema `nvoc/macos-metal-stream/1`：`elements`/`threads`/`passes`/`lanes`/
`storage`/`bytes_per_dispatch`/`dispatches`/`failed`/`elapsed_ms`/`gigabytes_moved`/
`gigabytes_per_second`/`state_changed`/`verification`/`stopped_by_signal`。

## 实测（GTX 1650 SUPER / NVMTL over NVK TU116-A）

| 项 | 命令 | 结果 |
| --- | --- | --- |
| 全量校验 | `--verify` | exit 0，65536/65536 元素比对，`verified_full_buffer=true` |
| 负例 | `--verify --inject-mismatch` | exit 1，`mismatched_elements=1`，首个坏点 index 32768 |
| 限时负载 | `--stress 60 --iters 20000 --verify-sample 512` | exit 0，1206 次 dispatch / 0 失败 / 617472 元素抽查 |
| 停止 | `--stress 120` + `SIGINT` | 立刻退出（0.06 s），exit 0，`stopped_by_signal=true` |

注意 `--iters` 同时决定 CPU 参考实现的代价：`--verify` 默认全量比对，
65536 元素 × `iters` 次混合会主导墙钟时间（`iters=20000` 约 4 s CPU/次），
GPU 端单位 dispatch 只有几十毫秒量级。要拉长负载请优先加 `--elements`
或用 `--stress`，不要靠 `--iters` 堆 CPU 时间。

## 实测补充：负载能压满，功耗读数不能信

| 项 | 命令 | 结果 |
| --- | --- | --- |
| 内存受限负载（主机内存） | `--stream 5 --stream-storage shared` | exit 0，**14.25–14.28 GB/s** |
| 内存受限负载（设备内存） | `--stream 5 --stream-storage private` | exit 0，**161.09 / 162.95 GB/s** ≈ 85 % 峰值 |
| 寄存器受限负载 | `--stress --elements 4194304 --iters 2000` | **104.1 G-mix/s**（65536 元素时只有 42.1） |
| 大网格校验 | `--verify --elements 4194304 --iters 20000 --verify-sample 256` | exit 0，**1.25 s**（修复自检前要几分钟） |
| 负例 | `--verify --inject-mismatch` | exit 1，`mismatched_elements=1`，首个坏点 32768 |

RM 遥测同步采样（40 s）：

| 形状 | 功耗峰值 | 利用率 | 温度 | 时钟 |
| --- | --- | --- | --- | --- |
| 空闲 | 11.55 W | 0 % | 46.5 ℃ | 1530 MHz |
| stream-private × 6 | 12.65 W | 100 % | → 59.4 ℃ | 1950 MHz |
| ALU-4M × 4 | 14.84 W | 100 % | → **69.8 ℃** | 1950 → **1920 MHz** |

**`power_w` / `power_mw` 是桩值**：空闲 11.55 W、带宽打满 12.65 W、热降频时 14.84 W。
一块正在降频的 1650 SUPER 不可能只吃 15 W，所以这两个字段不能用来判断"离 100 W 上限多远"，
本机的功耗上限行为**不可测**——原因不是负载不足，而是遥测不可信。`utilization_percent` 同理
只反映引擎有活排队（`engine_utilization_percent.gr` 与它同步），不是"满载"的证据。
