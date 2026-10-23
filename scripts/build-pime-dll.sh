#!/usr/bin/env bash
# 用 mingw-w64 交叉编译打过补丁的 PIME 1.3.0 PIMETextService.dll(x64 + x86),输出到 target/pime/{x64,x86}。
# 补丁:pime/patches/pime-mingw.patch(MSVC 专有写法改为可移植写法)、libime2-candidate-width.patch(候选按自身宽度排版)。
# 用法:scripts/build-pime-dll.sh [PIME 源码目录,默认 /tmp/PIME-pyime] [--baseline 不打候选宽度补丁,作对照]
set -euo pipefail
cd "$(dirname "$0")/.."
ROOT=$PWD
SRC=${1:-/tmp/PIME-pyime}
BASELINE=${2:-}
OUTDIR=$ROOT/target/pime${BASELINE:+-baseline}

if [ ! -d "$SRC/.git" ]; then
  git clone -q --branch v1.3.0-stable https://github.com/EasyIME/PIME.git "$SRC"
  git -C "$SRC" submodule update -q --init jsoncpp libIME2
fi
# 每次从干净源码重新打补丁
git -C "$SRC" checkout -q -- . && git -C "$SRC/libIME2" checkout -q -- .
git -C "$SRC" apply "$ROOT/pime/patches/pime-mingw.patch"
[ -n "$BASELINE" ] || git -C "$SRC/libIME2" apply "$ROOT/pime/patches/libime2-candidate-width.patch"
# resource.h 原为 UTF-16,windres/g++ 读不了
iconv -f UTF-16 -t UTF-8 "$SRC/PIMETextService/resource.h" | sed '1s/^\xEF\xBB\xBF//' > "$SRC/resource.h.tmp"
mv "$SRC/resource.h.tmp" "$SRC/PIMETextService/resource.h"

# 头文件大小写垫片(源码按 Windows 大小写不敏感写 include)
SHIM=$(mktemp -d)
trap 'rm -rf "$SHIM"' EXIT
cp "$ROOT/tsf/tsf_compat.h" "$SHIM/"
INC=/usr/x86_64-w64-mingw32/include
for h in ObjBase.h OleCtl.h Shellapi.h ShlObj.h Shlwapi.h Unknwn.h VersionHelpers.h Windows.h Winnls.h; do
  ln -s "$INC/${h,,}" "$SHIM/$h"
done
touch "$SHIM/Ctffunc.h"  # 所需声明已在 tsf_compat.h
ln -s "$SRC/libIME2/src/Window.h" "$SHIM/window.h"
ln -s "$SRC/libIME2" "$SHIM/LibIME2"

build() {  # build <x64|x86>
  local T OBJ OUT
  case $1 in x64) T=x86_64-w64-mingw32 ;; x86) T=i686-w64-mingw32 ;; esac
  OBJ=$SHIM/obj-$1; OUT=$OUTDIR/$1; mkdir -p "$OBJ" "$OUT"
  local CXXFLAGS="-std=c++17 -O2 -DUNICODE -D_UNICODE -DWIN32_LEAN_AND_MEAN -D_WIN32_WINNT=0x0600 -DNDEBUG
    -include tsf_compat.h -I$SHIM -I$SRC -I$SRC/libIME2/src -I$SRC/jsoncpp/include -I$SRC/PIMETextService"
  for f in ImeModule libIME TextService KeyEvent EditSession DisplayAttributeInfo DisplayAttributeInfoEnum \
      DisplayAttributeProvider LangBarButton Utils DrawUtils Window ImeWindow MessageWindow CandidateWindow; do
    $T-g++ $CXXFLAGS -c "$SRC/libIME2/src/$f.cpp" -o "$OBJ/ime_$f.o"
  done
  for f in json_reader json_value json_writer; do
    $T-g++ $CXXFLAGS -c "$SRC/jsoncpp/src/lib_json/$f.cpp" -o "$OBJ/json_$f.o"
  done
  for f in DllEntry PIMEClient PIMEImeModule PIMELangBarButton PIMETextService; do
    $T-g++ $CXXFLAGS -c "$SRC/PIMETextService/$f.cpp" -o "$OBJ/pime_$f.o"
  done
  sed -e 's/@PIME_VERSION_MAJOR@/1/g;s/@PIME_VERSION_MINOR@/3/g;s/@PIME_VERSION_PATCH@/0/g' \
    "$SRC/PIMETextService/PIMETextService.rc.in" > "$OBJ/PIMETextService.rc"
  $T-windres -I"$SRC/PIMETextService" -I"$SHIM" "$OBJ/PIMETextService.rc" -O coff -o "$OBJ/res.o"
  # --kill-at:x86 导出名去掉 stdcall 修饰,与 .def 一致
  $T-g++ -shared -static -static-libgcc -static-libstdc++ -s -Wl,--kill-at -Wl,--subsystem,windows \
    -o "$OUT/PIMETextService.dll" "$OBJ"/*.o "$SRC/PIMETextService/PIMETextService.def" \
    -lole32 -loleaut32 -luuid -lshlwapi -lgdi32 -luser32 -ladvapi32 -lshell32
  if x86_64-w64-mingw32-objdump -p "$OUT/PIMETextService.dll" | grep -E 'DLL Name: (libgcc|libstdc|libwinpthread)'; then
    echo "$1 DLL 依赖了 mingw 运行库,中止" >&2; exit 1
  fi
}
build x64
build x86
ls -l "$OUTDIR"/*/PIMETextService.dll
