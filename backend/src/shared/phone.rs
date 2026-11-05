//! Indian mobile number normalisation.

/// Reduce a phone number to its 10-digit Indian mobile form.
///
/// Accepts spaces, dashes, a `+91` / `91` country prefix or a leading `0`.
pub fn normalize_phone(s: &str) -> Result<String, String> {
    let bad = || "Enter a valid 10-digit mobile number".to_string();
    if s.chars().any(char::is_alphabetic) {
        return Err(bad());
    }
    let digits: String = s.chars().filter(char::is_ascii_digit).collect();
    let local = match digits.len() {
        12 if digits.starts_with("91") => &digits[2..],
        11 if digits.starts_with('0') => &digits[1..],
        10 => &digits[..],
        _ => return Err(bad()),
    };
    if matches!(local.as_bytes()[0], b'6'..=b'9') {
        Ok(local.to_string())
    } else {
        Err(bad())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalizes_indian_mobiles() {
        for s in [
            "+91 98620-12345",
            "09862012345",
            "919862012345",
            "9862012345",
        ] {
            assert_eq!(normalize_phone(s).unwrap(), "9862012345", "{s}");
        }
    }

    #[test]
    fn rejects_bad_phones() {
        for s in ["98620123", "5862012345", "98620123456", "abc", ""] {
            assert!(normalize_phone(s).is_err(), "{s}");
        }
    }
}
