# user.txt 老化淘汰与空闲写盘 实现计划

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 用户学习记录带最后使用日,一次性且 60 天未用的条目自动淘汰;`@ng` 满额时淘汰最旧一次性条目;学习不再在请求里同步写盘,改为空闲 1 s 或首次改动后 10 s 写盘。

**Architecture:** `pyime-engine` 的 `UserModel` 管日期、格式、淘汰(纯函数式,"今天"由调用方传入);`pyime-backend` 的 `Ime` 管置脏、按日淘汰、备份、写盘;`hub::run` 的接收循环按置脏时刻决定何时 `flush`。

**Tech Stack:** Rust(std `mpsc::recv_timeout`、`HashMap`、`select_nth_unstable_by_key`)。

设计:`docs/superpowers/specs/2026-11-11-user-aging-design.md`。

## Global Constraints

- 回复、commit、注释一律简体中文;commit 末尾加 `Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>`。
- `AGE_DAYS` = 60;`DAY_FLOOR` = 20758(2026-11-01);`EVICT_BATCH` = 1000;`MAX_COLLOCATIONS` = 100_000(不变);`IDLE_FLUSH` = 1 s;`MAX_DELAY` = 10 s。
- 日期 = 自 1970-01-01(UTC)起的天数,`u32`。
- 加分公式(`bonus`、`collocation`、`fuzzy_discount`)不得改动。
- 英文学习 `en_learned.txt` 的写盘(`save_english`)不动。
- 工作区里 `Cargo.lock`、`crates/*/Cargo.toml` 的版本号改动是用户的未提交改动,**不要提交、不要还原**;每次 `git add` 只加本任务列出的文件。
- 不削弱既有断言;因格式变化需改的断言只改格式部分(下文逐条列出)。

## File Structure

- `crates/pyime-engine/src/user.rs`:`Entry`、日期、格式、`prune`、`evict_next`(Task 1–3)。
- `crates/pyime-engine/src/engine.rs`:加 `user_mut`(Task 1)。
- `crates/pyime-engine/tests/real_engine.rs`:`import` 新签名(Task 1)。
- `crates/pyime-backend/src/ime.rs`:`now_day`(Task 1);置脏、`flush`、`age`、备份、重载/编辑语义(Task 4)。
- `crates/pyime-backend/src/ime/tests.rs`:Task 4 测试。
- `crates/pyime-backend/src/hub.rs`:`run_with` 写盘时机与测试(Task 5)。
- `CLAUDE.md`:待办记录(Task 6)。

---

### Task 1: 学习记录带最后使用日,文件格式加日期列

**Files:**
- Modify: `crates/pyime-engine/src/user.rs`
- Modify: `crates/pyime-engine/src/engine.rs:236-238`
- Modify: `crates/pyime-engine/tests/real_engine.rs:17`
- Modify: `crates/pyime-backend/src/ime.rs`(import 两处 + 新增 `now_day`)

**Interfaces:**
- Produces:`UserModel::import(text: &str, today: u32) -> UserModel`;`UserModel::set_today(&mut self, today: u32)`;私有 `struct Entry { n: u32, day: u32 }`;`Engine::user_mut(&mut self) -> &mut UserModel`;`pyime_backend::ime::now_day() -> u32`。导出格式 `拼音⇥词⇥次数⇥日`、`@ng⇥上文⇥词⇥次数⇥日`。

- [ ] **Step 0: 建分支、存基线**

```bash
cd /data/bak20250527/PYIME
git checkout -b user-aging
PYIME_DICT=$PWD/installer/pyime.dict PYIME_USER=/tmp/user.txt cargo test -q -p pyime-engine --release --test real_engine -- --ignored --nocapture > /tmp/aging-base.txt 2>&1; tail -3 /tmp/aging-base.txt
```
Expected:输出若干行候选,无 panic。`/tmp/user.txt` 是用户真实数据,只读,不得修改。

- [ ] **Step 1: 写失败测试**(加在 `user.rs` 的 `mod tests` 末尾)

```rust
    #[test]
    fn 学习记下当天_导出带日期_导入保留日期() {
        let mut m = UserModel::default();
        m.set_today(20800);
        m.learn("ni", "你");
        m.learn_next("我", "你");
        assert_eq!(m.export(), "@ng\t我\t你\t1\t20800\nni\t你\t1\t20800");
        m.set_today(20810);
        m.learn("ni", "你");
        assert!(m.export().contains("ni\t你\t2\t20810"));
        let back = UserModel::import(&m.export(), 20900);
        assert_eq!(back.export(), m.export());
    }

    #[test]
    fn 旧格式缺日期按读入当天() {
        let m = UserModel::import("ni\t你\t3\n@ng\t我\t你\t2\n@ng\t我\t他\t1\t坏\n", 20800);
        assert_eq!(m.export(), "@ng\t我\t他\t1\t20800\n@ng\t我\t你\t2\t20800\nni\t你\t3\t20800");
    }
```

同时把本文件既有测试里的 `UserModel::import(&x)` 全部改为 `UserModel::import(&x, 0)`(共 5 处:L220、L237、L261、L263、L269),并把 `读回时丢掉上文含英文字母的个人搭配` 的期望改为 `"@ng\t全局\t配置\t2\t0"`(仅加日期列)。

- [ ] **Step 2: 跑测试确认失败**

Run: `cargo test -q -p pyime-engine --lib user::`
Expected:编译失败(`import` 参数个数不符、`set_today` 不存在)。

- [ ] **Step 3: 实现**

`user.rs` 顶部常量区之后加:

```rust
/// 一条学习记录:次数与最后使用日(自 1970-01-01 起的天数)。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Entry {
    n: u32,
    day: u32,
}
```

`UserModel` 改为:

```rust
#[derive(Default)]
pub struct UserModel {
    pairs: HashMap<(String, String), Entry>,
    words: HashMap<String, u32>,
    /// 各模糊规则被接受的次数(下标同 `fuzzy::variants` 的规则位:声母 0–4,韵母 5–9)。
    fuzzy: [u32; 10],
    /// (上文末尾 1–3 个字, 紧随其后的词) → 次数与最后使用日。
    next: HashMap<(String, String), Entry>,
    /// 今天(自 1970-01-01 起的天数),学习时记为最后使用日;由调用方设置。
    today: u32,
}
```

各方法改动(其余方法不动):

```rust
    /// 设置"今天",此后的学习以它为最后使用日。
    pub fn set_today(&mut self, today: u32) {
        self.today = today;
    }

    pub fn learn(&mut self, typed: &str, word: &str) {
        let today = self.today;
        let p = self.pairs.entry((typed.to_string(), word.to_string())).or_insert(Entry { n: 0, day: today });
        p.n = (p.n + 1).min(MAX_COUNT);
        p.day = today;
        let w = self.words.entry(word.to_string()).or_insert(0);
        *w = (*w + 1).min(MAX_COUNT);
    }

    pub fn unlearn(&mut self, typed: &str, word: &str) {
        dec_entry(&mut self.pairs, &(typed.to_string(), word.to_string()));
        dec(&mut self.words, &word.to_string());
    }

    pub fn unlearn_next(&mut self, prev: &str, word: &str) {
        let chars: Vec<char> = prev.chars().collect();
        for k in 1..=chars.len().min(MAX_CONTEXT) {
            dec_entry(&mut self.next, &(chars[chars.len() - k..].iter().collect::<String>(), word.to_string()));
        }
    }

    pub fn learn_next(&mut self, prev: &str, word: &str) {
        let chars: Vec<char> = prev.chars().collect();
        let today = self.today;
        for k in 1..=chars.len().min(MAX_CONTEXT) {
            let key = (chars[chars.len() - k..].iter().collect::<String>(), word.to_string());
            if self.next.len() >= MAX_COLLOCATIONS && !self.next.contains_key(&key) {
                continue;
            }
            let e = self.next.entry(key).or_insert(Entry { n: 0, day: today });
            e.n = (e.n + 1).min(MAX_COUNT);
            e.day = today;
        }
    }

    pub fn collocation(&self, prev: &str, word: &str) -> f64 {
        let chars: Vec<char> = prev.chars().collect();
        let best = (1..=chars.len().min(MAX_CONTEXT))
            .filter_map(|k| self.next.get(&(chars[chars.len() - k..].iter().collect::<String>(), word.to_string())).map(|e| e.n))
            .max();
        best.map_or(0.0, |n| (COLLOCATION_SLOPE * f64::from(n + 1).ln()).min(COLLOCATION_CAP))
    }

    pub fn bonus(&self, typed: &str, word: &str) -> f64 {
        let mut b = 0.0;
        if let Some(e) = self.pairs.get(&(typed.to_string(), word.to_string())) {
            b += PAIR_BASE + PAIR_SLOPE * f64::from(e.n).ln();
        }
        if let Some(&n) = self.words.get(word) {
            b += WORD_BASE + f64::from(n).ln();
        }
        b
    }

    /// 序列化为 `拼音⇥词⇥次数⇥日` 行,便于存盘。
    pub fn export(&self) -> String {
        let mut lines: Vec<String> = self.pairs.iter().map(|((t, w), e)| format!("{t}\t{w}\t{}\t{}", e.n, e.day)).collect();
        lines.sort();
        lines.extend(self.next.iter().map(|((p, w), e)| format!("@ng\t{p}\t{w}\t{}\t{}", e.n, e.day)));
        lines.sort();
        lines.extend(self.fuzzy.iter().enumerate().filter(|(_, &n)| n > 0).map(|(i, n)| format!("@fuzzy\t{i}\t{n}")));
        lines.join("\n")
    }

    /// 读回 `export` 的结果;坏行跳过。词层计数由精确层累加得出。
    /// 缺日期列(旧格式)或日期无法解析时,最后使用日取 `today`。
    /// 上文含英文字母的搭配(旧版选英文候选后把 est 之类记作上文)丢弃。
    pub fn import(text: &str, today: u32) -> UserModel {
        let mut m = UserModel { today, ..UserModel::default() };
        let day = |d: Option<&str>| d.and_then(|d| d.parse::<u32>().ok()).unwrap_or(today);
        for line in text.lines() {
            if let Some(rest) = line.strip_prefix("@ng\t") {
                let c: Vec<&str> = rest.split('\t').collect();
                if let [p, w, n, more @ ..] = c.as_slice() {
                    if more.len() > 1 || p.bytes().any(|b| b.is_ascii_alphabetic()) {
                        continue;
                    }
                    if let Ok(n) = n.parse::<u32>() {
                        m.next.insert((p.to_string(), w.to_string()), Entry { n: n.min(MAX_COUNT), day: day(more.first().copied()) });
                    }
                }
                continue;
            }
            let mut c = line.split('\t');
            let (Some(t), Some(w), Some(n)) = (c.next(), c.next(), c.next().and_then(|n| n.parse::<u32>().ok())) else {
                continue;
            };
            let n = n.min(MAX_COUNT);
            if t == "@fuzzy" {
                if let Some(slot) = w.parse::<usize>().ok().and_then(|i| m.fuzzy.get_mut(i)) {
                    *slot = n;
                }
                continue;
            }
            m.pairs.insert((t.to_string(), w.to_string()), Entry { n, day: day(c.next()) });
            let e = m.words.entry(w.to_string()).or_insert(0);
            *e = (*e + n).min(MAX_COUNT);
        }
        m
    }
```

文件末尾 `dec` 之后加:

```rust
/// 学习记录次数减一,归零删除;不存在则不动。日期不回退。
fn dec_entry<K: std::hash::Hash + Eq>(map: &mut HashMap<K, Entry>, key: &K) {
    if let Some(e) = map.get_mut(key) {
        e.n -= 1;
        if e.n == 0 {
            map.remove(key);
        }
    }
}
```

`engine.rs` 在 `pub fn user(&self)` 之后加:

```rust
    pub fn user_mut(&mut self) -> &mut UserModel {
        &mut self.user
    }
```

`tests/real_engine.rs:17` 改为 `UserModel::import(&std::fs::read_to_string(u).unwrap(), 0)`(不淘汰,只观察)。

`crates/pyime-backend/src/ime.rs`:`use std::time::SystemTime;` 改为 `use std::time::{SystemTime, UNIX_EPOCH};`,在 `fn stamp` 之前加:

```rust
/// 今天:自 1970-01-01(UTC)起的天数。
pub fn now_day() -> u32 {
    SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| (d.as_secs() / 86_400) as u32)
}
```

并把 `read_user` 里的 `UserModel::import(&normalize_user(&text))` 改为 `UserModel::import(&normalize_user(&text), now_day())`,`reload_user` 里的 `UserModel::import(&text)` 改为 `UserModel::import(&text, now_day())`。

- [ ] **Step 4: 跑测试确认通过**

Run: `cargo test -q -p pyime-engine && cargo test -q -p pyime-backend`
Expected:全部通过(`test result: ok`)。backend 中断言 `contains("ni\t你\t1")` 的用例因新格式为 `ni\t你\t1\t<日>` 仍通过。

- [ ] **Step 5: Commit**

```bash
git add crates/pyime-engine/src/user.rs crates/pyime-engine/src/engine.rs crates/pyime-engine/tests/real_engine.rs crates/pyime-backend/src/ime.rs
git commit -m "引擎:用户学习记录带最后使用日,文件格式加日期列(旧格式按读入当天)

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---

### Task 2: 老化淘汰 `prune`

**Files:**
- Modify: `crates/pyime-engine/src/user.rs`

**Interfaces:**
- Consumes:Task 1 的 `Entry`、`today`、`import(text, today)`、`dec`。
- Produces:`pub const AGE_DAYS: u32 = 60;`、`pub const DAY_FLOOR: u32 = 20758;`、`UserModel::prune(&mut self) -> (usize, usize)`(返回删除的词对数、搭配数)。

- [ ] **Step 1: 写失败测试**(`mod tests` 末尾)

```rust
    #[test]
    fn 一次性且满60天才淘汰_词层随之扣减() {
        let t = DAY_FLOOR + 100;
        let text = format!(
            "ni\t你\t1\t{}\nni\t泥\t1\t{}\nhao\t好\t2\t{}\nnihao\t你\t1\t{}\n@ng\t我\t你\t1\t{}\n@ng\t我\t泥\t1\t{}\n@ng\t我\t好\t2\t{}",
            t - 60, t - 59, t - 200, t - 59, t - 60, t - 59, t - 200
        );
        let mut m = UserModel::import(&text, t);
        assert_eq!(m.prune(), (1, 1));
        assert!(!m.chose("ni", "你") && m.chose("ni", "泥") && m.chose("hao", "好") && m.chose("nihao", "你"));
        assert_eq!(m.bonus("zzz", "你"), WORD_BASE); // 词层 2 次扣成 1 次:WORD_BASE + ln1
        assert_eq!(m.collocation("我", "你"), 0.0);
        assert!(m.collocation("我", "泥") > 0.0 && m.collocation("我", "好") > 0.0);
    }

    #[test]
    fn 读入当天的旧行不淘汰_时钟不可信或倒退不淘汰() {
        let t = DAY_FLOOR + 100;
        let mut m = UserModel::import("ni\t你\t1\n@ng\t我\t你\t1\n", t);
        assert_eq!(m.prune(), (0, 0));
        let mut m = UserModel::import(&format!("ni\t你\t1\t{}", t + 500), t); // 记录日晚于今天
        assert_eq!(m.prune(), (0, 0));
        let mut m = UserModel::import("ni\t你\t1\t0", DAY_FLOOR - 1);
        assert_eq!(m.prune(), (0, 0));
    }
```

- [ ] **Step 2: 跑测试确认失败**

Run: `cargo test -q -p pyime-engine --lib user::`
Expected:编译失败(`DAY_FLOOR`、`prune` 不存在)。

- [ ] **Step 3: 实现**

常量区加:

```rust
/// 老化:计数为 1 且这么多天没再用的词对与搭配被淘汰。
pub const AGE_DAYS: u32 = 60;
/// "今天"早于此日(2026-11-01)视为时钟不可信,不淘汰。
pub const DAY_FLOOR: u32 = 20758;
```

`impl UserModel` 内(`forget` 之后)加:

```rust
    /// 淘汰计数为 1 且 `AGE_DAYS` 天未再用的词对与搭配,返回 (词对, 搭配) 删除条数;删词对时词层扣 1。
    /// 今天早于 `DAY_FLOOR` 不淘汰;时钟倒退(记录日晚于今天)按距今 0 天算。
    pub fn prune(&mut self) -> (usize, usize) {
        let today = self.today;
        if today < DAY_FLOOR {
            return (0, 0);
        }
        let stale = |e: &Entry| e.n == 1 && today.saturating_sub(e.day) >= AGE_DAYS;
        let gone: Vec<(String, String)> = self.pairs.iter().filter(|(_, e)| stale(e)).map(|(k, _)| k.clone()).collect();
        for k in &gone {
            self.pairs.remove(k);
            dec(&mut self.words, &k.1);
        }
        let before = self.next.len();
        self.next.retain(|_, e| !stale(e));
        (gone.len(), before - self.next.len())
    }
```

- [ ] **Step 4: 跑测试确认通过**

Run: `cargo test -q -p pyime-engine --lib user::`
Expected:`test result: ok`。

- [ ] **Step 5: Commit**

```bash
git add crates/pyime-engine/src/user.rs
git commit -m "引擎:用户学习老化淘汰(计数 1 且 60 天未用),时钟不可信或倒退不淘汰

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---

### Task 3: 个人搭配满额时淘汰最旧一次性条目

**Files:**
- Modify: `crates/pyime-engine/src/user.rs`

**Interfaces:**
- Consumes:Task 1 的 `Entry`、`learn_next`;常量 `MAX_COLLOCATIONS`。
- Produces:`const EVICT_BATCH: usize = 1000;`、私有 `fn evict_next(&mut self)`。

- [ ] **Step 1: 写失败测试**(`mod tests` 末尾)

```rust
    #[test]
    fn 搭配满额时淘汰最旧的一次性条目再收新的() {
        let mut m = UserModel::default();
        m.set_today(DAY_FLOOR);
        m.learn_next("甲", "旧0");
        m.learn_next("甲", "旧0"); // 计数 2,不淘汰
        for i in 1..1500 {
            m.learn_next("甲", &format!("旧{i}"));
        }
        m.set_today(DAY_FLOOR + 1);
        for i in 1500..MAX_COLLOCATIONS {
            m.learn_next("甲", &format!("新{i}"));
        }
        assert_eq!(m.next.len(), MAX_COLLOCATIONS);
        m.learn_next("甲", "来了");
        assert_eq!(m.next.len(), MAX_COLLOCATIONS - EVICT_BATCH + 1);
        assert!(m.collocation("甲", "来了") > 0.0 && m.collocation("甲", "旧0") > 0.0);
        assert_eq!((1..1500).filter(|i| m.collocation("甲", &format!("旧{i}")) > 0.0).count(), 499);
        assert!((1500..MAX_COLLOCATIONS).all(|i| m.collocation("甲", &format!("新{i}")) > 0.0));
    }

    #[test]
    fn 搭配满额且无一次性条目时仍拒收() {
        let mut m = UserModel::default();
        for i in 0..MAX_COLLOCATIONS {
            let w = format!("词{i}");
            m.learn_next("甲", &w);
            m.learn_next("甲", &w);
        }
        m.learn_next("甲", "来了");
        assert_eq!(m.next.len(), MAX_COLLOCATIONS);
        assert_eq!(m.collocation("甲", "来了"), 0.0);
    }
```

- [ ] **Step 2: 跑测试确认失败**

Run: `cargo test -q -p pyime-engine --lib user::`
Expected:编译失败(`EVICT_BATCH` 不存在)。

- [ ] **Step 3: 实现**

常量区:`MAX_COLLOCATIONS` 的注释改为 `/// 个人搭配条目上限;满了先淘汰最旧的一次性条目,仍满则不收新条目(已有的仍累加)。`,其后加:

```rust
/// 个人搭配满额时一次淘汰的最旧一次性条目数。
const EVICT_BATCH: usize = 1000;
```

`learn_next` 循环内满额判断改为:

```rust
            if self.next.len() >= MAX_COLLOCATIONS && !self.next.contains_key(&key) {
                self.evict_next();
                if self.next.len() >= MAX_COLLOCATIONS {
                    continue;
                }
            }
```

`impl UserModel` 内加:

```rust
    /// 个人搭配满额时,淘汰最后使用日最早的至多 `EVICT_BATCH` 条计数为 1 的条目。
    fn evict_next(&mut self) {
        let mut ones: Vec<(u32, (String, String))> = self.next.iter().filter(|(_, e)| e.n == 1).map(|(k, e)| (e.day, k.clone())).collect();
        if ones.len() > EVICT_BATCH {
            ones.select_nth_unstable_by_key(EVICT_BATCH - 1, |o| o.0);
            ones.truncate(EVICT_BATCH);
        }
        for (_, k) in ones {
            self.next.remove(&k);
        }
    }
```

- [ ] **Step 4: 跑测试确认通过**

Run: `cargo test -q -p pyime-engine --lib user::`
Expected:`test result: ok`。

- [ ] **Step 5: Commit**

```bash
git add crates/pyime-engine/src/user.rs
git commit -m "引擎:个人搭配满额时淘汰最旧的一次性条目,不再静默拒收

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---

### Task 4: `Ime` 置脏、按日淘汰、备份、重载与编辑语义

**Files:**
- Modify: `crates/pyime-backend/src/ime.rs`
- Modify: `crates/pyime-backend/src/ime/tests.rs`

**Interfaces:**
- Consumes:`UserModel::{import(text, today), set_today, prune}`、`Engine::user_mut`、`now_day()`。
- Produces(Task 5 用):`Ime::dirty_since(&self) -> Option<std::time::Instant>`、`Ime::flush(&self)`。另有私有 `fn age(&self, today: u32, force: bool) -> (usize, usize)`、`fn write_now(&self)`、`fn mark_dirty(&self)`。

- [ ] **Step 1: 改既有测试 + 写失败测试**(`ime/tests.rs`)

既有用例改动(只在学习后、读文件前补 `ime.flush();`,断言不动):
- `文件不存在时照常写_自己写盘后再学习照常写回`:第二个 `learn` 后加 `ime.flush();`。
- `文件被外部修改后不覆盖`:第一个 `learn` 后加 `ime.flush();`;第二个 `learn` 后加 `ime.flush();`。
- `启动时读不出的文件不被覆盖`:`learn` 后加 `ime.flush();`。
- `重新加载读入外部修改并恢复写回`:`learn` 后加 `ime.flush();`。
- `重新加载遇非utf8不读入且继续保护`:第二个 `learn` 后加 `ime.flush();`。

新增用例(文件末尾):

```rust
#[test]
fn 学习只置脏_flush_才写盘() {
    let p = temp_user_path("dirty");
    let ime = Ime::with_user_path(engine(), p.clone());
    assert!(ime.dirty_since().is_none());
    ime.learn("ni", "你", 0, "", &[]);
    assert!(ime.dirty_since().is_some() && !p.exists());
    ime.flush();
    assert!(ime.dirty_since().is_none());
    let t = std::fs::read_to_string(&p).unwrap();
    assert!(t.contains(&format!("ni\t你\t1\t{}", now_day())), "{t}");
}

#[test]
fn 重新加载丢弃未写盘学习() {
    let p = temp_user_path("reload-dirty");
    std::fs::write(&p, "hao\t好\t3").unwrap();
    let ime = Ime::with_user_path(engine(), p.clone());
    ime.learn("ni", "你", 0, "", &[]);
    assert_eq!(ime.reload_user(), Ok(()));
    assert!(ime.dirty_since().is_none());
    assert!(!ime.user_export().contains("ni\t你"));
    assert_eq!(std::fs::read_to_string(&p).unwrap(), "hao\t好\t3");
}

#[test]
fn 启动时淘汰过期一次性条目_写盘前备份原文件() {
    let p = temp_user_path("aged");
    let old = now_day() - 61;
    let orig = format!("ni\t泥\t1\t{old}\nhao\t好\t3\t{old}\n@ng\t我\t泥\t1\t{old}");
    std::fs::write(&p, &orig).unwrap();
    let ime = Ime::with_user_path(engine(), p.clone());
    assert!(ime.dirty_since().is_some());
    let t = ime.user_export();
    assert!(!t.contains('泥') && t.contains("hao\t好\t3"), "{t}");
    ime.flush();
    assert_eq!(std::fs::read_to_string(p.with_extension("txt.aged.bak")).unwrap(), orig);
    assert!(!std::fs::read_to_string(&p).unwrap().contains('泥'));
}

#[test]
fn 升级后首次写盘条目数不变() {
    let p = temp_user_path("upgrade");
    std::fs::write(&p, "ni\t泥\t1\nhao\t好\t3\n@ng\t我\t泥\t1\n").unwrap();
    let ime = Ime::with_user_path(engine(), p.clone());
    ime.learn("ni", "你", 0, "", &[]);
    ime.flush();
    let t = std::fs::read_to_string(&p).unwrap();
    assert_eq!(t.lines().count(), 4, "{t}");
    assert!(t.contains("ni\t泥\t1\t") && t.contains("@ng\t我\t泥\t1\t") && t.contains("ni\t你\t1\t"), "{t}");
    assert!(!p.with_extension("txt.aged.bak").exists());
}

#[test]
fn 同一天只淘汰一次_force_照做() {
    let p = temp_user_path("age-once");
    let ime = Ime::with_user_path(engine(), p.clone());
    let today = now_day();
    let old = format!("ni\t泥\t1\t{}", today - 61);
    ime.engine.borrow_mut().set_user(UserModel::import(&old, today));
    assert_eq!(ime.age(today, false), (0, 0)); // 启动时已淘汰过今天
    assert_eq!(ime.age(today, true), (1, 0));
    ime.engine.borrow_mut().set_user(UserModel::import(&old, today + 1));
    assert_eq!(ime.age(today + 1, false), (1, 0));
}

#[test]
fn 编辑用户词库强制写盘并清掉置脏() {
    let p = temp_user_path("edit-flush");
    let ime = Ime::with_user_path(engine(), p.clone());
    ime.learn("ni", "你", 0, "", &[]);
    ime.edit_user();
    assert!(ime.dirty_since().is_none());
    assert!(std::fs::read_to_string(&p).unwrap().contains("ni\t你\t1\t"));
}
```

- [ ] **Step 2: 跑测试确认失败**

Run: `cargo test -q -p pyime-backend --lib ime::`
Expected:编译失败(`dirty_since`、`flush`、`age` 不存在)。

- [ ] **Step 3: 实现**(`ime.rs`)

`use std::time::{SystemTime, UNIX_EPOCH};` 改为 `use std::time::{Instant, SystemTime, UNIX_EPOCH};`。

`struct Ime` 在 `generation` 字段之后加:

```rust
    /// 有未写盘的学习时为首次置脏时刻;hub 据此在空闲或到期时写盘。
    dirty: Cell<Option<Instant>>,
    /// 上次老化淘汰的日期,一天至多淘汰一次。
    pruned_day: Cell<u32>,
    /// 淘汰删了条目:下次写盘前先把原文件备份为 `user.txt.aged.bak`。
    aged_backup: Cell<bool>,
```

`build` 里 `generation: Cell::new(0),` 之后加:

```rust
            dirty: Cell::new(None),
            pruned_day: Cell::new(0),
            aged_backup: Cell::new(false),
```

`load()` 末尾 `Self::build(engine, user_path, en_learned_path, user_stamp)` 改为:

```rust
        let ime = Self::build(engine, user_path, en_learned_path, user_stamp);
        ime.age(now_day(), true);
        ime
```

`with_user_path` 末尾同样改为:

```rust
        let ime = Self::build(engine, Some(path), None, stamp);
        ime.age(now_day(), true);
        ime
```

`learn` / `unlearn` / `forget` 改为只改内存并置脏:

```rust
    /// 学习一次选择(连同整句拆出的模糊词),只置脏;由 hub 在空闲或到期时写盘。
    /// `pairs` 是该候选用到的模糊规则,`prev` 是其上文,`lessons` 见 `Candidate::lessons`。
    pub fn learn(&self, typed: &str, text: &str, pairs: u16, prev: &str, lessons: &[Lesson]) {
        let mut engine = self.engine.borrow_mut();
        engine.user_mut().set_today(now_day());
        engine.learn(typed, text);
        engine.learn_fuzzy(pairs);
        engine.learn_next(prev, text);
        engine.learn_lessons(lessons);
        self.mark_dirty();
    }

    /// 撤销一次 `learn`(含 lessons),只置脏。
    pub fn unlearn(&self, typed: &str, text: &str, pairs: u16, prev: &str, lessons: &[Lesson]) {
        let mut engine = self.engine.borrow_mut();
        engine.unlearn(typed, text, pairs, prev);
        engine.unlearn_lessons(lessons);
        self.mark_dirty();
    }

    /// 删除词 `text` 的全部个人学习:用户数据置脏,英文学习照旧立即存盘。
    pub fn forget(&self, text: &str) {
        let mut engine = self.engine.borrow_mut();
        engine.forget(text);
        self.mark_dirty();
        self.save_english(&engine);
    }
```

在 `fn save` 之前加:

```rust
    fn mark_dirty(&self) {
        if self.dirty.get().is_none() {
            self.dirty.set(Some(Instant::now()));
        }
    }

    /// 首次出现未写盘学习的时刻;没有则为 None。
    pub fn dirty_since(&self) -> Option<Instant> {
        self.dirty.get()
    }

    /// 有未写盘的学习时写盘(先按需老化淘汰)。
    pub fn flush(&self) {
        if self.dirty.get().is_some() {
            self.write_now();
        }
    }

    /// 立即按需淘汰并写盘,清掉置脏;文件被外部修改或写盘失败时 `save` 不写(只记日志),置脏同样清掉,免得反复重试。
    fn write_now(&self) {
        self.age(now_day(), false);
        self.save(&self.engine.borrow());
        self.dirty.set(None);
    }

    /// 老化淘汰:同一天只做一次(`force` 时照做)。删了条目则记日志、待备份并置脏。返回 (词对, 搭配) 删除条数。
    fn age(&self, today: u32, force: bool) -> (usize, usize) {
        if !force && self.pruned_day.get() == today {
            return (0, 0);
        }
        self.pruned_day.set(today);
        let (p, n) = {
            let mut engine = self.engine.borrow_mut();
            let user = engine.user_mut();
            user.set_today(today);
            user.prune()
        };
        if p + n > 0 {
            log::line(&format!("老化淘汰:词对 {p} 条、个人搭配 {n} 条"));
            self.aged_backup.set(true);
            self.mark_dirty();
        }
        (p, n)
    }
```

`fn save` 中,`create_dir_all` 之后、`match write_atomic(...)` 之前加:

```rust
        if self.aged_backup.replace(false) && path.exists() {
            if let Err(e) = std::fs::copy(path, path.with_extension("txt.aged.bak")) {
                log::line(&format!("备份 {path:?} 失败: {e}"));
            }
        }
```

`edit_user` 中 `self.save(&self.engine.borrow());` 改为 `self.write_now();`,doc 注释"写盘一次(文件已被外部修改则 `save` 自动跳过)"保持语义不变。

`reload_user` 尾部(从 `let text = normalize_user(&text);` 起)改为:

```rust
        let text = normalize_user(&text);
        {
            let mut engine = self.engine.borrow_mut();
            engine.set_user(UserModel::import(&text, now_day()));
            engine.set_english(load_english(self.en_learned_path.as_ref()));
        }
        self.user_stamp.set(stamp(path));
        self.skip_logged.set(false);
        self.generation.set(self.generation.get() + 1);
        // 以文件为准:未写盘的学习丢弃,再同启动一样淘汰一次
        self.dirty.set(None);
        self.age(now_day(), true);
        Ok(())
```

`reload_user` 的 doc 注释末尾加一行:`/// 未写盘的学习(至多约 10 s)随之丢弃。`

- [ ] **Step 4: 跑测试确认通过**

Run: `cargo test -q -p pyime-backend`
Expected:全部通过。若 `session/tests.rs` 有用例依赖"学习后立即落盘"而失败,在该用例读文件前补 `s.ime.flush();`,不改断言,并在提交说明里列出。

- [ ] **Step 5: Commit**

```bash
git add crates/pyime-backend/src/ime.rs crates/pyime-backend/src/ime/tests.rs
git commit -m "后端:用户学习只置脏、按日老化淘汰并备份;重新加载丢弃未写盘学习

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

> 注:Task 4 提交后、Task 5 完成前,生产路径不会写盘;两者须一起合并。

---

### Task 5: hub 空闲或到期写盘

**Files:**
- Modify: `crates/pyime-backend/src/hub.rs`

**Interfaces:**
- Consumes:`Ime::dirty_since() -> Option<Instant>`、`Ime::flush()`;测试用 `Ime::with_user_path`、`crate::ime::temp_user_path`、`Ime::learn`。
- Produces:`pub const IDLE_FLUSH: Duration`、`pub const MAX_DELAY: Duration`、`pub fn run_with(rx: Receiver<Msg>, ime: Rc<Ime>, busy: &Busy, idle: Duration, max_delay: Duration)`;`run` 签名不变(`pipe.rs` 无需改)。

- [ ] **Step 1: 写失败测试**(`hub.rs` 的 `mod tests` 末尾;`use` 区加 `use std::time::Duration;`)

```rust
    fn wait_exists(p: &std::path::Path) -> bool {
        (0..200).any(|_| {
            std::thread::sleep(Duration::from_millis(10));
            p.exists()
        })
    }

    #[test]
    fn 有改动时空闲即写盘() {
        let p = crate::ime::temp_user_path("hub-idle");
        let (tx, rx) = mpsc::channel::<Msg>();
        let q = p.clone();
        let worker = std::thread::spawn(move || {
            let ime = Ime::with_user_path(engine(), q);
            ime.learn("ni", "你", 0, "", &[]);
            run_with(rx, ime, &Busy::default(), Duration::from_millis(50), Duration::from_secs(60));
        });
        assert!(wait_exists(&p), "空闲 50ms 后应写盘");
        drop(tx);
        worker.join().unwrap();
    }

    #[test]
    fn 持续有消息时到期也写盘() {
        let p = crate::ime::temp_user_path("hub-max");
        let (tx, rx) = mpsc::channel::<Msg>();
        let q = p.clone();
        let worker = std::thread::spawn(move || {
            let ime = Ime::with_user_path(engine(), q);
            ime.learn("ni", "你", 0, "", &[]);
            run_with(rx, ime, &Busy::default(), Duration::from_secs(60), Duration::from_millis(100));
        });
        let (rtx, _rrx) = mpsc::channel();
        let mut written = false;
        for seq in 0..200 {
            tx.send(Msg::Request { conn: 1, body: req(seq, N), reply: rtx.clone() }).unwrap();
            std::thread::sleep(Duration::from_millis(10));
            if p.exists() {
                written = true;
                break;
            }
        }
        assert!(written, "距首次改动 100ms 应写盘(空闲 60s 不会触发)");
        drop(tx);
        worker.join().unwrap();
    }

    #[test]
    fn 无改动时不写盘() {
        let p = crate::ime::temp_user_path("hub-clean");
        let (tx, rx) = mpsc::channel::<Msg>();
        let q = p.clone();
        let worker = std::thread::spawn(move || {
            run_with(rx, Ime::with_user_path(engine(), q), &Busy::default(), Duration::from_millis(10), Duration::from_millis(10))
        });
        let (rtx, _rrx) = mpsc::channel();
        for seq in 0..5 {
            tx.send(Msg::Request { conn: 1, body: req(seq, N), reply: rtx.clone() }).unwrap();
        }
        std::thread::sleep(Duration::from_millis(100));
        drop(tx);
        worker.join().unwrap();
        assert!(!p.exists());
    }
```

(连接 1 未发 `Opened`,按其他程序默认英文,`N` 不触发学习。)

- [ ] **Step 2: 跑测试确认失败**

Run: `cargo test -q -p pyime-backend --lib hub::`
Expected:编译失败(`run_with` 不存在)。

- [ ] **Step 3: 实现**

`use std::sync::mpsc::{Receiver, Sender};` 改为 `use std::sync::mpsc::{Receiver, RecvTimeoutError, Sender};`;`use std::time::{SystemTime, UNIX_EPOCH};` 改为 `use std::time::{Duration, SystemTime, UNIX_EPOCH};`。`WATCHDOG_LIMIT_MS` 之后加:

```rust
/// 有未写盘的学习时,连续这么久没有新消息即写盘。
pub const IDLE_FLUSH: Duration = Duration::from_secs(1);
/// 有未写盘的学习时,距首次置脏这么久必写盘(连续打字时每个键都经 hub,只等空闲可能很久不写)。
pub const MAX_DELAY: Duration = Duration::from_secs(10);
```

`pub fn run` 整体替换为:

```rust
/// 工作线程主循环,直到所有发送端关闭。处理每条请求期间在 `busy` 上记开始时刻供看门狗检查。
pub fn run(rx: Receiver<Msg>, ime: Rc<Ime>, busy: &Busy) {
    run_with(rx, ime, busy, IDLE_FLUSH, MAX_DELAY);
}

/// 同 `run`,写盘时机可调(测试用短值):有未写盘的学习时,空闲 `idle` 或距首次置脏 `max_delay` 即写盘。
/// 写盘不在 `busy` 计时内,磁盘慢不触发看门狗。
pub fn run_with(rx: Receiver<Msg>, ime: Rc<Ime>, busy: &Busy, idle: Duration, max_delay: Duration) {
    let mut hub = Hub::new(ime.clone());
    loop {
        let msg = match ime.dirty_since() {
            None => match rx.recv() {
                Ok(m) => m,
                Err(_) => break,
            },
            Some(since) => {
                let left = max_delay.saturating_sub(since.elapsed());
                if left.is_zero() {
                    ime.flush();
                    continue;
                }
                match rx.recv_timeout(idle.min(left)) {
                    Ok(m) => m,
                    Err(RecvTimeoutError::Timeout) => {
                        ime.flush();
                        continue;
                    }
                    Err(RecvTimeoutError::Disconnected) => break,
                }
            }
        };
        match msg {
            Msg::Opened { conn, exe } => hub.open(conn, exe),
            Msg::Request { conn, body, reply } => {
                busy.begin();
                let out = hub.request(conn, &body);
                busy.end();
                // 无法解析时丢弃 reply,I/O 线程收不到回复即断开该连接
                if let Some(out) = out {
                    let _ = reply.send(out);
                }
            }
            Msg::Closed { conn } => hub.close(conn),
        }
    }
}
```

(原 `run` 上方的 doc 注释行 `/// 工作线程主循环,直到所有发送端关闭。…` 已并入上面,删除原处那一行以免重复。)

- [ ] **Step 4: 跑测试确认通过**

Run: `cargo test -q -p pyime-backend`
Expected:全部通过。

- [ ] **Step 5: Commit**

```bash
git add crates/pyime-backend/src/hub.rs
git commit -m "后端:用户数据改为空闲 1s 或首次改动后 10s 写盘,不再在选词请求里同步写

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---

### Task 6: 全量验证、排序零变化对照、文档

**Files:**
- Modify: `CLAUDE.md`(`### user.txt 机制` 与 `### 待办` 两节)

- [ ] **Step 1: 全量测试与 lint**

```bash
cargo test -q > /tmp/aging-test.log 2>&1; grep -E "test result|FAILED|panicked" /tmp/aging-test.log
cargo clippy --all-targets > /tmp/aging-clippy.log 2>&1; grep -cE "^(warning|error)" /tmp/aging-clippy.log
```
Expected:每个 `test result: ok`,无 FAILED;clippy 计数与改动前(在 main 上同命令)相同,不新增警告。

- [ ] **Step 2: 带真实用户数据对照候选(排序零变化)**

```bash
PYIME_DICT=$PWD/installer/pyime.dict PYIME_USER=/tmp/user.txt cargo test -q -p pyime-engine --release --test real_engine -- --ignored --nocapture > /tmp/aging-after.txt 2>&1
diff <(awk '{$2=""; print}' /tmp/aging-base.txt) <(awk '{$2=""; print}' /tmp/aging-after.txt) && echo 零变化
```
Expected:`零变化`(去掉耗时列后逐行相同)。有差异即停,报告差异,不得调整代码迁就。

- [ ] **Step 3: 真实文件读入、淘汰模拟(只读 `/tmp/user.txt`)**

在 `crates/pyime-engine/src/user.rs` 的 `mod tests` 临时加一个 `#[ignore]` 用例运行后**删除,不提交**:

```rust
    #[test]
    #[ignore]
    fn 临时_真实文件淘汰模拟() {
        let text = std::fs::read_to_string("/tmp/user.txt").unwrap();
        let t = DAY_FLOOR + 10;
        let mut m = UserModel::import(&text, t);
        assert_eq!(m.prune(), (0, 0), "升级当天不得淘汰");
        let n0 = m.export().lines().count();
        m.set_today(t + AGE_DAYS);
        let (p, n) = m.prune();
        println!("读入 {n0} 行;60 天后淘汰 词对 {p}、搭配 {n};剩 {} 行", m.export().lines().count());
    }
```

Run: `cargo test -q -p pyime-engine --lib user::tests::临时_真实文件淘汰模拟 -- --ignored --nocapture`
Expected:通过;打印的淘汰数约为词对 2933、搭配 26274(与设计背景统计一致)。记下数字后删除该用例,`git diff crates/pyime-engine/src/user.rs` 应为空。

- [ ] **Step 4: 交叉编译检查**

```bash
cargo build --release --target x86_64-pc-windows-gnu -p pyime-backend > /tmp/aging-win.log 2>&1; tail -2 /tmp/aging-win.log
```
Expected:`Finished`,无 error。

- [ ] **Step 5: 更新 CLAUDE.md**

`### user.txt 机制(据 ime.rs/user.rs)` 节末尾加:

```markdown
- 每条词对/搭配带最后使用日(`拼音⇥词⇥次数⇥日`,`@ng⇥上文⇥词⇥次数⇥日`;旧格式缺日期按读入当天)。计数 1 且 60 天未用即淘汰(启动、重新加载、每天首次写盘时),淘汰前备份 `user.txt.aged.bak`;`@ng` 满 10 万时淘汰最旧 1000 条一次性条目。降级到旧版会丢个人搭配。
- 学习只置脏,hub 在空闲 1 s 或首次改动后 10 s 写盘;worker 被结束最多丢约 10 s 学习;重新加载丢弃未写盘学习;"编辑用户词库…"强制写盘。
```

`### 待办` 节末尾加:

```markdown
- user.txt 老化淘汰与空闲写盘(2026-11-11,分支 user-aging,设计 `docs/superpowers/specs/2026-11-11-user-aging-design.md`):带真实 user.txt 候选零变化;待实机验证(选词后约 1 s 文件更新、托盘重新加载/编辑行为不变)。
```

- [ ] **Step 6: Commit**

```bash
git add CLAUDE.md
git commit -m "CLAUDE.md:记 user.txt 老化淘汰与空闲写盘

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
git status --short
```
Expected:`git status` 只剩用户原有的 `Cargo.lock`、`crates/*/Cargo.toml` 改动。
