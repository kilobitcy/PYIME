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

Function .onInit
  ; RunningX64 在 ARM64 上对 32 位安装器返回假;两者都判,提示更明确
  ${IfNot} ${RunningX64}
  ${OrIf} ${IsNativeARM64}
    MessageBox MB_ICONSTOP "PYIME 只支持 x64 版 Windows(不支持 32 位与 ARM64)。" /SD IDOK
    Abort
  ${EndIf}
  SetRegView 64
  ; 忽略 /D=:装到普通用户可写的目录时,Run 键与升级/卸载时提权执行的 --quit 都可被劫持
  StrCpy $INSTDIR "$PROGRAMFILES64\PYIME"
FunctionEnd

; $R0 是否位于 Program Files(两种位数)下:是则 $R1 = "1"。只有那里普通用户写不了
Function IsUnderProgramFiles
  StrCpy $R1 "0"
  StrLen $R3 "$PROGRAMFILES32\"
  StrCpy $R4 $R0 $R3
  StrCmp $R4 "$PROGRAMFILES32\" 0 +2 ; StrCmp 不区分大小写
    StrCpy $R1 "1"
  StrLen $R3 "$PROGRAMFILES64\"
  StrCpy $R4 $R0 $R3
  StrCmp $R4 "$PROGRAMFILES64\" 0 +2
    StrCpy $R1 "1"
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
  StrCpy $R0 $1
  Call IsUnderProgramFiles
  ${If} $1 != ""
  ${AndIf} $R1 != "1"
    ; 不在 Program Files 下的旧卸载程序可能被普通用户替换,不以管理员身份执行
    DetailPrint "旧版附加包在 $1\pyime,不在 Program Files 下,跳过自动卸载"
    MessageBox MB_ICONEXCLAMATION "检测到旧版 PYIME 附加包装在 $1\pyime。$\n出于安全考虑不自动卸载,请装完后在控制面板手动卸载它。" /SD IDOK
  ${ElseIf} $1 != ""
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
  SetRebootFlag false ; 挪开的旧文件留待下次重启删除即可,不提示重启
  ; 先写卸载程序与登记,注册失败时也能从控制面板卸载干净
  WriteUninstaller "$INSTDIR\uninstall.exe"
  WriteRegStr HKLM "${UNINST_KEY}" "DisplayName" "PYIME ${VERSION}"
  WriteRegStr HKLM "${UNINST_KEY}" "UninstallString" '"$INSTDIR\uninstall.exe"'
  WriteRegStr HKLM "${UNINST_KEY}" "DisplayVersion" "${VERSION}"
  WriteRegStr HKLM "${UNINST_KEY}" "InstallLocation" "$INSTDIR"
  ; 32 位安装器:Sysnative 指向真正的 64 位 System32,SysWOW64 是 32 位 regsvr32
  ExecWait '"$WINDIR\Sysnative\regsvr32.exe" /s "$INSTDIR\x64\PYIME.dll"' $0
  ExecWait '"$WINDIR\SysWOW64\regsvr32.exe" /s "$INSTDIR\x86\PYIME.dll"' $1
  ${If} $0 <> 0
  ${OrIf} $1 <> 0
    MessageBox MB_ICONSTOP "注册输入法 DLL 失败(x64 返回 $0,x86 返回 $1)。$\n请在控制面板卸载 PYIME 后重新安装。" /SD IDOK
    Abort
  ${EndIf}
  WriteRegStr HKLM "${RUN_KEY}" "PYIME" '"$INSTDIR\pyime-server.exe" --supervise'
  ; 不能直接 Exec:安装器是提权的,服务端会以管理员身份运行(菜单开的记事本也跟着提权)
  ${StdUtils.ExecShellAsUser} $0 "$INSTDIR\pyime-server.exe" "open" "--supervise"
  DetailPrint "启动 pyime-server.exe --supervise:$0"
  ; "fallback" 只出现在关了 UAC 的系统上(没有拆分令牌,登录后 Run 键启动的也是同一身份),照常使用
  ${If} $0 != "ok"
  ${AndIf} $0 != "fallback"
    MessageBox MB_ICONEXCLAMATION "未能以普通用户身份启动 PYIME 服务($0)。$\n请注销再登录一次后使用。" /SD IDOK
  ${EndIf}
  ${If} $Migrated == "1"
    MessageBox MB_ICONINFORMATION "已移除旧版 PYIME 附加包,并还原了 PIME 的原版 DLL。$\n如果你还在用 PIME 的其他输入法,需注销再登录一次才能恢复。" /SD IDOK
  ${EndIf}
SectionEnd

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
