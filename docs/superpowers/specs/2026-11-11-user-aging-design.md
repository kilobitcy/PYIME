# user.txt 老化淘汰与空闲写盘 设计

日期:2026-11-11

## 背景

用户实际 `user.txt`(2026-11-11):636 KB、33,242 行。

| 类型 | 条数 | 计数为 1 |
|---|---|---|
| 词对(拼音→词) | 4,588 | 2,933(64%) |
| 个人搭配 `@ng` | 28,645(占 86%) | 26,274(92%) |

问题:
1. 学习数据只增不减。`@ng` 达到上限 `MAX_COLLOCATIONS`=10 万后拒收新条目,新搭配静默学不进。
2. 误选永久留存:一次误选即在该拼音下永久 +8(`PAIR_BASE`)。样例:`zhongmingguocai 中民国才`、`hanyebiaoshun 行业表顺`、`shengshaoer 省少儿`,均计数 1。
3. 每次选词在请求里同步整份写盘(`ime.rs` `learn` → `save`),本规模约 40 ms(Linux 基准),宿主等回复期间被占住。

## 一、老化淘汰(`crates/pyime-engine/src/user.rs`)

### 数据
- `pairs`、`next` 的值由 `u32` 改为 `Entry { n: u32, day: u32 }`;`day` 为最后使用日,自 1970-01-01 起的天数。
- `UserModel` 新增 `today: u32`,由调用方传入:`import(text, today)`、`set_today(today)`;引擎不直接取时钟,便于单测。
- `today < DAY_FLOOR`(20758,即 2026-11-01)视为时钟不可信:不淘汰,新条目日期仍记 `today`。
- 词层 `words` 仍由词对累加得出,不存日期。
- 模糊规则计数 `fuzzy` 不变,不参与老化。

### 文件格式
- 词对:`拼音⇥词⇥次数⇥日`;搭配:`@ng⇥上文⇥词⇥次数⇥日`;`@fuzzy` 行不变。
- `import(text, today)` 读入时,缺日期列(旧格式)或日期无法解析的行,日期取传入的 `today`。因此现有条目在升级 60 天后才开始被淘汰;升级后首次写盘前后条目数不变(必测)。
- 降级兼容:旧版读新文件,词对正常(多余列被忽略),`@ng` 因列数不符整行跳过,旧版再写盘后搭配与日期永久丢失。接受,发布说明写明"降级会丢个人搭配"。

### 更新
- `learn`、`learn_next`(含 `learn_lessons`)把条目日期置为 `today`。
- `unlearn` / `unlearn_next` 只减次数(归零删除),不回退日期。
- `forget` 不变。

### 淘汰
- `prune()`:删除 `n == 1 && today.saturating_sub(day) >= AGE_DAYS`(`AGE_DAYS` = 60)的词对与 `@ng`,返回删除条数;删词对时从 `words` 扣减 1,归零删除(`words` 有 `MAX_COUNT` 封顶,扣减对超过 1000 的词略有低估,可忽略)。时钟倒退时 `saturating_sub` 得 0,不删。
- 调用时机:由 `ime` 在日期变化时调用——启动读入后、重新加载后各一次,之后 `flush` 时发现 `today` 与上次淘汰日不同再调;一天至多一次。
- 淘汰删了条目时:写盘前先把现有 `user.txt` 复制为 `user.txt.aged.bak`(覆盖旧的),并在日志记删除的词对数与搭配数。启动时的淘汰同样置 `dirty`,随后正常写盘。
- `@ng` 达到 `MAX_COLLOCATIONS` 且要收新条目时:淘汰最旧(`day` 最小)的至多 `EVICT_BATCH`=1000 条计数为 1 的条目(`select_nth_unstable_by_key`,不全排序),再收新条目;没有计数为 1 的条目时仍拒收(同现状)。
- 词对不设上限,靠老化控制规模。

### 排序
加分公式(`bonus`、`collocation`、`fuzzy_discount`)不变;回归对照期望零变化。

## 二、空闲写盘(`crates/pyime-backend/src/ime.rs`、`hub.rs`)

- `Ime` 新增 `dirty: Cell<bool>`。`learn`、`unlearn`、`forget` 只改内存并置 `dirty`,不再调 `save`。
- `Ime::flush()`:`dirty` 时设 `today`、按需 `prune`(见上)、`save`(外部修改保护逻辑不变:文件被外部改过则不写回),然后清 `dirty`。保护期内不写回时 `dirty` 也清掉,避免每秒重试;下一次学习会再置位。
- `hub::run` 写盘时机:有改动后,满足任一即 `flush`——空闲 `IDLE_FLUSH`=1 s 无新消息;或距首次置脏已达 `MAX_DELAY`=10 s(连续打字时每个键都经 hub,只靠空闲判定可能几分钟不写)。实现:`dirty` 时 `recv_timeout(min(IDLE_FLUSH, 截止时刻剩余))`,剩余为 0 则处理完当前消息后立即 `flush`;不脏时用 `recv()`,不用超大超时。两个时长可传入,测试用短值。
- worker 实际退出路径(看门狗 `exit(3)`、panic `exit(4)`、监护进程 `kill`)都不经过 hub 循环末尾,不做退出前写盘。
- `edit_user`(编辑用户词库…)原有的 `save` 改为强制写盘(无论 `dirty`)。
- `reload_user`(重新加载用户词库)不先写盘,读入后清 `dirty`,再同启动一样淘汰一次(删了条目则置 `dirty`):未写盘的学习(至多约 10 s)随之丢弃,与现状"重新加载以文件为准"一致,也不增加冲掉用户编辑的机会。
- `flush` 不在 `busy.begin/end` 之间,写盘慢不触发看门狗。
- 代价:worker 被结束(`--quit`、注销、看门狗、崩溃)时最多丢最后约 10 秒的学习。
- 写盘仍在 hub 线程:连续打字时至多每 10 s 有一个键多等一次 `export`+写盘(本规模约 40 ms,淘汰后预计显著变小)。
- 英文学习 `en_learned.txt` 的写盘(`save_english`)不在本次范围,保持同步。

## 测试

- `user.rs`:淘汰边界(59/60 天、次数 1/2);淘汰后词层扣减;缺日期旧行按 `today`,读入后立即淘汰不删任何旧行;时钟倒退(`today < day`)不删;`today < DAY_FLOOR` 不淘汰;导出再导入一致(含日期);`learn` 刷新日期;`@ng` 满时淘汰最旧计数 1 条目、无计数 1 条目时拒收。
- `ime`:学习后文件未变,`flush` 后写入;`reload_user` 丢弃未写盘学习并清 `dirty`;外部修改保护仍有效;`edit_user` 强制写盘;淘汰删了条目时生成 `user.txt.aged.bak`;同一天第二次 `flush` 不再淘汰。现有断言"`learn` 后立即落盘"的用例(`ime/tests.rs`)改为 `flush` 后断言,断言内容不放松。
- `hub`:有改动时空闲超时触发写盘;持续有消息时到 `MAX_DELAY` 也写盘;无改动时不写。
- 回归对照(CLAUDE.md 所列命令)前后逐例比对,期望零变化。

## 不做

- 计数随时间衰减、整句候选降低单次加分(待本次上线后视剩余误选再定)。退格删完刚上屏的词已会撤回学习(`session.rs` `retract`),部分删除不撤回,本次不动。
- 把写盘挪到专用线程:基准显示 `export` 本身就约 40 ms,单挪磁盘写收益有限。
- 对现有误选的一次性清理(用户选择等 60 天自动清,期间用 Ctrl+数字 手动删)。
