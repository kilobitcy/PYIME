# 成对括号光标与中/英切换提示 设计

日期:2026-11-04 状态:方向已与用户确认,待审阅

## 背景与依据

1. **括号补全后选中了右括号**(用户截图 2026-11-04):中文标点下打 `(` 上屏 `()`,`session.rs` `punctuate` 经 `ime.press` → `send_key` 用 `SendInput` 模拟一次 ←。打 `(` 要按住 Shift,模拟 ← 时 Shift 常未松开,应用收到 Shift+← 即选中 `)`。跳过右括号时模拟的 → 同样会变成 Shift+→。
2. **"首键不参与选词"实为误切英文**(用户日志 2026-11-04,13 小时):
   - 截图那次:中文打 `panfu` 退格删光 → 单击左 Shift(按下约 95ms)切到英文 → 打 `pan` 直接进文档 → 退格、Shift 切回中文、再退格 → 文档剩 `p`,接着打 `andian`。组字下划线从 `a` 起,与日志一致。
   - `tsf.log` 无放行记录;后端日志中文状态下无修饰键的字母放行 0 次,排除首键丢失。
   - Shift 共切换 63 次:切到英文后 8 秒内又切回 16 次(英文期间多为拼音 duo/shen/qi/pan,另有 4 次什么都没打就切回,像在试探状态)。中/英状态目前只在托盘图标上,打字时看不到。
   - "退格后短时间内的 Shift 不算切换"不可取:退格后按 Shift 切回中文且保持的有 26 次。

## 目标

- 成对括号上屏后光标停在括号中间、跳过右括号时光标越过它,与 Shift 是否松开无关。
- 单击 Shift 切换中/英后,光标旁短暂显示"中"或"英"。

## 不做

- 不改切换规则(单击 Shift 照旧切换,不加退格后屏蔽)。
- 托盘左键、"本程序默认中文"菜单引起的切换不提示(焦点在任务栏/菜单,光标位置不可靠)。
- 不校验跳过右括号时光标后是否真是该右括号(沿用现有 `closers` 栈判断)。

## 设计

### 1. 光标移动:回复字段 `caretMove`

后端(`crates/pyime-backend/src/session.rs`):

- `punctuate` 中 `Mapped::Open` 不再 `inject(VK_LEFT)`,改为在回复里写 `caretMove = -1`;`Mapped::Close` 命中 `closers` 栈顶时不再 `inject(VK_RIGHT)`,改为 `caretMove = 1`(仍吃掉该键、不上屏)。`moved` 的含义不变(光标已移动,不再跟踪退格)。
- `punctuate` 现返回 `Option<String>`;光标移动量经会话字段暂存(如 `caret_move: i32`),在 `on_key_down` 组装回复处写入并清零,写法同 `commitString`。
- 删掉随之无用的:`inject`、`injected`、`inject_echo`/`INJECT_ECHO`、`note_passthrough` 开头对 VK_LEFT/VK_RIGHT 回声的跳过、`ime.rs` 的 `press`/`send_key`/`pressed`/`take_pressed`。

前端(`tsf/`):

- libIME2 `TextService` 加 `moveCaret(EditSession*, int n)`:取当前选区,`n < 0` 时 `ShiftStart(n)` 后向起点折叠,`n > 0` 时 `ShiftEnd(n)` 后向终点折叠,`SetSelection`。
- `PIMEClient.cpp` `handleRpcResponse`:在上屏(`updateCommitString`)与组字(`updateComposition`)处理之后,若 `caretMove` 为非零整数、有编辑会话且不在组字中,调 `moveCaret`。无会话(filterKeyDown/filterKeyUp/onCommand)时忽略。
- `(` 的回复走 `onKeyDown`,带编辑会话;`updateCommitString` 已 `endComposition`,光标在 `()` 之后,左移 1 即到中间。

### 2. 切换提示:回复字段 `modeHint`

后端:单击 Shift 切换(`session.rs` filterKeyUp 分支调用 `toggle_english` 处)时在回复里写 `modeHint` = `"中"` 或 `"英"`(切换后的状态)。托盘与菜单的切换不写。

前端:

- filterKeyUp 的回复没有编辑会话,不能直接 `showMessage`(`updateMessageWindow` 判空前执行会崩宿主,见 CLAUDE.md)。`handleRpcResponse` 收到 `modeHint` 且无会话时,仿 `abandonComposition` 的写法自行 `RequestEditSession`(`TF_ES_ASYNCDONTCARE | TF_ES_READ`,lambda 持 `ComPtr<TextService>`),在会话内取光标矩形并显示提示。
- 取光标矩形:现有 `selectionRect` 仅在组字时返回矩形,新增 libIME2 `caretRect(EditSession*, RECT*)`,写法同 `selectionRect` 但不要求组字(`GetActiveView` → 当前选区 `GetTextExt`);失败或矩形全为 0 时不显示(不落到屏幕左上角)。新增 `showModeHint(session, text)` 用它定位,不改 `showMessage`/`selectionRect`。
- 显示约 1 秒(`SetTimer`);下一次按键进入 `onKeyDown` 时先隐藏,免得与候选窗叠在一起。窗口复用 `MessageWindow`(白底、阴影已有)。

### 3. 兼容

服务端与 DLL 同包发布;旧 DLL 遇新字段忽略(括号光标不移动,仅此一版过渡),新 DLL 遇旧服务端无新字段即不动作。

## 测试与验收

- Rust 会话测试:改写现有 4 处 `take_pressed` 断言为回复中的 `caretMove`(打 `(` 为 -1、跳过 `)` 为 1、其他标点无此字段);删掉注入回声相关测试;新增"单击 Shift 切换回复带 modeHint 中/英""托盘左键切换不带 modeHint"。
- DLL:`scripts/build-tsf.sh` 交叉编译 x64/x86 通过;无单测。
- 实机(`test-kit/tsf.txt` 新增一节):记事本、浏览器、微信中按住 Shift 连打 `()`,光标在中间,再打 `)` 越过右括号;单击 Shift 光标旁闪"英"约 1 秒,再单击闪"中";组字中单击 Shift(拼音原样上屏并切英文)也提示;托盘左键切换不提示。
