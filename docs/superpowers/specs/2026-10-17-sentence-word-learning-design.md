# 整句逐词学习 设计

日期:2026-10-17 · 分支:`feat/sentence-word-learning`(基于 main = 0c3581c)

## 背景与根因(已用用户真实日志复现)

- 回放 `/tmp/input-log.jsonl`(630 次选词,约 7.6 小时):首选 80.3%,前 3 名 91.4%。
- 名次 ≥ 4 的 54 条中最大一类是"`qin` + 词 → 请X",共 14 条(请思考、请检查、请确认、请删除、请打开、请推荐……);
  `woxiwan → 我希望` 同理。
- `qin` 精确匹配"亲/秦","请"(`qing`)要走 in=ing 模糊。每个**新**的"请X"都排第 4–5 名;
  选过一次后该整句升到第 1,但换个"请Y"又从头开始。
- 根因:选中 Viterbi 整句时,`engine.rs` `sentences()` 回溯得到的词序列在拼成字符串后丢弃,
  `session.rs` `select` 只学到 `(qinshikao, 请思考)`,学不到 `(qin, 请)`;
  词层与搭配层记的也是整句字符串,Viterbi 按词查时永远命中不了。学习无法推广。
- 参照 libime `HistoryBigram`:提交的句子按词序列入库,每个词记 unigram、相邻词记 bigram。

## 目标

选中整句候选时,除学整句外,还把句中每个完整的词按"普通选词"学一次(同一精确层、同等权重),
句内相邻词记个人搭配,使"请"等词的选择能推广到新句子。

非目标:近期优先/衰减、个人搭配归一化、`user.txt` 格式改动、独立的弱学习层(回放出现退步时再议)。

## 1. 引擎:整句候选带出词序列(`crates/pyime-engine/src/engine.rs`)

- `Candidate` 新增 `pub parts: Vec<(usize, usize, String)>`:句中每个词的 `(起, 止, 词)`,
  起止为输入中的字节位置,按出现顺序。
- 只有 `sentences()` 产出的整句填写;`assemble` 中的单词候选、用户短语为空。
- 末尾残缺音节凑出的词(`Word.partial == true`)不放入 `parts`,避免隐式学到 `(d, 的)` 之类缩写。
  残缺词只可能是句末一个,略去不影响前面各词的上文。
- 新增 `Candidate::lessons(&self, typed: &str, context: &str) -> Vec<(String, String, String)>`,
  返回 `(typed[起..止], 词, 上文)`:首词上文为 `context`;之后各词上文为
  "`context` + 前面各词文字"的末尾 3 个字(与 `tail` 一致)。
- `typed` 即选中时消耗的拼音(从当前输入开头起),与 `parts` 的偏移同基准。

## 2. 后端:选中时逐词学习(`crates/pyime-backend/src/session.rs` `select`)

- 对 `c.lessons(&typed, &ctx)` 每项调用 `self.ime.learn(片段, 词, 0, 上文)` 并压入 `self.learned`;
  现有撤回(退格删光、整串重选)逐项 `unlearn`,自动连带撤销。
- **顺序约束**:词级记录先于本次候选自身的记录压入。`pyime-misses` 以 retract 列表**末项**识别误选组
  (`pyime-misses.rs:110`),分段上屏时末项仍是整句,约定不变。
- 模糊规则偏好已由候选自身的 `pairs` 记过,词级记录 pairs 填 0,不重复计数。
- 日志格式不变:select 行照旧;retract 行自然多出词级项,`pyime-replay` 已能逐项 `unlearn`。

## 3. 回放(`crates/pyime-backend/src/bin/pyime-replay.rs`)

选中 `cands[r]` 后,同样对 `cands[r].lessons(typed, ctx)` 逐项 `learn` + `learn_next`,与后端一致。
未出现在候选中的选择(`rank == None`)无从拆词,照旧只学整句。

## 风险

- 单字词被拔高:整句每多一个词付 `WORD_COST = 18`,长词不会因此被拆;以回放验证。
- 误选放大:误选整句(如"亲确认")会顺带学到 `(qin, 亲)`。依赖现有撤回清理;
  未撤回的误选影响面比以前大,回放中逐条检查退步项。

## 测试(先写测试,确认失败再实现)

引擎单测:
1. 整句候选的 `parts` 切分正确;单词候选 `parts` 为空。
2. 句末残缺词不进 `parts`。
3. `lessons` 的片段与上文链拼接正确(含 `context` 非空时)。
4. 场景:按 `lessons` 学过"请思考"后,`qinjiancha` 首选为"请检查"(学习前不是)。

后端单测:
5. 选中整句后 `learned` 中词级项在前、候选自身项在末。
6. 退格删光后候选排序恢复到学习前。

验收:
- `cargo test`、`cargo clippy` 通过。
- `pyime-replay installer/pyime.dict /tmp/input-log.jsonl --gram /tmp/gram/pyime.gram -v`:
  前 3 名比例不低于基线 91.4%,"请X"类名次明显提前;逐条列出新增退步项并分析。
  出现不可接受的退步则改用独立弱学习层(另立设计)。
