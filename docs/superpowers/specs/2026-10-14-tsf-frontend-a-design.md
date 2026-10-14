# 子项目 A:自写 TSF 前端打通主链路

日期:2026-10-14　状态:待复核
前置:`2026-10-14-tsf-gnu-spike-design.md`(已通过:mingw 产出的 TSF DLL 可在 Win11 注册、加载、上屏)。

## 目标
摆脱 PIME 的第一步:用自写 TSF DLL + 独立 server 进程,让 PYIME 在记事本、浏览器输入框、Windows Terminal 中完成"输入拼音 → 候选窗 → 选词上屏",并保留现有会话行为(模糊音、学习、标点、Shift 切换)。
引擎 crate 与会话逻辑不改;候选窗只做最小可用样式。

## 范围
做:
- `pyime-protocol`:协议类型与编解码。
- `pyime-backend` 加 `lib.rs` 导出会话相关模块;PIME 版 `main.rs` 行为不变。
- `pyime-server`:管道服务端、适配层、最小候选窗。
- `pyime-tsf`:搬青简 `com/*`(按键、组字串、上屏、光标矩形),重写 `client/`。

不做:绿色高亮、⌄ 展开、字号配置、主题(子项目 B);安装包、32 位 DLL(C);清理 PIME 后端、`Session` 重构为类型化状态机(D)。

## 组件与边界
| 组件 | 内容 | Linux 可测 |
|---|---|---|
| `pyime-protocol`(新 crate) | `ClientMessage`/`ServerMessage`/`Frame`;4 字节小端长度前缀 + JSON;管道名 `\\.\pipe\pyime` | 是 |
| `pyime-backend`(现有) | 新增 `lib.rs` 导出 `session`/`ime`/`punct`/`inputlog`/`log`;`main.rs` 改用库 | 是(现有 42 个测试) |
| `pyime-server`(新 bin) | 管道服务端;每会话一个 `Session`;适配层;最小候选窗 | 适配层是 |
| `pyime-tsf`(spike 扩展) | 搬青简 `com/*`,保留按键/组字串/上屏/光标矩形;重写 `client/` | 否(只交叉编译) |

适配层(server 内纯函数):
- `KeyEvent → PIME 风格 JSON 请求`(`filterKeyDown`/`onKeyDown`/`filterKeyUp`/`onKeyUp`,含 `keyCode`、`keyStates`)。
- `响应 JSON → Frame`(`commitString`、`compositionString`、`compositionCursor`、`candidateList`、`candidateCursor`)。
- `Session::handle` 不改。

## 数据流
1. `OnTestKeyDown` → `Key{test:true}` → server `filterKeyDown`,只回"吃不吃"。
2. `OnKeyDown` → `Key{test:false}` → server `onKeyDown`,回 `KeyResult{eaten, commit, preedit, cursor, candidates, highlight}`。
3. TSF 经 edit session 更新组字串;有 `commit` 则上屏。
4. TSF 把光标矩形报给 server,server 在该处画候选窗(横排、序号、普通高亮)。
5. 方括号自动补全仍由 server 进程 `SendInput` 注入方向键,沿用现有回声保护。

## 错误处理
- 管道连不上:TSF 放行所有按键,同时尝试拉起 `pyime-server.exe`,输入不被卡住。
- server 崩溃:下次按键重连,丢当前组字串与会话状态。
- 日志沿用 `%LOCALAPPDATA%\PYIME\log.txt`(`log.disabled` 关闭)。

## 测试与验收
Linux 自动:协议编解码往返;适配层(测试引擎驱动 `Session`);现有 42 个后端测试保持通过。
Windows 实机(用户操作):
1. 记事本输入 `ni`,候选窗出现在光标处,按 `1` 上屏"你"。
2. 翻页(`-`/`=`)、空格上屏、回车上屏拼音。
3. 单击 Shift 切中英文。
4. 选过的词再输入,排名上升(学习生效)。
5. 杀掉 server 后输入不被卡、按键放行;重新输入可自动恢复。
6. 浏览器输入框与 Windows Terminal 同样可用。

## 许可证
- 搬青简代码保留文件头版权声明;`installer/NOTICE.md` 增加"部分代码源自青简(GPL-3.0-or-later)"。
- 不使用"青简"名称与 logo。

## 已知风险
- 每次 `OnTestKeyDown` 一次管道往返,延迟需实机观察。
- server 进程 `SendInput` 注入的方向键会重新进入 TSF key sink,依赖现有回声保护。
- 候选窗在高 DPI、多显示器、全屏程序下的定位待实机验证。
