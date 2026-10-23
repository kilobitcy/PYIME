#!/usr/bin/env bash
# 组装 TSF 前端实机验证包 dist/tsf-dev.zip(不经安装器):两份 PYIME.dll + pyime-server.exe + 词典等 + tsf-dev.bat。
# 词典 pyime.dict / pyime.en 由 scripts/package.sh 生成,先跑过一次。用法:scripts/tsf-dev-kit.sh
set -euo pipefail
cd "$(dirname "$0")/.."
OUT=dist/tsf-dev
EXE=target/x86_64-pc-windows-gnu/release/pyime-server.exe
scripts/build-tsf.sh
cargo build --release --target x86_64-pc-windows-gnu -p pyime-backend
for f in "$EXE" installer/pyime.dict installer/pyime.en installer/symbols.txt; do
  [ -f "$f" ] || { echo "缺 $f" >&2; exit 1; }
done
if x86_64-w64-mingw32-objdump -p "$EXE" | grep -E 'DLL Name: (libgcc|libwinpthread)'; then
  echo "pyime-server.exe 依赖了 mingw 运行库,中止" >&2; exit 1
fi
rm -rf "$OUT" dist/tsf-dev.zip
mkdir -p "$OUT/x64" "$OUT/x86" "$OUT/icons"
cp target/tsf/x64/PYIME.dll "$OUT/x64/"
cp target/tsf/x86/PYIME.dll "$OUT/x86/"
cp "$EXE" installer/pyime.dict installer/pyime.en installer/symbols.txt "$OUT/"
cp installer/icons/chi_full_capsoff.ico installer/icons/eng_full_capsoff.ico "$OUT/icons/"
cp test-kit/tsf-dev.bat test-kit/tsf-dev.txt "$OUT/"
(cd dist && zip -qr tsf-dev.zip tsf-dev)
ls -l dist/tsf-dev.zip
