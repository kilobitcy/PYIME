# 学习防污染 + lue/nue 召回修复 设计

日期:2026-10-15 · 分支:`feat/learning-hygiene`(基于 main = v0.1.7)

## 背景与根因(已用用户真实数据复现)

- 现象:`qinjixu` 首选是"亲继续",期望"请继续"。
- 用本机 0.1.7 引擎 + 用户 `user.txt` 复现出与截图完全一致的排序;空白学习时"请继续"首位;
  删去 `qinjixu	亲继续	4` 一行即恢复。根因是**错误选择被学习**。
- 用户纠错方式:上屏后按退格删掉,再拆开重打(`qin`→请、`jixu`→继续)。
  现有撤销(`session.rs` `finish_commit`)只认"退格后重打**同一串**拼音并改选",故未触发;
  错词登顶后顺手空格又再学一次,自我强化。`user.txt` 中同类污染还有
  车路饿、更随、主峰子、中民国才、的不配为、去搜说、卫生革命、金宏义 等。
- 另:`chelue` 永远召回不了"策略"。rime-ice 中"略/掠/虐"等的读音键是 `lve`/`nve`,
  而 `lue`/`nue` 本身也是合法音节,直接查不到 `lve`/`nve`(实测 `celve` 出"策略",`celue` 不出)。

## 目标

1. 上屏后用退格删光即撤销该次学习(A)。
2. 提供 Ctrl+Shift+数字 删除候选的个人学习(D),兼作存量清理。
3. `lue`/`nue` 与 `lve`/`nve` 等价。

非目标:整句"选两次才生效"(B,暂缓)、日志补版本号/"看过没选"事件、错例报告工具、
模糊整句与精确生僻组合的排序(如 `wobugangyu`)。

## A. 退格删光即撤销学习(`crates/pyime-backend/src/session.rs`)

- `Last.backspaced: bool` 改为 `backspaces: usize`;`note_passthrough` 中退格时累加。
- 累加后若 `backspaces >= text.chars().count()`:写 `inputlog::retract(&learned)`,
  对每项调用 `ime.unlearn`,随后 `self.last = None`(防重复撤回)。
- 保留原规则:`finish_commit` 中 `backspaces > 0 && 同一 typed && 不同 text` 仍撤回
  (此时尚未删光的情形,例如只删了部分后整串重打)。
- 其他键清空 `last` 的现有行为不变(方向键、回车、标点等视为接受上屏)。
- 已知局限:光标不在上屏文字末尾时的退格会被误计;后果仅为少学一次,可接受。

## D. Ctrl+Shift+数字 删除学习(engine + backend)

### 引擎
- `UserModel::forget(&mut self, word: &str)`:删除 `pairs` 中词为 `word` 的全部条目、
  `words[word]`、`next` 中后继为 `word` 的全部条目。词库不受影响。
- `Engine::forget(&mut self, word)` 透传;`Ime::forget(word)` 透传并存盘(与 learn 同一存盘路径)。

### 后端
- 组字中按下 Ctrl+Shift+N(N 为当前页有效序号):对该候选 `forget`,写日志
  `{"ts":…, "forget": 词, "input": 当前拼音}`,刷新候选(保持当前输入),不上屏。
- 修正现状:Ctrl 按下时数字键不得当作选词;Ctrl+Shift+数字在组字中由输入法接管,
  非组字时放行。其他 Ctrl+数字 组合在组字中同样不选词(放行)。
- `pyime-replay` 识别 `forget` 行,调用 `engine.forget`,保证回放与线上一致。

### 效果
- 词库外整句(如"亲继续")从学习补充中消失,最多以整句组合出现在后面;词库词仅失去个人加分。

## 2. lue/nue 规范化(`crates/pyime-engine/src/fuzzy.rs`)

- `variants()` 入口把 `lue→lve`、`nue→nve`,规范写法作为精确项(模糊次数 0、规则位 0),
  后续模糊展开基于规范写法(如 `nue` 经 n=l 得 `lve`)。
- 切分(`pinyin.rs`)不变:两种写法都已是合法音节。

## 测试

- 引擎单测:`forget` 删除三类条目且不影响其他词;`variants("lue")` 首项为 `lve` 精确;
  `nue` 经 n=l 得到 `lve`。
- 真实词典回归(忽略型,`PYIME_DICT`):`chelue`、`celue` 召回"策略"。
- 后端会话测试:
  - 学过"亲继续"后上屏,退格 3 次 → 撤回,`qinjixu` 首选不再是"亲继续";
  - 只退格 1 次 → 不撤回;退格中途按方向键 → 不撤回;
  - 组字中 Ctrl+Shift+2 → 第 2 候选被 forget,不上屏,候选刷新;
  - 非组字时 Ctrl+Shift+数字 → 放行;组字中 Ctrl+数字 → 不选词。
- 离线验证:用用户 `user.txt`(仅本机 `/tmp`,不入库)对"亲继续" forget 后,
  `qinjixu` 首选恢复"请继续"。

## 发布

- 版本 0.1.8;release 说明写:退格删光撤销学习、Ctrl+Shift+数字删除学习词、lue/nue 修复。
- 用户升级后用 Ctrl+Shift+数字 清理已污染的词。push/tag/发布前先征得同意。
