# TSF 文本服务 mingw 交叉编译可行性 Spike

日期:2026-10-14　状态:已按复核意见修订,待确认

## 背景
目标是摆脱 PIME,自写 TSF 前端(参考青简 `apps/windows/tsf`,GPL-3.0-or-later,可并入本项目 GPL-3.0-only)。
最大未知数:本机 mingw 交叉编译(`x86_64-pc-windows-gnu`)能否产出可注册、可被宿主加载且稳定的 TSF DLL。青简自身只在 Linux 上 `cargo check` gnu 目标,真编译走 MSVC + `crt-static`,故 gnu 未经验证。
本 spike 只回答这一问题,不搬青简代码;TSF 用法(类别集合、引用计数等)只参照青简,以隔离变量。

## 范围
新分支 `spike/tsf-gnu`;新增 crate `crates/pyime-tsf`(cdylib),依赖 `windows` / `windows-core` 0.62.2(与青简一致,是否有更新版本未确认)。

### 阶段 0:空壳 DLL(先做,半小时内出工具链结论)
- 导出 `DllGetClassObject`、`DllCanUnloadNow`、`DllRegisterServer`、`DllUnregisterServer` 的空实现,加 `DllMain`(保存 `HINSTANCE`,供 `GetModuleFileNameW` 取自身路径)。
- 自检:`objdump -p` 过依赖与导出表;Windows 上 `regsvr32` 注册、反注册均成功。
- 阶段 0 不过则直接得出结论,不进入阶段 1。

### 阶段 1:最小 TSF 逻辑
- COM:`DLL_REFERENCES` 引用计数(对象构造/析构 + `IClassFactory::LockServer`),`DllCanUnloadNow` 据此返回。
- 注册:`HKCR\CLSID\{新GUID}\InprocServer32`(ThreadingModel=Apartment);`ITfInputProcessorProfiles` 登记 profile(名称"PYIME 实验",zh-CN,宽字符串须含结尾 0);`ITfCategoryMgr` 登记与青简 `registry/mod.rs` 相同的类别集合(含键盘类 TIP 及沉浸式/托盘等,否则 Win10/11 切换器会过滤掉)。
- `ITfTextInputProcessor`:`Activate` 注册 key sink(`fForeground=TRUE`),`Deactivate` 注销。
- `ITfKeyEventSink`:`OnTestKeyDown` 对 a–z 返回 TRUE;`OnKeyDown` 经 edit session 用 `ITfInsertAtSelection` 上屏"测试";edit session 失败时返回 FALSE 放行按键。`OnTestKeyUp`/`OnKeyUp`/`OnPreservedKey`/`OnSetFocus` 返回 FALSE/Ok。
- 使用全新 GUID,不触碰 PIME 及 PYIME v0.1.x 的任何注册项。

不做:拷贝青简代码、引擎、server/IPC、候选窗、组字串、安装包、32 位 DLL。

Linux 兼容:Windows 专有依赖置于 `[target.'cfg(windows)'.dependencies]`;crate 在 Linux 上为空壳,开工时确认 `cargo test --workspace` 与 `clippy` 无新增报错。

## 构建与自检(本机)
- 不改 workspace 的 profile,避免波及 `pyime-backend`:单次命令
  `cargo rustc --release --target x86_64-pc-windows-gnu -p pyime-tsf -- -C panic=abort <静态链接 rustflags>`。
- 静态链接 libgcc/winpthread 的具体 rustflags 在阶段 0 试出并记入本 spec。
- `x86_64-w64-mingw32-objdump -p <dll>`:DLL Name 不得含 libgcc/libwinpthread(须对 DLL 本身检查,exe 的检查结果不能代替);导出表含 4 个符号。

## Windows 实机验收(用户操作)
- DLL 固定放在 `C:\PYIME-spike\`;`regsvr32` 须在管理员终端运行。
1. 注册成功,输入法列表出现"PYIME 实验"。
2. 记事本、浏览器输入框、Windows Terminal 切换到它,按 a–z 上屏"测试";某宿主不支持时按键被放行也算正常,须在记录里注明。
3. 连续使用约 10 分钟,宿主不崩溃。
4. `regsvr32 /u` 后注册项清除;注销重新登录后 DLL 文件可删除;PIME 与 PYIME v0.1.7 不受影响。

## 判定
- 全部通过 → 进入阶段 1(搬青简 `tsf/`,新建 `pyime-protocol`)。
- 编译或加载失败 → 整理错误报告;备选为在 Windows 上用 MSVC 构建,由用户决定。
- 能加载但行为异常 → 崩溃先看事件查看器 Faulting module 是否为本 DLL;上屏异常先对照青简实现排除 TSF 用法错误,再归咎工具链。

## 交付物
本 spec;DLL 位于 `target/x86_64-pc-windows-gnu/release/`,不入库。
