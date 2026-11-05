# 首键暂存补发 + 漏键诊断 设计

日期:2026-11-05 · 基于 main = 616afd8(0.1.18)· 经 grilling 逐项与用户确认。

## 背景与证据

- 用户反馈:0.1.18 仍有"拼音状态下首键不响应",多在已正常上屏若干次后新输入时出现,频率比以前低。
  现象:首字母以英文原样进了文本,组字从第二个字母开始(如 `d` + `ui` 的候选)。
- 主要场景是 Mullion(Rust winit 0.30 + egui 的 SSH 终端),其次是 chrome / Claude 桌面版 / WeChatAppEx。
- `tsf.log`(11-02 ~ 11-04 10:38,0.1.17 时期):打字时的放行 55 条,全是"字母键未交给输入法:
  键盘开=1 焦点上下文=按键上下文(禁用=1 空=1)",chrome 29 / Claude 25 / WeChatAppEx 1,均 Chromium 系;
  管道断开、退避只出现在 `onDeactivate` 或连接时。连续放行只报第一次,实际次数更多。
- `log.txt`(11-03 21:22 ~ 11-04 10:38)Mullion 会话 4520 次 filterKeyDown:中文状态下字母回 false 的 26 次全是
  Ctrl/Shift 组合;无"filterKeyDown 回 true 却没有 onKeyDown"。tsf.log 中 Mullion 放行 0 条。
  **结论:Mullion 里丢的键根本没交给 PYIME**(推测窗口当时无输入法上下文,待 Mullion 侧日志证实)。
- 以 `i`/`u`/`v` 开头的组字全日志 6 次,全部在 Mullion(il、ilo、i+退格、in+退格、it、ia+退格)。
- 能与 log.txt 对上的 Chromium 放行仅 2 次:Claude 一次放行后 0.72 s 正常输入框才来下一个键;
  chrome 一次之后 10 分钟无请求(更像网页非输入区按键)。

## 需求 1:首键暂存补发(Chromium 路径)

### 触发(全部满足,所有程序一律,不按 exe 名单)
- 中文状态(DLL 记住的 `english_` 为 false,且服务端告知过 `hasEnglish_`);
- 未在组字;
- 字母键 A–Z,Ctrl/Alt/Shift 均未按下;
- 按键上下文 `GUID_COMPARTMENT_KEYBOARD_DISABLED` 与 `GUID_COMPARTMENT_EMPTYCONTEXT` 均为非 0
  (即日志里 55 次的形态;键盘关闭等其他放行原因不暂存);
- 不是自动重复(lParam 第 30 位为 0;按住不放的键不暂存,暂存期间来了自动重复即立即补发、该键放行);
- 不是本模块自己补发的键(见"补发标记");
- 开关未关闭(需求 3)。

满足时 `*pfEaten = TRUE`,按键入暂存队列(记 vk、扫描码、扩展键标志、入队时刻),首个入队时起 0.5 s 计时。

**入队点**:`OnTestKeyDown` 与 `OnKeyDown` 两个入口(有的宿主不调前者)都做判断,以先到者入队;
后到的同一键(vk 与 lParam 相同、其间无其他按下)只回 eaten,不重复入队。

### 补发(统一用 SendInput 原序重发整个队列)
触发补发的四种时机,先到先算:
1. **焦点切换**:`ITfThreadMgrEventSink::OnSetFocus` 收到新的文档管理器 → 立即补发。
2. **隔间变化**:在按键上下文的 `KEYBOARD_DISABLED`、`EMPTYCONTEXT` 两个隔间上挂 `ITfCompartmentEventSink`
   (仿 `ImeEngine.cpp` 现有挂法),任一变为 0 → 立即补发。覆盖"Chromium 只改隔间值、不换文档管理器"的可能。
3. **下一个键先到**:暂存期间又来按键——
   - 仍满足暂存条件的字母:追加入队(保序),不重启计时;
   - 文本类键(字母、数字、标点、空格、回车、退格):吃掉、追加入队,立即补发;
   - 其余键(修饰键、Win/Apps、F 键、方向键、媒体键、PrintScreen 等):不吃、不入队,只触发立即补发已暂存的字母。
4. **超时**:0.5 s(`kStashTimeoutMs`)到期 → 补发。

补发后的键重新走 TSF:若此时上下文已正常,照常组字;若仍是禁用+空上下文,按补发标记放行给程序
(效果等于今天的放行,只是晚了最多 0.5 s)。

### 回放在途
调用 SendInput 之后、注入键全部回到本模块之前为"在途"。SendInput 只保证排在调用之后按下的键前面,
调用前已在线程消息队列里的物理键会先到(焦点切换路径正是在宿主处理 IPC 时回调,最易命中)。
在途期间:不带补发标记的文本类按下一律吃掉、排入下一轮队列;在途键全部到达后再发下一轮。
在途超过 1 s 仍未收齐(宿主丢了注入键)即放弃在途状态,下一轮照发,并记日志。

### 补发标记与细节
- 注入时 `INPUT.ki.dwExtraInfo` 填本模块魔数,按键回来时用 `GetMessageExtraInfo()` 识别
  (宿主在消息分发中同步调 `TestKeyDown` 时成立;实现时日志验证一次)。带标记的键不再暂存。
- 补发时若用户正按着 Shift:同一次 SendInput 里先抬起 Shift、发完再按回(同 `ime.rs` 成对括号)。
  带标记的 Shift 事件在 DLL 本地直接放行(`pfEaten=FALSE`)、不发 RPC,否则服务端的单击 Shift 判定
  (`session.rs` `shift_pending`)会把注入的 Shift 抬起当成单击、切到英文,暂存字母以英文漏出。
- 用户物理抬起的被吃掉的那个键,宿主会收到一个无对应按下的 KeyUp,不处理(与现状 TSF 吃键时一致)。
- 计时用本线程 `SetTimer`(无窗口 + 静态 TIMERPROC,经 timer id → 实例映射找回 TextService);
  `onKillFocus`(`ITfKeyEventSink::OnSetFocus(FALSE)`,Alt+Tab 走这里)、失活、析构时 `KillTimer` 并丢弃队列,不补发。
  SendInput 前再校验 `GetForegroundWindow()` 所属线程即本线程,否则丢弃,避免字母注入到别的程序。

### 副作用(已与用户确认可接受)
中文状态下在网页非输入区按单字母快捷键(YouTube `k`、Gmail `j`/`k`),最多晚 0.5 s 生效;按住不放的字母不暂存,自动重复照旧;英文状态不受影响。

### 结局日志(tsf.log)
每次暂存结束记一条:`首键暂存:<结局> 等待 <ms> ms 共 <n> 键`,结局为 焦点切换 / 下一个键 / 超时;
补发后若仍放行,另记 `首键暂存:补发后仍为禁用上下文,放行`。不记是哪个键。用几天后按数据调超时。

## 需求 2:漏键诊断(Mullion 路径)

- 不用 `ITfKeyTraceEventSink`:它由 keystroke manager 在 TestKeyDown/KeyDown 内部触发,而窗口无 HIMC 时
  (winit `set_ime_allowed(false)` 走 `ImmAssociateContextEx(hwnd, NULL)`)imm32 根本不进 TSF,大概率不会被调用。
- 直接用本进程线程级 `SetWindowsHookEx(WH_GETMESSAGE, …, nullptr, GetCurrentThreadId())`(不是全局钩子),
  只看本线程取出的 `WM_KEYDOWN`;本模块的 `OnTestKeyDown`/`OnKeyDown` 收到同一键时标记"已到"。
  对应 `WM_KEYUP` 时若该键按下从未到达本模块,且为中文状态、无修饰键的字母,则记一条:
  `字母键未经输入法:焦点文档=<有/无> 窗口输入法上下文=<有/无> 距上次上屏 <ms> ms`。
  "窗口输入法上下文"为 `ImmGetContext(GetFocus())` 是否非空;"上次上屏"为本模块最近一次提交的时刻。
- 按 `TsfLog.h` 口径不记是哪个键;同一秒内至多记一条,防刷屏。钩子随 TextService 激活挂、失活摘。
- 本版不做补偿(补字母 + 退格),等日志与 Mullion 侧日志对时证实根因后再定。

## 需求 3:隐藏开关

- `fuzzy.txt` 中一行 `首键暂存` 表示关闭(与 `纠错` 同写法);不加托盘入口。
- 服务端在 `init` 回复里带 `"firstKeyStash": bool`,DLL 记住;缺省(旧服务端)视为开启。
- 引擎 `FuzzyConfig` 不认识这一行(前端开关不进引擎 crate)。后端自己扫 `fuzzy.txt` 判断;
  托盘菜单改模糊音/纠错重写 `fuzzy.txt` 时,后端把原文件里的 `首键暂存` 行追加保留。
  (既有缺陷:重写会丢用户手写的注释等未知行,本次不修,另报。)
- 生效方式:改文件后重启输入法(worker 重启 → DLL 重连 `init`)。

## Mullion 侧(另开会话,不在本分支)

在 Mullion 仓库记:每次 `set_ime_allowed`(带调用来源)、收到 `Ime::Enabled/Disabled`、
组字结束后几百毫秒内又有字母以普通 `KeyboardInput` 到达。复现一次,与 PYIME tsf.log 对时。

## 不做

全局低级键盘钩子;按 exe 名单;托盘开关;Mullion 补字母 + 退格。

## 验收(test-kit/tsf.txt 新增一节)

1. Chrome 地址栏/网页输入框、Claude 桌面版、微信小程序里,中文状态连续上屏多次后新输入,首字母不再漏出;
   tsf.log 有 `首键暂存:` 结局行。
2. 网页非输入区(如 YouTube)中文状态按 `k`:约 0.5 s 内生效,tsf.log 记"超时"与"补发后仍为禁用上下文,放行"。
3. 暂存期间快速连打 `dui`:组字为 `dui`,顺序不乱。
4. `fuzzy.txt` 加 `首键暂存` 并重启输入法:行为回到 0.1.18;托盘改一次模糊音后该行仍在。
5. Mullion 里复现首键丢失:tsf.log 有"字母键未经输入法"一行,与 Mullion 侧日志对时。
6. 记事本、WPS 打字无变化,tsf.log 无暂存行。
7. 暂存期间按着 Shift 补发:不切到英文、不弹"英"提示,字母正常组字。
8. 暂存后 0.5 s 内 Alt+Tab 到记事本:记事本不出现该字母。
