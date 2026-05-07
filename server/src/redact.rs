/// Mask the middle of a bearer token for safe logging. Keeps the first 4 and
/// last 4 characters so the user can verify which token they configured.
pub fn redact_bearer(token: &str) -> String {
    let chars: Vec<char> = token.chars().collect();
    if chars.len() <= 8 {
        return "****".to_string();
    }
    let head: String = chars.iter().take(4).collect();
    let tail: String = chars
        .iter()
        .rev()
        .take(4)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect();
    format!("{}***{}", head, tail)
}

/// Replace any `Bearer xxx` substring in a string with the redacted form.
pub fn redact_in(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(idx) = rest.find("Bearer ") {
        out.push_str(&rest[..idx]);
        out.push_str("Bearer ");
        let after = &rest[idx + 7..];
        let token_end = after
            .find(|c: char| c.is_whitespace() || c == '"' || c == '\'')
            .unwrap_or(after.len());
        let token = &after[..token_end];
        out.push_str(&redact_bearer(token));
        rest = &after[token_end..];
    }
    out.push_str(rest);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn redacts_normal_token() {
        let r = redact_bearer("abcd1234efgh5678ijklmnop");
        assert_eq!(r, "abcd***mnop");
    }

    #[test]
    fn short_token_fully_masked() {
        assert_eq!(redact_bearer("abc"), "****");
        assert_eq!(redact_bearer("abcdefgh"), "****");
    }

    #[test]
    fn redact_in_string_preserves_surrounding_text() {
        let s = "Authorization: Bearer abcd1234efgh5678 and stuff";
        let r = redact_in(s);
        assert_eq!(r, "Authorization: Bearer abcd***5678 and stuff");
    }

    #[test]
    fn redact_in_handles_quoted_token() {
        let s = r#"{"auth": "Bearer abcd1234efgh5678ijkl"}"#;
        let r = redact_in(s);
        assert!(r.contains("abcd***ijkl"));
        assert!(!r.contains("1234efgh5678"));
    }
}
