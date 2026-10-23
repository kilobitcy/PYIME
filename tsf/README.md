# tsf/:PYIME 的 TSF 文本服务 DLL 源码

收编自 PIME v1.3.0-stable(https://github.com/EasyIME/PIME),由 PYIME 自行维护。

| 目录 | 来源 commit | 许可证 |
|---|---|---|
| `libIME2/` | EasyIME/libIME2 `8ad3c9b433d930ce5614c483461dfa78cedb5efd` | LGPL-2.1(`libIME2/LICENSE.txt`) |
| `PIMETextService/` | EasyIME/PIME `26fcf6ac8874e76b8f75f6826811b03bfdfc2e89` | GNU Library GPL v2 或更高(文件头;PIME `PIME-LICENSE.txt` 记作 LGPL 2.0,全文 `LGPL-2.0.txt`) |
| `jsoncpp/` | open-source-parsers/jsoncpp `ddabf50f72cf369bf652a95c4d9fe31a1865a781` | MIT / 公有领域(`jsoncpp/LICENSE`),原样未改 |
| `tsf_compat.h` | 声明取自 microsoft/windows-rs 的 Windows 元数据 | MIT / Apache-2.0 |

收编部分按原许可证分发修改版,不转 GPL;与 PYIME 其余 GPL-3.0-only 部分组合发布。
修改过的文件在版权块下注明 `PYIME 修改`,改动处注释以 `PYIME:` 开头。

收编时已合入的改动(原为 `pime/patches/`):
- `pime-mingw.patch`:MSVC 专有写法改为可移植写法,使 mingw-w64 可编译。
- `libime2-candidate-width.patch`:候选按自身宽度排版,序号/间距参照微信,选中项绿色圆角块。
- `PIMETextService/resource.h`:原为 UTF-16,已转 UTF-8(windres/g++ 读不了 UTF-16)。

构建:`scripts/build-tsf.sh` → `target/tsf/{x64,x86}/`。
