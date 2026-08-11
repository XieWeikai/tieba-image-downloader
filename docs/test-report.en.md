# Test Report

English | [简体中文](test-report.md)

Test date: 2026-08-11. Platform: macOS arm64.

## Local Automation

Using native arm64 Rust/Cargo 1.97.1, `cargo test --all-targets` ran 27 tests: 27 passed and 0 failed. Coverage includes URL handling, HTML and page API parsing, original-image field priority, stable deduplication and naming, Chrome cookie domain filtering, cookie-file import, concurrency bounds, content-type rejection, atomic state writes, structured-result and CLI argument contracts, and mock HTTP behavior for 200/206/416/429 with `Retry-After`.

`cargo fmt --all --check`, `cargo check --all-targets`, `cargo clippy --all-targets --all-features -- -D warnings`, and `cargo build --release` all passed.

v0.5.0 also runs `scripts/check-version-sync.sh v0.5.0` to compare Cargo, the lockfile, marketplace, Codex/Claude plugin manifests, and bootstrap script. The weekly Live Regression uses the public test post, parses metadata, and samples one bounded image. It may use a fresh isolated browser for rendering, but never handles CAPTCHAs automatically, uses or saves private sessions, or retains test artifacts. GitHub Actions records that continuing result separately from the local full end-to-end data.

The v0.5.0 release build then ran `--metadata-only --output-format json` against the same public post. The isolated browser captured the current 16 pages and produced 343 deduplicated original-image records. Stdout was one JSON line, `manifest.json` had the same count, and the first bounded sample returned `image/jpeg`. The test intentionally did not download the full set, so the contract reports zero completed images.

## Real End-to-End Test

The Release build was tested against `https://tieba.baidu.com/p/10918721568`. Preflight recognized Baidu's security challenge and opened the isolated Chrome flow. The session was reused through macOS Keychain. The program captured 11 `page_pc` API pages and produced 216 deduplicated body-image records.

With maximum concurrency 8 and adaptive control enabled, all 216 images downloaded successfully: 216 succeeded, 0 failed, and 0 `.part` files remained. The output totaled 274 MB. The manifest, 216 completed state entries, and filesystem count matched. A full MIME scan found no HTML or JSON masquerading as images. The application did not solve or bypass a CAPTCHA.

| Concurrency | Data | Duration | MiB/s | Success/Failure | 429/403 | CPU/Peak Memory |
|---:|---:|---:|---:|---:|---:|---:|
| 1 | Not tested | Not tested | Not tested | Not tested | Not tested | Not tested |
| 4 | Not tested | Not tested | Not tested | Not tested | Not tested | Not tested |
| 8 | 216 images / 274 MB | Completed | Not separately timed | 216/0 | 0 | Not sampled |
| 16 | Not tested | Not tested | Not tested | Not tested | Not tested | Not tested |
| 32 | Not tested | Not tested | Not tested | Not tested | Not tested | Not tested |
| 64 | Not tested | Not tested | Not tested | Not tested | Not tested | Not tested |
| 128 | Not tested | Not tested | Not tested | Not tested | Not tested | Not tested |

Untested levels are intentionally not populated with estimates. The end-to-end acceptance target was completeness and recoverability; a performance benchmark requires controlled network conditions, cache state, and repeated trials.
