# 托盘输入法图标与右键菜单 设计

日期:2026-10-22 · 基于 main = 0feb027 · 经 grilling 逐项与用户确认

## 背景

- 需求:右下角输入法指示区右键弹功能菜单(版本、重载词库、编辑词库等)。
- 现状:后端未处理任何语言栏消息(`session.rs` `handle` 的 `_ => {}` 原样回 `success:true`),
  托盘里没有 PYIME 自己的图标;`user.txt` 只在启动时读(`ime.rs` `Ime::load`),
  手改须靠 `scripts/windows/user-reload.ps1` 停输入法再重启。
- PIME 1.3.0 原生支持(源码 `/tmp/PIME-pyime`,v1.3.0-stable):
  - 回复中带 `"addButton":[{id,icon,commandId,text,tooltip,type,enable,toggled}]` 即加按钮,
    另有 `removeButton`(id 数组)、`changeButton`(同 addButton 字段);
  - id 为 `windows-mode-icon` 的按钮用 `GUID_LBI_INPUTMODE`,即 Win8+ 托盘的输入法模式图标
    (`PIMELangBarButton.cpp:55-61`);
  - 左键:`{"method":"onCommand","id":<commandId>,"type":0}`;
  - 右键:`{"method":"onMenu","id":"windows-mode-icon"}`,回复 `"return":[菜单项…]`,
    `TrackPopupMenu` 弹出;选中项发 `onCommand(id, type=2)`;菜单为空则回落到基类(发 type=1);
  - 菜单项:`{"text","id"}`;`{}` 为分隔线;`"checked":bool`(默认 false);
    `"enabled":bool`(默认 true,false 置灰);`"submenu":[…]` 子菜单;
  - `init` 带 `isWindows8Above`,`onActivate` 带 `isKeyboardOpen`;
    上游 Python 后端仅在 Win8+ 时加 `windows-mode-icon`。
- 改动只在 Rust 后端与安装包,**不动补丁版 DLL**。

## 1. 托盘图标

- `init` 只记 `isWindows8Above`;Win8+ 时在 `onActivate` 回复里 `addButton` 一个 `windows-mode-icon`,
  `onDeactivate` 时 `removeButton`(同上游 `chewing_ime.py:196-231,291-292`)。
  不得在 `init` 与 `onActivate` 都加:DLL 的 `buttons_` 为 map,同 id 第二次 `emplace` 失败但
  按钮已加进 TSF,之后删不掉(`PIMEClient.cpp:237-241`)。
- 图标:中文态 `chi_full_capsoff.ico`,英文态 `eng_full_capsoff.ico`;取自 PIME 上游
  `python/cinbase/icons/`(md5 与用户提供的 `/tmp/*.ico` 一致,48/32/24/16 四尺寸,黑字透明底)。
  复制到 `installer/icons/`,安装到 `server.exe` 同目录的 `icons\`;icon 字段传绝对路径。
  不引用 PIME 目录里的原文件(用户的 PIME 不一定装了 python 后端)。
- 许可:图标随 PIME 为 LGPL,安装包许可证说明中注明来源。
- 左键(`onCommand` type=0):非组字时切换中英,与单击 Shift 同效。**组字中忽略**(不切换、不换图标,
  记一行日志):`onCommand` 的回复经 `updateStatus(msg, nullptr)` 处理,无 EditSession 时 DLL
  只处理按钮,不处理上屏/组字/候选(`PIMEClient.cpp:319-328`),清空组字会使界面与后端失同步。
- 单击 Shift 或左键切换后,回复里 `changeButton` 换图标。
- 中/英状态维持现状:按连接(程序)各自独立(`Session.english`);图标跟随焦点程序的会话。
- 已知风险:黑字在 Win11 深色任务栏上对比度低。先照用,实机看不清再从原图生成白字版,
  按注册表 `SystemUsesLightTheme` 选用(仅在加/换按钮时读,不监听主题变化)。

## 2. 右键菜单

```
PYIME 0.1.14          (置灰,enabled=false,版本取 env!("CARGO_PKG_VERSION"))
──────────
编辑用户词库…
重新加载用户词库        (检测到文件被外部修改时文字为"重新加载用户词库(文件已修改)")
──────────
打开数据目录           (先 create_dir_all,再用资源管理器打开 %LOCALAPPDATA%\PYIME)
```

- 菜单项 id 从 1 起:`menuFromJson` 以 `id==0` 且 `text` 空判分隔线,`TrackPopupMenu` 返回值 >0
  才回调(`PIMEClient.cpp:545-546`,`PIMELangBarButton.cpp:158`)。左键与菜单命令靠 `type`(0/2)区分。

- 不放:模糊音开关(日后单独设置页)、日志开关(已有 `log.disabled`)、中英标点(已有 Ctrl+`.`)。
- 菜单命令执行于 server.exe 进程内;`Ime` 为全部连接共享(`main.rs:71` `ime.clone()`),
  外部修改保护与重载对所有程序同时生效。
- 每次菜单命令写一行 `log.txt`。

## 3. 编辑与重载语义(外部修改保护,经复核替代原"暂停写回"方案)

- **外部修改保护**:`Ime` 记下上次写盘(或读入)后 `user.txt` 的 mtime 与长度。
  `save()` 写盘前 `metadata()` 比对,不一致即视为被外部修改:跳过写盘、记日志(学习照常更新内存)。
  文件不存在时照常写。只保护 `user.txt`;`en_learned.txt` 走 `save_english`,不受影响。
- **编辑用户词库…**(无状态):
  1. 若文件未被外部修改,把内存数据写盘一次(保证最新);已被修改则跳过,免冲掉用户已保存的编辑;
  2. 备份为 `user.txt.bak`(打开前备份,与 `user-reload.ps1` 一致,供改坏后回退);
  3. 用 `notepad.exe <user.txt>` 打开,不等待进程退出
     (Win11 新记事本可能并入已有窗口、启动进程立即退出,等待不可靠)。
- **重新加载用户词库**:
  1. 读 `user.txt`,校验 UTF-8;去 BOM,CRLF→LF;
  2. 校验失败:不读入、记日志;记录值不更新,保护继续生效,文件不会被覆盖
     (避免"非 UTF-8 时按空数据启动、下次选词写空"的旧风险在重载路径重现);
  3. 成功:重新导入用户数据,同时重读 `en.txt`;更新 mtime/长度记录(写回随之恢复);
     代际号 +1,各 Session 的 `learned` 撤销列表带代际号,旧代的在退格撤销时丢弃,
     免得对新载入的计数做 `dec`(`session.rs:283`、`user.rs:184-191`)。
- 以文件为准:用户保存前后新学到的词,在重载时被文件内容覆盖(与 `user-reload.ps1` 效果一致)。
  外部编辑器改完直接点"重新加载",流程相同。
- 子进程(记事本、资源管理器)三路 stdio 显式 `Stdio::null()`:后端经 stdin/stdout 与 PIME 通信
  (`main.rs:34-79`),继承句柄会使子进程持有管道,server.exe 退出后 Launcher 读不到 EOF。

## 4. 不动的部分

- `scripts/windows/` 三个脚本保留不改;文档中降级为"输入法卡死或菜单不可用时的备用办法"
  (进程无响应时只有脚本能强杀重启)。
- 未知方法的回复保持 `success:true`。
- 启动路径仅加一处(经用户确认顺手修):`Ime::load` 读 `user.txt` 失败时仍按空数据启动
  (`ime.rs:142-144`),但**不记 mtime/长度**;此时文件存在即被视为外部修改,`save()` 不覆盖它,
  用户修好编码后点"重新加载"恢复。由此堵住"非 UTF-8 时学习数据全丢"的旧风险。
  只有成功读入(或文件不存在)才记录。

## 5. 测试与交付

- 单测(Linux):菜单 JSON 结构、版本项、id ≥1;`onCommand` 各 id 的分发;
  文件被外部修改后 `save` 不落盘、重载后恢复;启动读入失败时 `save` 不覆盖原文件;重载的 BOM/CRLF/非 UTF-8 处理;编辑前 `.bak` 备份;
  重载后旧代 `learned` 不触发 `unlearn`;Shift 与非组字左键切换都回 `changeButton`,组字中左键无动作;
  `addButton` 只出现在 `onActivate` 回复。
  启动记事本/资源管理器的调用隔离成可替换函数,单测不真正起进程。
- 测试包 `installer/PYIME-0.0.0-menucheck-setup.exe`:同时包含符号候选与托盘菜单,
  `pime/test-kit/` 写实机验证清单,至少含:
  - Win11 右键托盘图标能否弹出菜单,各项是否生效;左键与 Shift 切换时图标是否同步;
  - 编辑→保存→重载后新词生效,`user.txt.bak` 已生成;
  - 黑字图标在深色任务栏上的可读性;
  - `changeButton` 是否真能刷新托盘模式图标(上游 `PIMEClient.cpp:261` 有 `FIXME: handle
    windows-mode-icon`);不生效则回退为固定图标、只保留菜单;
  - 浏览器/Office 等多线程程序切换标签页后图标是否变空白(上游 `clearIconCache` 跨实例
    `DestroyIcon`,`PIMELangBarButton.cpp:141-146`);
  - 点菜单外部时菜单能否关闭(上游 `TrackPopupMenu` 未 `SetForegroundWindow`);
  - 符号候选原有待验项(`↑ ② ✓ ★` 是否显示为方框、选 `↑` 后 `user.txt` 无新增)。
- 两项均验证后发 0.1.15,release 说明写符号候选与托盘菜单;发版前先问用户。
