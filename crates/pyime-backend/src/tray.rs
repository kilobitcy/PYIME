//! 托盘输入法模式图标(PIME 的 `windows-mode-icon`)及其右键菜单的 JSON 与命令 id。
use serde_json::{json, Value};

/// PIME 把这个 id 的按钮放到 Win8+ 托盘的输入法模式图标位置(`PIMELangBarButton.cpp`)。
pub const MODE_ICON: &str = "windows-mode-icon";
/// 左键单击模式图标时 onCommand 的 id。
pub const CMD_MODE_ICON: u64 = 1;
/// 菜单项 id(须 ≥1:id 为 0 且无文字的项被当成分隔线,选中返回 0 也不回调)。
pub const CMD_VERSION: u64 = 10;
pub const CMD_EDIT_USER: u64 = 11;
pub const CMD_RELOAD_USER: u64 = 12;
pub const CMD_OPEN_DIR: u64 = 13;
/// onCommand 的 type(libIME2 `TextService::CommandType`)。
pub const COMMAND_LEFT_CLICK: u64 = 0;
pub const COMMAND_MENU: u64 = 2;

/// 图标随安装包放在 `server.exe` 同目录的 `icons\`。
fn icon(english: bool) -> String {
    let name = if english { "eng_full_capsoff.ico" } else { "chi_full_capsoff.ico" };
    std::env::current_exe()
        .ok()
        .and_then(|p| Some(p.parent()?.join("icons").join(name)))
        .map(|p| p.display().to_string())
        .unwrap_or_default()
}

fn tooltip(english: bool) -> &'static str {
    if english { "PYIME 英文(单击 Shift 切换)" } else { "PYIME 中文(单击 Shift 切换)" }
}

/// `addButton` 用的模式图标按钮。
pub fn mode_button(english: bool) -> Value {
    json!({ "id": MODE_ICON, "icon": icon(english), "commandId": CMD_MODE_ICON, "tooltip": tooltip(english), "type": "button" })
}

/// `changeButton` 用:中英切换后换图标与提示。
pub fn mode_change(english: bool) -> Value {
    json!({ "id": MODE_ICON, "icon": icon(english), "tooltip": tooltip(english) })
}

/// 右键菜单;`user_changed` 为真(user.txt 被外部修改、尚未重载)时重载项带提示。
pub fn menu(user_changed: bool) -> Value {
    let reload = if user_changed { "重新加载用户词库(文件已修改)" } else { "重新加载用户词库" };
    json!([
        { "text": format!("PYIME {}", env!("CARGO_PKG_VERSION")), "id": CMD_VERSION, "enabled": false },
        {},
        { "text": "编辑用户词库…", "id": CMD_EDIT_USER },
        { "text": reload, "id": CMD_RELOAD_USER },
        {},
        { "text": "打开数据目录", "id": CMD_OPEN_DIR },
    ])
}
