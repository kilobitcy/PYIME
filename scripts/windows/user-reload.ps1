# 让 %LOCALAPPDATA%\PYIME\user.txt 的手工改动生效(Windows 11,无需管理员)。
# 用法:user-reload.ps1 [-Edit]
#   不带参数 = 规整 user.txt 后重启 PIMELauncher,server.exe 重新读入
#   -Edit    = 先停输入法,用记事本打开 user.txt,改完按回车后再规整、重启(推荐)
# 每次运行先把 user.txt 备份为 user.txt.bak-时间戳;非 UTF-8 时中止,输入法保持停止,改好后重跑即可。
# 背景:server.exe 只在启动时读 user.txt,且每次选词都把内存里的学习数据整份写回 user.txt。
#       输入法运行中改文件,改完到重启之间只要选过一次词,改动就会被覆盖;-Edit 先停输入法可避免。
param([switch]$Edit)
$ErrorActionPreference = 'Stop'

function Find-Pime {
    $dirs = @()
    foreach ($k in 'HKLM:\SOFTWARE\WOW6432Node\PIME', 'HKLM:\SOFTWARE\PIME') {
        try { $v = (Get-Item $k -ErrorAction Stop).GetValue(''); if ($v) { $dirs += $v } } catch {}
    }
    $dirs += (Join-Path ${env:ProgramFiles(x86)} 'PIME')
    foreach ($d in $dirs) { if (Test-Path (Join-Path $d 'backends.json')) { return $d } }
    throw '未找到 PIME(backends.json)。请先安装官方 PIME 1.3.0-stable。'
}

# 退出 PIMELauncher,并确认 PYIME 的 server.exe 已退出(未退出则结束它)
function Stop-Ime([string]$pime) {
    $launcher = Join-Path $pime 'PIMELauncher.exe'
    if (Get-Process PIMELauncher -ErrorAction SilentlyContinue) {
        Start-Process $launcher -ArgumentList '/quit' -Wait
    }
    $server = Join-Path $pime 'pyime\server.exe'
    for ($i = 0; $i -lt 20; $i++) {
        $left = @(Get-Process PIMELauncher, server -ErrorAction SilentlyContinue |
            Where-Object { $_.Name -eq 'PIMELauncher' -or $_.Path -eq $server })
        if ($left.Count -eq 0) { break }
        Start-Sleep -Milliseconds 250
    }
    Get-Process server -ErrorAction SilentlyContinue | Where-Object { $_.Path -eq $server } | Stop-Process -Force
    Write-Host '输入法已停止'
}

function Start-Ime([string]$pime) {
    Start-Process (Join-Path $pime 'PIMELauncher.exe')
    Write-Host 'PIMELauncher 已重启,下次按键时输入法自动重连并读入 user.txt'
}

# 校验并规整 user.txt:必须是 UTF-8;去掉 BOM、CRLF 改 LF(server.exe 不认 BOM 与 \r)。
# 非 UTF-8 时 server.exe 会读失败、按空数据启动,下次选词再把空数据写回,学习数据全丢,所以直接中止。
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
$pime = Find-Pime

Stop-Ime $pime
$bak = "$user.bak-$(Get-Date -Format yyyyMMddHHmmss)"
Copy-Item -LiteralPath $user -Destination $bak
Write-Host "已备份到 $bak"
if ($Edit) {
    # Win11 记事本是多标签的,已开着时新文件进现有窗口、进程立即返回,故不用 -Wait,改为等回车
    Start-Process notepad.exe -ArgumentList "`"$user`""
    Read-Host '请在记事本里修改并保存(UTF-8),然后回到这里按回车继续' | Out-Null
}
Normalize-User $user
Start-Ime $pime
