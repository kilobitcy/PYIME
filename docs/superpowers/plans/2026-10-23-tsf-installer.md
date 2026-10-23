# 自带 TSF 前端:安装器重写、实机测试包与 PIME 遗留清理 实施计划(计划 C)

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 产出不依赖官方 PIME 的独立安装包 `PYIME-<版本>-setup.exe`(装到 `$PROGRAMFILES64\PYIME`,自动迁移旧附加包),配套实机验证清单;验证通过后删除 PIME 遗留代码与文档。

**Architecture:** NSIS 脚本 `installer/pyime.nsi` 整体重写:停服务端 → 改名法替换占用文件 → `regsvr32` 注册 x64/x86 两份 `PYIME.dll` → 写 HKLM Run → StdUtils `ExecShellAsUser` 以非提权身份启动 `pyime-server.exe --supervise`。`scripts/package.sh` 改为调 `scripts/build-tsf.sh`。对应设计 `docs/superpowers/specs/2026-10-23-tsf-frontend-design.md` 组件 3、实施顺序第 4–6 步。

**Tech Stack:** NSIS 3.09(本机 `makensis`,`Unicode true`,x86-unicode 目标)、StdUtils 插件 2015-11-16(LGPL-2.1 + 插件例外,取自 PIME 源码树)、bash、mingw-w64 objdump、7z。

## Global Constraints
- 前置:计划 A(`tsf/`、`scripts/build-tsf.sh` → `target/tsf/{x64,x86}/PYIME.dll`、`test-kit/tsf-dev.bat`)与计划 B(`cargo build --release --target x86_64-pc-windows-gnu -p pyime-backend` 产出 `pyime-server.exe`,参数 `--supervise`/`--worker`/`--quit`)已完成。
- `pyime-server.exe --quit`:通知监护进程连同子进程退出,等 ≤3s;监护进程不在时立即返回。无参数 = 旧 PIME stdin 模式(Task 4 删除)。
- 安装目录 `$PROGRAMFILES64\PYIME`:`pyime-server.exe`、`pyime.dict`、`pyime.en`、`symbols.txt`、可选 `pyime.gram`、`icons\`、`x64\PYIME.dll`、`x86\PYIME.dll`、`licenses\`、`NOTICE.md`、`uninstall.exe`。
- HKLM Run 值名 `PYIME` = `"<安装目录>\pyime-server.exe" --supervise`;卸载登记键 `HKLM\Software\Microsoft\Windows\CurrentVersion\Uninstall\PYIME`(64 位视图,`SetRegView 64`)。
- 新 CLSID `{8298C4A9-D032-442A-90AE-1983DF71CDBE}`,语言配置 GUID `{87977804-6ADE-4B62-8715-4B1C29435A52}`(由 DLL 的 `DllRegisterServer` 注册,安装器不直接写)。
- 只支持 x64 Windows;32 位与 ARM64 提示后退出。
- 用户数据 `%LOCALAPPDATA%\PYIME` 安装、升级、卸载都不动。
- 版本号沿用 0.1.15(未发布;用户要求本次改动完成后再发)。测试包用版本串 `0.0.0-tsfcheck`。push/打 tag/发布前必须先问用户。
- bat 纯 ASCII;ps1 UTF-8 BOM + CRLF;注释、提交信息简体中文;提交信息末尾加 `Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>`。

## 文件结构
| 文件 | 动作 | 职责 |
|---|---|---|
| `installer/nsis/StdUtils/{Include/StdUtils.nsh,Plugins/Release_Unicode/StdUtils.dll,LGPL.txt,LGPL_EXCEPTION.txt}` | 新建(复制) | 提供 `${StdUtils.ExecShellAsUser}`;本机 `/usr/share/nsis/Plugins` 无此插件 |
| `installer/pyime.nsi` | 重写 | 独立安装/升级/迁移/卸载 |
| `installer/NOTICE.md` | 改 | PIME 平台与补丁 DLL 两条改为"收编的 TSF 前端" |
| `.gitignore` | 改 | 忽略 `installer/pyime-server.exe`、`installer/licenses/` |
| `scripts/package.sh` | 改 | 编 `pyime-server.exe` 与两个 DLL、依赖检查、收集许可证、makensis |
| `scripts/windows/user-reload.ps1` | 重写 | 托盘菜单不可用时的备用脚本改为停启 pyime-server.exe |
| `test-kit/`(由 `pime/test-kit/` `git mv`)+ `test-kit/tsf.txt` | 移动 + 新建 | 实机验证清单 |
| Task 5 删除清单 | 删(须用户确认) | PIME 遗留 |
| `CLAUDE.md` | 改 | PIME 段落改写 |

---

### Task 1: 安装器重写(含 StdUtils、NOTICE、.gitignore)

**Files:**
- Create: `installer/nsis/StdUtils/Include/StdUtils.nsh`、`installer/nsis/StdUtils/Plugins/Release_Unicode/StdUtils.dll`、`installer/nsis/StdUtils/LGPL.txt`、`installer/nsis/StdUtils/LGPL_EXCEPTION.txt`
- Modify: `installer/pyime.nsi`(整体替换,原 1-109 行)、`installer/NOTICE.md:7-8`、`.gitignore`

**Interfaces:**
- Consumes: `../target/tsf/x64/PYIME.dll`、`../target/tsf/x86/PYIME.dll`(计划 A);`installer/pyime-server.exe`(Task 2 的 package.sh 复制;本任务验证时手工复制);`pyime-server.exe --quit/--supervise`(计划 B)。
- Produces: `makensis -DVERSION=<版本> installer/pyime.nsi` → `installer/PYIME-<版本>-setup.exe`;安装器要求 `installer/licenses/` 目录存在(Task 2 生成)。

- [ ] **Step 1: 收编 StdUtils 插件**

```bash
S=/tmp/PIME-pyime/installer/StdUtils.2015-11-16
[ -d "$S" ] || { git clone -q --branch v1.3.0-stable https://github.com/EasyIME/PIME.git /tmp/PIME-pyime; }
mkdir -p installer/nsis/StdUtils/Include installer/nsis/StdUtils/Plugins/Release_Unicode
cp "$S/Include/StdUtils.nsh" installer/nsis/StdUtils/Include/
cp "$S/Plugins/Release_Unicode/StdUtils.dll" installer/nsis/StdUtils/Plugins/Release_Unicode/
cp "$S/LGPL.txt" "$S/LGPL_EXCEPTION.txt" installer/nsis/StdUtils/
grep -n "_StdUtils_ExecShlUser" installer/nsis/StdUtils/Include/StdUtils.nsh
```
Expected:`!define StdUtils.ExecShellAsUser  '!insertmacro _StdUtils_ExecShlUser'` 一行与 `!macro _StdUtils_ExecShlUser out file verb args` 一行。

- [ ] **Step 2: 写新 `installer/pyime.nsi` 前半(头部与宏)**

用以下内容整体覆盖 `installer/pyime.nsi`:

```nsis
; PYIME 安装包:自带 TSF 前端(x64\、x86\PYIME.dll)与 pyime-server.exe,不需要官方 PIME。
; 装过旧版"PIME 附加包"的,安装时先静默运行其卸载程序(删 backends.json 条目、还原 PIME 原版 DLL)。
Unicode true
!include "LogicLib.nsh"
!include "x64.nsh"
!addincludedir "nsis\StdUtils\Include"
!addplugindir /x86-unicode "nsis\StdUtils\Plugins\Release_Unicode"
!include "StdUtils.nsh" ; ${StdUtils.ExecShellAsUser}:从提权安装器以普通用户身份启动
!ifndef VERSION
  !define VERSION "0.0.0"
!endif
!define UNINST_KEY "Software\Microsoft\Windows\CurrentVersion\Uninstall\PYIME"
!define RUN_KEY "Software\Microsoft\Windows\CurrentVersion\Run"

Name "PYIME ${VERSION}"
OutFile "PYIME-${VERSION}-setup.exe"
RequestExecutionLevel admin
InstallDir "$PROGRAMFILES64\PYIME"
ShowInstDetails show
ShowUninstDetails show
Page instfiles
UninstPage uninstConfirm
UninstPage instfiles

Var Migrated

; 占用中的文件(已加载 DLL 的程序、未退出的进程)不能覆盖或删除,但可以改名:
; 改名为 .old-<毫秒数> 挪开,下次重启时删除。无需重启/注销。
!macro RetireFile PATH
  ${If} ${FileExists} "${PATH}"
    System::Call 'kernel32::GetTickCount()i.r9'
    Rename "${PATH}" "${PATH}.old-$9"
    Delete /REBOOTOK "${PATH}.old-$9"
  ${EndIf}
!macroend

; 清掉上次挪开的旧文件(仍被占用的删不掉,留着),挪开当前文件,放入新文件
!macro SwapFile DIR NAME SRC
  Delete "${DIR}\${NAME}.old-*"
  !insertmacro RetireFile "${DIR}\${NAME}"
  SetOutPath "${DIR}"
  File "/oname=${NAME}" "${SRC}"
!macroend

; 先请监护进程连同 worker 退出(≤3s),再按名强杀兜底(含其他用户会话里的实例)
!macro StopServer
  ${If} ${FileExists} "$INSTDIR\pyime-server.exe"
    ExecWait '"$INSTDIR\pyime-server.exe" --quit'
  ${EndIf}
  nsExec::Exec '"$SYSDIR\taskkill.exe" /F /IM pyime-server.exe'
  Pop $0
!macroend
```

- [ ] **Step 3: 追加 `.onInit` 与迁移函数**

追加到 `installer/pyime.nsi` 末尾:

```nsis
Function .onInit
  ; RunningX64 在 ARM64 上对 32 位安装器返回假;两者都判,提示更明确
  ${IfNot} ${RunningX64}
  ${OrIf} ${IsNativeARM64}
    MessageBox MB_ICONSTOP "PYIME 只支持 x64 版 Windows(不支持 32 位与 ARM64)。" /SD IDOK
    Abort
  ${EndIf}
  SetRegView 64
FunctionEnd

; 旧版附加包(0.1.x,装在 PIME 目录下的 pyime\)由 32 位安装器写入 32 位注册表视图,
; 键名与新版同为 PYIME。必须在写新卸载登记之前读出 PimeDir。
; 旧卸载程序(旧 pyime.nsi:93-108)已含:PIMELauncher /quit、--unregister、还原 .orig、重注册 PIME DLL、删 pyime\。
; `_?=` 让它原地运行(不复制到临时目录),ExecWait 才能等到它结束;它删不掉自身,随后由这里删。
Function MigrateAddon
  StrCpy $Migrated "0"
  SetRegView 32
  ReadRegStr $1 HKLM "${UNINST_KEY}" "PimeDir"
  SetRegView 64
  ${If} $1 != ""
  ${AndIf} ${FileExists} "$1\pyime\uninstall.exe"
    DetailPrint "发现旧版 PYIME 附加包:$1\pyime,先卸载"
    ExecWait '"$1\pyime\uninstall.exe" /S _?=$1\pyime' $0
    DetailPrint "旧版卸载程序返回 $0"
    Delete "$1\pyime\uninstall.exe"
    RMDir "$1\pyime"
    SetRegView 32
    DeleteRegKey HKLM "${UNINST_KEY}" ; 旧卸载程序正常会删,失败时兜底
    SetRegView 64
    StrCpy $Migrated "1"
  ${EndIf}
FunctionEnd
```

- [ ] **Step 4: 追加安装段**

追加到 `installer/pyime.nsi` 末尾:

```nsis
Section "安装"
  Call MigrateAddon
  !insertmacro StopServer
  !insertmacro SwapFile "$INSTDIR" "pyime-server.exe" "pyime-server.exe"
  SetOutPath "$INSTDIR"
  File "pyime.dict"
  File "pyime.en"
  File "symbols.txt"
  File "NOTICE.md"
  File /r "licenses" ; GPL-3.0 与收编 TSF 部分的 LGPL 等全文(scripts/package.sh 生成)
  ; 托盘中/英模式图标:pyime-server.exe 从同目录 icons\ 取(tray.rs)
  SetOutPath "$INSTDIR\icons"
  File "icons\chi_full_capsoff.ico"
  File "icons\eng_full_capsoff.ico"
  ; 可选语法模型:安装程序旁边有 pyime.gram 就一并装上(体积大,单独下载)
  ${If} ${FileExists} "$EXEDIR\pyime.gram"
    CopyFiles /SILENT "$EXEDIR\pyime.gram" "$INSTDIR\pyime.gram"
  ${EndIf}
  !insertmacro SwapFile "$INSTDIR\x64" "PYIME.dll" "..\target\tsf\x64\PYIME.dll"
  !insertmacro SwapFile "$INSTDIR\x86" "PYIME.dll" "..\target\tsf\x86\PYIME.dll"
  ; 32 位安装器:Sysnative 指向真正的 64 位 System32,SysWOW64 是 32 位 regsvr32
  ExecWait '"$WINDIR\Sysnative\regsvr32.exe" /s "$INSTDIR\x64\PYIME.dll"' $0
  ExecWait '"$WINDIR\SysWOW64\regsvr32.exe" /s "$INSTDIR\x86\PYIME.dll"' $1
  ${If} $0 <> 0
  ${OrIf} $1 <> 0
    MessageBox MB_ICONSTOP "注册输入法 DLL 失败(x64 返回 $0,x86 返回 $1)。" /SD IDOK
    Abort
  ${EndIf}
  SetRebootFlag false ; 挪开的旧文件留待下次重启删除即可,不提示重启
  WriteUninstaller "$INSTDIR\uninstall.exe"
  WriteRegStr HKLM "${UNINST_KEY}" "DisplayName" "PYIME ${VERSION}"
  WriteRegStr HKLM "${UNINST_KEY}" "UninstallString" '"$INSTDIR\uninstall.exe"'
  WriteRegStr HKLM "${UNINST_KEY}" "DisplayVersion" "${VERSION}"
  WriteRegStr HKLM "${UNINST_KEY}" "InstallLocation" "$INSTDIR"
  WriteRegStr HKLM "${RUN_KEY}" "PYIME" '"$INSTDIR\pyime-server.exe" --supervise'
  ; 不能直接 Exec:安装器是提权的,服务端会以管理员身份运行(菜单开的记事本也跟着提权)
  ${StdUtils.ExecShellAsUser} $0 "$INSTDIR\pyime-server.exe" "open" "--supervise"
  DetailPrint "启动 pyime-server.exe --supervise:$0"
  ${If} $Migrated == "1"
    MessageBox MB_ICONINFORMATION "已移除旧版 PYIME 附加包,并还原了 PIME 的原版 DLL。$\n如果你还在用 PIME 的其他输入法,需注销再登录一次才能恢复。" /SD IDOK
  ${EndIf}
SectionEnd
```

- [ ] **Step 5: 追加卸载段**

追加到 `installer/pyime.nsi` 末尾:

```nsis
Function un.onInit
  SetRegView 64
FunctionEnd

; 顺序:先删 Run 键并停监护进程(之后不会再有人拉起 worker),再反注册、删文件
Section "Uninstall"
  DeleteRegValue HKLM "${RUN_KEY}" "PYIME"
  !insertmacro StopServer
  ExecWait '"$WINDIR\Sysnative\regsvr32.exe" /u /s "$INSTDIR\x64\PYIME.dll"'
  ExecWait '"$WINDIR\SysWOW64\regsvr32.exe" /u /s "$INSTDIR\x86\PYIME.dll"'
  Delete "$INSTDIR\x64\PYIME.dll.old-*"
  Delete "$INSTDIR\x86\PYIME.dll.old-*"
  !insertmacro RetireFile "$INSTDIR\x64\PYIME.dll"
  !insertmacro RetireFile "$INSTDIR\x86\PYIME.dll"
  Delete "$INSTDIR\pyime-server.exe.old-*"
  !insertmacro RetireFile "$INSTDIR\pyime-server.exe"
  Delete "$INSTDIR\pyime.dict"
  Delete "$INSTDIR\pyime.en"
  Delete "$INSTDIR\symbols.txt"
  Delete "$INSTDIR\pyime.gram"
  Delete "$INSTDIR\NOTICE.md"
  RMDir /r "$INSTDIR\icons"
  RMDir /r "$INSTDIR\licenses"
  Delete "$INSTDIR\uninstall.exe"
  RMDir /REBOOTOK "$INSTDIR\x64"
  RMDir /REBOOTOK "$INSTDIR\x86"
  RMDir /REBOOTOK "$INSTDIR"
  SetRebootFlag false
  DeleteRegKey HKLM "${UNINST_KEY}"
SectionEnd
```

- [ ] **Step 6: 改 `installer/NOTICE.md` 第 7–8 行**

把这两行:
```
- 运行平台:PIME 1.3.0-stable(LGPL 2.0 等(各目录不同,见其 LICENSE.txt)),https://github.com/EasyIME/PIME ,需另行安装。
- 候选窗 DLL `PIMETextService.dll`(x64/x86,安装时替换 PIME 自带的同名文件,卸载时还原):由 PIME 1.3.0-stable 的 PIMETextService(LGPL 2.0)、libIME2(LGPL 2.1)与 jsoncpp(公有领域/MIT)修改后编译。修改内容:候选按自身宽度排版、序号与间距参照微信输入法、MSVC 专有写法改为可用 mingw-w64 编译。修改的源码以补丁形式随 PYIME 源码提供(`pime/patches/`、`pime/tsf_compat.h`),`scripts/build-pime-dll.sh` 可从上游源码复现构建。
```
替换为:
```
- 输入法前端 `x64\PYIME.dll`、`x86\PYIME.dll`:由 PIME 1.3.0-stable(https://github.com/EasyIME/PIME )的 PIMETextService(Library GPL v2 or later / LGPL 2.0)、libIME2(LGPL 2.1)与 jsoncpp(公有领域/MIT)修改后编译,按原许可证分发,许可证全文见 `licenses\`。修改内容:不再依赖 PIME 平台、直连 pyime-server.exe、超时放行、候选按自身宽度排版、序号与间距参照微信输入法、可用 mingw-w64 编译。完整修改后源码在 PYIME 源码仓库的 `tsf/` 目录(https://github.com/kilobitcy/PYIME ),`scripts/build-tsf.sh` 可复现构建;如需源码副本也可在该仓库提 issue 索取。`tsf/tsf_compat.h` 取自 windows-rs(MIT/Apache-2.0)。
- 安装程序使用 NSIS 插件 StdUtils(LGPL 2.1 + 插件例外,https://github.com/lordmulder/stdutils ),仅用于以普通用户身份启动 pyime-server.exe。
```

- [ ] **Step 7: `.gitignore` 追加两行**

在 `.gitignore` 的 `installer/server.exe` 行后加:
```
installer/pyime-server.exe
installer/licenses/
```

- [ ] **Step 8: 编译验证(用临时 licenses 与已构建产物)**

```bash
ls target/tsf/x64/PYIME.dll target/tsf/x86/PYIME.dll target/x86_64-pc-windows-gnu/release/pyime-server.exe
cp target/x86_64-pc-windows-gnu/release/pyime-server.exe installer/pyime-server.exe
mkdir -p installer/licenses && cp LICENSE installer/licenses/GPL-3.0.txt
makensis -V2 -DVERSION=0.0.0-nsitest installer/pyime.nsi; echo exit=$?
7z l installer/PYIME-0.0.0-nsitest-setup.exe | grep -E 'PYIME.dll|pyime-server.exe|pyime.dict|icons|licenses|NOTICE'
```
Expected:`exit=0`;7z 列出 `pyime-server.exe`、`pyime.dict`、`x64/PYIME.dll`、`x86/PYIME.dll`、`NOTICE.md`、`licenses/GPL-3.0.txt`、`icons/chi_full_capsoff.ico`、`icons/eng_full_capsoff.ico`(另有 `$PLUGINSDIR/StdUtils.dll`、`uninstall.exe`)。本脚本已在占位产物上试编译通过(2026-10-23,makensis v3.09-4)。随后 `rm installer/PYIME-0.0.0-nsitest-setup.exe`。若前置产物不存在,先完成计划 A/B,不得跳过本步。

- [ ] **Step 9: 提交**

```bash
git add installer/pyime.nsi installer/NOTICE.md installer/nsis .gitignore
git commit -m "安装器:独立安装到 Program Files\PYIME,注册自带 TSF DLL,迁移旧附加包

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
git show --stat HEAD | head -12
```

---

### Task 2: `scripts/package.sh` 改为打包自带 TSF 前端

**Files:**
- Modify: `scripts/package.sh`(整体替换,原 1-20 行)

**Interfaces:**
- Consumes: `scripts/build-tsf.sh`(计划 A,无参数,产出 `target/tsf/{x64,x86}/PYIME.dll`);`target/x86_64-pc-windows-gnu/release/pyime-server.exe`(计划 B);`tsf/` 下各许可证全文(计划 A,文件名以 `LICENSE` 或 `COPYING` 开头);`installer/nsis/StdUtils/LGPL*.txt`(Task 1)。
- Produces: `scripts/package.sh <版本>` → `installer/PYIME-<版本>-setup.exe`;中间产物 `installer/pyime-server.exe`、`installer/licenses/`。

- [ ] **Step 1: 整体替换 `scripts/package.sh`**

```bash
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
done < <(find tsf -type f \( -iname 'LICENSE*' -o -iname 'COPYING*' \) | sort)
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
```

- [ ] **Step 2: 出测试包并验证**

```bash
scripts/package.sh 0.0.0-tsfcheck > /tmp/pkg-tsfcheck.log 2>&1; echo exit=$?
grep -E '^test result' /tmp/pkg-tsfcheck.log | grep -vc ' 0 failed'
ls -la installer/PYIME-0.0.0-tsfcheck-setup.exe installer/licenses/
7z l installer/PYIME-0.0.0-tsfcheck-setup.exe | grep -E 'PYIME.dll|pyime-server.exe|pyime.dict|pyime.en|symbols.txt|licenses/|icons/|NOTICE.md'
x86_64-w64-mingw32-objdump -p target/tsf/x64/PYIME.dll | grep -E '\b(DllRegisterServer|DllUnregisterServer|DllGetClassObject|DllCanUnloadNow)\b'
i686-w64-mingw32-objdump -p target/tsf/x86/PYIME.dll | grep -E '\b(DllRegisterServer|DllUnregisterServer|DllGetClassObject|DllCanUnloadNow)\b'
```
Expected:`exit=0`;第二条输出 `0`(所有 `test result` 行都是 `0 failed`);`installer/licenses/` 至少含 `GPL-3.0.txt`、tsf 的 LGPL 文件、`StdUtils-LGPL-2.1.txt`;7z 列出 `pyime-server.exe`、`x64/PYIME.dll`、`x86/PYIME.dll`(大小非 0)、`pyime.dict`、`pyime.en`、`symbols.txt`、`licenses/…`、`icons/…` 两个、`NOTICE.md`;两个 objdump 各列出 4 个导出名。

- [ ] **Step 3: 提交**

```bash
git add scripts/package.sh
git commit -m "打包:改为编 pyime-server.exe 与自带 TSF DLL,随包附许可证全文

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
git show --stat HEAD | head -8
```

---

### Task 3: 备用脚本改为停启 pyime-server.exe

设计文档未提及,但不改就会失效:`scripts/windows/user-reload.ps1` 靠 PIME 注册表/`backends.json` 找目录、靠 `PIMELauncher /quit` 停输入法(原第 11-41 行),新安装形态下找不到 PIME 会直接报错。

**Files:**
- Modify: `scripts/windows/user-reload.ps1`(整体替换;保持 UTF-8 BOM + CRLF)。`edit-user.bat`、`reload-user.bat` 不变。

**Interfaces:**
- Consumes: 卸载登记 `HKLM\Software\Microsoft\Windows\CurrentVersion\Uninstall\PYIME` 的 `InstallLocation`(Task 1 写入 64 位视图);`pyime-server.exe --quit`、`--supervise`(计划 B)。
- Produces: 行为同原脚本:停输入法 → 备份 → (可选)记事本编辑 → 规整 → 重启。

- [ ] **Step 1: 写新脚本源文本 `/tmp/user-reload.ps1`(LF、无 BOM)**

```powershell
# 让 %LOCALAPPDATA%\PYIME\user.txt 的手工改动生效(Windows 11,无需管理员)。
# 用法:user-reload.ps1 [-Edit]
#   不带参数 = 规整 user.txt 后重启 pyime-server.exe,重新读入
#   -Edit    = 先停输入法,用记事本打开 user.txt,改完按回车后再规整、重启(推荐)
# 每次运行先把 user.txt 备份为 user.txt.bak-时间戳;非 UTF-8 时中止,输入法保持停止,改好后重跑即可。
# 背景:pyime-server.exe 只在启动时读 user.txt,且每次选词都把内存里的学习数据整份写回 user.txt。
#       输入法运行中改文件,改完到重启之间只要选过一次词,改动就会被覆盖;-Edit 先停输入法可避免。
param([switch]$Edit)
$ErrorActionPreference = 'Stop'

# 安装目录:卸载登记的 InstallLocation(64 位视图),缺省 C:\Program Files\PYIME
function Find-Pyime {
    $dirs = @()
    try {
        $v = (Get-Item 'HKLM:\SOFTWARE\Microsoft\Windows\CurrentVersion\Uninstall\PYIME' -ErrorAction Stop).GetValue('InstallLocation')
        if ($v) { $dirs += $v }
    } catch {}
    $dirs += (Join-Path $env:ProgramFiles 'PYIME')
    foreach ($d in $dirs) { if (Test-Path (Join-Path $d 'pyime-server.exe')) { return $d } }
    throw '未找到 PYIME(pyime-server.exe)。请先安装 PYIME。'
}

# 请监护进程连同 worker 退出,并确认本目录的 pyime-server.exe 都已退出(未退出则结束它)
function Stop-Ime([string]$dir) {
    $exe = Join-Path $dir 'pyime-server.exe'
    Start-Process $exe -ArgumentList '--quit' -Wait
    for ($i = 0; $i -lt 20; $i++) {
        $left = @(Get-Process pyime-server -ErrorAction SilentlyContinue | Where-Object { $_.Path -eq $exe })
        if ($left.Count -eq 0) { break }
        Start-Sleep -Milliseconds 250
    }
    Get-Process pyime-server -ErrorAction SilentlyContinue | Where-Object { $_.Path -eq $exe } | Stop-Process -Force
    Write-Host '输入法已停止'
}

function Start-Ime([string]$dir) {
    Start-Process (Join-Path $dir 'pyime-server.exe') -ArgumentList '--supervise'
    Write-Host '输入法已重新启动,下次按键时自动重连并读入 user.txt'
}

# 校验并规整 user.txt:必须是 UTF-8;去掉 BOM、CRLF 改 LF(启动时的导入不认 BOM 与 \r)。
# 非 UTF-8 时 pyime-server.exe 读不出,会保护着不写回,但改动也不会生效,所以直接中止。
function Normalize-User([string]$path) {
    $bytes = [IO.File]::ReadAllBytes($path)
    $utf8 = New-Object Text.UTF8Encoding($false, $true)  # 无 BOM、遇非法字节抛异常
    try { $text = $utf8.GetString($bytes) }
    catch { throw "user.txt 不是 UTF-8 编码,已中止且未重启。请用记事本「另存为」选 UTF-8 后重试。备份见同目录 user.txt.bak-*" }
    $fixed = $text.TrimStart([char]0xFEFF) -replace "`r`n", "`n" -replace "`r", "`n"
    if ($fixed -ne $text) {
        [IO.File]::WriteAllText($path, $fixed, $utf8)
        Write-Host '已规整 user.txt(去 BOM / 换行改 LF)'
    }
    $n = @($fixed -split "`n" | Where-Object { $_ -match "^[^\t]+\t[^\t]+\t\d+$|^@ng\t[^\t]+\t[^\t]+\t\d+$" }).Count
    Write-Host "user.txt 有效行数:$n"
}

$user = Join-Path $env:LOCALAPPDATA 'PYIME\user.txt'
if (-not (Test-Path $user)) { throw "未找到 $user(还没选过词?)" }
$dir = Find-Pyime

Stop-Ime $dir
$bak = "$user.bak-$(Get-Date -Format yyyyMMddHHmmss)"
Copy-Item -LiteralPath $user -Destination $bak
Write-Host "已备份到 $bak"
if ($Edit) {
    # Win11 记事本是多标签的,已开着时新文件进现有窗口、进程立即返回,故不用 -Wait,改为等回车
    Start-Process notepad.exe -ArgumentList "`"$user`""
    Read-Host '请在记事本里修改并保存(UTF-8),然后回到这里按回车继续' | Out-Null
}
Normalize-User $user
Start-Ime $dir
```

- [ ] **Step 2: 转成 UTF-8 BOM + CRLF 覆盖原文件并核对**

```bash
python3 -c "t=open('/tmp/user-reload.ps1',encoding='utf-8').read().replace('\r\n','\n'); open('scripts/windows/user-reload.ps1','w',encoding='utf-8-sig',newline='\r\n').write(t)"
file scripts/windows/user-reload.ps1
grep -c 'PIME' scripts/windows/user-reload.ps1
```
Expected:`Unicode text, UTF-8 (with BOM) text, with CRLF line terminators`;`grep -c` 输出 `0`(`PYIME` 不含子串 `PIME`,不会误计)。本机无 pwsh,不做语法检查;实机由清单第 29、30 项验证。

- [ ] **Step 3: 提交**

```bash
git add scripts/windows/user-reload.ps1
git commit -m "脚本:user.txt 生效脚本改为停启 pyime-server.exe,不再依赖 PIME

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
git show --stat HEAD | head -6
```

---

### Task 4: 实机测试包与清单(实施顺序第 5 步)

**Files:**
- Move: `pime/test-kit/*` → `test-kit/`(`git mv`;`test-kit/tsf-dev.bat` 已由计划 A 创建,目录可能已存在)
- Create: `test-kit/tsf.txt`(UTF-8 BOM + CRLF,与原 `menucheck.txt` 同格式)

**Interfaces:**
- Consumes: Task 2 产出的 `installer/PYIME-0.0.0-tsfcheck-setup.exe`。
- Produces: 交给用户的实机测试包与清单;用户贴回结果后,不通过项转成新任务修复,全部通过才进入 Task 4。

- [ ] **Step 1: 移动测试包目录**

```bash
mkdir -p test-kit
git mv pime/test-kit/1-baseline.bat pime/test-kit/2-patched.bat pime/test-kit/3-restore.bat \
  pime/test-kit/README.txt pime/test-kit/menucheck.txt pime/test-kit/pime-dll-test.ps1 test-kit/
git status --short test-kit pime
```
Expected:6 行 `R  pime/test-kit/… -> test-kit/…`;`pime/test-kit` 目录不再存在。

- [ ] **Step 2: 写清单源文本 `/tmp/tsf.txt`(LF、无 BOM,下一步转换)**

把下面整段原样写入 `/tmp/tsf.txt`:

```text
PYIME 0.0.0-tsfcheck 实机验证清单(自带 TSF 前端,不再需要 PIME)

每项记"通过 / 不通过 + 现象"。不通过的附上:
  %LOCALAPPDATA%\PYIME\log.txt(后端日志)
  %LOCALAPPDATA%\PYIME\tsf.log(输入法前端日志,只记连接/超时/断线,不含按键)
基准:各项表现不差于 0.1.15(PIME 附加包版)。user.txt 等用户数据全程不受影响。

一、从旧附加包升级(你现在的机器就是这种情况,先做这一节)
 1. 双击 PYIME-0.0.0-tsfcheck-setup.exe,同意管理员授权;安装过程无报错,结束时提示"已移除旧版 PYIME 附加包"。
 2. C:\Program Files\PYIME 下有 pyime-server.exe、pyime.dict、pyime.en、symbols.txt、icons\、x64\PYIME.dll、x86\PYIME.dll、licenses\、uninstall.exe。
 3. PIME 目录(C:\Program Files (x86)\PIME)里:pyime\ 文件夹已没有;backends.json 里没有 pyime;x64\、x86\ 下没有 PIMETextService.dll.orig(已还原为原版)。
 4. 设置 → 时间和语言 → 语言和区域 → 中文 → 键盘:能看到 PYIME;旧的 PIME 版 PYIME 条目已不在(如果还在,截图)。
 5. 任务管理器 → 详细信息:有两个 pyime-server.exe(监护进程 + worker),用户名是你自己,不是管理员/SYSTEM;没有黑色控制台窗口。
 6. 控制面板 → 程序和功能:只有一个 PYIME 条目,版本 0.0.0-tsfcheck。

二、各程序里打字(切到 PYIME 后,每个程序打 ceshi 选"测试",再打一句 nihaoshijie 空格上屏)
 7. 记事本(Win11 新版)。
 8. Edge 地址栏与网页输入框。
 9. Chrome 地址栏与网页输入框。
10. Word。
11. 微信聊天输入框。
12. 资源管理器地址栏与右上角搜索框。
13. 开始菜单搜索(按 Win 键直接打字)。
14. Microsoft Store 搜索框(商店应用)。
15. 一个 32 位程序(如 32 位的旧版软件;不确定是不是 32 位,看任务管理器"平台"列)。
16. 以管理员身份运行的记事本。
17. 以上程序里:候选窗外观(绿色高亮、序号、字号)、数字键选词、-/= 翻页、单击 Shift 切中英、中文标点、学过的词靠前,与 0.1.15 一致。

三、托盘图标与菜单
18. 右下角显示"中"字图标;单击 Shift 与左键单击图标都能切换"中"/"英"。
19. 右键菜单:首项"PYIME 0.1.15"(灰色)、编辑用户词库…、重新加载用户词库、打开数据目录,各项功能与 0.1.15 相同。

四、后端异常恢复
20. 任务管理器结束"命令行带 --worker"的那个 pyime-server.exe(详细信息页右键列标题可加"命令行"列),马上在记事本里打字:前一两个键可能直接出英文字母,1-2 秒内恢复出候选;若当时正在打拼音,拼音串与候选窗应消失,不残留。
21. 连续结束 worker 三次,每次都能恢复。
22. 两个 pyime-server.exe 都结束:打字直接出英文字母,程序不卡、不崩;注销再登录后恢复。

五、登录与重启
23. 重启电脑并登录:无黑窗弹出;任务管理器里有两个 pyime-server.exe;直接能打中文。

六、同版本覆盖安装
24. 在记事本开着、PYIME 正在用的情况下,再运行一次同一个安装包:安装成功,无"文件被占用"报错;记事本里继续打字正常(1-2 秒内恢复)。

七、卸载
25. 控制面板卸载 PYIME:无报错;键盘列表里 PYIME 消失;任务管理器里 pyime-server.exe 消失。
26. C:\Program Files\PYIME 已删除(若残留 x64\、x86\ 下的 .old-* 文件,重启后应消失);%LOCALAPPDATA%\PYIME 仍在(用户数据保留)。
27. 卸载后重新安装:恢复正常使用,学过的词还在。

八、全新机器(可选,有另一台没装过 PIME 的 Win11 时做)
28. 不装 PIME,直接装本包:第 2、4、5、7-19 项同样通过。

九、托盘菜单不可用时的备用脚本(随本清单附带的 edit-user.bat、reload-user.bat、user-reload.ps1,放同一文件夹)
29. 双击 reload-user.bat:窗口显示"输入法已停止""已备份到 …""输入法已重新启动";任务管理器里 pyime-server.exe 先消失再出现;打字正常。
30. 双击 edit-user.bat:记事本打开 user.txt;加一行"ceshi<Tab>测试词<Tab>50"保存,回到窗口按回车;之后打 ceshi,"测试词"排在前面。
```

- [ ] **Step 3: 转成 UTF-8 BOM + CRLF 写入 `test-kit/tsf.txt` 并核对**

```bash
python3 -c "import sys; t=open('/tmp/tsf.txt',encoding='utf-8').read().replace('\r\n','\n'); open('test-kit/tsf.txt','w',encoding='utf-8-sig',newline='\r\n').write(t)"
file test-kit/tsf.txt
grep -c '' test-kit/tsf.txt
```
Expected:`Unicode text, UTF-8 (with BOM) text, with CRLF line terminators`;行数与 `/tmp/tsf.txt` 相同(`grep -c '' /tmp/tsf.txt`)。

- [ ] **Step 4: 提交**

```bash
git add test-kit
git commit -m "测试包:test-kit 目录移出 pime/;自带 TSF 前端实机验证清单

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
git show --stat HEAD | head -12
```

- [ ] **Step 5: 交给用户实机验证(人工)**

把 `installer/PYIME-0.0.0-tsfcheck-setup.exe`、`test-kit/tsf.txt` 与 `scripts/windows/` 下三个文件(`edit-user.bat`、`reload-user.bat`、`user-reload.ps1`,Task 3 已改)交给用户,请其按清单逐项填写并贴回结果与两个日志。不通过项逐条用 superpowers:systematic-debugging 定位,修复后重出测试包、只复测相关项。**全部通过前不得进入 Task 5。**

---

### Task 5: 删除 PIME 遗留并改写 CLAUDE.md(实施顺序第 6 步)

**前置:Task 4 实机清单全部通过。删除前必须把下面"删除清单"原样列给用户,得到用户明确确认后才执行 Step 2 起的任何删除;用户删减清单就按删减后的执行。**

**Files:**
- Delete:见 Step 1 清单
- Modify: `crates/pyime-backend/src/main.rs`、`crates/pyime-backend/src/ime.rs:264-265`(注释)、`.gitignore`、`CLAUDE.md`

**Interfaces:**
- Consumes: 计划 B 完成后的 `main.rs`(已有 `--supervise`/`--worker`/`--quit` 分支;无参数分支仍为 stdin 循环)。
- Produces: `pyime-server.exe` 只接受 `--supervise`/`--worker`/`--quit`,无参数或未知参数打印用法、退出码 2。

- [ ] **Step 1: 列出删除清单并请用户确认(人工)**

```bash
git ls-files crates/pyime-backend/src/protocol.rs crates/pyime-backend/src/register.rs pime \
  scripts/build-pime-dll.sh scripts/package-pime-zip.sh installer/zip-pime installer/input_methods \
  test-kit/1-baseline.bat test-kit/2-patched.bat test-kit/3-restore.bat test-kit/pime-dll-test.ps1 \
  test-kit/README.txt test-kit/menucheck.txt
```
把输出连同下面说明发给用户,等待确认:
- `protocol.rs`、`register.rs`:PIME 行协议与 `backends.json` 登记,新架构不用。
- `main.rs` 中:`mod protocol;`、`mod register;`、`edit_backends`、`--register/--unregister` 分支、`disinherit_std_handles`、无参数时的 stdin 循环(clientId 多路复用与 `close` 处理)。
- `pime/`(剩余的补丁与说明;`tsf_compat.h` 若计划 A 已移入 `tsf/` 则不在此列)、`scripts/build-pime-dll.sh`:上游克隆 + 打补丁的构建,已由 `tsf/` + `scripts/build-tsf.sh` 取代。
- `scripts/package-pime-zip.sh`、`installer/zip-pime/`:PIME 附加包 zip 版。
- `installer/input_methods/`:PIME 的 `ime.json`,新安装器不再打包。
- `test-kit/` 中 PIME DLL 替换脚本与旧清单(`1-baseline.bat`、`2-patched.bat`、`3-restore.bat`、`pime-dll-test.ps1`、`README.txt`、`menucheck.txt`);保留 `tsf.txt`、`tsf-dev.bat`。
- `.gitignore` 中 `installer/server.exe` 一行。

- [ ] **Step 2: 删除文件**

```bash
git rm -q crates/pyime-backend/src/protocol.rs crates/pyime-backend/src/register.rs
git rm -rq pime scripts/build-pime-dll.sh scripts/package-pime-zip.sh installer/zip-pime installer/input_methods
git rm -q test-kit/1-baseline.bat test-kit/2-patched.bat test-kit/3-restore.bat test-kit/pime-dll-test.ps1 test-kit/README.txt test-kit/menucheck.txt
sed -i '/^installer\/server\.exe$/d' .gitignore
git status --short
```
Expected:只有 `D ` 行与 `.gitignore` 的 `M`。

- [ ] **Step 3: 改 `main.rs`**

1. 删除 `mod protocol;`、`mod register;` 两行,删除函数 `edit_backends`(原第 12-19 行)与 `disinherit_std_handles`(原第 21-43 行,含 `#[cfg(windows)]` 属性与文档注释)。
2. 删除 `main` 开头的 `if let [_, flag, dir] = args.as_slice() { … }` 整块(`--register`/`--unregister`)。
3. 删除无参数路径:从 `let stdin = io::stdin();` 起,到 `log::line("stdin 已关闭,退出");` 止(含 `sessions` HashMap、`disinherit_std_handles()` 调用、`for line in stdin.lock().lines()` 循环)。
4. 在计划 B 的参数分派 `match` 里,把兜底分支改为:

```rust
        _ => {
            eprintln!("用法: pyime-server.exe --supervise | --worker | --quit");
            std::process::exit(2);
        }
```
5. 删掉因此变得无用的 `use`(如 `std::collections::HashMap`、`std::io::{self, BufRead, Write}` 中不再用到的项),以编译器警告为准,不动其他代码。

- [ ] **Step 4: 改 `ime.rs` 注释(原第 264-265 行)**

把:
```rust
    /// 启动外部程序,不等待。三路 stdio 置空,让子进程的标准句柄指向 NUL;
    /// 与 PIME 通信的管道句柄已在 `main` 启动时设为不可继承(见 `disinherit_std_handles`)。测试里只记录。
```
改为:
```rust
    /// 启动外部程序,不等待。三路 stdio 置空,让子进程的标准句柄指向 NUL。测试里只记录。
```

- [ ] **Step 5: 编译、测试、lint**

```bash
cargo build -p pyime-backend 2>&1 | tail -3
cargo build --release --target x86_64-pc-windows-gnu -p pyime-backend 2>&1 | grep -E 'warning|error|Finished'
cargo test --workspace 2>&1 | grep -E '^test result|FAILED|panicked'
cargo clippy --workspace 2>&1 | grep -E '^(warning|error)' | sort | uniq -c
grep -rn 'protocol::\|register::\|disinherit_std_handles\|backends.json' crates/ || echo 无残留
```
Expected:两次构建 `Finished` 且无 warning;所有 `test result: ok`、无 `FAILED`;clippy 无新增告警(与改动前 `git stash` 对照);最后一行输出 `无残留`。

- [ ] **Step 6: 改写 CLAUDE.md 的 PIME 段落**

把下面脚本存为 `/tmp/claude_md_edit.py` 并运行。每条按"行首前缀"定位整行替换(前缀须恰好命中 1 行,否则报错中止),少数按子串替换;第 59、60 行(v0.1.0 历史验证、PIME 自带 Rime)与第 88 行(图标取自 PIME)保持不变。

```python
import sys
p = 'CLAUDE.md'
lines = open(p, encoding='utf-8').read().split('\n')

def whole(prefix, new):
    hit = [i for i, l in enumerate(lines) if l.startswith(prefix)]
    if len(hit) != 1:
        sys.exit(f'前缀命中 {len(hit)} 行: {prefix}')
    lines[hit[0]:hit[0] + 1] = new.split('\n')

def sub(old, new):
    hit = [i for i, l in enumerate(lines) if old in l]
    if len(hit) != 1:
        sys.exit(f'子串命中 {len(hit)} 行: {old}')
    lines[hit[0]] = lines[hit[0]].replace(old, new)

whole('- Windows 前端:复用 PIME 1.3.0-stable',
      '- Windows 前端:自带 TSF 前端 `PYIME.dll`(x64/x86,C++,收编自 PIME 1.3.0-stable 的 libIME2 + PIMETextService,源码在 `tsf/`),经命名管道 `\\\\.\\pipe\\<用户名>\\PYIME` 直连 Rust 服务进程 `pyime-server.exe`;一条消息一个 JSON,字段沿用 PIME 协议。TSF 层长期保持 C++(设计 `docs/superpowers/specs/2026-10-23-tsf-frontend-design.md`)。')
whole('- 安装形态:附加安装包',
      '- 安装形态:独立安装包 `PYIME-<版本>-setup.exe`,不需要官方 PIME;装到 `C:\\Program Files\\PYIME`,`regsvr32` 注册 `x64\\`、`x86\\PYIME.dll`,HKLM Run `PYIME` = `pyime-server.exe --supervise`,以非提权身份启动(NSIS 插件 StdUtils `ExecShellAsUser`,在 `installer/nsis/StdUtils/`)。占用中的 DLL/exe 靠改名替换,无需重启。检测到旧版 PIME 附加包时先静默运行其卸载程序(还原 PIME 原版 DLL)。只支持 x64 Windows(32 位、ARM64 拒装)。')
whole('- 补丁版 PIME DLL:',
      '- TSF 前端构建:`scripts/build-tsf.sh` 用 mingw 交叉编译 `tsf/` 到 `target/tsf/{x64,x86}/PYIME.dll`;`tsf/tsf_compat.h` 补 mingw 缺的 TSF 接口(取自 windows-rs)。实机验证清单与开发脚本在 `test-kit/`(`tsf.txt`、`tsf-dev.bat`)。')
whole('- `crates/pyime-backend`:PIME 后端',
      '- `crates/pyime-backend`:服务进程 `pyime-server.exe`(`--supervise` 监护并重启 worker,`supervise.rs`;`--worker` 管道服务端 `pipe.rs` + `session`/`ime`/`tray`/`log`;`--quit` 让监护进程连同 worker 退出);词典 `pyime.dict`、英文词表 `pyime.en`、符号表 `symbols.txt`(自带 `installer/symbols.txt`)放在 `pyime-server.exe` 同目录,用户数据 `%LOCALAPPDATA%\\PYIME\\user.txt`,用户英文词表 `en.txt`、回车学习计数 `en_learned.txt` 同目录。')
whole('#   cp target/x86_64-pc-windows-gnu/release/pyime-backend.exe installer/server.exe',
      '#   scripts/build-tsf.sh\n#   cp target/x86_64-pc-windows-gnu/release/pyime-server.exe installer/pyime-server.exe')
sub('PIME 为 LGPL,独立进程调用不波及。',
    '收编的 TSF 前端(`tsf/`:libIME2 LGPL-2.1、PIMETextService LGPL-2.0、jsoncpp 公有领域/MIT)按原许可证分发修改版、不转 GPL;安装包附 `licenses\\` 全文与源码获取方式(`installer/NOTICE.md`)。')
whole('- 引擎 crate 保持独立于 PIME',
      '- 引擎 crate 保持独立于前端:只做拼音→候选,不依赖 PIME 协议与管道。')
whole('- 里程碑:① 先做核心引擎',
      '- 里程碑:① 核心引擎(已完成);② 原"安装包自带 PIME 平台文件"已放弃;③ 自带 TSF 前端、去掉 PIME 依赖(收编 libIME2,C++,见上方设计文档)已完成;此后外观改进(⌄ 展开按钮、深色模式)直接改 `tsf/`。')
sub('PIME 协议只能改字体/字号/每行个数/是否高亮;外观改动改补丁版 DLL 的 `CandidateWindow`',
    '协议字段只能改字体/字号/每行个数/是否高亮;外观改动改 `tsf/` 的 `CandidateWindow`')
sub('(`PIMEClient.cpp:126-137` 的 `updateMessageWindow`',
    '(`tsf/` 下 `PIMEClient.cpp` 的 `updateMessageWindow`')
whole('- 菜单起的子进程(记事本/资源管理器)不继承管道句柄',
      '- 菜单起的子进程(记事本/资源管理器)三路 stdio 置空(`ime.rs` `launch`);服务端经命名管道通信,不再有 stdin/stdout 管道句柄被继承的问题。')
sub('- server.exe 启动时读 `user.txt`', '- pyime-server.exe(worker)启动时读 `user.txt`')
whole('- 托盘菜单与符号候选已按',
      '- 托盘菜单与符号候选已按 `menucheck.txt` 实机验证通过(2026-10-23);自带 TSF 前端已按 `test-kit/tsf.txt` 实机验证通过;0.1.15 待发布(push/打 tag/发布前须先问用户)。')
whole('- `scripts/windows/user-reload.ps1` 未在 Windows 实机运行过',
      '- `scripts/windows/user-reload.ps1` 已改为停启 pyime-server.exe,由 `test-kit/tsf.txt` 第 29、30 项实机验证。')
i = lines.index('### 待查')
assert lines[i + 1].startswith('- PIME JSON 协议能否承载'), lines[i + 1]
del lines[i - 1:i + 2]  # 连同前面的空行
open(p, 'w', encoding='utf-8').write('\n'.join(lines))
print('ok')
```

```bash
python3 /tmp/claude_md_edit.py
grep -n 'PIME' CLAUDE.md
git diff --stat CLAUDE.md
```
Expected:输出 `ok`;剩余含 `PIME` 的行只有:Windows 前端(收编来源)、安装形态(旧附加包迁移)、许可证、第 59-60 行历史验证、里程碑、`PIMEClient.cpp` 注意事项、图标取自 PIME、`menucheck.txt` 待办——均为有意保留。

- [ ] **Step 7: 最终回归并出正式安装包**

```bash
scripts/package.sh 0.1.15 > /tmp/pkg-0.1.15.log 2>&1; echo exit=$?
grep -E '^test result' /tmp/pkg-0.1.15.log | grep -vc ' 0 failed'
ls -la installer/PYIME-0.1.15-setup.exe
```
Expected:`exit=0`;`0`;安装包存在(大小约 13–14 MB,与 `0.0.0-tsfcheck` 相近)。

- [ ] **Step 8: 提交**

```bash
git add -A crates/pyime-backend/src .gitignore CLAUDE.md
git status --short
git commit -m "清理:删除 PIME 遗留(stdin 协议、backends.json 登记、补丁构建、附加包 zip、旧测试脚本);文档改写为自带 TSF 前端

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
git show --stat HEAD | head -30
```
Expected:`git status --short` 在提交前只列出本任务的删除与修改(`.claude/` 等无关未跟踪文件不得加入)。发布 0.1.15(合并 main、push、打 tag、`gh release create`)不在本计划内,须另行先问用户。
