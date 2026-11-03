# 按键纠错(误按键)设计

日期:2026-11-03 状态:已与用户确认设计;已按独立复核意见修订,待审阅

## 背景与依据

用户举例:想打"推荐",按成 `tuojian`。`tuo` 本身是合法音节,切分不报错,只会拼出无意义整句。

据 0.1.17 日志(约 15.6 小时、748 次上屏,分析脚本 `/tmp/typo_ana/`)统计:

- 可确认的误按约 12 例(≈1.5%),集中在 i/u/o 相邻键互错(`kailv`→考虑、`jueju`→解决)、漏打 o(`xiachenxu`→小程序)、多打字母(`thuijian`→推荐)、其他相邻键(m/n、y/h)、字母颠倒(`nie`→内容)。
- 其中 `jueju`→解决 需改两处(u→i、u→e),按"一个词内不纠两处"不覆盖。
- 多数误按在多打 1–2 个字母后即被发现;纠错真正有收益的是"打完整个词、首选是乱拼整句"的 3–4 例/天。
- 错拼本身是合法词时(`gongkao`→公考)纠错帮不上,也不应触发。

同音选错(福利/复利 等)更多,但多为首次遇到、需上下文才能分辨,学习一次即纠正;上下文加分已于 2026-10-28 评估无效,留待装完整 `pyime.gram` 后的新日志再归因,不在本设计范围。

## 目标

- 输入没有精确整段词、且纠错词得分高于现有最佳整段词与最佳整句时,给出"改一处即成词"的候选,放在第 2 位。
- 原有回归用例名次不变;空格上屏结果与现在一致(含候选为空时空格原样上屏字母)。
- 学习记在正确拼音下,`user.txt` 不出现错拼。
- 可在托盘菜单关闭,默认开。

## 不做

- 不改整句 Viterbi,纠错不进拼句。
- 一个词内不纠两处;不纠单字;不学错拼。
- 用户短语不参与纠错(变体只查词典);用户短语只用于触发判定。
- 原有候选为空时不插(如 `thuijian`:`th` 切不出音节,词表为空),保持空格原样上屏字母;此类不覆盖。

## 设计

### 1. 生成变体:新文件 `crates/pyime-engine/src/typo.rs`

`pub fn edits(input: &str) -> Vec<String>`:对 ASCII 小写输入只改一处,去重,不含原串。四类:

- 替换:换成 QWERTY 相邻键(按键盘三排写一张邻接表,同排左右 + 上下排斜邻,如 i 的邻键为 u、o、j、k)。
- 删除:删掉任意一个字母(`thuijian`→`tuijian`)。
- 互换:相邻两个字母对调(`nie`→`nei`)。
- 插入:任意位置补 a、e、i、o、u、g、h 之一(`xiachenxu`→`xiaochenxu`)。

纯字符串函数,不依赖词典,单独单测。

### 2. 接入引擎:`engine.rs` `candidates_after`

前置条件(全满足才生成变体):

- `FuzzyConfig.typo` 为真;
- `TYPO_MIN_LEN`(=4)≤ 输入长度 ≤ `TYPO_MAX_LEN`(=12;目标词 2–4 字,更长的输入几乎不可能整段成词,也限住每键开销);
- `words` 中没有 `start == 0 && end == input.len() && !partial && fuzzy == 0` 的词(精确词与用户短语算;模糊整段词不算,因 `tuojian` 经 an=ang 有模糊整段词"脱缰"(词频 2810),若排除就救不了这个首要例子);
- `cover(words, len) != Cover::Partial`(末尾残缺音节时不纠,免得 `tuij` 这类词中途闪出"突击",也护住 `yinp`、`kube` 等残缺回归用例)。

对每个变体,从 0 沿切分格收"吃满整段、音节完整(非残缺)"的词(复用 `grow`,可叠加已开启的模糊音),过滤:至少 2 个字;在该拼音键前 `PER_KEY` 个以内(沿用 `top_entries`)。

得分:直接用 `grow` 产出的 `Word.score`(已含词频、模糊惩罚、个人模糊折扣、抢音惩罚、以纠正后拼音为键的个人加分),纠错分 = `Word.score − WORD_COST − TYPO_PENALTY`。

门槛:与 `assemble` 同口径比分,纠错分须同时高于现有最佳整段词(`w.score − WORD_COST`,即模糊整段词)与最佳整句(`sentences` 首条分数);没有的一方视为通过。即只有现有结果明显不像话(拖见、开绿、脱缰 这类)时才出;"你是""我去"这类高分整句不受影响。按分取不重复、且不在现有候选中的词,至多 `TYPO_MAX`(=2)个。

放置:在 `assemble` 结果上插到下标 1(第 2 位),在英文候选与符号放置之前,英文现有放置规则不变(Cover::Exact 时英文仍按 `EN_SLOT` 插,纠错在其前;Cover::None 时英文照旧排最前)。现有候选为空时不插(保持空格原样上屏字母)。

候选字段:

- `source`:新增 `Source::Typo`(回归按来源分桶,`regression.rs` 的 `match c.source` 加分支);
- 新增 `fixed: Option<String>`:纠正后的拼音,其余来源为 `None`(`engine.rs` 现有四处 `Candidate` 字面量一并补上);
- `consumed = input.len()`,`cuts`、`parts` 为空(组字串原样显示用户键入,同用户短语);`fuzzy`/`pairs` 取纠正后匹配的模糊信息。

新增常量:`TYPO_PENALTY`(具体值以回归用例定)、`TYPO_MAX = 2`、`TYPO_MIN_LEN = 4`、`TYPO_MAX_LEN = 12`。

### 3. 学习与日志:`crates/pyime-backend/src/session.rs` `select`

- 候选 `fixed` 为 `Some(p)` 时,`ime.learn` 与 `learned` 记录的拼音用 `p`;撤回走同一份记录,学习与撤回一致。
- 分段选词:本次组词任一段为纠错候选时,跳过 `segments > 1` 的 `typed_all` 整句学习(否则会写入 `womentuojian⇥我们推荐` 这类错拼)。用一个会话字段记"本组词含纠错段",组词结束时清零。
- `typed_all` 与 `inputlog::select` 的 `typed` 仍记实际键入;`inputlog::select` 在纠错时多写一个 `fixed` 字段。`retract` 日志项来自 `learned`(记的是 `fixed`),回放按 select 的 `fixed`(有则用)与 retract 配对。

### 4. 开关

- `FuzzyConfig` 新增 `typo: bool`,`Default` 为真,`none()` 为假(新测试"纠错叠加模糊"须用 `default()` 再逐对关)。
- `fuzzy.txt` 中出现一行 `纠错` 表示关闭;`parse_off` 识别、`export_off` 在关闭时写出。十对规则的下标与 `rule_names` 不变。
- 托盘:顶层菜单"模糊音 ▸"之后加一项"按键纠错"(勾选态 = `typo`),新命令 id `CMD_TYPO = 15`;不放进模糊音子菜单,现有子菜单"恰 10 项"与 `fuzzy_items` 测试不受影响。`session.rs` 处理该 id,调 `ime.toggle_typo()`,即时生效并写 `fuzzy.txt`,日志记"菜单:按键纠错 开/关"。

## 测试与验收

单测:

- `typo.rs`:四类改法各一例(`tuojian`→含 `tuijian`;`thuijian`→含 `tuijian`;`nie`→含 `nei`;`xiachenxu`→含 `xiaochenxu`),结果不含原串、无重复。
- 引擎:`tuojian` 第 2 位为"推荐"、来源 `Typo`、`fixed == "tuijian"`;有整段词(`gongkao`)时不触发;末尾残缺(`tuij`)不触发;`typo` 关时不触发;单字不出;短于 4 或长于 12 不触发;最佳整句分高于纠错分时不出;无候选时不插。
- `FuzzyConfig`:`纠错` 行的读写。
- 会话:选中纠错候选后按 `tuijian` 学习、撤回后恢复;分段(先选前缀词再选纠错词)不做整句学习;inputlog 带 `fixed`;托盘 `CMD_TYPO` 切换并写 `fuzzy.txt`。

回归:

- 改前 `PYIME_RANKS_OUT=/tmp/base.tsv` 存逐例名次,改后 `PYIME_RANKS_BASE=/tmp/base.tsv` 比对;原有用例名次不应变化,`TYPO_PENALTY` 在此约束下取能让新用例通过的值。
- 新增用例:`kailv`→考虑、`xiachenxu`→小程序、`tuojian`→推荐,期望名次 2。
- 实测依据(installer/pyime.dict):推荐 501446、考虑 500740、小程序 13470;脱缰 2810;`kailv`、`binglupan` 无整段词。
- 另用 0.1.17 日志全部上屏输入回放,统计触发次数与"触发但所选不是纠错词"的次数,作为误伤参考。

性能:用 `real_engine` 实测触发路径(取 12 字母输入),目标比当前多出 ≤ 5ms;超出则收窄插入字母集或替换邻键范围。

实机:装包后在记事本打 `tuojian`、`kailv`,确认第 2 位出现正确词;托盘关闭"按键纠错"后不再出现。
