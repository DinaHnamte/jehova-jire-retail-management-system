use jj_retail_api::shared::code::{normalize_code, normalize_query};
use serde_json::Value;

#[test]
fn matches_shared_fixtures() {
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/../fixtures/codes.json");
    let cases: Vec<Value> = serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
    assert!(cases.len() >= 11);
    for c in cases {
        let raw = c["raw"].as_str().unwrap();
        let got = normalize_code(raw);
        match c["normalized"].as_str() {
            None => assert!(got.is_none(), "{raw:?} should be rejected, got {got:?}"),
            Some(n) => {
                let got = got.unwrap_or_else(|| panic!("{raw:?} was rejected"));
                assert_eq!(got.normalized, n, "normalized for {raw:?}");
                assert_eq!(got.base, c["base"].as_str().unwrap(), "base for {raw:?}");
                assert_eq!(
                    got.variant.as_deref(),
                    c["variant"].as_str(),
                    "variant for {raw:?}"
                );
                assert_eq!(
                    got.digits,
                    c["digits"].as_str().unwrap(),
                    "digits for {raw:?}"
                );
            }
        }
    }
}

#[test]
fn normalize_query_never_fails() {
    assert_eq!(
        normalize_query(" ip 1060 \u{2013} 11\u{2033} "),
        "IP1060-11"
    );
    assert_eq!(normalize_query("   "), "");
}
