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
模糊整句与精确生僻组合的排序(如 `wobugangyu`)、组字中 Ctrl+字母(Ctrl+C/V 等)放行、分段 forget。

## A. 退格删光即撤销学习(`crates/pyime-backend/src/session.rs`)

- `Last.backspaces: usize` 取代 `backspaced: bool`;放行的退格在 `note_passthrough` 中累加,
  **只在 `filterKeyDown` 计数**(`onKeyDown` 不重复计),避免同一次按键被计两次。
- 累加后若 `backspaces >= text.chars().count()`:写 `inputlog::retract(&learned)`,
  对每项调用 `ime.unlearn`,随后 `self.last = None`(防重复撤回)。
- 保留原规则:`finish_commit` 中 `backspaces > 0 && 同一 typed && 不同 text` 仍撤回
  (只删了部分后整串重打的情形)。
- **标点结尾的上屏也受保护**:`punctuate()` 现在 `commit_all()` 后清空 `last`;改为
  若本次有组词上屏,保留 `last` 并把输出的标点字符并入 `Last.text`(退格阈值随之计入标点)。
  成对符号(`Mapped::Open`,会注入左方向键)例外,仍清空 `last`。
- **上文同步回退**:放行的退格每计一次,`context` 去掉末字;删光时清空 `context`。
  否则删掉"亲继续"后重打 `qin`→请 会学到假搭配 (亲继续 → 请)。
- 其他键清空 `last` 的现有行为不变(方向键、回车等视为接受上屏)。
- 已知局限(后果至多少学或少撤一次,可接受):光标不在上屏文字末尾时退格被误计;
  Ctrl+Backspace 整词删除只计 1 次;选中后删除(Shift+方向键/鼠标)不触发撤销。

## D. Ctrl+Shift+数字 删除学习(engine + backend)

### 引擎
- `UserModel::forget(&mut self, word: &str)`:删除 `pairs` 中词为 `word` 的全部条目、
  `words[word]`、`next` 中后继为 `word` 的全部条目。词库不受影响。
- `Engine::forget(&mut self, word)` 透传;`Ime::forget(word)` 透传并存盘(与 learn 同一存盘路径)。

### 后端
- 组字中按下 Ctrl+Shift+N(N 为当前页有效序号):对该候选 `forget`,写日志
  `{"ts":…, "forget": 词, "input": 当前拼音}`,刷新候选(保持当前输入;页码仍有效则保持当前页),不上屏。
- 修正现状:Ctrl 按下时数字键不得当作选词;Ctrl+Shift+数字在组字中由输入法接管,
  非组字时放行。其他 Ctrl+数字 组合在组字中同样不选词(放行)。
- `pyime-replay` 识别 `forget` 行(须在解析普通选词行之前判断,否则会被静默跳过),
  调用 `engine.forget`,保证回放与线上一致。

### 效果与局限
- 词库外整句(如"亲继续",本次污染即为整串 pair `qinjixu→亲继续`)从学习补充中消失,
  最多以整句组合出现在后面;词库词仅失去个人加分。
- 若整句排前是**分段学习**造成的(例如学过 `qin→亲`),`forget("亲继续")` 不改变排序。
  本次不做分段 forget;forget 后候选文本序列无变化时,日志记 `"noop": true`,供后续评估。

## 2. lue/nue 规范化(`pinyin.rs` 定义,三处共用)

- `pinyin::canonical(s) -> &str`:`lue→lve`、`nue→nve`,其余原样。
- `fuzzy::variants()`:入口先规范化,规范写法作为精确项(模糊次数 0、规则位 0),
  模糊展开基于规范写法(如 `nue` 经 n=l 得 `lve`);同步修改其文档注释
  ("第一项恒为精确写法"改为"第一项恒为规范化后的精确写法")。
- `fuzzy::partial()`:输出音节经 `canonical` 映射并去重,使残缺前缀 `chelu` 也能命中 `lve`。
- `rime::parse_line()`:音节逐个 `canonical`,保证换词库或追加词库时 `lue` 键不会变得不可达。
- 切分(`syllables_at`)不变:两种写法都已是合法音节。

## 测试

- 引擎单测:`forget` 删除三类条目且不影响其他词;`variants("lue")` 首项为 `lve` 精确;
  `nue` 经 n=l 得到 `lve`。
- 真实词典回归(忽略型,`PYIME_DICT`):`chelue`、`celue` 召回"策略"。
- 后端会话测试:
  - 学过"亲继续"后上屏,退格 3 次 → 撤回,`qinjixu` 首选不再是"亲继续";
  - 只退格 1 次 → 不撤回;退格中途按方向键 → 不撤回;
  - 整句 + 逗号上屏("亲继续，"),退格 4 次 → 撤回;成对括号上屏后 → 不跟踪;
  - 同一退格的 `filterKeyDown` 与 `onKeyDown` 只计一次;
  - 删光后重打 `qin`→请,不产生 (亲继续 → 请) 搭配;
  - 残缺前缀 `chelu` 召回"策略"(引擎单测用小词典);
  - 组字中 Ctrl+Shift+2 → 第 2 候选被 forget,不上屏,候选刷新;
  - 非组字时 Ctrl+Shift+数字 → 放行;组字中 Ctrl+数字 → 不选词。
- 离线验证:用用户 `user.txt`(仅本机 `/tmp`,不入库)对"亲继续" forget 后,
  `qinjixu` 首选恢复"请继续"。

## 发布

- 版本 0.1.8;release 说明写:退格删光撤销学习、Ctrl+Shift+数字删除学习词、lue/nue 修复。
- release 说明须写明:**已学进去的错词(如重复选过 4 次的"亲继续")A 只撤最近一次,
  需用 Ctrl+Shift+数字 手动删除**;否则升级后会误以为退格撤销无效。
- push/tag/发布前先征得同意。
