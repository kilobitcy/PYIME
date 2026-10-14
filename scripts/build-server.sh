#!/usr/bin/env bash
# 交叉编译 pyime-server 与 pyime-probe(x86_64-pc-windows-gnu)并检查依赖。
# panic=abort 与静态链接只在此处传入,不改 workspace profile。可用 PYIME_RUSTFLAGS 覆盖。
set -euo pipefail
cd "$(dirname "$0")/.."
FLAGS="${PYIME_RUSTFLAGS:--C panic=abort -C target-feature=+crt-static}"
RUSTFLAGS="$FLAGS" cargo build --release --target x86_64-pc-windows-gnu -p pyime-server
for exe in pyime-server pyime-probe; do
  f="target/x86_64-pc-windows-gnu/release/$exe.exe"
  test -f "$f" || { echo "找不到 $f"; exit 1; }
  OUT="$(x86_64-w64-mingw32-objdump -p "$f")"
  if grep -E 'DLL Name:.*(libgcc|libwinpthread|libstdc)' <<<"$OUT"; then
    echo "FAIL: $f 依赖了不应出现的运行库"; exit 1
  fi
  echo "OK: $f"
done
