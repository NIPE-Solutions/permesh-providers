// SPDX-License-Identifier: MIT
use aws_credential_types::Credentials;
pub(crate) fn valid_credentials(c: &Credentials) -> bool {
    valid_values(c.access_key_id(), c.secret_access_key(), c.session_token())
}
pub(crate) fn valid_values(key: &str, secret: &str, token: Option<&str>) -> bool {
    let good =
        |s: &str| !s.is_empty() && s.len() <= 8192 && s.bytes().all(|b| b.is_ascii_graphic());
    (16..=128).contains(&key.len())
        && key.bytes().all(|b| b.is_ascii_alphanumeric())
        && good(secret)
        && token.is_none_or(good)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rejects_empty_control_whitespace_and_excessive_credentials() {
        for secret in ["", "space key", "line\nkey", "nonasciié"] {
            assert!(!valid_values("AKIATEST1234567890123", secret, None));
        }
        assert!(!valid_values("short", "secret", None));
        assert!(!valid_values("AKIATEST1234567890123", "secret", Some("")));
        assert!(!valid_values(
            "AKIATEST1234567890123",
            &"a".repeat(8193),
            None
        ));
        assert!(valid_values(
            "AKIATEST1234567890123",
            "secret",
            Some("session")
        ));
    }
}
