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
  SetOutPath "$INSTDIR\pyime\input_methods\pyime"
  File "input_methods\pyime\ime.json"
  nsExec::ExecToLog '"$INSTDIR\pyime\server.exe" --register "$INSTDIR"'
  Pop $0
  StrCmp $0 "0" +3
    MessageBox MB_ICONSTOP "登记 backends.json 失败(代码 $0)。"
    Abort
  ExecWait '"$SYSDIR\regsvr32.exe" /s "$INSTDIR\x86\PIMETextService.dll"'
  ${If} ${RunningX64}
    ExecWait '"$SYSDIR\regsvr32.exe" /s "$INSTDIR\x64\PIMETextService.dll"'
  ${EndIf}
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
  ; 先 /u 清除含本输入法的 TSF 注册,再按更新后的 backends.json 重新注册
  ExecWait '"$SYSDIR\regsvr32.exe" /u /s "$INSTDIR\x86\PIMETextService.dll"'
  ExecWait '"$SYSDIR\regsvr32.exe" /s "$INSTDIR\x86\PIMETextService.dll"'
  ${If} ${RunningX64}
    ExecWait '"$SYSDIR\regsvr32.exe" /u /s "$INSTDIR\x64\PIMETextService.dll"'
    ExecWait '"$SYSDIR\regsvr32.exe" /s "$INSTDIR\x64\PIMETextService.dll"'
  ${EndIf}
  DeleteRegKey HKLM "${UNINST_KEY}"
  RMDir /r "$INSTDIR\pyime"
SectionEnd
