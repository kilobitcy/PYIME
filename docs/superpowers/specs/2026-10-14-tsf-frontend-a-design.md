# 子项目 A:自写 TSF 前端打通主链路

日期:2026-10-14　状态:已按复核意见修订,待确认
前置:`2026-10-14-tsf-gnu-spike-design.md`(已通过:mingw 产出的 TSF DLL 可在 Win11 注册、加载、上屏)。

## 目标
摆脱 PIME 的第一步:自写 TSF DLL + 独立 server 进程,让 PYIME 在记事本、浏览器输入框、Windows Terminal 中完成"输入拼音 → 候选窗 → 选词上屏",保留现有会话行为(模糊音、学习、标点、成对符号、Shift 切换)。引擎与 `Session` 逻辑不改;候选窗只做最小样式。

## 已定决策
- 管道直传 PIME 风格 JSON:消息体就是 `Session::handle` 的请求/响应 JSON;类型化协议留到子项目 D 与 `Session` 重构一起做。
- TSF 基于 spike 的自写 `com/service.rs` 扩展,按需参照青简(`/tmp/qingjian`,GPL-3.0-or-later)。仅在直接拷贝某段代码时才加版权头并更新 `installer/NOTICE.md`。

## 范围
做:`pyime-protocol`(信封与帧)、`pyime-backend` 加 `lib.rs` 与 `hub`、`pyime-server`(新 crate)、`pyime-tsf` 增加组字串/光标矩形/管道客户端。
不做:绿色高亮、⌄、字号、主题(B);安装包、32 位(C);清理 PIME 后端、`Session` 类型化重构(D)。

## 组件
| 组件 | 内容 | Linux 可测 |
|---|---|---|
| `pyime-protocol`(新) | 4 字节小端长度前缀的帧编解码;信封 `ClientMessage{Open{sid,host}, Req{sid,json}, Rect{sid,l,t,r,b}, Close{sid}}`、`ServerMessage{Resp{sid,json}}`;管道名 `\\.\pipe\pyime-<用户 SID>` | 是 |
| `pyime-backend`(现有) | 新增 `lib.rs` 导出 `session`/`ime`/`punct`/`inputlog`/`log`/`hub`;`main.rs` 改用库,PIME 版行为不变。`hub`:`Hub{sessions: HashMap<u64,Session>}`,`handle(sid,&Value)->Value`、`open`/`close`,测试放在 crate 内(可用 `#[cfg(test)]` 的 `Ime::with_engine`) | 是 |
| `pyime-server`(新 crate,Windows) | 管道服务、候选窗、单实例;依赖 `pyime-backend` 库与 `windows`,不把 Windows 依赖带进 PIME 后端 | 否 |
| `pyime-tsf`(扩展) | 组字串、`GetTextExt` 光标矩形、管道客户端、超时与放行逻辑 | 否 |

## server 线程模型
- 一个主线程同时做引擎与 UI:独占 `Rc<Ime>`、全部 `Session` 与候选窗,跑 Win32 消息循环。
- 每条管道连接一个 I/O 线程:读帧 → 经 `mpsc` 送主线程并 `PostMessage` 唤醒 → 等响应 → 写回。跨线程只传帧字节与 `serde_json::Value`(均为 `Send`)。
- 单实例:命名互斥体 `Local\PYIME-Server` + 管道首实例标志(`FILE_FLAG_FIRST_PIPE_INSTANCE`);后者保证不会有两个 server 同时改写 `user.txt`。
- 管道 DACL:授予当前用户与 `ALL APPLICATION PACKAGES`,参照 PIME `PipeServer`,以便 UWP/Store 宿主连接。

## 按键与数据流
1. `OnTestKeyDown` → `Req{method:"filterKeyDown", keyCode, keyStates(256 项,GetKeyboardState), charCode(ToUnicode)}`,只用响应的 `return` 决定吃不吃。
2. `OnKeyDown` → `Req{method:"onKeyDown", ...}`,按响应字段更新:`commitString` 上屏;`compositionString`/`compositionCursor` 更新组字串;`showCandidates`、`candidateList`、`candidateCursor` 决定候选窗。语义沿用 PIME:字段缺省 = 不变;`compositionString:""` = 清空;`showCandidates:false` = 隐藏。
3. Shift 单击:`Session` 在 `filterKeyUp` 里切换并可能返回 `commitString`。TSF 在 `OnTestKeyUp` 发 `filterKeyUp` 并缓存响应;响应含 commit/清空组字串时返回 TRUE,让 `OnKeyUp` 取缓存经 edit session 应用,避免丢拼音。
4. 组字被宿主终止(`OnCompositionTerminated`):TSF 补发一次 `onKeyDown` 的 `VK_ESCAPE`,让 `Session` 清空组字且保留中英文状态。
5. 光标矩形:每次组字串变化后 TSF 发 `Rect`;server 在此处显示候选窗。
6. 方括号等成对符号补全的方向键仍由 server 进程 `SendInput` 注入,沿用现有回声保护。
7. 密码框/`GUID_COMPARTMENT_KEYBOARD_DISABLED`:TSF 一律放行,不向 server 发按键。

## 候选窗(最小)
横排、序号、普通高亮;窗口样式 `WS_EX_NOACTIVATE|WS_EX_TOOLWINDOW|WS_EX_TOPMOST`(不抢焦点,否则注入的方向键会打到窗口自己);server 以 per-monitor v2 DPI 感知启动(manifest)。参照青简 `ui/candidates`,直接拷贝时才加版权头。

## 错误处理
- 管道读写使用 overlapped,单次往返超时 150 ms;超时、断开一律按"放行"处理,不阻塞宿主 UI 线程。
- DLL 侧拉起 `pyime-server.exe`:每进程至多每 2 秒尝试一次;多进程同时拉起由单实例互斥体收敛。
- server 崩溃/重启:会话状态丢失,下次 `Open` 重建。
- 日志:server 沿用 `%LOCALAPPDATA%\PYIME\log.txt`;DLL 追加写 `%LOCALAPPDATA%\PYIME\tsf.log`(不轮转,`log.disabled` 同样关闭)。

## 部署布局(开发期)
`C:\PYIME\` 内放 `pyime_tsf.dll`、`pyime-server.exe`、`pyime.dict`;DLL 按自身模块目录找 server,server 按自身目录找词典。正式安装包属于子项目 C。

## 测试与验收
Linux 自动:帧编解码往返;`hub` 单测(开/关会话、按键往返、Esc 清空保留英文状态);现有全部测试保持通过。
Windows 实机(用户操作):
1. 记事本输入 `ni`,候选窗出现在光标处,按 `1` 上屏"你"。
2. 翻页(`-`/`=`)、空格上屏、回车上屏拼音、退格、Esc。
3. 组字中单击 Shift:拼音原样上屏且切到英文,不丢字。
4. 中文标点、成对括号补全与跳过。
5. 选过的词再输入,排名上升(学习生效)。
6. 点击别处终止组字后再输入,状态正常。
7. 结束 server 进程 / 挂起 server:输入不卡,按键放行;恢复后自动重连。
8. 浏览器输入框、Windows Terminal 同样可用;密码框不弹候选窗。

## 已知风险
- 每次 `OnTestKeyDown` 一次管道往返,延迟需实机观察。
- 提权宿主(管理员窗口)受 UIPI 限制,`SendInput` 注入与管道连接均可能失败,与现有 PIME 方案同样的限制,不在 A 里解决。
- 候选窗在高 DPI、多显示器、全屏程序下的定位待实机验证。
- 多用户/RDP 场景依赖管道名带用户 SID,未专门测试。

## A3 部署
开发期把 `pyime_tsf.dll`(`scripts/build-tsf.sh` 产出)、`pyime-server.exe`、`pyime.dict` 放进同一目录 `C:\PYIME\`;管理员终端 `regsvr32 C:\PYIME\pyime_tsf.dll` 注册,语言列表里添加"PYIME(自研预览)"。DLL 按自身目录找 `pyime-server.exe` 并在管道不可用时自动拉起(每进程至多每 2 秒一次)。若装过 spike 版,先 `regsvr32 /u` 旧 DLL。日志:`%LOCALAPPDATA%\PYIME\tsf.log`(DLL)、`log.txt`(server)。
