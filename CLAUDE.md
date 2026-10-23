# PYIME

## Overview
Windows 11 拼音输入法。目标:界面与功能像微信输入法一样简洁,但在福建人常用的模糊拼音下,候选排序要准、选过的词要被学习。
痛点:① 模糊音展开后期望词排名靠后;② 用户词学习不足。

## Tech Stack
- 引擎:Rust(独立 crate,Linux 上可单测,不依赖 Windows)。
- Windows 前端:复用 PIME 1.3.0-stable(TSF 宿主;2.0.0-beta 在用户 Win11 上选词会崩宿主进程,见上游 issue #888/#904,不用),Rust 引擎作为后端进程,经 PIME 的 JSON 协议通信。
- 词库:开源词库 + 词频起步;第二阶段训练 bigram 重排。完全离线。
- 构建/发布:GitHub Actions 因账单锁定不可用(`release.yml` 保留待解锁);现走本机交叉编译(mingw-w64,目标 `x86_64-pc-windows-gnu`)+ `makensis` 出安装包,`gh release create` 手动发布。
- 安装形态:附加安装包 `PYIME-<版本>-setup.exe`,装进已安装的官方 PIME **1.3.0-stable**(往其 `backends.json` 登记后端并重新 `regsvr32`);同时把 PIME 的 `x64`/`x86\PIMETextService.dll` 换成补丁版(原版改名 `.orig` 备份,卸载还原;占用中的 DLL 靠改名替换,无需重启)。
- 补丁版 PIME DLL:`pime/patches/`(mingw 可移植修补 + 候选按自身宽度排版、序号/间距参照微信)、`pime/tsf_compat.h`(mingw 缺的 TSF 接口,取自 windows-rs);`scripts/build-pime-dll.sh` 克隆上游 v1.3.0-stable 打补丁,mingw 交叉编译到 `target/pime/{x64,x86}`(`--baseline` 出不改排版的对照版)。实机验证包脚本在 `pime/test-kit/`。

## Project Layout
- `crates/pyime-engine`:引擎(`dict` fst 词典、`rime` 词库导入、`pinyin` 音节表与切分格、`fuzzy` 10 对模糊音、`engine` 候选生成与整句 Viterbi、`user` 用户学习、`english` 英文词表与回车学习、`symbol` 符号表(精确整段词后附加符号));`pyime-dict-build` 编译词典、`pyime-en-build` 编译英文词表 `pyime.en`(自带 IT 词表 `installer/en_it.txt`)。
- `crates/pyime-backend`:PIME 后端(`protocol`/`session`/`ime`/`log`/`register`);词典 `pyime.dict`、英文词表 `pyime.en`、符号表 `symbols.txt`(自带 `installer/symbols.txt`)放在 `server.exe` 同目录,用户数据 `%LOCALAPPDATA%\PYIME\user.txt`,用户英文词表 `en.txt`、回车学习计数 `en_learned.txt` 同目录。
- `scripts/package.sh <版本>`:测试 + 交叉编译 + 编词典 + makensis 一条龙(rime-ice 默认在 /tmp/rime-ice)。
- `scripts/windows/`:托盘菜单不可用(输入法卡死)时的备用办法,用户手工改 `user.txt` 后使其生效(`edit-user.bat` 停输入法→记事本编辑→重启;`reload-user.bat` 直接重启;逻辑在 `user-reload.ps1`,含备份、UTF-8 校验、去 BOM/CRLF)。ps1 须存为 UTF-8 BOM + CRLF,bat 保持纯 ASCII。
- 调参观察:`PYIME_DICT=… cargo test -p pyime-engine --release --test real_engine -- --ignored --nocapture`(`PYIME_INPUTS` 指定拼音)。
- 回归对照:`PYIME_DICT=$PWD/installer/pyime.dict PYIME_GRAM=/tmp/gram/pyime.gram PYIME_EN=$PWD/installer/pyime.en cargo test -q -p pyime-engine --release --test regression -- --ignored --nocapture`;输出按期望候选来源(整段词/整句/前缀词,·模糊)分桶的通过数。调参前加 `PYIME_RANKS_OUT=/tmp/base.tsv` 存逐例名次,调参后加 `PYIME_RANKS_BASE=/tmp/base.tsv` 逐例比对(救回首位/跌出首位/其余变化)。不设 `PYIME_EN` 则不载英文、不跑英文用例。

## Commands
```bash
# build: cargo build
# test:  cargo test
# lint:  cargo clippy
# 交叉编译 + 打包(本机):
#   cargo build --release --target x86_64-pc-windows-gnu -p pyime-backend
#   cp target/x86_64-pc-windows-gnu/release/pyime-backend.exe installer/server.exe
#   makensis -DVERSION=<版本> installer/pyime.nsi
# 依赖验收:x86_64-w64-mingw32-objdump -p <exe> | grep 'DLL Name'(不得含 libgcc/libwinpthread)
```

## Conventions
- 回复、commit、注释一律简体中文。
- 版本号:每次发布第三位 +1(0.1.0 → 0.1.1),tag `vX.Y.Z` 触发 CI,须与 `Cargo.toml` 一致;release 说明写本版改动。
- push / 打 tag / 发布前必须先问用户。
- 许可证:项目 GPL-3.0-only(已与用户确认;因词库用 rime-ice,GPL-3.0)。PIME 为 LGPL,独立进程调用不波及。发布含词库的包时附各词库许可证与来源。

## Notes for Claude
### 模糊音(10 对,可配置,默认全开)
- 声母:`z=zh` `c=ch` `s=sh` `n=l` `r=l`
- 韵母:`an=ang` `en=eng` `in=ing` `on=ong` `uan=uang`
- 规则两两成对,**不传递**(`n` 与 `r` 不互通);其余对(g=k、h=f 等)默认关。
- 精确匹配优先,模糊匹配带惩罚权重。

### v1 范围
- 必做:整句拼音、模糊音、候选排序、用户词学习、数字键选词、翻页、空格/回车上屏、Shift 中英切换、中文标点。
- 第二批:简拼、中英混输、自定义短语、外观。
- 不做:云输入、表情包、皮肤商店、广告/推荐、语音、手写。保持简洁,勿让范围蔓延。

### 验收
- 回归用例 `(拼音, 期望候选, 期望名次)`:来自用户真实例子 + 按模糊规则批量构造。
- 目标:用户真实用例中期望候选进前 3 位的比例持续上升。
- 输入法写本地、可关闭的日志(拼音、候选、所选),用户贴回后转成用例;不上传。

### 已验证(Windows 11 实机)
- v0.1.0 附加安装包在 PIME 1.3.0-stable 上安装成功,输入字母可弹出候选窗(测试/候选/拼音)。
- PIME 自带 Rime 默认输出繁体(朙月拼音),与本项目无关。

### 前端路线(已与用户确认)
- 引擎 crate 保持独立于 PIME:只做拼音→候选,不依赖 PIME 协议;PIME 视为可替换前端。
- 里程碑:① 先做核心引擎(切分、模糊音、排序、学习);② 安装包自带 PIME 1.3.0 平台文件,用户只装一个 exe(附 LGPL 文本与源码获取方式);③ 引擎稳定后,再用 windows-rs 自写 TSF 前端,彻底摆脱 PIME。
- 日志:0.x 默认开启,写 `%LOCALAPPDATA%\PYIME\log.txt`,建 `log.disabled` 关闭;含输入拼音原文,仅本机。

### 目标:尽量接近微信输入法(依据用户提供的微信设置截图,2026-10-13)
- 候选窗:横排,每个候选带序号,当前高亮为实心绿色圆角块+白字,末尾有 ⌄ 展开按钮;候选字号可调,主题跟随系统。PIME 协议只能改字体/字号/每行个数/是否高亮;外观改动改补丁版 DLL 的 `CandidateWindow`(已做:按自身宽度排版、序号灰色小一号无点号、间距按字号换算,字号 12;选中为绿色 #00B176 圆角块+白字,非选中文字 #333;白底,Win11 经 DWM 圆角,旧系统自绘 1px #E5E5E5 边框;窗口类加 CS_DROPSHADOW 阴影(消息窗同类,一并有阴影)。未做:末尾 ⌄ 展开按钮、主题跟随系统/深色模式;GDI 圆角块无抗锯齿)。
- 中英文切换:**单击 Shift**(默认开);Ctrl 切换默认关;系统自带 Ctrl+空格。
- 翻页:`-`/`=`(上/下翻,默认开)、`[`/`]`(默认开);`,`/`.` 与 Tab/Shift+Tab 默认关。
- 候选选择:数字键;`;`/`'` 选第 2/3 位默认关;左右 Ctrl 选第 2/3 位默认关。
- 标点:Ctrl+`.` 切换中英文标点(默认开);中文模式下 `/?` 键输出 `、`(默认开);符号自动补全成对括号(默认开);数字间符号自动转半角(如 `12:00`,默认开)。
- 默认关:全半角切换(Shift+空格)、简繁切换(Ctrl+Shift+F)、AI 助手(`=`)、V 模式。
- 模糊拼音:单独设置页,每对可单独开关(见上方 10 对)。
- 用户确认:截图之外的其他微信设置(语音、跨设备、手机版、工具栏、常用语/剪贴板、表情推荐等)本项目不需要。
- 词库:rime-ice(GPL-3.0-only,简体+词频)。

### 符号候选(2026-10-22 合并到 main,0a0e7df;设计 `docs/superpowers/specs/2026-10-22-symbol-candidates-design.md`)
- 按词挂:候选文字精确命中 `symbols.txt` 时,符号插在其紧后面,每词至多表内前 2 个,已在列表中的跳过。
- 宿主只限精确整段词(`words` 中有 0→len、fuzzy==0、非残缺的同文字词);模糊、残缺补全(`sh`→上)、前缀词、只靠整句拼出的不挂。
- 选中符号不学习、整句不学、上文清空(`session.rs` `select`)。
- 收录原则:只收"打这个词多半就是要符号"的条目;不收 ASCII 符号、单位缩写、标点名、以普通词义为主的高频词;例外保留 上/下/左/右、一至十、破折号、间隔号(用户确认)。不收 emoji。
- rime-ice `opencc/emoji.txt` 不合用(上→⬆️、二→2️⃣ 是彩色 emoji)。

### 托盘图标与菜单(设计 `docs/superpowers/specs/2026-10-22-tray-menu-design.md`)
- Win8+ 托盘 `windows-mode-icon`:`onActivate` 加、`onDeactivate` 删;左键切中英(组字中忽略:onCommand 回复无编辑会话);Shift 切换同步 `changeButton`。
- 右键菜单:版本(置灰)/编辑用户词库…/重新加载用户词库/打开数据目录;命令 id 见 `tray.rs`。
- 图标为 PIME 原版黑字,深色任务栏可读性待实机确认。
- `onCommand`/`onMenu` 回复无 EditSession,不得带 `showMessage`/`commitString`/`compositionString`/候选(`PIMEClient.cpp:126-137` 的 `updateMessageWindow` 在判空前执行,会崩宿主)。
- 菜单起的子进程(记事本/资源管理器)不继承管道句柄:`main.rs` 启动时已去掉 std 句柄的继承标志,否则 server.exe 崩溃后 Launcher 读不到 EOF、不重启后端。

### user.txt 机制(据 `ime.rs`/`user.rs`)
- server.exe 启动时读 `user.txt`;每次选词把内存数据整份写回。
- 外部修改保护:记下上次读入/写盘后的 mtime 与长度,不一致即不写回(学习只留在内存),直到托盘菜单"重新加载用户词库"成功;启动时读不出(非 UTF-8)也不写回,不再丢数据。
- 重新加载会去 BOM、CRLF→LF;非 UTF-8 时不读入、继续保护。"编辑用户词库…"先写盘、备份 `user.txt.bak`,再开记事本。
- 启动时的导入仍不处理 BOM(首行失效)与 CRLF(`\r` 致计数解析失败、整行跳过);重新加载一次即被规整。

### 待办(截至 2026-10-23)
- 托盘菜单与符号候选已按 `pime/test-kit/menucheck.txt` 实机验证通过(2026-10-23),随 0.1.15 发布。
- `scripts/windows/user-reload.ps1` 未在 Windows 实机运行过(本机无 pwsh,未做语法检查)。
- 符号候选终审未修 Minor:"英文候选不挂符号"无单测;英文候选会被带符号的宿主挤后 1–2 位;`symbols.txt` 非 UTF-8 时整表跳过。

### 待查
- PIME JSON 协议能否承载所需功能(需读源码确认)。
