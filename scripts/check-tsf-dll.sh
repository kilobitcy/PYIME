#!/usr/bin/env bash
# 自检 TSF DLL:依赖不得含 libgcc/libwinpthread,须导出 4 个 Dll* 符号。
set -euo pipefail
cd "$(dirname "$0")/.."
DLL="${1:-target/x86_64-pc-windows-gnu/release/pyime_tsf.dll}"
test -f "$DLL" || { echo "找不到 $DLL"; exit 1; }
OUT="$(x86_64-w64-mingw32-objdump -p "$DLL")"
echo "$OUT" | grep 'DLL Name' || true
if echo "$OUT" | grep -E 'DLL Name:.*(libgcc|libwinpthread|libstdc)'; then
  echo "FAIL: 依赖了不应出现的运行库"; exit 1
fi
for s in DllGetClassObject DllCanUnloadNow DllRegisterServer DllUnregisterServer; do
  grep -qw "$s" <<<"$OUT" || { echo "FAIL: 缺少导出 $s"; exit 1; }
done
echo "OK: $DLL"
