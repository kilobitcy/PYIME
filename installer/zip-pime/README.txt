PYIME 拼音输入法(附加包)
========================
本包把 PYIME 后端装进已安装的官方 PIME 1.3.0-stable(不要用 2.0.0-beta)。

安装
1. 先安装官方 PIME 1.3.0-stable(只勾"PIME 输入法平台")。
2. 解压本 zip 到任意文件夹(不要直接在压缩包里运行)。
3. 想用语法模型:把 pyime.gram 放进同一文件夹(可选,体积大,单独下载)。
4. 双击 install.bat,在 UAC 弹窗里选"是"。
5. 装完后若输入法列表里没有 PYIME,注销再登录一次。

卸载
双击 uninstall.bat。用户数据(%LOCALAPPDATA%\PYIME,含用户词和日志)会保留,可手动删除。
本包没有"设置 → 应用"里的卸载入口。卸载 PIME 前请先卸载 PYIME。

日志
%LOCALAPPDATA%\PYIME\log.txt、input-log.jsonl(仅本机,建 log.disabled 关闭)。

许可证
PYIME GPL-3.0-only;词库与英文词表 rime-ice GPL-3.0。详见 NOTICE.md 与 LICENSE。
