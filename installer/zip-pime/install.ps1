
. (Join-Path $PSScriptRoot 'common.ps1')
try {
    Require-Admin $PSCommandPath
    $pime = Find-Pime
    Write-Host "找到 PIME:$pime"
    Quit-Launcher $pime
    $dst = Join-Path $pime 'pyime'
    New-Item -ItemType Directory -Force -Path "$dst\input_methods\pyime" | Out-Null
    foreach ($f in 'server.exe', 'pyime.dict', 'pyime.en', 'NOTICE.md') {
        Copy-Item -Force (Join-Path $PSScriptRoot $f) $dst
    }
    $gram = Join-Path $PSScriptRoot 'pyime.gram'
    if (Test-Path $gram) { Copy-Item -Force $gram $dst; Write-Host '已安装语法模型 pyime.gram' }
    Copy-Item -Force (Join-Path $PSScriptRoot 'ime.json') "$dst\input_methods\pyime"
    $code = Run "$dst\server.exe" "--register `"$pime`""
    if ($code -ne 0) { throw "登记 backends.json 失败(代码 $code)。" }
    Reg-Dlls $pime $false
    Start-Process (Join-Path $pime 'PIMELauncher.exe')
    Write-Host ''
    Write-Host '安装完成。若输入法列表里没有 PYIME,请注销再登录一次。'
} catch {
    Write-Host "安装失败:$($_.Exception.Message)" -ForegroundColor Red
}
Read-Host '按回车键退出' | Out-Null
