// PYIME:DLL 侧本地日志,见 TsfLog.h。格式与后端 log.txt 一致:"<毫秒> [<进程号> <进程名>] <内容>"。
#include "TsfLog.h"
#include <windows.h>
#include <ShlObj.h>
#include <cstdarg>
#include <cstdio>
#include <string>
#include "libIME2/src/Utils.h"

namespace PIME {

static const ULONGLONG kMaxBytes = 1024 * 1024;  // 超过即改名为 tsf.old.log,重新开始

// %LOCALAPPDATA%\PYIME;用 SHGetKnownFolderPath,不依赖宿主进程的环境变量
static std::wstring logDir() {
    PWSTR path = nullptr;
    std::wstring dir;
    if (SUCCEEDED(::SHGetKnownFolderPath(FOLDERID_LocalAppData, 0, nullptr, &path)))
        dir = std::wstring(path) + L"\\PYIME";
    ::CoTaskMemFree(path);
    return dir;
}

static std::string processName() {
    wchar_t buf[MAX_PATH] = {0};
    DWORD n = ::GetModuleFileNameW(nullptr, buf, MAX_PATH);
    const wchar_t* name = buf;
    for (DWORD i = 0; i < n; ++i) {
        if (buf[i] == L'\\')
            name = buf + i + 1;
    }
    return utf16ToUtf8(name);
}

void tsfLog(const char* format, ...) {
    std::wstring dir = logDir();
    if (dir.empty() || ::GetFileAttributesW((dir + L"\\log.disabled").c_str()) != INVALID_FILE_ATTRIBUTES)
        return;
    ::CreateDirectoryW(dir.c_str(), nullptr);
    std::wstring path = dir + L"\\tsf.log";
    WIN32_FILE_ATTRIBUTE_DATA info;
    if (::GetFileAttributesExW(path.c_str(), GetFileExInfoStandard, &info) &&
        ((ULONGLONG(info.nFileSizeHigh) << 32) | info.nFileSizeLow) > kMaxBytes)
        ::MoveFileExW(path.c_str(), (dir + L"\\tsf.old.log").c_str(), MOVEFILE_REPLACE_EXISTING);
    // 多个宿主进程同时追加:FILE_APPEND_DATA + 共享读写删
    HANDLE file = ::CreateFileW(path.c_str(), FILE_APPEND_DATA,
                                FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE,
                                nullptr, OPEN_ALWAYS, FILE_ATTRIBUTE_NORMAL, nullptr);
    if (file == INVALID_HANDLE_VALUE)
        return;
    char msg[512];
    va_list args;
    va_start(args, format);
    std::vsnprintf(msg, sizeof(msg), format, args);
    va_end(args);
    FILETIME ft;
    ::GetSystemTimeAsFileTime(&ft);
    // FILETIME(1601 起,100ns)→ Unix 毫秒
    unsigned long long ms = ((((ULONGLONG)ft.dwHighDateTime) << 32 | ft.dwLowDateTime) - 116444736000000000ULL) / 10000;
    char line[768];
    int len = std::snprintf(line, sizeof(line), "%llu [%lu %s] %s\n",
                            ms, (unsigned long)::GetCurrentProcessId(), processName().c_str(), msg);
    if (len > 0) {
        if (len >= (int)sizeof(line))
            len = sizeof(line) - 1;
        DWORD written = 0;
        ::WriteFile(file, line, (DWORD)len, &written, nullptr);
    }
    ::CloseHandle(file);
}

}
