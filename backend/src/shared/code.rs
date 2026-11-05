//! Product code normalisation (Architecture.md §6.3, ADR-0003).
//! Mirrored in `frontend/src/shared/code.ts`; both run `fixtures/codes.json`.
use unicode_normalization::UnicodeNormalization;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProductCode {
    /// `B6275-4.75`: the stored, compared form.
    pub normalized: String,
    /// `B6275`: shared by every size of a model.
    pub base: String,
    /// `4.75`: the size part, if any.
    pub variant: Option<String>,
    /// `6275-4.75`: without the letter prefix, for digit-prefix search.
    pub digits: String,
}

const DASHES: &[char] = &[
    '\u{2010}', '\u{2011}', '\u{2012}', '\u{2013}', '\u{2014}', '\u{2015}', '\u{2212}', '\u{FE63}',
    '\u{FF0D}',
];
/// Inch marks and look-alikes printed or typed after a size. NFKC turns `″` into `′′`.
const INCH_MARKS: &[char] = &[
    '"', '\'', '\u{2033}', '\u{2032}', '\u{201D}', '\u{2019}', '`',
];

/// Normalise free text the same way codes are: never fails, may return "".
pub fn normalize_query(q: &str) -> String {
    let s: String = q
        .nfkc()
        .map(|c| if DASHES.contains(&c) { '-' } else { c })
        .filter(|c| !c.is_whitespace())
        .collect::<String>()
        .to_uppercase();
    let s = s.trim_end_matches(INCH_MARKS).trim_matches('-');
    // `IP-1060-11` is `IP 1060-11` typed with a hyphen: drop hyphens between the letter
    // prefix and the first digit so it can't become a different code.
    let prefix_len = s.find(|c: char| !c.is_alphabetic()).unwrap_or(s.len());
    let (prefix, rest) = s.split_at(prefix_len);
    let rest_trimmed = rest.trim_start_matches('-');
    if !prefix.is_empty() && rest_trimmed.starts_with(|c: char| c.is_ascii_digit()) {
        format!("{prefix}{rest_trimmed}")
    } else {
        s.to_string()
    }
}

/// Longest design code accepted (`MP4952` is 6).
pub const MAX_DESIGN: usize = 20;

/// A design code that isn't letters and digits, or is too long.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct InvalidDesign;

impl std::fmt::Display for InvalidDesign {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "design code must be 1 to {MAX_DESIGN} letters and digits"
        )
    }
}

impl std::error::Error for InvalidDesign {}

/// Normalise a design/colour code such as `MP4952`. `Ok(None)` when blank;
/// an error unless it is letters and digits only, at most [`MAX_DESIGN`] long.
pub fn normalize_design(raw: &str) -> Result<Option<String>, InvalidDesign> {
    let s = normalize_query(raw);
    if s.is_empty() {
        return Ok(None);
    }
    if s.len() > MAX_DESIGN || !s.chars().all(|c| c.is_ascii_alphanumeric()) {
        return Err(InvalidDesign);
    }
    Ok(Some(s))
}

/// Normalise a printed code. `None` if nothing code-like is left (empty or no digit).
pub fn normalize_code(raw: &str) -> Option<ProductCode> {
    let normalized = normalize_query(raw);
    if normalized.is_empty() || !normalized.chars().any(|c| c.is_ascii_digit()) {
        return None;
    }
    let (base, variant) = match normalized.split_once('-') {
        Some((b, v)) if !v.is_empty() => (b.to_string(), Some(v.to_string())),
        Some((b, _)) => (b.to_string(), None),
        None => (normalized.clone(), None),
    };
    let digits = normalized
        .trim_start_matches(|c: char| c.is_alphabetic())
        .to_string();
    Some(ProductCode {
        normalized,
        base,
        variant,
        digits,
    })
}
