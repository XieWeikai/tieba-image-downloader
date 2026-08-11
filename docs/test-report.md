# 测试报告

[English](test-report.en.md) | 简体中文

测试日期：2026-08-11；平台：macOS arm64。

## 本地自动化测试

Rust/Cargo 1.97.1，原生架构 arm64。`cargo test --all-targets` 共运行 27 个测试，27 通过、0 失败。范围包括链接、HTML 与页面 API 解析、原图字段优先级、稳定去重命名、Chrome Cookie 域过滤、Cookie 文件导入、并发升降边界、Content-Type、状态原子写入、结构化结果与 CLI 参数契约，以及 mock HTTP 200/206/416/429 Retry-After。

`cargo fmt --all --check`、`cargo check --all-targets`、`cargo clippy --all-targets --all-features -- -D warnings` 与 `cargo build --release` 均通过。

v0.5.0 另外运行 `scripts/check-version-sync.sh v0.5.0`，确认 Cargo、lockfile、市场清单、Codex/Claude 插件清单和引导脚本一致。每周 Live Regression 使用公开测试帖，只解析元数据并对一张图片执行有界抽样；必要时使用全新隔离浏览器渲染，但不自动处理验证码、不使用或保存私人会话、不保存测试产物。该结果由 GitHub Actions 持续记录，不与本地完整端到端数据混合。

v0.5.0 Release 构建随后对同一公开帖子执行 `--metadata-only --output-format json` 真实测试。隔离浏览器捕获当前 16 页响应，得到 343 条去重原图记录；stdout 为单行 JSON，`manifest.json` 数量一致，第一张有界抽样返回 `image/jpeg`。该测试未下载全量图片，因此完成数按契约为 0。

## 真实端到端与吞吐

使用 `https://tieba.baidu.com/p/10918721568` 进行 Release 端到端测试。预检识别安全验证后自动打开独立 Chrome，会话从 macOS 钥匙串复用；程序通过浏览器捕获 11 页 `page_pc` API 响应，解析并去重得到 216 张正文原图。以最大并发 8、自动调节开启完成下载：成功 216、失败 0、残留 `.part` 0，总大小 274 MB；manifest、下载状态和实际文件数一致。程序没有自动破解或绕过验证码。

| 并发 | 数据量 | 耗时 | MiB/s | 成功/失败 | 429/403 | CPU/峰值内存 |
|---:|---:|---:|---:|---:|---:|---:|
| 1 | 未测试 | 未测试 | 未测试 | 未测试 | 未测试 | 未测试 |
| 4 | 未测试 | 未测试 | 未测试 | 未测试 | 未测试 | 未测试 |
| 8 | 216 张 / 274 MB | 已完成 | 未单独计时 | 216/0 | 0 | 未采样 |
| 16 | 未测试 | 未测试 | 未测试 | 未测试 | 未测试 | 未测试 |
| 32 | 未测试 | 未测试 | 未测试 | 未测试 | 未测试 | 未测试 |
| 64 | 未测试 | 未测试 | 未测试 | 未测试 | 未测试 | 未测试 |
| 128 | 未测试 | 未测试 | 未测试 | 未测试 | 未测试 | 未测试 |

未测试的并发档位不会用推测数据填充。端到端验收目标是完整性与可恢复性；性能基准需要固定网络、缓存状态和重复次数后另行执行。
