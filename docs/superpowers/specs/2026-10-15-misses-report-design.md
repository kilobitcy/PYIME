# 日志增强 + 错例报告 设计(子项目 3)

日期:2026-10-15 · 分支:`feat/misses-report`(基于 main = 0.1.8)

## 背景

`input-log.jsonl` 目前只记每段选词(`select`)、删光撤回(`retract`)、Ctrl+Shift+数字(`forget`)。
`qinjixu` 这类错例在日志里不可见:上屏"亲继续"→删光→分两次打出"请""继续",
日志只是两三条互不相关的选词,没有一处标出"首选错了,期望是请继续"。
也没有版本号,无法区分哪个版本产生的数据。

## 目标

1. 日志每条记录带版本号;新增"放弃输入"事件。
2. 新工具 `pyime-misses` 从日志直接生成回归用例与"疑似找不到"清单。

报告只用日志里当时实际显示的 `top`/`index`,不加载词典、不重放学习,结果即用户当时所见。

## 1. 日志改动(`crates/pyime-backend/src/inputlog.rs`、`session.rs`)

- 版本:`select`/`retract`/`forget`/`cancel` 每行加 `"v": env!("CARGO_PKG_VERSION")`。
- `cancel` 事件:`{"ts", "v", "cancel": 拼音, "top": 前 5 候选}`。
  - 触发:组字中按 Esc;或退格删光拼音且无上屏文字(`flush_raw` 返回空)。
  - 记录内容:本次组字过程中最长的那串拼音及其当时的候选前 5(`Session` 新增字段
    `peak: Option<(String, Vec<String>)>`,在 `refresh` 中当 `input` 比已记的更长时更新,
    在 `flush_raw` 与 `select` 的上屏分支两处清空,避免带入下一轮)。例:打 `chelue` 后退格删光,记 `chelue` 而非 `c`。
  - 已部分选词(`pending` 非空)后按 Esc 同样记录;回车、Shift 切英文、空格无候选都是把字母原样上屏的主动输出,不记。
  - 版本号取 backend crate 的版本;发版时 `pyime-backend` 与 `pyime-engine` 两个 `Cargo.toml` 同步改。

## 2. `pyime-misses <input-log.jsonl>`(`crates/pyime-backend/src/bin/pyime-misses.rs`)

分析写成纯函数 `analyze(lines: &[serde_json::Value]) -> Report`,`main` 只负责读文件与打印。
crate 没有 lib,工具自包含,不引用后端模块(候选前 5 等常量不需要)。

### 上屏分组
- select 行 = 同时有 `typed` 与 `text` 字段的行;`forget`/`cancel` 行跳过,不打断分组。
- 若某 select 行 `typed` 短于 `input`,且下一 select 行 `input` 等于前者 `input` 去掉 `typed` 后的剩余,
  则归入同一组。组拼音 = 各段 `typed` 拼接,组文字 = 各段 `text` 拼接,`ts` 取末段。
- **完整组**:组拼音 == 首行 `input`(输入被选完)。末尾多余字母被退格删掉、链条中途追加/删字母
  等情况为不完整组,不计入 ② ③(仍参与 ① 的累加)。

### 三类错例(输出为 `regression.rs` 的 `(拼音, 期望, 1)` 格式)

| 类型 | 规则 | 输出 |
|---|---|---|
| ① 纠错 | 遇 `retract`:取 items **最后一项**的 (typed, text) 为 (R, W)(分段上屏时最后一项是整句,单段时只有一项)。向前找最近的组拼音 == R 且组文字 == W 的组(误选组),找不到则忽略此 retract。从误选组之后的组开始(可在 retract 之前——"整串重选改词"路径先写新 select 再写 retract)按序累加组拼音,保持为 R 的前缀;恰等于 R 时累加文字为期望 E。只累加 retract 后 60 秒内的组。 | `(R, E, 1)`,注释 `误选 W`;E == W 不出。 |
| ② 拆开选 | 完整组,多于一段,且首段当时的 `top[0]` ≠ 组文字。 | `(组拼音, 组文字, 1)`,注释 `N 段` |
| ③ 选了非首位 | 完整的单段组(`typed == input`)且 `index > 0`。 | `(拼音, 词, 1)`,注释 `上文: …` |

- 误选组、以及作为 ① 期望被累加的组,不计入 ② ③。
- ① 中累加出的第一个组拼音就不是 R 的前缀(如模糊音改打 `qinjixu`→`qingjixu`),或 60 秒内凑不满 R:不出用例,列入疑似清单。
- 相同 (拼音, 期望) 合并计数;各类内按次数降序,同次数按首次出现顺序。

### 疑似找不到(只列清单,不出用例)
- `cancel` 配对:`cancel` 之后 30 秒内的第一个组,列为
  `chelue(候选: 车路饿 …)→ 改打 celve 得 策略`。拼音长度 < 2 或 `top` 为空的 `cancel` 视为误敲,忽略;
  其余情况以最近的有效 `cancel` 为准。
- 纠错未配上(见上):列为 `qinjixu 误选 亲继续 → 改打 qingjixu 得 请继续`(无后续组时只写前半)。

### 报告格式
开头一行:日志条数、出现过的版本(无 `v` 字段记为"旧版")。
随后依次:① ② ③ 三段用例(每行 `("拼音", "期望", 1), // ×次数 注释`)、疑似清单。

## 测试

- `pyime-misses` 单测(构造日志行):
  - qinjixu 删光后分两次重打 `qin`→请、`jixu`→继续 → `("qinjixu","请继续",1)`,注释含 亲继续;
  - 分段上屏"请"+"继续"后删光(retract items 含两段 + 整句三项)→ 以整句为 (R, W);
  - "整串重选改词"路径:`select(W) → select(E) → retract(W)` → ① 输出 E,且 E 组不被当作误选组;
  - 删光后改打模糊拼音(不是前缀)→ 不出用例,进疑似清单;retract 后超过 60 秒才重打 → 同样不出;
  - 拆开选且首选不是整句 → ②;拆开选但首选就是整句 → 不出;
  - 末尾多余字母(组拼音 ≠ 首行 input)→ 不出 ②;链条断裂的首行(`typed` < `input`)→ 不出 ③;
  - 两段之间夹一条 `forget` 行 → 仍归为同一组;
  - 单段 `index > 0` → ③;被撤回的上屏不出 ② ③;
  - 两次相同用例 → 合并为 ×2;
  - `cancel` + 30 秒内上屏 → 疑似清单;超过 30 秒 → 不配对;单字母 `cancel` 被忽略。
- 可观测性:`inputlog` 四个写函数现各自拼 `ts` 并写文件,加 `v` 时合并为一个私有
  `write(line: Value)`(补 `ts`、`v` 后写入)。`#[cfg(test)]` 下 `write` 同时把该行推入
  线程局部的捕获列表,并提供 `#[cfg(test)] pub fn take_captured() -> Vec<Value>`,
  供会话测试断言日志内容(测试不打开文件,`FILE` 为空也照常捕获)。用到捕获的测试开头先调用一次
  `take_captured()` 清掉残留。
- 后端会话测试(`session/tests.rs`):
  - Esc 取消 → 记 `cancel`;退格删光 → 记最长拼音;部分选词后 Esc → 也记;
  - 回车、Shift 切英文 → 不记;
  - 选词上屏后再打新字母并 Esc → `cancel` 记的是新串(`peak` 已清空)。

## 非目标

从 `user.txt` 快照回放;"上屏后只删一部分"事件;排序优化(子项目 4);LLM 重排。

## 发布

版本 0.1.9;release 说明写:日志带版本号、记录放弃输入、新增 `pyime-misses` 工具(开发用,不进安装包)。
push/tag/发布前先征得同意。
