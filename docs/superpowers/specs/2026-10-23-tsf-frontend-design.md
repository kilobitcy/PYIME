# 自带 TSF 前端、去掉 PIME 依赖 设计

日期:2026-10-23 · 分支 `feat/tsf-frontend`(基于 `feat/tray-menu`)

## 目标
- 用户只装一个 `PYIME-<版本>-setup.exe`,不再需要先装官方 PIME;不再改 PIME 的 `backends.json`、不再替换 PIME 的 DLL。
- 用户可见行为与 0.1.15 一致(组字、候选窗外观、选词翻页、Shift 切换、托盘图标与菜单、标点、学习)。
- 本期不加新功能;⌄ 展开按钮、深色模式等外观改进留到替换完成后。

## 不做
- TSF 层长期保持 C++,不用 Rust 重写;仅当上游 libIME2 不可维护时再评估(见"方案选择")。
- 不改引擎 crate;不改 `session.rs` 的按键/组字逻辑与 JSON 字段语义。
- 不支持 ARM64 Windows(安装器检测到即提示并退出)。

## 方案选择
| 方案 | 内容 | 主要问题 |
|---|---|---|
| A. Rust + windows-rs 从零写 TSF DLL | 原定里程碑③ | `windows` 0.62 在 `i686-pc-windows-gnu`(Rust 1.88 起 Tier 2)上的链接/unwind 坑;无经实机验证的 Rust TSF 参考(ime-rs 停更、bombless 自述未实测);TSF 细节(编辑会话、UI-less、沉浸式、托盘图标)全部从零踩,每轮靠用户实机验证 |
| **B. 收编 libIME2 + 精简 PIMETextService(C++),DLL 直连自有服务进程(采用)** | 把已在用户 Win11 上验证过的 DLL 源码纳入本仓库,只换掉"连 Launcher"这一层 | 仍是 C++;自己维护约 7.5k 行 LGPL 代码(另 vendored jsoncpp,不改) |
| C. 里程碑②:随包分发官方 PIMELauncher + 补丁 DLL | 不写管道服务端 | 仍带 PIME 安装形态(Launcher、`backends.json`、vcredist);Launcher 不回复时 DLL 无限阻塞的问题仍在;不能算"删掉 PIME" |
| D. 维持现状 | — | 依赖官方 PIME 安装、补丁 DLL 替换,外观受协议限制 |

采用 B:用户真正要的是"不装 PIME、一个安装包、能改外观",B 以最小未知量达成;TSF 层用什么语言对用户无感。业界同构做法:清风输入法 WindInput(Rust 服务 + C++ 薄 TSF)、Weasel(C++ TSF + 服务进程,命名管道)。
不做"混合"(Rust staticlib 链进 C++ DLL):i686 下系统 mingw 与 Rust 自带 libgcc 的 unwind 不兼容风险真实存在,且两套构建链都要维护。Rust 在 DLL 场景的安全收益也有限:TSF 全是裸 COM 指针,Rust 版同样满篇 `unsafe`;`extern "system"` 里 panic 直接 abort 宿主(Rust 1.81 起);重叠 I/O 生命周期这类问题 Rust 也防不住。常改的只有 `CandidateWindow.cpp`(外观)与 `PIMEClient.cpp`(IPC),⌄ 按钮、深色模式都在前者的 GDI 绘制里,C++ 改法直接。

## 现状(据源码,行号为 2026-10-23)
- 数据流:`PIMETextService.dll`(在各应用进程内)→ 命名管道 `\\.\pipe\<用户名>\PIME\Launcher`(名字拼接 `PIMEClient.cpp:787-800`;收发 `TransactNamedPipe`,`:639`,消息模式)→ `PIMELauncher.exe`(libuv,按 `init.id` 选后端,给每个客户端发 UUID 作 clientId)→ 后端 `server.exe` 的 stdin/stdout,行协议 `<clientId>|<JSON>` / `PIME_MSG|<clientId>|<JSON>`。
- Launcher 职责:单实例;登录时由 HKLM Run 键启动(安装时 `ExecShellAsUser` 以非提权身份立即启动,`installer.nsi:588`);隐藏后端控制台窗口(`BackendServer.cpp:168`);后端 stdout EOF 或请求 30s 无响应时杀掉重启后端;客户端断开时向后端发 `close`;Win8+ 管道安全描述符拒绝 NETWORK,允许 SYSTEM、Administrators、ALL APPLICATION PACKAGES、登录 SID,并带低完整性标签(`PipeSecurity.cpp:123-149`,`PipeServer.cpp:355-360`)。
- DLL 注册类别(`ImeModule.cpp:296-333`):TIP_KEYBOARD、DISPLAYATTRIBUTEPROVIDER、TIPCAP_INPUTMODECOMPARTMENT、TIPCAP_UIELEMENTENABLED;Win8+ 加 IMMERSIVESUPPORT、SYSTRAYSUPPORT。x86/x64 各一份,regsvr32 注册。
- 后端与 PIME 耦合只在:`protocol.rs`(行协议)、`main.rs`(clientId→Session 多路复用、`close`、`--register` 改 backends.json、去管道句柄继承)、`register.rs`。`session.rs`/`tray.rs` 只依赖 JSON 字段语义,与传输无关。
- DLL 侧 IPC 现状(`PIMEClient.cpp:635-760`):管道不存在时 `WaitNamedPipe` 立即失败;但 Launcher 在而不回复时,`TransactNamedPipe` 无限阻塞宿主 UI 线程;断线时 `resetTextServiceState`(`:763-777`)只删按钮,不结束组字、不藏候选窗。本期顺带修掉。
- `isPipeCreatedByPIMEServer`(`:696-705`)是空实现,不校验管道对端。

## 架构
```
应用进程(x64 或 x86)
  PYIME.dll  = libIME2(TSF 骨架、候选窗、语言栏按钮) + 精简 TextService/Client(C++)
     │ 命名管道 \\.\pipe\<用户名>\PYIME,消息模式,一条消息 = 一个 JSON(字段同现 PIME 协议)
     ▼
pyime-server.exe --worker(Rust):管道服务端 → 工作线程:HashMap<连接id, Session>(session.rs/tray.rs/ime.rs 原样)
     ▲ 崩溃/看门狗退出后重启
pyime-server.exe --supervise(监护进程,单实例;由 Run 键/安装器启动)
```
- 连接即会话:一条管道连接对应原来的一个 clientId;连接断开等价于原 `close`。去掉 `<clientId>|` 与 `PIME_MSG|` 包装。
- `server.exe` 改名 `pyime-server.exe`(按名结束进程时不误杀别的 server.exe),GUI 子系统(`#![windows_subsystem = "windows"]`),不弹控制台窗。
- DLL **不负责**拉起服务端:只连接、超时放行。避开宿主 Job、提权、AppContainer、锁屏 SYSTEM 身份、环境变量继承等一串问题。

## 组件
### 1. `tsf/`:收编的 DLL 源码(C++)
- 来源:PIME v1.3.0-stable 的 `libIME2/`、`PIMETextService/`、`jsoncpp`(第三方,原样);已有两份补丁直接合入源码,`pime/tsf_compat.h` 移入。收编部分仍按原许可证(LGPL)分发修改版、不转 GPL,保留原版权头,改过的文件加"PYIME 修改"说明;`tsf/README.md` 记来源 commit 与各部分许可证。
- 改动:
  - `PIMEImeModule`/`DllEntry`:去掉 `backends.json`/`ime.json` 扫描;CLSID、语言配置 GUID、名称、图标写死(**新 GUID**,与官方 PIME 可并存);删 `onConfigure`(无设置页)。
  - `PIMEClient`:
    - 管道名改 `<用户名>\PYIME`;`CreateFile` 加 `FILE_FLAG_OVERLAPPED`,收发改重叠 I/O(处理 `ERROR_MORE_DATA`)。
    - 连接:`WaitNamedPipe` 最多 200ms、1 次。
    - 请求超时 2s(含 `init`);超时/读写失败即断线。超时收尾:`CancelIoEx` → `GetOverlappedResult(bWait=TRUE)` 等取消完成 → 才关句柄;收发缓冲区为 `Client` 成员,不用栈变量(原 `PIMEClient.cpp:636` 的栈上 `buf`),防内核迟到写入宿主栈。
    - 退避:连接失败、请求超时、断线三种失败共用一个计数,失败后本进程退避(1s 起倍增,上限 30s)内不再尝试,按键直接放行;成功一次清零。
    - 断线处理:在原 `resetTextServiceState` 基础上结束组字、藏候选窗与消息窗(同 `PIMETextService.cpp:181-189` 的做法),再放行当前键。
    - 不做管道对端校验(同原版):AppContainer/低完整性宿主(开始菜单搜索、商店应用)打不开中完整性服务端进程句柄,校验会让这些宿主永远打不出中文;同用户恶意进程本就能做任何事,收益≈0;DACL 已拒绝 NETWORK。
    - 日志:`%LOCALAPPDATA%\PYIME\tsf.log`(同后端,`log.disabled` 关闭),只记连接/超时/断线/退避与宿主进程名,不记按键;路径用 `SHGetKnownFolderPath`,AppContainer 内写不了就不写。
  - 其余(按键→请求、响应→组字/候选/`addButton`/`changeButton`/菜单)不动,故 `session.rs` 不用改。
- 构建:`scripts/build-tsf.sh`(由 `build-pime-dll.sh` 改写,不再克隆/打补丁)→ `target/tsf/{x64,x86}/PYIME.dll`;静态链接 libgcc/libstdc++,objdump 验收不依赖 mingw 运行库。

### 2. `pyime-server.exe`(Rust,仅 Windows 部分新增 `pipe.rs`、`supervise.rs`)
- 沿用现有手写 `extern "system"` 调 Win32 的写法,不引入 windows crate。
- **监护进程**(`--supervise`;无参数仍是 PIME stdin 模式,过渡期 PIME 照旧这样启动后端):命名互斥量 `Local\PYIME-supervisor` 保单实例,已有则静默退出;循环启动 `--worker` 子进程并等待其退出;子进程 60s 内连续退出 5 次则停止重启并记日志(防词典缺失等致命错误刷屏);`--quit` 通知已运行的监护进程连同子进程退出(供安装器用):监护进程建命名事件 `Local\PYIME-quit`(DACL 允许当前用户与 Administrators 置位),`--quit` 打开并置位后等监护进程退出 ≤3s;事件不存在(监护进程不在)立即返回。
- 过渡期 `server.exe` 改名后,`register.rs` 写入 `backends.json` 的可执行名同步改。
- **worker**:先加载引擎,再建管道(加载期间 DLL 连不上,按键放行)。
  - 安全描述符同 PIME Launcher(见"现状"),其中"登录 SID"改为当前用户 SID(管道名已含用户名,效果等同),SDDL 经 `ConvertStringSecurityDescriptorToSecurityDescriptorW`。ALL APPLICATION PACKAGES 只授 `0x12019B`(读写,不含 `FILE_CREATE_PIPE_INSTANCE`),防商店应用另开同名实例截获其他程序的按键;客户端(DLL、pipe-cli)因此以 `GENERIC_READ | FILE_WRITE_DATA | FILE_WRITE_ATTRIBUTES` 打开,不用 `GENERIC_WRITE`。单条请求上限 1 MiB,超出读完丢弃并断开。
  - 首个管道实例带 `FILE_FLAG_FIRST_PIPE_INSTANCE`,失败(名字被占)记日志后退出。
  - 线程:accept 线程先建下一个实例再把连接交给该连接的 I/O 线程;请求经 mpsc 送唯一工作线程(拥有 `Rc<Ime>` 与全部 Session,保持单线程),回复经该连接的 channel 返回。
  - 看门狗:独立线程监视工作线程"当前请求开始时间",超过 5s 记日志并退出进程,由监护进程重启。
- `user.txt`、`en_learned.txt` 写盘(`en.txt` 服务端只读不写)改为写临时文件再 rename(被看门狗/taskkill 打断时不截断);外部修改保护用的 mtime/长度在 rename 后记录。
- 过渡期保留 stdin 模式(PIME 后端)到第 6 步再删;届时删除 `protocol.rs`、`register.rs`、`main.rs` 的 stdin 循环/`--register`/`disinherit_std_handles`。
- 开发工具 `pyime-pipe-cli.exe`(`crates/pyime-backend/src/bin/`):直连管道,读入 JSON 行、打印回复,用于第 2 步在 Windows 上单独验收服务端。不进安装包。

### 3. 安装器 `installer/pyime.nsi`(重写)
- 64 位 Windows 且非 ARM64 才继续;`SetRegView 64`。
- 装到 `$PROGRAMFILES64\PYIME`(Program Files 默认对 AppContainer 可读,不改 ACL):`pyime-server.exe`、词典/英文表/符号表/可选 gram、图标、`x64\PYIME.dll`、`x86\PYIME.dll`、各许可证、LGPL 源码获取说明。
- 安装/升级顺序:`pyime-server.exe --quit`(再 `taskkill /F /IM pyime-server.exe` 兜底)→ 复制文件;占用中的 DLL 沿用现 `SwapDll` 的改名法(`installer/pyime.nsi:20-30`)→ `System32\regsvr32` 注 x64、`SysWOW64\regsvr32` 注 x86 → 写 HKLM Run `PYIME` = `pyime-server.exe --supervise` → 用 StdUtils `ExecShellAsUser` 以非提权身份启动(同 PIME `installer.nsi:28,588`)。
- 迁移旧版:旧附加包安装器是 32 位、未设注册表视图,其卸载登记在 32 位视图;先切 `SetRegView 32` 读 `HKLM\...\Uninstall\PYIME\PimeDir`(现 `pyime.nsi:85`);存在则直接静默运行旧卸载程序 `ExecWait '"<PimeDir>\pyime\uninstall.exe" /S _?=<PimeDir>\pyime'`(它已含删条目、还原 `.orig`、重注册 PIME DLL,`pyime.nsi:93-108`),再删残留的 `uninstall.exe`。不卸载官方 PIME 本身;旧卸载会 `/quit` PIMELauncher 且不重启,用户若还用 PIME 的其他输入法,要到下次登录才恢复(安装完成页提示)。用户数据 `%LOCALAPPDATA%\PYIME` 不动。
- 卸载顺序:删 Run 键 → `--quit` + taskkill 兜底(监护进程先退,不会再重启 worker)→ regsvr32 /u 两份 → 删文件(占用中的 DLL 改名,`Delete /REBOOTOK`)。

## 错误处理汇总
| 情况 | 行为 |
|---|---|
| 服务端未启动/加载中 | `WaitNamedPipe` ≤200ms 失败 → 退避期内按键放行,不冻宿主 |
| 服务端卡死 | DLL 2s 超时 → 断线处理、放行、进入退避;worker 看门狗 5s 退出,监护进程重启 |
| worker 崩溃 | 管道断开 → DLL 断线处理;监护进程重启;下次按键(退避期满)重连 |
| 监护进程也没了(含 60s 内 5 次退出后主动停止) | 输入法失效、按键放行;重新登录或重装恢复(日志记下) |
| 同一用户同时开两个登录会话 | 管道名跨会话共享,后一个会话的 worker 建首实例失败、无输入法。已知限制,不处理 |
| 本机另一用户抢先建同名管道 | worker 建首实例失败(记日志、监护按策略重试后停止),DLL 可能连到冒名服务端。需本机另有恶意账户;不做对端校验(理由见组件 1),已知限制 |
| 同用户低完整性进程另开同名实例 | 用户 SID 须保留建实例权限(worker 开后续实例要用),无法经 DACL 排除;同用户进程本可直接读该用户数据,已知限制 |
| 锁屏/UAC 安全桌面(SYSTEM 身份) | 管道名 `SYSTEM\PYIME` 不存在,按键放行,同现状 |

## 测试
- Linux `cargo test`:`session.rs` 原测试不变;新增工作线程分发单测(连接 id 映射、断开即 close、请求顺序,直接用 mpsc 驱动);写盘 rename 单测;监护进程重启/停止策略的纯逻辑单测。
- 交叉编译验收:两个 DLL 与 exe 的 objdump 依赖检查;`DllRegisterServer` 等导出符号存在。
- 实机清单 `test-kit/tsf.txt`(由 `pime/test-kit/` 移来):全新安装(无 PIME)、从旧附加包升级、同版本覆盖安装;记事本、Win11 新记事本、Edge、Chrome、Word、微信、资源管理器地址栏、开始菜单搜索;一个 32 位程序;托盘图标与右键菜单;任务管理器结束 worker 后继续打字;登录后无黑窗;卸载后语言列表与文件干净。基准:不劣于 0.1.15。

## 实施顺序(每步可单独验收)
1. `tsf/` 收编 + `build-tsf.sh`,编出两个 DLL(仍连 PIME Launcher,装进现有 PIME 可替代补丁 DLL 实测,确认收编无回归)。
2. 服务端:`--worker` 管道模式 + 监护进程 + 看门狗 + rename 写盘 + `pyime-pipe-cli`;stdin 模式保留。用户在 Windows 上用 cli 验收。
3. DLL 改连新管道、超时与退避、断线处理、日志、新 GUID。附 `test-kit/tsf-dev.bat`(regsvr32 注册/反注册两份 DLL、起停 `--supervise`),不依赖安装器即可实机验收。
4. 安装器重写与迁移。
5. 出实机测试包,用户按清单验证。
6. 删除 PIME 遗留:stdin 协议代码、`pime/`、`scripts/build-pime-dll.sh`、`scripts/package-pime-zip.sh`、`installer/zip-pime/`、`installer/input_methods/`;`scripts/windows/user-reload.ps1` 改为停启 `pyime-server.exe`;改写 CLAUDE.md 的 PIME 段落(删除前列清单请用户确认)。

## 许可证
项目 GPL-3.0-only。收编部分:libIME2 为 LGPL-2.1(`libIME2/LICENSE.txt`);PIMETextService 文件头为 "Library GPL v2 or later"(`PIMEClient.cpp:4-7`;PIME `LICENSE.txt:5-12` 记作 LGPL 2.0);jsoncpp 为公有领域/MIT;`tsf_compat.h` 取自 windows-rs(MIT/Apache-2.0)。收编部分不转 GPL,按原 LGPL 分发修改版(保留原声明),与 GPL-3 的其余部分组合发布;各许可证全文放 `tsf/`,并随安装包分发。
