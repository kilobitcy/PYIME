//! 按键判定的纯逻辑,不依赖 Windows,Linux 上可单测。

/// 虚拟键码是否为字母 a–z(VK_A..=VK_Z 即 0x41..=0x5A)。
pub fn is_letter_vk(vk: u32) -> bool {
    (0x41..=0x5A).contains(&vk)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn 字母键范围() {
        assert!(is_letter_vk(0x41));
        assert!(is_letter_vk(0x5A));
        assert!(is_letter_vk(0x4D));
    }

    #[test]
    fn 非字母键() {
        assert!(!is_letter_vk(0x40));
        assert!(!is_letter_vk(0x5B));
        assert!(!is_letter_vk(0x30)); // 数字 0
        assert!(!is_letter_vk(0x20)); // 空格
    }
}
