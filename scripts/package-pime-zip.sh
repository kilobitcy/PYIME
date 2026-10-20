#!/usr/bin/env bash
# 把已构建的 PIME 附加包内容打成单个 zip(含安装/卸载脚本)。用法:scripts/package-pime-zip.sh <版本>
# 使用 installer/ 下现成的 server.exe、pyime.dict 与 pyime.en(由 scripts/package.sh 产生),不重新编译。
set -euo pipefail
cd "$(dirname "$0")/.."
VERSION=${1:?用法: scripts/package-pime-zip.sh <版本>}
CARGO_VER=$(sed -n 's/^version = "\(.*\)"/\1/p' crates/pyime-backend/Cargo.toml | head -1)
[ "$VERSION" = "$CARGO_VER" ] || { echo "版本 $VERSION 与 Cargo.toml($CARGO_VER)不一致" >&2; exit 1; }
for f in installer/server.exe installer/pyime.dict installer/pyime.en installer/NOTICE.md installer/input_methods/pyime/ime.json LICENSE; do
  test -f "$f" || { echo "缺少 $f" >&2; exit 1; }
done
DEPS="$(x86_64-w64-mingw32-objdump -p installer/server.exe)"
if grep -E 'DLL Name: (libgcc|libwinpthread)' <<<"$DEPS"; then
  echo "server.exe 依赖了 mingw 运行库,中止" >&2; exit 1
fi
NAME="PYIME-$VERSION-win64"
OUT="dist/$NAME"
rm -rf "$OUT" "dist/$NAME.zip"
mkdir -p "$OUT"
cp installer/server.exe installer/pyime.dict installer/pyime.en installer/NOTICE.md installer/input_methods/pyime/ime.json LICENSE "$OUT/"
cp installer/zip-pime/* "$OUT/"
(cd dist && zip -q -r -X "$NAME.zip" "$NAME" && sha256sum "$NAME.zip" > SHA256SUMS.txt)
unzip -l "dist/$NAME.zip"
cat dist/SHA256SUMS.txt
