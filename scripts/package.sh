#!/usr/bin/env bash
# 本机打包:测试、交叉编译后端与 TSF DLL、编译词典、收集许可证、makensis 出安装包。
# 用法:scripts/package.sh <版本> [rime-ice 目录]
set -euo pipefail
cd "$(dirname "$0")/.."
VERSION=${1:?用法: scripts/package.sh <版本> [rime-ice 目录]}
RIME=${2:-/tmp/rime-ice}
cargo test --workspace
cargo build --release --target x86_64-pc-windows-gnu -p pyime-backend
cp target/x86_64-pc-windows-gnu/release/pyime-server.exe installer/pyime-server.exe
scripts/build-tsf.sh  # PYIME.dll → target/tsf/{x64,x86}
# 用户机上没有 mingw 运行库,产物不得依赖它们
for f in x86_64:installer/pyime-server.exe x86_64:target/tsf/x64/PYIME.dll i686:target/tsf/x86/PYIME.dll; do
  if "${f%%:*}-w64-mingw32-objdump" -p "${f#*:}" | grep -E 'DLL Name: (libgcc|libstdc|libwinpthread)'; then
    echo "${f#*:} 依赖了 mingw 运行库,中止" >&2; exit 1
  fi
done
# 许可证全文随包分发:本项目 GPL-3.0;收编的 TSF 前端各部分;安装器用的 StdUtils 插件
rm -rf installer/licenses && mkdir installer/licenses
cp LICENSE installer/licenses/GPL-3.0.txt
n=0
while IFS= read -r l; do
  cp "$l" "installer/licenses/$(echo "${l#tsf/}" | tr '/' '-')"; n=$((n + 1))
# tsf/ 下文件名形如 jsoncpp/LICENSE、LGPL-2.0.txt、libIME2/LICENSE.txt、PIME-LICENSE.txt,故三种模式都要匹配
done < <(find tsf -type f \( -iname '*LICENSE*' -o -iname 'COPYING*' -o -iname 'LGPL*' \) | sort)
[ "$n" -gt 0 ] || { echo "tsf/ 下没有许可证文件,中止" >&2; exit 1; }
cp installer/nsis/StdUtils/LGPL.txt installer/licenses/StdUtils-LGPL-2.1.txt
cp installer/nsis/StdUtils/LGPL_EXCEPTION.txt installer/licenses/StdUtils-LGPL-EXCEPTION.txt
D="$RIME/cn_dicts"
cargo run --release -q -p pyime-engine --bin pyime-dict-build -- installer/pyime.dict \
  "$D/8105.dict.yaml" "$D/base.dict.yaml" "$D/ext.dict.yaml" "$D/others.dict.yaml"
E="$RIME/en_dicts"
cargo run --release -q -p pyime-engine --bin pyime-en-build -- installer/pyime.en \
  installer/en_it.txt "$E/en_ext.dict.yaml" "$E/en.dict.yaml"
makensis -V2 -DVERSION="$VERSION" installer/pyime.nsi
