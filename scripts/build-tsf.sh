#!/usr/bin/env bash
# 交叉编译 pyime-tsf(x86_64-pc-windows-gnu)。panic=abort 与静态链接只在此处传入,不改 workspace profile。
# 可用 PYIME_TSF_RUSTFLAGS 覆盖默认 rustflags(阶段 0 用来试静态链接参数)。
set -euo pipefail
cd "$(dirname "$0")/.."
FLAGS="${PYIME_TSF_RUSTFLAGS:--C panic=abort -C target-feature=+crt-static}"
RUSTFLAGS="$FLAGS" cargo build --release --target x86_64-pc-windows-gnu -p pyime-tsf
echo "产物: target/x86_64-pc-windows-gnu/release/pyime_tsf.dll"
