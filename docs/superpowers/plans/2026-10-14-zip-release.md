# C-1:单 zip 发布包与安装脚本 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** `scripts/package-zip.sh <版本>` 产出 `PYIME-<版本>-preview-win64.zip`(含 DLL、server、词典、安装/卸载脚本、许可证)和 `SHA256SUMS.txt`。

**Architecture:** 脚本模板放 `installer/zip/`(明文 `.ps1`/`.bat`/`README.txt`),打包脚本只负责构建、自检、拷贝、压缩。设计见 `docs/superpowers/specs/2026-10-14-zip-release-design.md`。

**Tech Stack:** bash、zip、sha256sum、PowerShell 5.1(Windows 11 自带)、现有 `scripts/build-tsf.sh`、`scripts/build-server.sh`。

## Global Constraints

- 回复、注释、commit 用简体中文;commit 末尾加 `Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>`。
- 安装目录 `%ProgramFiles%\PYIME`;`.ps1` 文件以 **UTF-8 带 BOM** 保存(PowerShell 5.1 才能正确读中文),`.bat` 以 GBK 之外的纯 ASCII 编写(只放英文,避免代码页问题),中文提示都放在 `.ps1`。
- 不 push、不打 tag、不发布;本计划只产出本地 zip。
- zip 内程序文件依赖自检:不得含 libgcc/libwinpthread/libstdc;DLL 须导出 4 个 `Dll*`(复用 `scripts/check-tsf-dll.sh`)。
- 不改 `installer/pyime.nsi`、`scripts/package.sh`(旧 PIME 附加包流程保持可用)。

---

### Task 1: 安装/卸载脚本模板

**Files:**
- Create: `installer/zip/install.bat`、`installer/zip/uninstall.bat`、`installer/zip/install.ps1`、`installer/zip/uninstall.ps1`、`installer/zip/README.txt`

- [ ] **Step 1: `install.bat` / `uninstall.bat`**(ASCII,各 3 行)
```bat
@echo off
powershell -NoProfile -ExecutionPolicy Bypass -File "%~dp0install.ps1"
```
`uninstall.bat` 同理指向 `uninstall.ps1`。

- [ ] **Step 2: `install.ps1`**(带 BOM,中文输出)。必须实现 spec「安装脚本行为」1–7:
  - 非管理员(`[Security.Principal.WindowsPrincipal]`)→ `Start-Process powershell -Verb RunAs -ArgumentList "-NoProfile -ExecutionPolicy Bypass -File `"$PSCommandPath`""` 后退出。
  - `$dst = Join-Path $env:ProgramFiles 'PYIME'`;`Stop-Process -Name pyime-server -Force -ErrorAction SilentlyContinue`。
  - 已有 `$dst\pyime_tsf.dll` 则 `regsvr32 /u /s`;`Copy-Item` 三个程序文件(`pyime_tsf.dll`、`pyime-server.exe`、`pyime.dict`)与 `LICENSE`、`NOTICE.md`;DLL 复制失败(被占用)→ 复制为 `pyime_tsf.dll.new`,用 `Add-Type` 调 `MoveFileEx(src, dst, MOVEFILE_DELAY_UNTIL_REBOOT|MOVEFILE_REPLACE_EXISTING = 5)` 登记重启后替换,提示注销/重启,**不**执行 `regsvr32`。
  - `regsvr32 /s "$dst\pyime_tsf.dll"`,检查 `$LASTEXITCODE`;非 0 → `regsvr32 /u /s`,删除刚复制的文件,打印失败原因并 `exit 1`。
  - 检测 `HKLM:\SOFTWARE\WOW6432Node\PIME` 或 `HKLM:\SOFTWARE\PIME` 存在时,提示可共存。
  - 成功后打印添加输入法的菜单路径,`pause`。
  - 全程 `$ErrorActionPreference = 'Stop'`,try/catch 里打印错误并 `pause`。

- [ ] **Step 3: `uninstall.ps1`**:提权同上;结束 server;`regsvr32 /u /s`;`Remove-Item -Recurse -Force $dst`,失败(占用)则对每个剩余文件 `MoveFileEx(file, $null, 4 /*DELAY_UNTIL_REBOOT*/)`;`Read-Host '是否同时清除用户词和日志?(y/N)'`,`y` 才删 `$env:LOCALAPPDATA\PYIME`。

- [ ] **Step 4: `README.txt`**(中文,≤40 行):安装/卸载步骤、添加输入法的菜单路径、日志位置(`%LOCALAPPDATA%\PYIME\log.txt`、`tsf.log`)、已知限制(spec「已知限制」逐条)、问题反馈方式。

- [ ] **Step 5: 验证**:`file installer/zip/*` 显示 `.ps1` 为 UTF-8 with BOM;`grep -c . installer/zip/*` 无空文件;本机无 `pwsh`,在报告里注明 PowerShell 语法未做机器检查,靠 Windows 验收。

- [ ] **Step 6: Commit** `installer/zip:安装/卸载脚本与说明`

---

### Task 2: `scripts/package-zip.sh`

**Files:**
- Create: `scripts/package-zip.sh`(可执行)
- Modify: `.gitignore`(忽略 `dist/`)

- [ ] **Step 1: 实现**,`set -euo pipefail`,`cd "$(dirname "$0")/.."`,`VERSION=${1:?用法: scripts/package-zip.sh <版本>}`,步骤:
  1. 校验 `VERSION` 与 `crates/pyime-backend/Cargo.toml` 的 `version` 一致,不一致报错退出(沿用项目版本约定)。
  2. `cargo test --workspace`。
  3. `scripts/build-tsf.sh`;`scripts/build-server.sh`;`scripts/check-tsf-dll.sh`;对 `pyime-server.exe` 用 `x86_64-w64-mingw32-objdump -p` 检查依赖(同 `scripts/package.sh` 的写法,注意 here-string 避免 pipefail 误报)。
  4. `test -f installer/pyime.dict || { echo "缺少 installer/pyime.dict,先用 scripts/package.sh 的词典构建步骤生成"; exit 1; }`。
  5. 组装到 `dist/PYIME-$VERSION-preview-win64/`:三个程序文件、`installer/zip/*`、`LICENSE`(仓库根)、`installer/NOTICE.md`。
  6. `.ps1` 保持 BOM;`zip -r -X dist/PYIME-$VERSION-preview-win64.zip PYIME-$VERSION-preview-win64`(在 `dist` 内执行)。
  7. `(cd dist && sha256sum PYIME-$VERSION-preview-win64.zip > SHA256SUMS.txt)`;打印 zip 大小与清单(`unzip -l`)。

- [ ] **Step 2: 验证**:`scripts/package-zip.sh 0.1.7 > /tmp/pz.log 2>&1; tail -30 /tmp/pz.log`(版本取当前 `Cargo.toml` 值),Expected:清单含 8 类文件;再用错误版本 `scripts/package-zip.sh 9.9.9` 应报“版本不一致”并退出非 0;`unzip -l` 中 `install.ps1` 前 3 字节为 BOM(`unzip -p ... | head -c3 | xxd`)。

- [ ] **Step 3: Commit** `scripts:package-zip.sh 产出单 zip 发布包`

---

### Task 3: Windows 实机验收(用户操作)
1. 解压 zip,双击 `install.bat` → UAC → 安装成功 → 设置里添加“PYIME(自研预览)”→ 记事本输入可用(沿用 A3 验收清单前 3 项)。
2. 再次运行 `install.bat`(输入法正在使用)→ 提示正确(重启后替换或直接成功)。
3. `uninstall.bat` → 语言列表中该输入法消失、`%ProgramFiles%\PYIME` 被清除(或提示重启后清除)、选择 `n` 时用户数据保留。
4. 中文路径/带空格路径下解压运行脚本均正常。
出问题时提供安装窗口截图或文字。
