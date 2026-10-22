; PYIME 附加安装包:把 PYIME 后端装进已安装的 PIME,并重新注册 TSF。需先安装官方 PIME。
Unicode true
!include "LogicLib.nsh"
!include "x64.nsh"
!ifndef VERSION
  !define VERSION "0.0.0"
!endif
!define PIME_KEY "Software\PIME"
!define UNINST_KEY "Software\Microsoft\Windows\CurrentVersion\Uninstall\PYIME"

Name "PYIME ${VERSION}"
OutFile "PYIME-${VERSION}-setup.exe"
RequestExecutionLevel admin
InstallDir "$PROGRAMFILES32\PIME"
ShowInstDetails show

; 替换 PIME 的候选窗 DLL(本项目补丁版:候选按自身宽度排版,见 pime/patches)。
; 已运行的程序占用着旧 DLL,无法覆盖但可改名:首次把官方原版改名为 .orig 备份,
; 重复安装时把上一版改名挪开,再放新文件;挪开的旧文件下次重启时删除。无需重启/注销。
!macro SwapDll ARCH
  Delete "$INSTDIR\${ARCH}\PIMETextService.dll.old-*"
  IfFileExists "$INSTDIR\${ARCH}\PIMETextService.dll.orig" +3
    Rename "$INSTDIR\${ARCH}\PIMETextService.dll" "$INSTDIR\${ARCH}\PIMETextService.dll.orig"
    Goto +4
  System::Call 'kernel32::GetTickCount()i.r9'
  Rename "$INSTDIR\${ARCH}\PIMETextService.dll" "$INSTDIR\${ARCH}\PIMETextService.dll.old-$9"
  Delete /REBOOTOK "$INSTDIR\${ARCH}\PIMETextService.dll.old-$9"
  SetOutPath "$INSTDIR\${ARCH}"
  File "/oname=PIMETextService.dll" "..\target\pime\${ARCH}\PIMETextService.dll"
!macroend

; 卸载时还原官方原版 DLL
!macro RestoreDll ARCH
  Delete "$INSTDIR\${ARCH}\PIMETextService.dll.old-*"
  IfFileExists "$INSTDIR\${ARCH}\PIMETextService.dll.orig" 0 +5
    System::Call 'kernel32::GetTickCount()i.r9'
    Rename "$INSTDIR\${ARCH}\PIMETextService.dll" "$INSTDIR\${ARCH}\PIMETextService.dll.old-$9"
    Delete /REBOOTOK "$INSTDIR\${ARCH}\PIMETextService.dll.old-$9"
    Rename "$INSTDIR\${ARCH}\PIMETextService.dll.orig" "$INSTDIR\${ARCH}\PIMETextService.dll"
!macroend

Function .onInit
  ReadRegStr $0 HKLM "${PIME_KEY}" ""
  StrCmp $0 "" +2
  StrCpy $INSTDIR $0
  IfFileExists "$INSTDIR\backends.json" +3
    MessageBox MB_ICONSTOP "未找到 PIME($INSTDIR\backends.json)。请先安装官方 PIME。"
    Abort
FunctionEnd

Section "安装"
  ExecWait '"$INSTDIR\PIMELauncher.exe" /quit'
  SetOutPath "$INSTDIR\pyime"
  File "server.exe"
  File "pyime.dict"
  File "pyime.en"
  File "symbols.txt"
  File "NOTICE.md"
  ; 托盘中/英模式图标(取自 PIME,见 NOTICE.md)
  SetOutPath "$INSTDIR\pyime\icons"
  File "icons\chi_full_capsoff.ico"
  File "icons\eng_full_capsoff.ico"
  SetOutPath "$INSTDIR\pyime"
  ; 可选语法模型:安装程序旁边有 pyime.gram 就一并装上(体积大,单独下载)
  IfFileExists "$EXEDIR\pyime.gram" 0 +2
    CopyFiles /SILENT "$EXEDIR\pyime.gram" "$INSTDIR\pyime\pyime.gram"
  SetOutPath "$INSTDIR\pyime\input_methods\pyime"
  File "input_methods\pyime\ime.json"
  nsExec::ExecToLog '"$INSTDIR\pyime\server.exe" --register "$INSTDIR"'
  Pop $0
  StrCmp $0 "0" +3
    MessageBox MB_ICONSTOP "登记 backends.json 失败(代码 $0)。"
    Abort
  !insertmacro SwapDll x86
  ExecWait '"$SYSDIR\regsvr32.exe" /s "$INSTDIR\x86\PIMETextService.dll"'
  ${If} ${RunningX64}
    !insertmacro SwapDll x64
    ExecWait '"$SYSDIR\regsvr32.exe" /s "$INSTDIR\x64\PIMETextService.dll"'
  ${EndIf}
  SetRebootFlag false  ; 挪开的旧 DLL 留待下次重启删除即可,不提示重启
  WriteUninstaller "$INSTDIR\pyime\uninstall.exe"
  WriteRegStr HKLM "${UNINST_KEY}" "DisplayName" "PYIME ${VERSION}"
  WriteRegStr HKLM "${UNINST_KEY}" "UninstallString" '"$INSTDIR\pyime\uninstall.exe"'
  WriteRegStr HKLM "${UNINST_KEY}" "DisplayVersion" "${VERSION}"
  WriteRegStr HKLM "${UNINST_KEY}" "PimeDir" "$INSTDIR"
  ExecShell "open" "$INSTDIR\PIMELauncher.exe"
SectionEnd

Function un.onInit
  ReadRegStr $INSTDIR HKLM "${UNINST_KEY}" "PimeDir"
FunctionEnd

Section "Uninstall"
  ExecWait '"$INSTDIR\PIMELauncher.exe" /quit'
  nsExec::ExecToLog '"$INSTDIR\pyime\server.exe" --unregister "$INSTDIR"'
  ; 先 /u 清除含本输入法的 TSF 注册,还原官方 DLL 后再按更新后的 backends.json 重新注册
  ExecWait '"$SYSDIR\regsvr32.exe" /u /s "$INSTDIR\x86\PIMETextService.dll"'
  !insertmacro RestoreDll x86
  ExecWait '"$SYSDIR\regsvr32.exe" /s "$INSTDIR\x86\PIMETextService.dll"'
  ${If} ${RunningX64}
    ExecWait '"$SYSDIR\regsvr32.exe" /u /s "$INSTDIR\x64\PIMETextService.dll"'
    !insertmacro RestoreDll x64
    ExecWait '"$SYSDIR\regsvr32.exe" /s "$INSTDIR\x64\PIMETextService.dll"'
  ${EndIf}
  SetRebootFlag false
  DeleteRegKey HKLM "${UNINST_KEY}"
  RMDir /r "$INSTDIR\pyime"
SectionEnd
