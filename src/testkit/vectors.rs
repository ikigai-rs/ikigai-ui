//! FND-H's escaping and link-target vectors (ledger #1813).
//!
//! [`HTML_VECTORS`] is ikigai-fn's `tests/vectors/html_vectors.json`, copied VERBATIM and held
//! to [`HTML_VECTORS_SHA256`] (the file as ikigai-fn 0.5.0 ships it). A UI repo runs every case
//! against its own escaper (an XSL template's, a script's) with [`cases`]; [`check_rust`] runs
//! them against `ikigai_fn::html`, which is what this crate and every Rust UI calls.

use serde_json::Value;

/// The vectors file, verbatim.
pub const HTML_VECTORS: &str = include_str!("html_vectors.json");

/// The SHA-256 of [`HTML_VECTORS`]: ikigai-fn 0.5.0's `tests/vectors/html_vectors.json`.
pub const HTML_VECTORS_SHA256: &str =
    "e6b3f0aea44302f58ae98e5158ac8d69aef325e91aaa6d7e42b56e18d1a07f91";

/// The case lists the file holds, by key.
pub const KINDS: [&str; 4] = ["escape", "url_segment", "safe_href", "json_attr"];

/// The cases under `kind` (one of [`KINDS`]): each a JSON object with `in` and `out`.
///
/// ```
/// let cases = ikigai_ui::testkit::vectors::cases("escape");
/// assert!(cases.iter().any(|c| c["in"] == "<" && c["out"] == "&lt;"));
/// ```
pub fn cases(kind: &str) -> Vec<Value> {
    let doc: Value = serde_json::from_str(HTML_VECTORS).expect("the vectors file is JSON");
    doc[kind].as_array().cloned().unwrap_or_default()
}

/// Run every case against `ikigai_fn::html`, returning each case that disagrees.
pub fn check_rust() -> Vec<String> {
    use ikigai_fn::html::{attr, html_text, json_attr, safe_href, url_segment};
    let mut failures = Vec::new();
    let text = |v: &Value| v.as_str().unwrap_or_default().to_string();
    for case in cases("escape") {
        let (input, want) = (text(&case["in"]), text(&case["out"]));
        for (name, got) in [("html_text", html_text(&input)), ("attr", attr(&input))] {
            if got != want {
                failures.push(format!("{name}({input:?}) = {got:?}, want {want:?}"));
            }
        }
    }
    for case in cases("url_segment") {
        let (input, want) = (text(&case["in"]), text(&case["out"]));
        let got = url_segment(&input);
        if got != want {
            failures.push(format!("url_segment({input:?}) = {got:?}, want {want:?}"));
        }
    }
    for case in cases("safe_href") {
        let input = text(&case["in"]);
        let want = case["out"].as_str().map(str::to_string);
        let got = safe_href(&input).map(str::to_string);
        if got != want {
            failures.push(format!("safe_href({input:?}) = {got:?}, want {want:?}"));
        }
    }
    for case in cases("json_attr") {
        let pairs: Vec<(String, String)> = case["in"]
            .as_array()
            .map(|ps| ps.iter().map(|p| (text(&p[0]), text(&p[1]))).collect())
            .unwrap_or_default();
        let want = text(&case["out"]);
        let got = json_attr(pairs.iter().map(|(k, v)| (k.as_str(), v.as_str())));
        if got != want {
            failures.push(format!("json_attr({pairs:?}) = {got:?}, want {want:?}"));
        }
    }
    failures
}
