#!/bin/sh
set -eu
if ! command -v cargo >/dev/null 2>&1; then
  if command -v brew >/dev/null 2>&1; then
    echo "未检测到 Cargo，使用 Homebrew 安装 Rust..."
    brew install rust
  else
    echo "未检测到 Cargo 或 Homebrew。请先从 https://rustup.rs 安装 Rust。" >&2
    exit 1
  fi
fi
cd "$(dirname "$0")"
cargo build --release
echo "安装完成：$(pwd)/target/release/tieba-image-downloader"
