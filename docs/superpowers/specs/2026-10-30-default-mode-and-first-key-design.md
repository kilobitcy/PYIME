# 按程序默认中/英文 + 首键丢失 设计

日期:2026-10-30 · 基于 main = 1d71948(工作区含未提交的首键诊断改动与版本号 0.1.17)· 经 grilling 逐项与用户确认,
按复核意见修订一次

## 背景

- 需求 1:现在所有程序默认中文,希望默认英文;可配置哪些程序默认中文。
- 需求 2:经常第一个字母没交给输入法,如想打"对不起",`d` 直接进了应用,组字变成 `uibuqi`。
  用户说不清发生场景(切窗口后、开机后、停顿后都有)。
- 现状:
  - 服务端每条管道连接一个 `Session`,在首个请求时惰性创建(`hub.rs:50`),`english` 初值 `false`(`session.rs:126`)。
    DLL 每个 TextService 实例(约等于每进程每 UI 线程)各连一次,状态互相独立、手动切换后在该会话内保持。
  - DLL `init` 请求不带进程名(`PIMEClient.cpp` `Client::init`)。
  - 首键放行的已知路径:
    1. TSF 上下文禁用/空上下文/键盘未开(`TextService::OnTestKeyDown`/`OnKeyDown`);
       工作区未提交的 `reportPassThrough` 诊断会写 `tsf.log`。
    2. `waitForRpcConnection` 连接最多等 `kConnectTimeoutMs`=200 ms,失败放行并退避。
    3. 请求失败(`callRpcMethod` → `onRpcFailure`):断线、退避、放行。worker 被监护进程重启或崩溃后,
       DLL 手里的旧管道已断,下一个键必走这条路——代码层面确定的首键丢失。

## 需求 1:按程序默认中/英文

### 1.1 生效时机与状态保留

- 会话新建时按该程序的默认值设 `english`;之后手动切换(Shift、托盘左键)的结果在会话内一直保留,
  不随焦点切换重置。
- worker 重启后重连保留重启前状态:服务端在每个带 `addButton`/`changeButton` 的回复里附 `"english": bool`;
  DLL 记住最后一次值,重连时在 `init` 请求里带 `"english"`(可选字段)。服务端 `init` 收到即用它覆盖默认值,
  没有(首次连接)则按程序默认。旧 DLL 不带该字段,行为退回按程序默认,向后兼容。

### 1.2 识别程序

- `pipe.rs` 在连接的服务线程开头用 `GetNamedPipeClientProcessId` 取对端 PID,再
  `OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION)` + `QueryFullProcessImageNameW` 取 exe 全路径,只留文件名;
  经新增的 `Msg::Opened { conn, exe }` 交给 hub,hub 记下,Session 创建时传入(`Session::new` 签名相应调整,
  `session/tests.rs` 配套)。**不改 DLL 协议**取 exe 名。三个 API 在 `win.rs` 新增声明。
- 取不到(受保护进程、进程已退出等)→ exe 名为空,按"其他程序"处理,即默认英文。
- `log.txt` 记一行:`会话 [conn] exe=<名> 默认<中文|英文>`。

### 1.3 配置文件 `chinese-apps.txt`

- 位置:`%LOCALAPPDATA%\PYIME\chinese-apps.txt`(与 `user.txt`、`fuzzy.txt` 同目录);目录不存在先 `create_dir_all`。
  取不到数据目录(`user_path` 为 None)时只用内存中的初始列表,托盘勾选只改内存。
- 格式:每行一个 exe 文件名;去首尾空白,忽略空行;匹配不区分大小写(ASCII 折叠)。
  读入时去 BOM、CRLF→LF;非 UTF-8 则视为空列表并记日志,不覆盖原文件(勾选也不写回)。
- 列在表里的默认中文,其余全部默认英文。
- 文件不存在时写入初始列表:`Weixin.exe`、`WeChat.exe`、`wps.exe`、`et.exe`、`wpp.exe`;之后以用户编辑为准。
  删掉文件会在下次启动时重新写入初始列表;想让所有程序都默认英文,请保留一个空文件。
- 读取时机:worker 启动时读;托盘勾选改动后同步写盘并更新内存。不做热加载,手改后需重启输入法。

### 1.4 托盘菜单「本程序默认中文」

- 右键菜单新增勾选项「本程序默认中文(<exe>)」,`checked` = 当前会话 exe 在列表中;exe 名为空时置灰。
  勾选反映的是"默认",不是当前中/英状态。
- 点击:在列表中加入/移除当前 exe,写 `chinese-apps.txt`;同时当前会话立即切到对应状态
  (勾上→中文、取消→英文),回复带 `changeButton`(连同 `english`)同步图标。
- 组字中只改列表、不切状态(同"组字中左键单击托盘图标忽略";`onCommand` 回复无编辑会话)。
- 只切当前会话;同程序其他已存在会话不动,新会话按新列表。
- 命令 id 在 `tray.rs` 新增常量(不与现有 1/10–13/20–29 冲突)。

## 需求 2:首键丢失

### 2.1 断管道立即重连重发(修复)

- 条件,同时满足才重发:
  - 请求是按键类:`filterKeyDown`、`onKeyDown`、`filterKeyUp`、`onKeyUp`(`onDeactivate` 等不重发,免得宿主退出路径上白连一次)。
  - `callRpcPipe` 失败且错误为管道已断:`ERROR_BROKEN_PIPE`、`ERROR_NO_DATA`、`ERROR_PIPE_NOT_CONNECTED`
    超时、回复无法解析、seqNum 不符不重发,照旧 `onRpcFailure`。0 字节读不单独判断:回复为空会走"回复无法解析"照旧 `onRpcFailure`。
  - 当前无组字。有组字时新会话与 DLL 组字状态对不上,照旧断线清理。
- 动作:复用 `onRpcFailure` 前半段——`closeRpcConnection()` + `resetTextServiceState()`(清掉 `buttons_` 与 TSF 按钮,
  避免重连后 `onActivate` 的 `addButton` 留下重复图标);跳过 `backoffFail()` 与 `abandonComposition()`;
  再 `waitForRpcConnection()`(连接、`init` 带 `english`、`onActivate`)→ 用新 seqNum 重发同一请求一次。
  重连或重发失败才 `onRpcFailure`(断线、退避、放行)。
- 耗时预算:重发使用首次请求剩余的时间(与首次请求共用 kRpcTimeoutMs=2000 ms 的截止时间);重连中的 init/onActivate 握手各自受 kRpcTimeoutMs 约束(与原有断线重连一致),连接受 kConnectTimeoutMs 约束。最坏情况(断管道后连接、init、onActivate 均卡到超时)宿主 UI 线程约冻 4.2 s,高于原先约 2 s;重发若已无剩余时间则以 0 超时发出、照旧放行。
- 只重发一次,不循环;`init`/`onActivate` 握手途中再失败不触发二次重连(沿用 `shouldWaitConnection_` 防递归)。
- `tsf.log` 记:`<method> 管道已断(err=…),已重连重发` 或 `…重连失败,放行`。
- 已知边界(本版不修):
  - worker 刚重启、仍在加载词典时管道尚不存在,`WaitNamedPipe` 立即失败 → 进退避,该键及其后约 1 s 内的键照旧放行。
    本修复只覆盖"worker 已就绪、用户回来继续打字"。
  - 若某按键请求本身让 worker 崩溃,重发会再崩一次,多耗一次监护重启预算(60 s 内 5 次);日志里的"重发"可据此识别。
  - 服务端对无法解析的请求不回复并断开,DLL 侧同样表现为断管道,会先重发一次再进 `onRpcFailure`(有界)。

### 2.2 取证日志

- 随本版发布工作区未提交的 `reportPassThrough` 诊断(TSF 上下文禁用/空上下文/键盘未开)。
- 补齐其余放行路径在 `tsf.log` 的原因记录:连接失败放行、退避期内放行、请求失败放行。
  只记原因,不记是哪个键;退避期内的放行按进程去重(退避状态本就按进程共享),每个退避周期只报第一次。
- 用户实机复现后贴回 `tsf.log` + `log.txt`,再针对路径 1(TSF 上下文)对症修;本版不做推测性加固
  (不加长连接超时、不在焦点切换时预连接)。

## 不做

- 焦点切换时重置中/英状态;全局"全部默认中文"开关;`chinese-apps.txt` 热加载;worker 加载期的丢键。

## 测试

- 后端单测:默认状态按 exe 名决定(在表/不在表/空名/大小写);`init` 带 `english` 时覆盖默认;
  `addButton`/`changeButton` 回复带 `english`;初始列表仅文件不存在时写入、空文件不重写;非 UTF-8 不覆盖;
  托盘勾选加入/移除、当前会话切换、组字中不切、空 exe 置灰。
- DLL:无单测框架,实机验证——
  - 记事本(默认英文)中 Shift 切中文,结束 `pyime-server.exe --worker`(监护进程重启它),等几秒回记事本打 `dui`:
    首键不丢、仍为中文、托盘只有一个图标且状态正确;`tsf.log` 有"已重连重发"。
  - 微信里同样操作(默认中文)结果相同。
- 实机:微信/WPS 新开默认中文、记事本/浏览器默认英文;托盘勾选即时生效且写入文件。

## 发布

- 两项一起作为 0.1.17;发布前先问用户。
