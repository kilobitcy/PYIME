# PYIME

## Overview
Windows 11 拼音输入法。目标:界面与功能像微信输入法一样简洁,但在福建人常用的模糊拼音下,候选排序要准、选过的词要被学习。
痛点:① 模糊音展开后期望词排名靠后;② 用户词学习不足。

## Tech Stack
- 引擎:Rust(独立 crate,Linux 上可单测,不依赖 Windows)。
- Windows 前端:自带 TSF 前端 `PYIME.dll`(x64/x86,C++,收编自 PIME 1.3.0-stable 的 libIME2 + PIMETextService,源码在 `tsf/`),经命名管道 `\\.\pipe\<用户名>\PYIME` 直连 Rust 服务进程 `pyime-server.exe`;一条消息一个 JSON,字段沿用 PIME 协议。TSF 层长期保持 C++(设计 `docs/superpowers/specs/2026-10-23-tsf-frontend-design.md`)。
- 词库:开源词库 + 词频起步;第二阶段训练 bigram 重排。完全离线。
- 构建/发布:GitHub Actions 因账单锁定不可用(`release.yml` 保留待解锁);现走本机交叉编译(mingw-w64,目标 `x86_64-pc-windows-gnu`)+ `makensis` 出安装包,`gh release create` 手动发布。
- 安装形态:独立安装包 `PYIME-<版本>-setup.exe`,不需要官方 PIME;装到 `C:\Program Files\PYIME`,`regsvr32` 注册 `x64\`、`x86\PYIME.dll`,HKLM Run `PYIME` = `pyime-server.exe --supervise`,以非提权身份启动(NSIS 插件 StdUtils `ExecShellAsUser`,在 `installer/nsis/StdUtils/`)。占用中的 DLL/exe 靠改名替换,无需重启。检测到旧版 PIME 附加包时先静默运行其卸载程序(还原 PIME 原版 DLL)。只支持 x64 Windows(32 位、ARM64 拒装)。安全:忽略 `/D=`、固定装到 Program Files;旧附加包不在 Program Files 下时不自动执行其卸载程序;降权启动退回提权(StdUtils 返回 fallback)时停掉服务端并提示注销重登。
- TSF 前端构建:`scripts/build-tsf.sh` 用 mingw 交叉编译 `tsf/` 到 `target/tsf/{x64,x86}/PYIME.dll`;`tsf/tsf_compat.h` 补 mingw 缺的 TSF 接口(取自 windows-rs)。实机验证清单与开发脚本在 `test-kit/`(`tsf.txt`、`tsf-dev.bat`)。

## Project Layout
- `crates/pyime-engine`:引擎(`dict` fst 词典、`rime` 词库导入、`pinyin` 音节表与切分格、`fuzzy` 10 对模糊音、`engine` 候选生成与整句 Viterbi、`user` 用户学习、`english` 英文词表与回车学习、`symbol` 符号表(精确整段词后附加符号)、`typo` 按键纠错(输入能完整切成音节且无精确整段词时,查改一处的整段词,第 2 位));`pyime-dict-build` 编译词典、`pyime-en-build` 编译英文词表 `pyime.en`(自带 IT 词表 `installer/en_it.txt`)。
- `crates/pyime-backend`:服务进程 `pyime-server.exe`(`--supervise` 监护并重启 worker,`supervise.rs`;`--worker` 管道服务端 `pipe.rs` + `session`/`ime`/`tray`/`log`;`--quit` 让监护进程连同 worker 退出);词典 `pyime.dict`、英文词表 `pyime.en`、符号表 `symbols.txt`(自带 `installer/symbols.txt`)放在 `pyime-server.exe` 同目录,用户数据 `%LOCALAPPDATA%\PYIME\user.txt`,用户英文词表 `en.txt`、回车学习计数 `en_learned.txt`、模糊音开关 `fuzzy.txt`(每行一个关闭的规则名,如 `r=l`;含一行 `纠错` 表示关闭按键纠错;含一行 首键暂存 表示关闭首键暂存补发,改后重启输入法生效;不存在即全开)同目录、默认中文程序列表 `chinese-apps.txt`(每行一个 exe 名,不分大小写;不在表的程序默认英文;文件不存在时写入 Weixin.exe/WeChat.exe/wps.exe/et.exe/wpp.exe,想全英文请留空文件)。
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
#   scripts/build-tsf.sh
#   cp target/x86_64-pc-windows-gnu/release/pyime-server.exe installer/pyime-server.exe
#   makensis -DVERSION=<版本> installer/pyime.nsi(需先有 installer/licenses/,一般直接用 scripts/package.sh <版本>)
# 依赖验收:x86_64-w64-mingw32-objdump -p <exe> | grep 'DLL Name'(不得含 libgcc/libwinpthread)
```

## Conventions
- 回复、commit、注释一律简体中文。
- 版本号:每次发布第三位 +1(0.1.0 → 0.1.1),tag `vX.Y.Z` 触发 CI,须与 `Cargo.toml` 一致;release 说明写本版改动。
- push / 打 tag / 发布前必须先问用户。
- 许可证:项目 GPL-3.0-only(已与用户确认;因词库用 rime-ice,GPL-3.0)。收编的 TSF 前端(`tsf/`:libIME2 LGPL-2.1、PIMETextService LGPL-2.0、jsoncpp 公有领域/MIT)按原许可证分发修改版、不转 GPL;安装包附 `licenses\` 全文与源码获取方式(`installer/NOTICE.md`)。发布含词库的包时附各词库许可证与来源。

## Notes for Claude
### 模糊音(10 对,可配置,默认全开)
- 声母:`z=zh` `c=ch` `s=sh` `n=l` `r=l`
- 韵母:`an=ang` `en=eng` `in=ing` `on=ong` `uan=uang`
- 规则两两成对,**不传递**(`n` 与 `r` 不互通);其余对(g=k、h=f 等)默认关。
- 精确匹配优先,模糊匹配带惩罚权重。
- 本身不是音节、模糊后才合法的写法(zhon→zhong、den→deng、tin→ting、zuang→zhuang)也参与切分(`fuzzy::spellings_at`);2026-10-27 前 on=ong 等规则对这类写法不生效。

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
- 引擎 crate 保持独立于前端:只做拼音→候选,不依赖 PIME 协议与管道。
- 里程碑:① 核心引擎(已完成);② 原"安装包自带 PIME 平台文件"已放弃;③ 自带 TSF 前端、去掉 PIME 依赖(收编 libIME2,C++,见上方设计文档)已完成;此后外观改进(⌄ 展开按钮、深色模式)直接改 `tsf/`。
- 日志:0.x 默认开启,写 `%LOCALAPPDATA%\PYIME\log.txt`,建 `log.disabled` 关闭;含输入拼音原文,仅本机。

### 目标:尽量接近微信输入法(依据用户提供的微信设置截图,2026-10-13)
- 候选窗:横排,每个候选带序号,当前高亮为实心绿色圆角块+白字,末尾有 ⌄ 展开按钮;候选字号可调,主题跟随系统。协议字段只能改字体/字号/每行个数/是否高亮;外观改动改 `tsf/` 的 `CandidateWindow`(已做:按自身宽度排版、序号灰色小一号无点号、间距按字号换算,字号 12;选中为绿色 #00B176 圆角块+白字,非选中文字 #333;白底,Win11 经 DWM 圆角,旧系统自绘 1px #E5E5E5 边框;窗口类加 CS_DROPSHADOW 阴影(消息窗同类,一并有阴影)。未做:末尾 ⌄ 展开按钮、主题跟随系统/深色模式;GDI 圆角块无抗锯齿)。
- 中英文切换:**单击 Shift**(默认开);Ctrl 切换默认关;系统自带 Ctrl+空格。
- 翻页:`-`/`=`(上/下翻,默认开)、`[`/`]`(默认开);`,`/`.` 与 Tab/Shift+Tab 默认关。
- 候选选择:数字键;`;`/`'` 选第 2/3 位默认关;左右 Ctrl 选第 2/3 位默认关。
- 标点:Ctrl+`.` 切换中英文标点(默认开);中文模式下 `/?` 键输出 `、`(默认开);符号自动补全成对括号(默认开)(模拟方向键回退光标时,按着的 Shift 在同一次 SendInput 里临时抬起再按回);数字间符号自动转半角(如 `12:00`,默认开)。
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
- Win8+ 托盘 `windows-mode-icon`:`onActivate` 加、`onDeactivate` 删;左键切中英(组字中忽略:onCommand 回复无编辑会话);Shift 切换同步 `changeButton`。单击 Shift 在带编辑会话的 `onKeyUp` 切换(`filterKeyUp` 只回 true),回复带 `showMessage` 在光标旁闪"中/英"约 1 秒;托盘、菜单切换不提示。
- 右键菜单:版本(置灰)/模糊音 ▸(10 对逐项勾选,即时生效、写 `fuzzy.txt`;0.1.16 加,待实机验证)/按键纠错(勾选即开,写 `fuzzy.txt`;待实机验证)/本程序默认中文(<exe>)(勾选即写 chinese-apps.txt 并切当前会话;组字中只改列表;取不到 exe 置灰;0.1.17 加,待实机验证)/编辑用户词库…/重新加载用户词库/打开数据目录;命令 id 见 `tray.rs`。
- 图标为 PIME 原版黑字,深色任务栏可读性待实机确认。
- `onCommand`/`onMenu` 回复无 EditSession,不得带 `showMessage`/`commitString`/`compositionString`/候选(`tsf/` 下 `PIMEClient.cpp` 的 `updateMessageWindow` 在判空前执行,会崩宿主)。
- 菜单起的子进程(记事本/资源管理器)三路 stdio 置空(`ime.rs` `launch`);服务端经命名管道通信,不再有 stdin/stdout 管道句柄被继承的问题。

### user.txt 机制(据 `ime.rs`/`user.rs`)
- pyime-server.exe(worker)启动时读 `user.txt`;每次选词把内存数据整份写回。
- 外部修改保护:记下上次读入/写盘后的 mtime 与长度,不一致即不写回(学习只留在内存),直到托盘菜单"重新加载用户词库"成功;启动时读不出(非 UTF-8)也不写回,不再丢数据。
- 重新加载会去 BOM、CRLF→LF;非 UTF-8 时不读入、继续保护。"编辑用户词库…"先写盘、备份 `user.txt.bak`,再开记事本。
- 启动时读入与重新加载同样规整(去 BOM、CRLF→LF,`ime.rs` `normalize_user`)。
- 每条词对/搭配带最后使用日(`拼音⇥词⇥次数⇥日`,`@ng⇥上文⇥词⇥次数⇥日`;旧格式缺日期按读入当天)。计数 1 且 60 天未用即淘汰(启动、重新加载、每天首次写盘时),淘汰前备份 `user.txt.aged.bak`;`@ng` 满 10 万时淘汰最旧 1000 条一次性条目。降级到旧版会丢个人搭配。
- 学习只置脏,hub 在空闲 1 s 或首次改动后 10 s 写盘;worker 被结束最多丢约 10 s 学习;重新加载丢弃未写盘学习;"编辑用户词库…"强制写盘。

### 待办(截至 2026-10-26)
- 托盘菜单与符号候选已按 `menucheck.txt` 实机验证通过(2026-10-23);自带 TSF 前端已按 `test-kit/tsf.txt` 实机验证通过;0.1.15 已发布(2026-10-24)。
- 长句不准(用户反馈 2026-10-26):主因是单选学过的单字(如 yi→已 ×17)个人加分 ≥11,在整句里压过多字词;已在 `sentences()` 把句中单字的个人加分封顶 `SENTENCE_CHAR_BONUS_CAP`=5.0。带用户 `user.txt` 对日志 342 条长输入首选变化 13 条,均为改善(转已→转移、我转前→我赚钱 等),无变坏;待实机确认。`real_engine` 支持 `PYIME_USER=<user.txt>`。
- `scripts/windows/user-reload.ps1` 已改为停启 pyime-server.exe,reload-user.bat 已按 `test-kit/tsf.txt` 第 14 项实机验证;edit-user.bat 未单独实机验证。
- 智能提升评估(2026-10-28,`docs/superpowers/specs/2026-10-28-slim-gram-design.md`):精简语法模型默认随包已放弃;调跨上屏上文加分、"最近一次选择"加分均无效。日志回放须剔除被撤回的选择,并按"当时首位是否即所选"拆易/难两类(难类可靠、易类有曝光偏差)。旧日志(0.1.9–0.1.12,基本未装 gram)难类未解决 185 条中 157 条是首次输入的词。待办:用户装完整 `pyime.gram` 用当前版本积累新日志后重做归因;届时若误选(退格删除未记为撤回)污染学习占比高,考虑退格删除刚上屏词时撤销学习。
- 符号候选终审未修 Minor:"英文候选不挂符号"无单测;英文候选会被带符号的宿主挤后 1–2 位;`symbols.txt` 非 UTF-8 时整表跳过。
- 默认英文 + 按程序默认中文、断管道重连重发(2026-10-30,设计 `docs/superpowers/specs/2026-10-30-default-mode-and-first-key-design.md`):待按 `test-kit/tsf.txt` 第十五至十八项实机验证;首键丢失的 TSF 上下文路径待用户贴回 `tsf.log`("字母键未交给输入法""退避期内…")后对症修。会话中/英状态经回复 `english` 由 DLL 记住、重连 `init` 带回。已合并到本地 main(e880250,未推送);0.1.17 测试包 `installer/PYIME-0.1.17-setup.exe` 已打(2026-10-31,词典用当日浅克隆的最新 rime-ice);版本号改动(Cargo.toml/Cargo.lock)未提交,实机验证通过后再问用户提交、打 tag、发布。遗留 Minor 见 `.superpowers/sdd/progress.md`。
- 按键纠错(2026-11-03,分支 typo-correction,设计 `docs/superpowers/specs/2026-11-03-typo-correction-design.md`):`TYPO_PENALTY`=5.0,回归零变化;待按 `test-kit/tsf.txt` 第九节实机验证。
- 成对括号光标与中/英提示(2026-11-04,分支 caret-and-mode-hint,设计 `docs/superpowers/specs/2026-11-04-caret-and-mode-hint-design.md`):待按 `test-kit/tsf.txt` 第十节实机验证。
- 0.1.18 已发布(2026-11-04,tag v0.1.18,含 0.1.16/0.1.17 测试版内容、按键纠错、括号光标、中/英提示);第九、十节及第十五至十八项仍待实机验证。
- 首键暂存补发 + 漏键诊断(2026-11-05,分支 first-key-stash,设计 `docs/superpowers/specs/2026-11-05-first-key-stash-design.md`):Chromium 禁用+空上下文的首个字母暂存后原序补发,超时 500 ms;Mullion 路径(键未交给输入法)只记诊断,补偿待 Mullion 侧日志对时后定。待按 `test-kit/tsf.txt` 第十一节实机验证。
- 未经输入法字母键拦截补发(2026-11-08,设计 `docs/superpowers/specs/2026-11-08-missed-key-replay-design.md`):0.1.19 实机日志证实记事本里首键按下进了本线程消息队列、焦点文档与 HIMC 都在,TSF 却没交给本模块(Mullion"无 HIMC"推测不成立);钩子取出该键 WM_CHAR 时拦下、经首键暂存补发。已合并 main,0.1.20 测试包已打(2026-11-08)。0.1.20 实机见立即补发的键同样未经输入法(窗口刚取得焦点时 TSF 一小段时间不接键),改为延后补发(200 ms 或下一个键到输入法时,2026-11-09 合并 main);0.1.21 测试包已打(版本号改动未提交),待按 `test-kit/tsf.txt` 第十二节实机验证。
- 微信 4.2 成对括号后 Shift 卡住(2026-11-11,已实机验证修复):Shift+9 后连打 1 得 `（）！！！1！`。日志证实服务端在 onKeyDown 里同步 `SendInput` 被拖约 1.24s(推断为宿主低级键盘钩子在等回复的线程上),注入序列与用户按键交错,注入的 Shift↓ 落在用户 Shift↑ 之后。改为专用线程按序注入(`ime.rs` `send_key_async`),回复先发出;实测 SendInput 3.4ms、顺序正确、光标回到括号中间。注入时日志记物理 Shift、注入条数与耗时。
- user.txt 老化淘汰与空闲写盘(2026-11-11,分支 user-aging,设计 `docs/superpowers/specs/2026-11-11-user-aging-design.md`):用户真实 user.txt 模拟 60 天后淘汰词对 2933、搭配 26274,文件 636 KB→94 KB;带该文件 4252 个拼音候选与改动前一致(仅既有的同分并列顺序波动,新旧版本同样存在)。待实机验证(选词后约 1 s 文件更新、托盘重新加载/编辑行为不变)。
