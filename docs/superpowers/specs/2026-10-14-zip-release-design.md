# 子项目 C-1:单个 zip 发布包 + Windows 11 安装脚本

日期:2026-10-14　状态:待确认
前置:A(自研 TSF 前端)Windows 验收通过后才发布含 TSF 的包。NSIS 安装包(含 32 位 DLL)仍属 C 的后续步骤,不在本 spec。

## 目标
Release 里只放**一个 zip**,用户解压后双击 `install.bat` 即可在 Windows 11 安装、`uninstall.bat` 卸载。用于自研前端的预览阶段,迭代快、脚本明文可审计。

## zip 内容
| 文件 | 说明 |
|---|---|
| `pyime_tsf.dll` | 自研 TSF 文本服务(64 位) |
| `pyime-server.exe` | 引擎 + 候选窗服务 |
| `pyime.dict` | 词典(rime-ice 编译) |
| `install.bat` / `install.ps1` | 安装 |
| `uninstall.bat` / `uninstall.ps1` | 卸载 |
| `README.txt` | 安装/卸载/日志位置/已知限制 |
| `LICENSE`、`NOTICE.md` | GPL-3.0 文本;词库来源与许可证(沿用 `installer/NOTICE.md`) |

文件名 `PYIME-<版本>-preview-win64.zip`,旁附 `SHA256SUMS.txt`(不放进 zip,单独作为 release 资产)。

## 安装脚本行为
1. `install.bat` 调 `powershell -NoProfile -ExecutionPolicy Bypass -File install.ps1`(绕开执行策略);`install.ps1` 检测非管理员则 `Start-Process -Verb RunAs` 重启自身。
2. 目标目录 `%ProgramFiles%\PYIME`(普通用户不可写,避免 DLL 被替换)。
3. 先结束 `pyime-server.exe`;若目录已有旧 `pyime_tsf.dll` 则先 `regsvr32 /u /s` 再覆盖;DLL 被占用复制失败时,改为复制成 `pyime_tsf.dll.new`、用 `MoveFileEx(DELAY_UNTIL_REBOOT)` 在重启后替换,并提示用户注销/重启。
4. 复制三个程序文件与许可证,`regsvr32 /s pyime_tsf.dll`,检查退出码;失败则回滚(反注册、删新文件)并显示原因。
5. 检测到 PIME 或旧 PYIME v0.1.x 后端时只提示“可共存,请在语言列表中选‘PYIME(自研预览)’”,不自动卸载。
6. 结尾提示:设置 → 时间和语言 → 语言和区域 → 中文(简体) → 语言选项 → 添加键盘“PYIME(自研预览)”。
7. 全程输出中文,结束 `pause`。

## 卸载脚本行为
结束 server → `regsvr32 /u /s` → 删除安装目录(DLL 被占用则标记重启后删除)→ 询问是否同时清除 `%LOCALAPPDATA%\PYIME`(用户词、日志;默认保留)。

## 打包脚本
`scripts/package-zip.sh <版本>`:`cargo test --workspace` → `scripts/build-tsf.sh` → `scripts/build-server.sh` → 对 `pyime_tsf.dll`、`pyime-server.exe` 做 objdump 依赖自检(不得含 libgcc/libwinpthread/libstdc,DLL 须导出 4 个 `Dll*`)→ 复制 `installer/pyime.dict`(缺失则报错,提示先跑 `scripts/package.sh` 或词典构建)→ 组装并 `zip` → 生成 `SHA256SUMS.txt`。版本号须与 `Cargo.toml` 一致的规则沿用项目约定。

## 测试与验收
- Linux:打包脚本跑通;解压后文件清单、大小、依赖自检均符合;`.ps1` 若本机有 `pwsh` 则做语法解析,否则注明未检查。
- Windows(用户):全新机器/账户双击 `install.bat` → 弹 UAC → 安装成功 → 语言列表可添加输入法 → 记事本可用;再运行 `uninstall.bat` → 注册项与目录清除;覆盖安装(DLL 被占用)路径提示正确。

## 已知限制
无“设置 → 应用”卸载入口;仅 64 位 DLL(32 位宿主不可用);覆盖安装被占用的 DLL 需注销/重启后生效;`regsvr32` 失败(杀软拦截等)只能给出提示。
