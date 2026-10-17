# 整句逐词学习(仅模糊词) 设计

日期:2026-10-17 · 分支:`feat/sentence-word-learning`(基于 main = 0c3581c)

## 背景与根因(已用用户真实日志复现)

- 回放 `/tmp/input-log.jsonl`(630 次选词,约 7.6 小时):首选 80.3%,前 3 名 91.4%。
- 名次 ≥ 4 的 54 条中最大一类是"`qin` + 词 → 请X",共 14 条(请思考、请检查、请确认、请删除、请打开、请推荐……);
  `woxiwan → 我希望` 同理。
- `qin` 精确匹配"亲/秦","请"(`qing`)要走 in=ing 模糊。每个**新**的"请X"都排第 4–5 名;
  选过一次后该整句升到第 1,但换个"请Y"又从头开始。
- 根因:选中 Viterbi 整句时,`engine.rs` `sentences()` 回溯得到的词序列在拼成字符串后丢弃,
  `session.rs` `select` 只学到 `(qinshikao, 请思考)`,学不到 `(qin, 请)`。学习无法推广。
- 上述用例全是"模糊词输给精确同音词"。参照 libime `HistoryBigram`(整句按词入库)。

## 目标

选中整句候选时,除学整句外,把句中**经模糊匹配得到**的完整词按"普通选词"学一次
(同一精确层、同等权重,含个人搭配),使"请""希望"等的选择能推广到新句子。

只学模糊词的理由:`PAIR_BASE = 8` + 词层 `3 + ln n` 是为显式选词校准的,足以翻盘同音字。
若句中每个词都学,选一次"我看到她了"就隐式学到 `(ta, 她)` +11,此后 `taishenme` 首选"她是什么";
的/得/地、在/再 同理且随整句累积。精确词(`fuzzy == 0`)不学,同音字偏置风险归零。

非目标:近期优先/衰减、个人搭配归一化、`user.txt` 格式改动、独立的弱学习层、
`matching()` 注入用户短语不受模糊开关约束(既有行为,见"已知局限")。

## 1. 引擎:整句候选带出模糊词(`crates/pyime-engine/src/engine.rs`)

- `Candidate` 新增 `pub parts: Vec<(usize, usize, String)>`:整句中**模糊且完整**的词
  (`Word.fuzzy > 0 && !Word.partial`)的 `(起, 止, 词)`,起止为输入中的字节位置,按出现顺序。
- 只有 `sentences()` 产出的整句填写;单词候选、用户短语为空。
  `matching()` 注入的用户短语 `fuzzy == 0`,自然不会被当成"词"再学一次。
- 新增 `Candidate::lessons(&self, typed: &str, context: &str) -> Vec<(String, String, String)>`,
  返回 `(typed[起..止], 词, 上文)`;上文为"`context` + 整句中该词之前的全部文字"的末尾 3 个字
  (与 `tail` 一致,前面的精确词也计入上文)。为此回溯时一并记下每个模糊词在句文字中的起点。
- `typed` 必须是 `input[..consumed]`(从当前输入开头起),与 `parts` 偏移同基准。

## 2. 后端(`crates/pyime-backend/src/{ime,session}.rs`)

- `Ime::learn` / `Ime::unlearn` 增加参数 `lessons: &[(String, String, String)]`:
  在同一次 `borrow_mut` 内学完(或撤完)候选自身与全部 lessons,**只 `save` 一次**
  (现有每次 `learn` 都整文件写 `user.txt`,不能按 lesson 次数放大)。lessons 的 pairs 记 0。
- `Learned` 增加第 5 项 `lessons`;`select` 中 `lessons = c.lessons(&typed, &ctx)`,随候选一起记入。
  撤回(退格删光、整串重选)逐项 `unlearn` 时连带撤 lessons。分段选完的整句那条 lessons 为空。
- **日志格式与语义均不变**:`inputlog::retract` 仍只写 `(typed, text, pairs, prev)` 四元组,
  不写 lessons。`pyime-misses` 以末项识别误选组(`pyime-misses.rs:110`)的约定不受影响。

## 3. 回放(`crates/pyime-backend/src/bin/pyime-replay.rs`)

- 选中 `cands[r]` 时用 `typed = &input[..cands[r].consumed]` 计算 `lessons`,逐项 `learn` + `learn_next`;
  并记入映射 `(typed, text, prev) → lessons`(覆盖旧值)。
- retract 行逐项 `unlearn` 时,查映射连带 `unlearn` 对应 lessons。后端撤回只针对最近一次上屏,
  映射取最近一次即正确。新旧日志行为一致(日志里本就不含 lessons)。
- `rank == None` 时无从拆词,照旧只学整句。

## 已知局限

- 隐式学到的模糊词无法从整句候选定点删除:对整句按 Ctrl+Shift+数字 只删整句文字的学习;
  需让该词单独出现在候选里再删。只学模糊词使影响面限于"模糊写法 → 某词"。
- `matching()` 会把学过的 `(qin, 请)` 以 `fuzzy: 0` 注入 `qin…` 的候选,即使之后关闭 in=ing;
  直接选词已有同样行为,本次不改。
- `matching()` 线性扫描全部 pairs;只学模糊词使条目增长有限,本次不做性能改动。

## 测试(先写测试,确认失败再实现)

测试词典需显式构造:`qin → 亲` 权重高于 `qing → 请`,"请检查"不在词典,in=ing 开启。

引擎单测:
1. 整句候选的 `parts` 只含模糊完整词,偏移正确;精确词、单词候选的 `parts` 为空。
2. 句末残缺模糊词不进 `parts`。
3. `lessons` 的片段与上文拼接正确(含 `context` 非空、模糊词前有精确词)。
4. 场景:学习前 `qinjiancha` 首选不是"请检查";按 `lessons` 学过"请思考"后首选为"请检查"。
5. 场景:学过含精确词"她"的整句后,`ta` 开头输入的排序不变。

后端单测:
6. 选中整句后 lessons 生效;退格删光后候选排序恢复到学习前。
7. retract 日志项仍为四元组,末项仍是候选自身/分段整句。

验收:
- `cargo test`、`cargo clippy` 通过。
- `pyime-replay installer/pyime.dict /tmp/input-log.jsonl --gram /tmp/gram/pyime.gram -v`:
  首选不低于 80.3%、前 3 名不低于 91.4%,"请X"类名次明显提前;
  对比改动前后 `-v` 输出,逐条列出新增或名次变差的项并分析。不可接受时另立设计。
