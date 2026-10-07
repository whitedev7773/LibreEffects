//! Pure reply interpretation, also tested without building a native UI binary.
pub(super) fn enabled(supported: u32, value: u32, flag: u32) -> bool {
    flag != 0 && supported & flag == flag && value & flag == flag
}
#[cfg(test)]
mod tests {
    use super::enabled;
    #[test]
    fn requires_supported_and_enabled_reply_bits() {
        assert!(enabled(1, 1, 1));
        assert!(!enabled(0, 0, 1));
        assert!(!enabled(1, 0, 1));
        assert!(!enabled(0, 1, 1));
    }
    #[test]
    fn unrelated_flags_neither_authorize_nor_block_capability() {
        assert!(!enabled(2, 2, 1));
        assert!(!enabled(3, 2, 1));
        assert!(!enabled(2, 3, 1));
        assert!(enabled(3, 3, 1));
        assert!(!enabled(0, 0, 0));
    }
}
