# 实机验证:替换已安装 PIME 的 PIMETextService.dll(x64/x86)与 PYIME 的 server.exe,或还原原版。全程无需重启/注销。
# 用法:pime-dll-test.ps1 -Variant baseline|patched|restore
#   baseline = mingw 编译、未改排版的 DLL(确认编译器没问题);server.exe 用原版
#   patched  = 候选按自身宽度排版、间距与序号参照微信的 DLL + 字号 12 的 server.exe
#   restore  = 全部还原为原版
# 原理:已运行的程序继续用已加载的旧 DLL(文件只是被改名),之后新启动的进程才加载新 DLL;
#       server.exe 由 PIMELauncher 按需启动,重启 PIMELauncher 即换上新版。
param(
    [Parameter(Mandatory)][ValidateSet('baseline', 'patched', 'restore')][string]$Variant,
    [switch]$SwapOnly  # 内部用:管理员权限下只做文件替换
)
$ErrorActionPreference = 'Stop'

function Is-Admin {
    $id = [Security.Principal.WindowsIdentity]::GetCurrent()
    (New-Object Security.Principal.WindowsPrincipal $id).IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)
}

function Find-Pime {
    $dirs = @()
    foreach ($k in 'HKLM:\SOFTWARE\WOW6432Node\PIME', 'HKLM:\SOFTWARE\PIME') {
        try { $v = (Get-Item $k -ErrorAction Stop).GetValue(''); if ($v) { $dirs += $v } } catch {}
    }
    $dirs += (Join-Path ${env:ProgramFiles(x86)} 'PIME')
    foreach ($d in $dirs) { if (Test-Path (Join-Path $d 'backends.json')) { return $d } }
    throw '未找到 PIME(backends.json)。请先安装官方 PIME 1.3.0-stable。'
}

# 正在使用的 DLL/EXE 不能删除或覆盖,但可以改名;改名后的旧文件下次运行时再尝试清理。
function Move-Aside([string]$path) {
    Move-Item -LiteralPath $path -Destination "$path.old-$(Get-Date -Format yyyyMMddHHmmss)"
}

function Remove-Stale([string]$path) {
    Get-ChildItem -Path (Split-Path $path) -Filter "$(Split-Path $path -Leaf).old-*" -ErrorAction SilentlyContinue |
        ForEach-Object { try { Remove-Item -LiteralPath $_.FullName -ErrorAction Stop } catch {} }
}

# 换成 $src;首次替换时把原版备份为 .orig
function Install-File([string]$dst, [string]$src, [string]$label) {
    Remove-Stale $dst
    $orig = "$dst.orig"
    if (-not (Test-Path $orig)) { Move-Item -LiteralPath $dst -Destination $orig; Write-Host "[$label] 原版已备份为 .orig" }
    elseif (Test-Path $dst) { Move-Aside $dst }
    Copy-Item -LiteralPath $src -Destination $dst
    if ((Get-FileHash $src).Hash -ne (Get-FileHash $dst).Hash) { throw "[$label] 复制后哈希不一致" }
    Write-Host "[$label] 已替换,校验一致"
}

function Restore-File([string]$dst, [string]$label) {
    Remove-Stale $dst
    $orig = "$dst.orig"
    if (-not (Test-Path $orig)) { Write-Host "[$label] 已是原版"; return }
    if (Test-Path $dst) { Move-Aside $dst }
    Move-Item -LiteralPath $orig -Destination $dst
    Write-Host "[$label] 已还原原版"
}

function Swap-Files([string]$pime) {
    $arches = @('x86')
    if ([Environment]::Is64BitOperatingSystem) { $arches = @('x64', 'x86') }
    foreach ($arch in $arches) {
        $dst = Join-Path $pime "$arch\PIMETextService.dll"
        if ($Variant -eq 'restore') { Restore-File $dst "$arch DLL" }
        else { Install-File $dst (Join-Path $PSScriptRoot "$Variant\$arch\PIMETextService.dll") "$arch DLL" }
    }
    $server = Join-Path $pime 'pyime\server.exe'
    if (-not (Test-Path $server)) { throw "未找到 $server,请先安装 PYIME" }
    if ($Variant -eq 'patched') { Install-File $server (Join-Path $PSScriptRoot 'patched\server.exe') 'server.exe' }
    else { Restore-File $server 'server.exe' }
    Set-Content -Path (Join-Path $pime 'pyime-dll-test.txt') -Value "$Variant $(Get-Date -Format s)" -Encoding UTF8
}

# 重启 PIMELauncher 以换上新 server.exe。已打开程序里的输入法会断开约 1-2 秒,下次按键自动重连(正在输入的拼音会丢)。
function Restart-Launcher([string]$pime) {
    $launcher = Join-Path $pime 'PIMELauncher.exe'
    Start-Process $launcher -ArgumentList '/quit' -Wait
    for ($i = 0; $i -lt 20 -and (Get-Process PIMELauncher -ErrorAction SilentlyContinue); $i++) { Start-Sleep -Milliseconds 250 }
    if (Is-Admin) { Start-Process explorer.exe -ArgumentList "`"$launcher`"" }  # 经资源管理器以普通权限启动
    else { Start-Process $launcher }
    Write-Host 'PIMELauncher 已重启'
}

# 新开一个独立进程的测试窗口:它会加载刚换上的 DLL,而已开着的程序仍是旧 DLL。
function Open-TestWindow {
    $code = @'
Add-Type -AssemblyName System.Windows.Forms
$f = New-Object Windows.Forms.Form
$f.Text = 'PYIME 候选窗测试(新进程,已加载新 DLL)'
$f.Width = 800; $f.Height = 240; $f.StartPosition = 'CenterScreen'; $f.TopMost = $true
$t = New-Object Windows.Forms.TextBox
$t.Multiline = $true; $t.Dock = 'Fill'; $t.Font = New-Object Drawing.Font('Microsoft YaHei UI', 14)
$f.Controls.Add($t)
[void]$f.ShowDialog()
'@
    $enc = [Convert]::ToBase64String([Text.Encoding]::Unicode.GetBytes($code))
    Start-Process powershell.exe -WindowStyle Hidden -ArgumentList "-NoProfile -WindowStyle Hidden -EncodedCommand $enc"
}

try {
    $pime = Find-Pime
    if ($SwapOnly) { Swap-Files $pime; exit 0 }
    Write-Host "PIME 目录:$pime"
    if (Is-Admin) { Swap-Files $pime }
    else {
        $exe = (Get-Process -Id $PID).Path
        $p = Start-Process $exe -Verb RunAs -Wait -PassThru `
            -ArgumentList "-NoProfile -ExecutionPolicy Bypass -File `"$PSCommandPath`" -Variant $Variant -SwapOnly"
        if ($p.ExitCode -ne 0) { throw '管理员窗口中的文件替换失败' }
        Write-Host '文件替换完成'
    }
    Restart-Launcher $pime
    Open-TestWindow
    Write-Host ''
    Write-Host "完成($Variant)。无需重启:在弹出的测试窗口里输入 ocr 即可看到效果。" -ForegroundColor Green
    Write-Host '已打开的程序仍用旧 DLL,等你自然关闭重开后才换新;出问题运行 3-restore.bat 即可,同样无需重启。'
    Read-Host '按回车关闭本窗口'
} catch {
    Write-Host "失败:$_" -ForegroundColor Red
    Read-Host '按回车退出'
    exit 1
}
