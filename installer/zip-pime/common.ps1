$ErrorActionPreference = 'Stop'

function Require-Admin([string]$script) {
    $id = [Security.Principal.WindowsIdentity]::GetCurrent()
    $admin = (New-Object Security.Principal.WindowsPrincipal $id).IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)
    if (-not $admin) {
        Start-Process powershell -Verb RunAs -ArgumentList "-NoProfile -ExecutionPolicy Bypass -File `"$script`""
        exit
    }
}

# 找已安装的官方 PIME 目录(安装程序是 32 位,登记在 WOW6432Node)。
function Find-Pime {
    $dirs = @()
    foreach ($k in 'HKLM:\SOFTWARE\WOW6432Node\PIME', 'HKLM:\SOFTWARE\PIME') {
        try { $v = (Get-Item $k -ErrorAction Stop).GetValue(''); if ($v) { $dirs += $v } } catch {}
    }
    $dirs += (Join-Path ${env:ProgramFiles(x86)} 'PIME')
    foreach ($d in $dirs) {
        if (Test-Path (Join-Path $d 'backends.json')) { return $d }
    }
    throw '未找到 PIME(backends.json)。请先安装官方 PIME 1.3.0-stable。'
}

function Run([string]$exe, [string]$argLine) {
    $p = Start-Process -FilePath $exe -ArgumentList $argLine -Wait -PassThru -WindowStyle Hidden
    return $p.ExitCode
}

# 32 位 TSF DLL 用 SysWOW64 的 regsvr32,64 位的用 System32 的。
function Reg-Dlls([string]$pime, [bool]$unregister) {
    $flag = if ($unregister) { '/u /s' } else { '/s' }
    $r32 = Join-Path $env:windir 'SysWOW64\regsvr32.exe'
    $r64 = Join-Path $env:windir 'System32\regsvr32.exe'
    [void](Run $r32 "$flag `"$pime\x86\PIMETextService.dll`"")
    if ([Environment]::Is64BitOperatingSystem) {
        [void](Run $r64 "$flag `"$pime\x64\PIMETextService.dll`"")
    }
}

function Quit-Launcher([string]$pime) {
    $l = Join-Path $pime 'PIMELauncher.exe'
    if (Test-Path $l) { [void](Run $l '/quit') }
}
