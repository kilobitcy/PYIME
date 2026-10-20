# 英文候选(IT/科技词补全) 设计

日期:2026-10-20 · 基于 main = 2b0bad0 · 已按复核修订

## 背景

- 用户常在中文里夹 IT 英文词(subagent、playwright、GitHub)。现状只能单击 Shift 切英文,
  或打完字母按回车原样上屏小写(`flush_raw`);拼音切不通时还会冒出乱拼的中文。
- 实测现有候选:`subagent` → 诉把跟他(末尾 `t` 残缺凑出整句)、`kube` → 苦被、`api` → 啊飘、
  `react` → 热啊;`playwright`/`github`/`docker`/`css` 无候选。
- 本次新增价值:前缀补全、规范大小写(`github` → `GitHub`)、选词学习。
- rime-ice `en_dicts/en.dict.yaml`(约 2.3 万行)与 `en_ext.dict.yaml`(约 2600 行)同为 GPL-3.0。
  行格式 `词⇥编码[⇥权重]`,词与编码可不同(`he'll⇥hell⇥1`、`Kubernetes⇥K8s`);大多无权重,按字母序。

## 方案

引擎新增独立模块 `english.rs`(词表载入、前缀查询);`Engine::set_english` 注入,在 `assemble` 内合并,
候选来源新增 `Source::English`。合并放在引擎内部,可直接用 `Word.partial` 判定拼音状态,
`Candidate` 公共字段不变。否决:英文编进 `pyime.dict`(键空间与拼音混杂);只在后端做(破坏分层、回归测不到)。

## 1. 词表

- 来源与档位(越小越优先):
  - 0:自带 IT 词表 `installer/en_it.txt`、用户词表 `%LOCALAPPDATA%\PYIME\en.txt`、回车学会的词(第 3 节)
  - 1:rime-ice `en_ext.dict.yaml`;2:rime-ice `en.dict.yaml`
- 统一键规则:**键必须全是 ASCII 字母**(转小写后),否则该行跳过。
  - rime-ice:跳过 yaml 头(`---` 至 `...`)、`#` 注释、空行;键取第 2 列,第 3 列忽略。
    `K8s`、`README.md` 之类编码因含非字母被跳过。
  - 自带/用户表:一行 `词` 或 `词⇥键`,`#` 注释;无键时键 = 词去空格转小写(`Claude Code` → `claudecode`),
    `C++` 这类需显式写键(`C++⇥cpp`)。
- 打包:新工具 `pyime-en-build` 合并 rime 两表与 IT 表为 `pyime.en`(每行 `键⇥词⇥档位`,同键同词留最小档位),
  放 `server.exe` 同目录。后端启动载入 `pyime.en`、`en.txt`、回车学会的词;任一缺失/读失败跳过并记日志。

## 2. 拼音状态与英文落位

仅针对当前待转换段 `input`(全小写字母)。按引擎内部 `Word` 判三态:

- **精确通**:存在吃满 `input` 的整段词或整句,且其中没有残缺音节词(`partial`);模糊词允许。
- **残缺通**:吃满 `input` 的候选都含残缺音节词(如 `subagent`、`kube`、`xiaom`、`api`、`react`)。
- **不通**:没有吃满 `input` 的中文候选(如 `playwright`、`css`)。

英文候选:完全匹配 = 键等于 `input`;补全 = 键以 `input` 开头且更长,要求 `input` ≥ 4 字母。

| 状态 | 完全匹配 | 补全 | 位置 |
|---|---|---|---|
| 不通 | 出 | 各档都出 | 英文排最前,合计至多 3 条,其后接中文 |
| 残缺通 | 出,排第 1 | 只出档位 0,至多 2 条 | 补全插在第 3 位起 |
| 精确通 | `input` ≥ 4 字母时出 1 条,插第 3 位 | 不出 | 中文不足 2 条则接末尾 |

- 示例:`subagent` → subagent 首位;`kube` → 苦被、苦、Kubernetes;`xiaom` 中文不受影响
  (Xiaomi 非档位 0);`api` → API 首位;`change` → 嫦娥、…、change(第 3 位)。
- 英文之间排序:档位升序 → 完全匹配优先 → 词长升序 → 词表文件原序。
- 学过的英文不另设规则:选中英文走现有 `learn(input, 词)`,下次由 `UserModel::matching` 作为用户短语
  (得分 18 起)自然排前,与中文"选过一次即翻盘"一致。与英文候选同文字时去重保留排在前的那条;
  凡文字全为 ASCII 的候选,`source` 记为 `English`,供评测分桶。
- 英文候选 `consumed = input.len()`,`cuts`/`parts` 为空,`fuzzy = 0`,`pairs = 0`。
- 已知体验:逐字母敲时状态会变(`suba` 精确通 → `subag` 残缺通),英文候选随之出现/移位;按上表确定性落位,接受。

## 3. 回车自动学习

- 条件:`pending` 为空(未分段选过词)、`input` 全字母且 ≥ 4、状态为不通或残缺通,**按回车**原样上屏。
  空格(无候选)、Esc、退格删光、Shift 切英文触发的 `flush_raw` 不学。
- 计数存独立文件 `%LOCALAPPDATA%\PYIME\en_learned.txt`(`词⇥次数`),**不写 `user.txt`**,
  避免经 `matching` 一次就以整段词冒到首位。次数 ≥ 2 时该词以档位 0 加入英文词表(当场生效)。
- 撤销:不支持退格撤回;组字中对该候选 Ctrl+Shift+数字时,`forget` 同时删掉 `en_learned.txt` 中该词。
  `en.txt`、自带词表中的词不能这样删(只删个人学习)。
- 输入日志新增事件 `enter_raw`(拼音、是否计入学习),供回放重建。

## 4. 不做

- 中英混输一串(`yongplaywrightceshi`):需分段,先上屏"用"再打 `playwright`。
- 英文词频表;首字母自动大写;上屏后自动补空格。

## 5. 测试与评测

- `english.rs` 单测:yaml 头/注释/三列行解析、含非字母键跳过、`Claude Code` 键、`词⇥键` 显式键、前缀查询与排序。
- 引擎单测(小词表):三态判定;各状态落位与条数;精确通 < 4 字母不出;学过后由用户短语排首位且去重;
  未注入英文词表时候选与现状逐条一致。
- 后端单测:回车学习的各条件(`pending` 非空/精确通/< 4 字母/非回车均不学),2 次后出现,`forget` 后消失。
- 评测链路:`regression.rs` 读 `PYIME_EN`、`pyime-replay` 加 `--en <pyime.en>` 载入英文词表,
  回放识别 `enter_raw` 重建回车学习;新增"英文"桶,用例 `subagent→subagent`、`playwright→Playwright`、
  `github→GitHub`、`kube→Kubernetes`(≤3)、`change` 首位不变。现有各桶不退步,用 `PYIME_RANKS_BASE` 逐例比对。

## 6. 打包

- `scripts/package.sh`:构建并运行 `pyime-en-build` 生成 `installer/pyime.en`。
- `installer/pyime.nsi`:安装/卸载 `pyime.en`。
- `installer/NOTICE.md`:补 rime-ice `en_dicts` 来源与 GPL-3.0 声明。

## 风险

- 无词频,普通英文词补全可能不准(`appl` 先出 `Apple`);IT 词有档位 0 兜底,待日志暴露再议。
- 精确通时英文占第 3 位会把原第 3 位中文挤到第 4;仅完全匹配且 ≥ 4 字母时发生。
