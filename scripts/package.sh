#!/usr/bin/env bash
# 本机打包:交叉编译后端、编译词典、makensis 出安装包。用法:scripts/package.sh <版本> [rime-ice 目录]
set -euo pipefail
cd "$(dirname "$0")/.."
VERSION=${1:?用法: scripts/package.sh <版本> [rime-ice 目录]}
RIME=${2:-/tmp/rime-ice}
cargo test --workspace
cargo build --release --target x86_64-pc-windows-gnu -p pyime-backend
cp target/x86_64-pc-windows-gnu/release/pyime-backend.exe installer/server.exe
scripts/build-pime-dll.sh  # 补丁版 PIMETextService.dll → target/pime/{x64,x86}
if x86_64-w64-mingw32-objdump -p installer/server.exe | grep -E 'DLL Name: (libgcc|libwinpthread)'; then
  echo "server.exe 依赖了 mingw 运行库,中止" >&2; exit 1
fi
D="$RIME/cn_dicts"
cargo run --release -q -p pyime-engine --bin pyime-dict-build -- installer/pyime.dict \
  "$D/8105.dict.yaml" "$D/base.dict.yaml" "$D/ext.dict.yaml" "$D/others.dict.yaml"
E="$RIME/en_dicts"
cargo run --release -q -p pyime-engine --bin pyime-en-build -- installer/pyime.en \
  installer/en_it.txt "$E/en_ext.dict.yaml" "$E/en.dict.yaml"
makensis -V2 -DVERSION="$VERSION" installer/pyime.nsi
