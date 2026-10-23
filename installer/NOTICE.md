# PYIME 第三方数据与许可证

- PYIME:GPL-3.0-only,源码 https://github.com/kilobitcy/PYIME
- 词典 `pyime.dict`:由雾凇拼音 rime-ice(GPL-3.0)的 `8105`、`base`、`ext`、`others` 词库编译而成,https://github.com/iDvel/rime-ice
- 英文词表 `pyime.en`:由雾凇拼音 rime-ice(GPL-3.0)的 `en_dicts/en`、`en_dicts/en_ext` 与本项目自带 IT 词表 `en_it.txt` 合并而成,https://github.com/iDvel/rime-ice
- 可选语法模型 `pyime.gram`:万象语法模型 wanxiang-lts-zh-hans(CC BY 4.0),作者 amzxyz,https://github.com/amzxyz/RIME-LMDG ;未修改,原样放置。
- 输入法前端 `x64\PYIME.dll`、`x86\PYIME.dll`:由 PIME 1.3.0-stable(https://github.com/EasyIME/PIME )的 PIMETextService(Library GPL v2 or later / LGPL 2.0)、libIME2(LGPL 2.1)与 jsoncpp(公有领域/MIT)修改后编译,按原许可证分发,许可证全文见 `licenses\`。修改内容:不再依赖 PIME 平台、直连 pyime-server.exe、超时放行、候选按自身宽度排版、序号与间距参照微信输入法、可用 mingw-w64 编译。完整修改后源码在 PYIME 源码仓库的 `tsf/` 目录(https://github.com/kilobitcy/PYIME ),`scripts/build-tsf.sh` 可复现构建;如需源码副本也可在该仓库提 issue 索取。`tsf/tsf_compat.h` 取自 windows-rs(MIT/Apache-2.0)。
- 安装程序使用 NSIS 插件 StdUtils(LGPL 2.1 + 插件例外,https://github.com/lordmulder/stdutils ),仅用于以普通用户身份启动 pyime-server.exe。
- 托盘中/英模式图标 `icons\chi_full_capsoff.ico`、`icons\eng_full_capsoff.ico`:原样取自 PIME 1.3.0-stable 的 `python/cinbase/icons/`(PIME README 声明各部分以 GNU LGPL v2.1 授权),https://github.com/EasyIME/PIME 。
- 设计参考:青简 Qingjian(GPL-3.0-or-later),https://github.com/qingjian-team/qingjian ;学习排序、个人模糊偏好、个人搭配、退格撤销与回放评测的思路借鉴自该项目,代码为本项目重写。
