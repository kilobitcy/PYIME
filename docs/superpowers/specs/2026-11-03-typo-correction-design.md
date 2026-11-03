# 按键纠错(误按键)设计

日期:2026-11-03 状态:已与用户确认设计,待审阅 spec

## 背景与依据

用户举例:想打"推荐",按成 `tuojian`。`tuo` 本身是合法音节,切分不报错,只会拼出无意义整句。

据 0.1.17 日志(约 15.6 小时、748 次上屏,分析脚本 `/tmp/typo_ana/`)统计:

- 可确认的误按约 12 例(≈1.5%),集中在 i/u/o 相邻键互错(`kailv`→考虑、`jueju`→解决)、漏打 o(`xiachenxu`→小程序)、多打字母(`thuijian`→推荐)、其他相邻键(m/n、y/h)、字母颠倒(`nie`→内容)。
- 多数误按在多打 1–2 个字母后即被发现;纠错真正有收益的是"打完整个词、首选是乱拼整句"的 3–4 例/天。
- 错拼本身是合法词时(`gongkao`→公考)纠错帮不上,也不应触发。

同音选错(福利/复利 等)更多,但多为首次遇到、需上下文才能分辨,学习一次即纠正;上下文加分已于 2026-10-28 评估无效,留待装完整 `pyime.gram` 后的新日志再归因,不在本设计范围。

## 目标

- 输入找不到整段词时,给出"改一处即成词"的候选,放在第 2 位。
- 不影响现有能找到整段词的输入的排序;空格上屏结果与现在一致。
- 学习记在正确拼音下,`user.txt` 不出现错拼。
- 可在托盘菜单关闭,默认开。

## 不做

- 不改整句 Viterbi,纠错不进拼句。
- 一个词内不纠两处;不纠单字;不学错拼。

## 设计

### 1. 生成变体:新文件 `crates/pyime-engine/src/typo.rs`

`pub fn edits(input: &str) -> Vec<String>`:对 ASCII 小写输入只改一处,去重,不含原串。四类:

- 替换:换成 QWERTY 相邻键(按键盘三排写一张邻接表,同排左右 + 上下排斜邻,如 i 的邻键为 u、o、j、k)。
- 删除:删掉任意一个字母(`thuijian`→`tuijian`)。
- 互换:相邻两个字母对调(`nie`→`nei`)。
- 插入:任意位置补 a、e、i、o、u、g、h 之一(`xiachenxu`→`xiaochenxu`)。

纯字符串函数,不依赖词典,单独单测。

### 2. 接入引擎:`engine.rs` `candidates_after`

触发条件(全满足):

- `FuzzyConfig.typo` 为真;
- 输入长度 ≥ 4;
- `words` 中没有 `start == 0 && end == input.len() && !partial` 的词(精确、模糊、用户短语都算,末尾残缺补全不算)。

对每个变体,从 0 沿切分格收"吃满整段、音节完整(非残缺)"的词(复用 `grow`,可叠加已开启的模糊音),过滤:

- 至少 2 个字;
- 在词典该拼音键的前 `PER_KEY` 个以内(沿用 `top_entries`)。

得分 = `ln(词频) − 模糊惩罚 × 模糊次数 − TYPO_PENALTY + user.bonus(纠正后拼音, 词)`。按分取不重复、且不在现有候选中的词,至多 `TYPO_MAX`(=2)个。

放置:插在下标 1(第 2 位);现有候选为空时从下标 0 起。在英文候选、符号放置之后、`truncate` 之前插入(触发时必无精确整段词,不会挂符号)。

候选字段:

- `source`:新增 `Source::Typo`(回归按来源分桶);
- 新增 `fixed: Option<String>`:纠正后的拼音,其余来源为 `None`;
- `consumed = input.len()`,`cuts` 为空(组字串原样显示用户键入,同用户短语),`parts` 为空;`fuzzy`/`pairs` 取纠正后匹配的模糊信息。

新增常量:`TYPO_PENALTY`(具体值实现时以回归用例定)、`TYPO_MAX = 2`、`TYPO_MIN_LEN = 4`。

### 3. 学习:`crates/pyime-backend/src/session.rs` `select`

- 候选 `fixed` 为 `Some(p)` 时,`ime.learn` 与 `learned` 记录的拼音用 `p`;撤回走同一份记录,保持一致。
- `inputlog::select` 与 `typed_all` 仍记用户实际键入的拼音,便于日志回放。

### 4. 开关

- `FuzzyConfig` 新增 `typo: bool`,`Default` 为真,`none()` 为假。
- `fuzzy.txt` 中出现一行 `纠错` 表示关闭;`parse_off` 识别、`export_off` 在关闭时写出。十对规则的下标与 `rule_names` 不变。
- 托盘:模糊音子菜单末尾加分隔线与"按键纠错"(勾选态 = `typo`),新命令 id `CMD_TYPO = 15`;`session.rs` 处理该 id,调 `ime.toggle_typo()`,即时生效并写 `fuzzy.txt`,日志记"菜单:按键纠错 开/关"。
- 不占用 `CMD_FUZZY + 10`,现有"越界 id 不处理"测试保持不变。

## 测试与验收

单测:

- `typo.rs`:四类改法各一例(`tuojian`→含 `tuijian`;`thuijian`→含 `tuijian`;`nie`→含 `nei`;`xiachenxu`→含 `xiaochenxu`),结果不含原串、无重复。
- 引擎:`tuojian` 第 2 位为"推荐"、来源 `Typo`、`fixed == "tuijian"`;有整段词(`gongkao`)时不触发;`typo` 关时不触发;单字不出;输入短于 4 不触发。
- `FuzzyConfig`:`纠错` 行的读写。
- 会话:选中纠错候选后按 `tuijian` 学习;撤回后恢复;托盘 `CMD_TYPO` 切换并写 `fuzzy.txt`。

回归:

- 改前 `PYIME_RANKS_OUT=/tmp/base.tsv` 存逐例名次,改后 `PYIME_RANKS_BASE=/tmp/base.tsv` 比对;原有用例名次不应变化。
- 新增用例:`kailv`→考虑、`xiachenxu`→小程序、`thuijian`→推荐、`tuojian`→推荐,期望名次 ≤ 2。

性能:用 `real_engine` 实测触发路径,目标比当前多出 ≤ 5ms;超出则收窄插入字母集或替换邻键范围。

实机:装包后在记事本打 `tuojian`、`kailv`,确认第 2 位出现正确词;托盘关闭"按键纠错"后不再出现。
