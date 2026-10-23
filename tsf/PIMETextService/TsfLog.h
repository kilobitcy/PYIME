// PYIME:DLL 侧本地日志 %LOCALAPPDATA%\PYIME\tsf.log(同目录有 log.disabled 则不写),仅本机、不上传。
// 只记连接/超时/断线/退避与宿主进程名,不记按键。写不了(如 AppContainer 内)就静默跳过。
#ifndef PYIME_TSF_LOG_H
#define PYIME_TSF_LOG_H

namespace PIME {

// printf 风格写一行,自动加毫秒时间戳、进程号、进程名与换行。
void tsfLog(const char* format, ...);

}

#endif
