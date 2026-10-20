
. (Join-Path $PSScriptRoot 'common.ps1')
try {
    Require-Admin $PSCommandPath
    $pime = Find-Pime
    Quit-Launcher $pime
    $dst = Join-Path $pime 'pyime'
    if (Test-Path "$dst\server.exe") { [void](Run "$dst\server.exe" "--unregister `"$pime`"") }
    # 先 /u 清除含本输入法的 TSF 注册,再按更新后的 backends.json 重新注册。
    Reg-Dlls $pime $true
    Reg-Dlls $pime $false
    if (Test-Path $dst) { Remove-Item -Recurse -Force $dst }
    Start-Process (Join-Path $pime 'PIMELauncher.exe')
    Write-Host '卸载完成。用户数据(%LOCALAPPDATA%\PYIME)已保留,可手动删除。'
} catch {
    Write-Host "卸载失败:$($_.Exception.Message)" -ForegroundColor Red
}
Read-Host '按回车键退出' | Out-Null
