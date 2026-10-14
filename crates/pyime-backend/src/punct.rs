//! 中文状态下的标点映射:按键 → 全角标点;成对符号与引号带状态,由会话处理。
use serde_json::Value;

/// 一个标点键在中文状态下的含义。
#[derive(Debug, PartialEq, Clone, Copy)]
pub enum Mapped {
    /// 直接换成一个字符。
    Fixed(char),
    /// 数字后半角、否则全角(`.` `,` `:`)。
    Number { half: char, full: char },
    /// 弯引号:`true` 为双引号,开闭交替。
    Quote(bool),
    /// 成对左符号:输出一对并让光标回到中间;`close` 为右符号。
    Open { open: char, close: char },
    /// 成对右符号:光标前正好是自动补全的它时跳过,否则输出它。
    Close(char),
}

/// 按键对应的字符(优先用前端给的 `charCode`,没有再按美式键盘 + Shift 推算)。
/// 字母、数字等不是标点的键返回 `None`。
pub fn typed_char(req: &Value) -> Option<char> {
    let cc = req["charCode"].as_u64().unwrap_or(0);
    if (33..127).contains(&cc) {
        return char::from_u32(cc as u32);
    }
    let shift = req["keyStates"][16].as_u64().unwrap_or(0) & 0x80 != 0;
    let key = req["keyCode"].as_i64().unwrap_or(0);
    let pair = match key {
        188 => (',', '<'),
        190 => ('.', '>'),
        191 => ('/', '?'),
        186 => (';', ':'),
        222 => ('\'', '"'),
        219 => ('[', '{'),
        221 => (']', '}'),
        220 => ('\\', '|'),
        192 => ('`', '~'),
        189 => ('-', '_'),
        187 => ('=', '+'),
        48..=57 if shift => {
            let c = b")!@#$%^&*(".get((key - 48) as usize).copied()? as char;
            return Some(c);
        }
        _ => return None,
    };
    Some(if shift { pair.1 } else { pair.0 })
}

pub fn map(c: char) -> Option<Mapped> {
    Some(match c {
        '.' => Mapped::Number { half: '.', full: '\u{3002}' },
        ',' => Mapped::Number { half: ',', full: '\u{FF0C}' },
        ':' => Mapped::Number { half: ':', full: '\u{FF1A}' },
        '?' => Mapped::Fixed('\u{FF1F}'),
        '!' => Mapped::Fixed('\u{FF01}'),
        ';' => Mapped::Fixed('\u{FF1B}'),
        '`' => Mapped::Fixed('\u{00B7}'),
        '$' => Mapped::Fixed('\u{FFE5}'),
        '\\' => Mapped::Fixed('\u{3001}'),
        '"' => Mapped::Quote(true),
        '\'' => Mapped::Quote(false),
        '(' => Mapped::Open { open: '\u{FF08}', close: '\u{FF09}' },
        '<' => Mapped::Open { open: '\u{300A}', close: '\u{300B}' },
        '{' => Mapped::Open { open: '\u{3010}', close: '\u{3011}' },
        '[' => Mapped::Open { open: '\u{300C}', close: '\u{300D}' },
        ')' => Mapped::Close('\u{FF09}'),
        '>' => Mapped::Close('\u{300B}'),
        '}' => Mapped::Close('\u{3011}'),
        ']' => Mapped::Close('\u{300D}'),
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn 优先用_charcode_否则按美式键盘推算() {
        assert_eq!(typed_char(&json!({"charCode": 36, "keyCode": 52})), Some('$'));
        let mut ks = vec![0; 256];
        ks[16] = 0x80;
        assert_eq!(typed_char(&json!({"keyCode": 52, "keyStates": ks})), Some('$'));
        assert_eq!(typed_char(&json!({"keyCode": 190})), Some('.'));
        assert_eq!(typed_char(&json!({"keyCode": 65})), None);
    }

    #[test]
    fn 映射表() {
        assert_eq!(map('\\'), Some(Mapped::Fixed('\u{3001}')));
        assert_eq!(map('$'), Some(Mapped::Fixed('\u{FFE5}')));
        assert_eq!(map('/'), None);
        assert_eq!(map('<'), Some(Mapped::Open { open: '\u{300A}', close: '\u{300B}' }));
        assert_eq!(map('}'), Some(Mapped::Close('\u{3011}')));
    }
}
