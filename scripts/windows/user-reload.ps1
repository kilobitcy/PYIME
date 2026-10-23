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
