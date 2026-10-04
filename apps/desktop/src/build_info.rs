//! Identity embedded in this executable, independent of its runtime directory.

pub(crate) const VERSION: &str = env!("CARGO_PKG_VERSION");
pub(crate) const NUMBER: &str = env!("LIBRE_EFFECTS_BUILD_NUMBER");
pub(crate) const BUILT_UTC: &str = env!("LIBRE_EFFECTS_BUILD_UTC");
pub(crate) const SOURCE: &str = env!("LIBRE_EFFECTS_SOURCE_DESCRIPTION");
pub(crate) const FINGERPRINT: &str = env!("LIBRE_EFFECTS_SOURCE_FINGERPRINT");
pub(crate) const TARGET: &str = concat!(
    env!("LIBRE_EFFECTS_BUILD_TARGET"),
    " / ",
    env!("LIBRE_EFFECTS_BUILD_PROFILE")
);

pub(crate) fn rows() -> [(&'static str, &'static str); 6] {
    [
        ("Version", VERSION),
        ("Build number", NUMBER),
        ("Built at", BUILT_UTC),
        ("Source", SOURCE),
        ("Source fingerprint (FNV-1a 64)", FINGERPRINT),
        ("Target / profile", TARGET),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn about_shows_complete_compile_time_identity() {
        let rows = rows();
        assert_eq!(rows[0], ("Version", env!("CARGO_PKG_VERSION")));
        assert!(
            rows.iter()
                .all(|(label, value)| !label.is_empty() && !value.is_empty())
        );
        assert_eq!(FINGERPRINT.len(), 16);
        assert!(FINGERPRINT.bytes().all(|byte| byte.is_ascii_hexdigit()));
        assert!(NUMBER.ends_with(FINGERPRINT));
        assert!(BUILT_UTC.ends_with(" UTC"));
        assert!(SOURCE.starts_with("Git ") || SOURCE == "Source archive / Git unavailable");
        assert!(TARGET.contains(" / "));
    }
}
