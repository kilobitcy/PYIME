#!/usr/bin/env bash
# 用 mingw-w64 交叉编译收编在 tsf/ 的 TSF 文本服务 DLL(x64 + x86),输出到 target/tsf/{x64,x86}。
# 源码来源与许可证见 tsf/README.md。用法:scripts/build-tsf.sh
set -euo pipefail
cd "$(dirname "$0")/.."
ROOT=$PWD
SRC=$ROOT/tsf
OUTDIR=$ROOT/target/tsf
DLL=PYIME.dll
PIME_SRCS="DllEntry PIMEClient PIMEImeModule PIMELangBarButton PIMETextService TsfLog"

# 头文件大小写垫片(源码按 Windows 大小写不敏感写 include)
SHIM=$(mktemp -d)
trap 'rm -rf "$SHIM"' EXIT
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
  for f in $PIME_SRCS; do
    $T-g++ $CXXFLAGS -c "$SRC/PIMETextService/$f.cpp" -o "$OBJ/pime_$f.o"
  done
  sed -e 's/@PIME_VERSION_MAJOR@/1/g;s/@PIME_VERSION_MINOR@/3/g;s/@PIME_VERSION_PATCH@/0/g' \
    "$SRC/PIMETextService/PIMETextService.rc.in" > "$OBJ/PIMETextService.rc"
  $T-windres -I"$SRC/PIMETextService" -I"$SHIM" "$OBJ/PIMETextService.rc" -O coff -o "$OBJ/res.o"
  # --kill-at:x86 导出名去掉 stdcall 修饰,与 .def 一致
  $T-g++ -shared -static -static-libgcc -static-libstdc++ -s -Wl,--kill-at -Wl,--subsystem,windows \
    -o "$OUT/$DLL" "$OBJ"/*.o "$SRC/PIMETextService/PIMETextService.def" \
    -lole32 -loleaut32 -luuid -lshlwapi -lgdi32 -luser32 -ladvapi32 -lshell32
  if $T-objdump -p "$OUT/$DLL" | grep -E 'DLL Name: (libgcc|libstdc|libwinpthread)'; then
    echo "$1 DLL 依赖了 mingw 运行库,中止" >&2; exit 1
  fi
  local DUMP; DUMP=$($T-objdump -p "$OUT/$DLL")  # 先落变量:pipefail 下 grep -q 提前退出会让 objdump 吃 SIGPIPE 误判
  for sym in DllGetClassObject DllCanUnloadNow DllRegisterServer DllUnregisterServer; do
    grep -qw "$sym" <<<"$DUMP" || { echo "$1 DLL 缺导出 $sym,中止" >&2; exit 1; }
  done
}

# 纯逻辑单测(本机 g++)
g++ -std=c++17 -Wall -I"$SRC/PIMETextService" "$SRC/tests/backoff_test.cpp" -o "$SHIM/backoff_test"
"$SHIM/backoff_test"

build x64
build x86
ls -l "$OUTDIR"/*/"$DLL"
