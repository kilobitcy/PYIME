# 成对括号光标与中/英切换提示 设计

日期:2026-11-04 状态:方向已与用户确认;已按独立复核意见修订,待审阅

## 背景与依据

1. **括号补全后选中了右括号**(用户截图 2026-11-04):中文标点下打 `(` 上屏 `()`,`session.rs` `punctuate` 经 `ime.press` → `send_key` 用 `SendInput` 模拟一次 ←。打 `(` 要按住 Shift,模拟 ← 时 Shift 常未松开,应用收到 Shift+← 即选中 `)`。跳过右括号时模拟的 → 同样会变成 Shift+→。
2. **"首键不参与选词"实为误切英文**(用户日志 2026-11-04,13 小时):
   - 截图那次:中文打 `panfu` 退格删光 → 单击左 Shift(按下约 95ms)切到英文 → 打 `pan` 直接进文档 → 退格、Shift 切回中文、再退格 → 文档剩 `p`,接着打 `andian`。组字下划线从 `a` 起,与日志一致。
   - `tsf.log` 无放行记录;后端日志中文状态下无修饰键的字母放行 0 次,排除首键丢失。
   - Shift 共切换 63 次:切到英文后 8 秒内又切回 16 次(英文期间多为拼音 duo/shen/qi/pan,另有 4 次什么都没打就切回,像在试探状态)。中/英状态目前只在托盘图标上,打字时看不到。
   - "退格后短时间内的 Shift 不算切换"不可取:退格后按 Shift 切回中文且保持的有 26 次。
3. **复核发现的两个现有缺陷**(与本设计同处,一并修):
   - 切换在 `filterKeyUp` 里做,该回复没有编辑会话(`PIMEClient.cpp` `updateStatus` 在 `session == nullptr` 时跳过上屏与组字),所以**组字中单击 Shift 时拼音并不会上屏**,而后端已 `flush_raw`,界面与会话失同步。
   - TSF 对同一次抬键先后调 `OnTestKeyUp`、`OnKeyUp`,两者都会发 `filterKeyUp`(部分宿主如此,日志中连接 13 每键两次)。第二次落到 `shift_combo_up = now`,使 300ms(`SHIFT_GUARD`)内的下一次单击 Shift 被当成误触不切换。

## 目标

- 成对括号上屏后光标停在括号中间、跳过右括号时光标越过它,与 Shift 是否松开无关;所有应用通用(含只走 IMM32 兼容层的程序)。
- 单击 Shift 切换中/英后,光标旁短暂显示"中"或"英"。
- 组字中单击 Shift:拼音原样上屏并切英文(修上面缺陷一);连续两次单击 Shift 都能切换(修缺陷二)。

## 不做

- 不改切换规则(单击 Shift 照旧切换,不加退格后屏蔽)。
- 托盘左键、"本程序默认中文"菜单引起的切换不提示(焦点在任务栏/菜单,且 onCommand 回复无会话)。
- 括号光标不改走 TSF 选区:复核指出只走 IMM32 兼容层的程序里 `SetSelection` 对已上屏文本无效,会让随后打的 `)` 被吞掉。

## 设计

### 1. 括号光标:模拟方向键时临时抬起 Shift(`crates/pyime-backend/src/ime.rs` `send_key`)

- 发方向键前用 `GetAsyncKeyState` 查左、右 Shift。有按着的,在**同一次** `SendInput` 里发 `[按着的 Shift ↑, 方向键 ↓, 方向键 ↑, 同一 Shift ↓]`;都没按着则照旧只发方向键。`SendInput` 保证同一次调用的事件不与用户或其他注入的输入交错。
- Shift 用 `VK_LSHIFT`/`VK_RSHIFT` 及各自扫描码(0x2A/0x36),不带 `KEYEVENTF_EXTENDEDKEY`;方向键仍带该标志。
- 剩余竞态:查询与 `SendInput` 之间(微秒级)用户恰好松开 Shift,会使 Shift 逻辑上停在按下,直到下次按 Shift。可接受,写入代码注释。
- 注入的 Shift 也会经输入法回来。会话按现有逻辑:用户按 Shift+9 时 `shift_pending` 已因 `9` 清掉;回来的 Shift ↑ 记 `shift_combo_up`,回来的 Shift ↓ 因刚记过而判 `shift_misfire`,用户随后松开 Shift 不切换。不对 Shift 做回声跳过(300ms 窗口内无法区分注入与用户真实松开,跳过反而丢掉真实事件),补测试锁住"按住 Shift 打 `(`、经回声、再松开不切换"。

### 2. 切换改到带会话的 `onKeyUp`(`session.rs`)

- `filterKeyUp` 收到 Shift 抬起、`shift_pending` 为真且非误触时:不再当场切换,改记 `toggle_on_up = true` 并回 `return: true`(TSF 随后调 `OnKeyUp` → 带编辑会话的 `onKeyUp`)。
- 同一次抬键的第二次 `filterKeyUp`(`toggle_on_up` 已为真):同样回 `true`,且**不**记 `shift_combo_up`(修缺陷二)。
- `onKeyUp` 收到 Shift 且 `toggle_on_up`:清标记,`toggle_english`,并写 `showMessage = { message: "中"|"英", duration: 1 }`,回复 `return: false`(不吃 Shift 抬起,宿主仍收到 Shift↑;`filterKeyUp` 仍回 `true` 以便 TSF 调 `OnKeyUp`)。组字中切到英文时 `commitString` 现在有会话,能上屏(修缺陷一)。
- 保险:`toggle_on_up` 仍为真时若先来了任何 `filterKeyDown`(宿主没调 `OnKeyUp`),当场切换且不提示;组字中不补(无会话的回复会丢 `commitString`),直接丢掉这次切换。
- 托盘、菜单切换仍直接 `toggle_english`,不写 `showMessage`。

### 3. 提示窗(`tsf/`)

- 复用现有 `showMessage` 协议字段与 `TextService::showMessage`。PIME 原写法在不组字时临时开一个空组字取位置再结束,网页编辑器会因此收到一对空的 compositionstart/end;改为不开组字:`Client::updateMessageWindow` 去掉临时开/结束组字,`showMessage` 不组字时用新增的 libIME2 `caretRect`(写法同 `selectionRect`,不要求组字)取当前选区矩形。
- 定位回退:组字时 `selectionRect`、不组字时 `caretRect` 取不到或矩形全为 0 时,依次退到 `GetGUIThreadInfo` 的 `rcCaret`(转屏幕坐标)、本上下文最近一次候选窗位置;都没有则不显示(不落到屏幕左上角)。
- 外观:`MessageWindow::onPaint` 桌面模式改为与候选窗一致(白底、1px #E5E5E5 边框、文字 #333,字体同候选窗),去掉 `COLOR_INFOBK` 与 3D 边框。后端此前不发 `showMessage`,不影响其他提示。
- 收起:约 1 秒计时;下一次 `filterKeyDown` 开头先隐藏(切到英文后字母被放行、不进 `onKeyDown`,故放在 `filterKeyDown`;隐藏不需要会话),免得与候选窗叠在一起。

### 4. 兼容

- 第 1 节只改服务端,旧 DLL 同样生效。
- 第 2 节用的是 TSF 标准流程与 PIME 原有字段,旧 DLL 也能切换并显示提示(外观、定位回退要新 DLL)。
- 吃掉 Shift 的 `WM_KEYUP` 后,个别应用是否误以为 Shift 仍按着须实机确认(见验收)。

## 测试与验收

- Rust 会话测试:按住 Shift 打 `(`(含注入回声序列)再松开不切换;单击 Shift:`filterKeyUp` 回 true 且不切换,`onKeyUp` 切换并带 `showMessage` 中/英;双 `filterKeyUp` 后 300ms 内再次单击仍切换;组字中单击 Shift 在 `onKeyUp` 回复带 `commitString`;`toggle_on_up` 悬空时下一次 `filterKeyDown` 补切换;托盘左键切换不带 `showMessage`。`send_key` 只在 Windows 下编译,事件序列抽成纯函数单测(给定左右 Shift 状态 → 事件列表)。
- DLL:`scripts/build-tsf.sh` 交叉编译 x64/x86 通过;无单测。
- 实机(`test-kit/tsf.txt` 新增一节):记事本、Chrome/Edge、微信、WPS 中按住 Shift 连打 `()`,光标在中间,再打 `)` 越过右括号;单击 Shift 光标旁闪"英"约 1 秒、再单击闪"中",快速连按两下也各切一次;组字中单击 Shift 拼音原样上屏并切英文;切换后打字母不变大写(Shift 未卡住);记下哪些应用提示没出现或位置不对;托盘左键切换不提示。
