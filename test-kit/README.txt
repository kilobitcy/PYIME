PYIME:PIME 候选窗等宽问题实机验证包

内容
  baseline\{x64,x86}\PIMETextService.dll  mingw 编译、未改排版(对照)
  patched\{x64,x86}\PIMETextService.dll   候选按自身宽度排版;间距、序号(小一号、灰色、无点号)参照微信
  patched\server.exe                      PYIME 后端,候选字号 16 → 12(参照微信截图换算)
  pime-dll-test.ps1                       替换/还原脚本(自动请求管理员权限)
  1-baseline.bat(对照版) / 2-patched.bat(改版) / 3-restore.bat(还原)   双击即可

步骤(全程无需重启或注销)
  1. 双击「2-patched.bat」,同意管理员授权。脚本会替换文件、重启 PIMELauncher,
     并弹出一个「PYIME 候选窗测试」窗口;在里面输入 ocr,对比微信输入法的间距与字号。
  2. 想先确认编译器本身没问题,可双击「1-baseline.bat」,同样在弹出的测试窗口里打字。
  3. 出问题:双击「3-restore.bat」,立即还原,同样无需重启。

对正在使用的程序的影响
  - 已打开的程序继续用旧 DLL,不受影响;它们自然关闭重开后才会用上新 DLL。
  - 重启 PIMELauncher 时,所有程序里的输入法会断开约 1-2 秒,下次按键自动重连;
    若当时正在输入拼音,未上屏的拼音会丢失。建议执行脚本前先停下手头的输入。
  - 字号由 server.exe 下发,所以已打开程序里的候选字号也会随之变为 12(排版仍是旧的)。

说明
  首次替换时原版备份为 .orig(DLL 与 server.exe 各一份),还原即改回它们。
  当前装的是哪一版记录在 PIME 目录的 pyime-dll-test.txt。
  被占用的旧文件会改名为 .old-时间戳(不能直接删除),下次运行脚本时自动清理。
  稳定性测试:之后新打开的程序(记事本、浏览器、微信、Office、任一 32 位程序)都会用新 DLL。
