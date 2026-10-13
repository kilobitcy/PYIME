# PYIME

## Overview
Windows 11 拼音输入法。目标:界面与功能像微信输入法一样简洁,但在福建人常用的模糊拼音下,候选排序要准、选过的词要被学习。
痛点:① 模糊音展开后期望词排名靠后;② 用户词学习不足。

## Tech Stack
- 引擎:Rust(独立 crate,Linux 上可单测,不依赖 Windows)。
- Windows 前端:复用 PIME 1.3.0-stable(TSF 宿主;2.0.0-beta 在用户 Win11 上选词会崩宿主进程,见上游 issue #888/#904,不用),Rust 引擎作为后端进程,经 PIME 的 JSON 协议通信。
- 词库:开源词库 + 词频起步;第二阶段训练 bigram 重排。完全离线。
- 构建/发布:GitHub Actions 因账单锁定不可用(`release.yml` 保留待解锁);现走本机交叉编译(mingw-w64,目标 `x86_64-pc-windows-gnu`)+ `makensis` 出安装包,`gh release create` 手动发布。
- 安装形态:附加安装包 `PYIME-<版本>-setup.exe`,装进已安装的官方 PIME **1.3.0-stable**(往其 `backends.json` 登记后端并重新 `regsvr32`)。

## Project Layout
(待建立,随代码补充)

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
- 候选窗:横排,每个候选带序号,当前高亮为实心绿色圆角块+白字,末尾有 ⌄ 展开按钮;候选字号可调,主题跟随系统。PIME 候选窗只能改字体/字号/每行个数/是否高亮,圆角与配色要等自写 TSF 前端。
- 中英文切换:**单击 Shift**(默认开);Ctrl 切换默认关;系统自带 Ctrl+空格。
- 翻页:`-`/`=`(上/下翻,默认开)、`[`/`]`(默认开);`,`/`.` 与 Tab/Shift+Tab 默认关。
- 候选选择:数字键;`;`/`'` 选第 2/3 位默认关;左右 Ctrl 选第 2/3 位默认关。
- 标点:Ctrl+`.` 切换中英文标点(默认开);中文模式下 `/?` 键输出 `、`(默认开);符号自动补全成对括号(默认开);数字间符号自动转半角(如 `12:00`,默认开)。
- 默认关:全半角切换(Shift+空格)、简繁切换(Ctrl+Shift+F)、AI 助手(`=`)、V 模式。
- 模糊拼音:单独设置页,每对可单独开关(见上方 10 对)。
- 用户确认:截图之外的其他微信设置(语音、跨设备、手机版、工具栏、常用语/剪贴板、表情推荐等)本项目不需要。
- 词库:rime-ice(GPL-3.0-only,简体+词频)。

### 待查
- PIME JSON 协议能否承载所需功能(需读源码确认)。
