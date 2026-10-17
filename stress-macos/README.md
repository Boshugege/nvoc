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
