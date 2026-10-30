//! 默认中文的程序列表 `chinese-apps.txt`:每行一个 exe 文件名,不区分大小写;不在表里的程序默认英文。

/// 文件不存在时写入的初始列表。
pub const DEFAULTS: [&str; 5] = ["Weixin.exe", "WeChat.exe", "wps.exe", "et.exe", "wpp.exe"];

#[derive(Clone, Debug, PartialEq)]
pub struct ChineseApps(Vec<String>);

impl ChineseApps {
    pub fn defaults() -> ChineseApps {
        ChineseApps(DEFAULTS.iter().map(|s| s.to_string()).collect())
    }

    /// 解析文件内容:去 BOM,按行去首尾空白(含 CR),忽略空行。
    pub fn parse(text: &str) -> ChineseApps {
        let text = text.strip_prefix('\u{feff}').unwrap_or(text);
        ChineseApps(text.lines().map(str::trim).filter(|l| !l.is_empty()).map(String::from).collect())
    }

    /// `exe` 是否在表(不区分大小写);空名不在表。
    pub fn contains(&self, exe: &str) -> bool {
        !exe.is_empty() && self.0.iter().any(|a| a.eq_ignore_ascii_case(exe))
    }

    /// 在表则移除(含各种大小写写法),不在则追加;返回切换后是否在表。
    pub fn toggle(&mut self, exe: &str) -> bool {
        if self.contains(exe) {
            self.0.retain(|a| !a.eq_ignore_ascii_case(exe));
            false
        } else {
            self.0.push(exe.to_string());
            true
        }
    }

    /// 存盘文本:每行一个,LF 结尾。
    pub fn export(&self) -> String {
        self.0.iter().map(|a| format!("{a}\n")).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn 初始列表含微信与wps三件套() {
        let a = ChineseApps::defaults();
        assert_eq!(a.export(), "Weixin.exe\nWeChat.exe\nwps.exe\net.exe\nwpp.exe\n");
    }

    #[test]
    fn 解析去bom_crlf_空白与空行() {
        let a = ChineseApps::parse("\u{feff}Weixin.exe\r\n\r\n  wps.exe  \n");
        assert_eq!(a.export(), "Weixin.exe\nwps.exe\n");
    }

    #[test]
    fn 匹配不区分大小写_空名不在表() {
        let a = ChineseApps::parse("Weixin.exe\n");
        assert!(a.contains("WEIXIN.EXE"));
        assert!(a.contains("weixin.exe"));
        assert!(!a.contains("notepad.exe"));
        assert!(!a.contains(""));
    }

    #[test]
    fn 切换加入与移除() {
        let mut a = ChineseApps::parse("Weixin.exe\n");
        assert!(a.toggle("Notepad.exe"));
        assert_eq!(a.export(), "Weixin.exe\nNotepad.exe\n");
        assert!(!a.toggle("weixin.exe"));
        assert_eq!(a.export(), "Notepad.exe\n");
    }

    #[test]
    fn 空文件即空列表() {
        assert_eq!(ChineseApps::parse("").export(), "");
    }
}
