# PYIME 第三方数据与许可证

- PYIME:GPL-3.0-only,源码 https://github.com/kilobitcy/PYIME
- 词典 `pyime.dict`:由雾凇拼音 rime-ice(GPL-3.0)的 `8105`、`base`、`ext`、`others` 词库编译而成,https://github.com/iDvel/rime-ice
- 英文词表 `pyime.en`:由雾凇拼音 rime-ice(GPL-3.0)的 `en_dicts/en`、`en_dicts/en_ext` 与本项目自带 IT 词表 `en_it.txt` 合并而成,https://github.com/iDvel/rime-ice
- 可选语法模型 `pyime.gram`:万象语法模型 wanxiang-lts-zh-hans(CC BY 4.0),作者 amzxyz,https://github.com/amzxyz/RIME-LMDG ;未修改,原样放置。
- 运行平台:PIME 1.3.0-stable(LGPL 2.0 等(各目录不同,见其 LICENSE.txt)),https://github.com/EasyIME/PIME ,需另行安装。
- 候选窗 DLL `PIMETextService.dll`(x64/x86,安装时替换 PIME 自带的同名文件,卸载时还原):由 PIME 1.3.0-stable 的 PIMETextService(LGPL 2.0)、libIME2(LGPL 2.1)与 jsoncpp(公有领域/MIT)修改后编译。修改内容:候选按自身宽度排版、序号与间距参照微信输入法、MSVC 专有写法改为可用 mingw-w64 编译。修改的源码以补丁形式随 PYIME 源码提供(`pime/patches/`、`pime/tsf_compat.h`),`scripts/build-pime-dll.sh` 可从上游源码复现构建。
- 托盘中/英模式图标 `icons\chi_full_capsoff.ico`、`icons\eng_full_capsoff.ico`:原样取自 PIME 1.3.0-stable 的 `python/cinbase/icons/`(PIME README 声明各部分以 GNU LGPL v2.1 授权),https://github.com/EasyIME/PIME 。
- 设计参考:青简 Qingjian(GPL-3.0-or-later),https://github.com/qingjian-team/qingjian ;学习排序、个人模糊偏好、个人搭配、退格撤销与回放评测的思路借鉴自该项目,代码为本项目重写。
