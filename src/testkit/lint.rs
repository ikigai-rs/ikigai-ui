//! The token lints (UI conventions D12, D18): no hex color outside the palette tables, and
//! every `var(--x)` used is defined.
//!
//! Both read CSS text (a stylesheet, a compose template, the CSS inside an XSL), and the color
//! lint reads the shipped script and markup too. They are lexical, deliberately small, and say
//! what they found; they are not a CSS parser.

use std::collections::BTreeSet;

use crate::palette::{LAYOUT, TOKENS};

/// Every hex color in `css`, as written (`#fff`, `#0b5cad`, `#0b5cad80`), in order.
///
/// A hex color is `#` followed by 3, 4, 6 or 8 hex digits and then a character that cannot
/// continue a name, where the `#` follows `:`, whitespace or `,`, and where the next of `{`,
/// `;` and `}` after it is not `{` (so it is in a declaration's value, not a selector). So
/// `#status`, `.x #add {` and `url(#fade)` are not colors, and `color:#fff` and
/// `1px solid #000` are.
///
/// Comments are not read (see [`blank_comments`]), so a comment citing an item such as
/// `// ledger #2899` is not a color; a color in a JS string still is.
///
/// ```
/// use ikigai_ui::testkit::lint::hex_colors;
/// assert_eq!(hex_colors("a{color:#fff;border:1px solid #0b5cad}"), ["#fff", "#0b5cad"]);
/// assert!(hex_colors("#status:empty{display:none} .x #add {} a{mask:url(#fade)}").is_empty());
/// assert!(hex_colors("el.focus(); // ledger #2899; see above\n").is_empty());
/// assert_eq!(hex_colors("/* was #abc; */ a{color: #289}"), ["#289"]);
/// ```
pub fn hex_colors(css: &str) -> Vec<String> {
    let css = &blank_comments(css);
    let bytes = css.as_bytes();
    let mut found = Vec::new();
    for (i, _) in css.match_indices('#') {
        let before = if i == 0 { b' ' } else { bytes[i - 1] };
        if !(before == b':' || before == b',' || before.is_ascii_whitespace()) {
            continue;
        }
        let in_selector = css[i..]
            .find(['{', ';', '}'])
            .is_some_and(|at| bytes[i + at] == b'{');
        if in_selector {
            continue;
        }
        let digits = bytes[i + 1..]
            .iter()
            .take_while(|b| b.is_ascii_hexdigit())
            .count();
        let next = bytes.get(i + 1 + digits).copied().unwrap_or(b' ');
        let continues = next.is_ascii_alphanumeric() || next == b'-' || next == b'_';
        if matches!(digits, 3 | 4 | 6 | 8) && !continues {
            found.push(css[i..i + 1 + digits].to_string());
        }
    }
    found
}

/// `text` with every comment blanked to spaces (newlines kept, so offsets and line numbers
/// hold): CSS and JS block comments (`/* */`), HTML comments (`<!-- -->`), and JS line
/// comments (`//` to the end of the line).
///
/// Lexical, like the rest of this module, and it errs toward reading MORE text, so a doubtful
/// case leaves a color to be found rather than hiding one:
/// - `//` opens a line comment only at the start of the text or after whitespace, `;`, `{` or
///   `}`, so the `//` of `url(https://…)` and of a protocol-relative `url(//…)` is not one;
/// - a block or HTML comment with no close is left as text, so a stray `/*` in a JS string
///   cannot hide the rest of the file.
///
/// It does not know JS strings: a `/* … */` or ` // ` inside one is still taken as a comment.
///
/// ```
/// use ikigai_ui::testkit::lint::blank_comments;
/// assert_eq!(blank_comments("a; // #2899\nb"), format!("a; {}\nb", " ".repeat(8)));
/// assert_eq!(blank_comments("x/* #abc */y"), format!("x{}y", " ".repeat(10)));
/// assert_eq!(blank_comments("url(//x.test/a)"), "url(//x.test/a)");
/// assert_eq!(blank_comments("\"src/*.js\""), "\"src/*.js\"");
/// ```
pub fn blank_comments(text: &str) -> String {
    let bytes = text.as_bytes();
    let mut out = bytes.to_vec();
    let blank = |out: &mut [u8], from: usize, to: usize| {
        for b in &mut out[from..to] {
            if *b != b'\n' {
                *b = b' ';
            }
        }
    };
    let mut i = 0;
    while i < bytes.len() {
        let rest = &text[i..];
        let close = if let Some(body) = rest.strip_prefix("/*") {
            body.find("*/").map(|at| i + 2 + at + 2)
        } else if let Some(body) = rest.strip_prefix("<!--") {
            body.find("-->").map(|at| i + 4 + at + 3)
        } else if rest.starts_with("//")
            && (i == 0
                || matches!(bytes[i - 1], b';' | b'{' | b'}')
                || bytes[i - 1].is_ascii_whitespace())
        {
            Some(rest.find('\n').map_or(bytes.len(), |at| i + at))
        } else {
            None
        };
        match close {
            Some(end) => {
                blank(&mut out, i, end);
                i = end;
            }
            None => i += rest.chars().next().map_or(1, char::len_utf8),
        }
    }
    // Every blanked run starts and ends at an ASCII delimiter, so whole characters were replaced
    // by spaces and the bytes are still UTF-8.
    String::from_utf8(out).expect("a comment run is whole characters")
}

/// Every custom property `css` uses through `var(--x)`, without its `--`.
///
/// ```
/// use ikigai_ui::testkit::lint::used_vars;
/// let used = used_vars("a{color:var(--accent);margin:var( --gap , 1rem)}");
/// assert_eq!(used.into_iter().collect::<Vec<_>>(), ["accent", "gap"]);
/// ```
pub fn used_vars(css: &str) -> BTreeSet<String> {
    let mut used = BTreeSet::new();
    let mut rest = css;
    while let Some(at) = rest.find("var(") {
        let after = rest[at + 4..].trim_start();
        if let Some(name) = after.strip_prefix("--") {
            let end = name
                .find(|c: char| !(c.is_ascii_alphanumeric() || c == '-' || c == '_'))
                .unwrap_or(name.len());
            if end > 0 {
                used.insert(name[..end].to_string());
            }
        }
        rest = &rest[at + 4..];
    }
    used
}

/// Every custom property `css` defines (`--x:`), without its `--`.
///
/// ```
/// use ikigai_ui::testkit::lint::defined_vars;
/// let defined = defined_vars(":root{--bg: #fff; --card-gap:1rem}");
/// assert_eq!(defined.into_iter().collect::<Vec<_>>(), ["bg", "card-gap"]);
/// ```
pub fn defined_vars(css: &str) -> BTreeSet<String> {
    let mut defined = BTreeSet::new();
    for (at, _) in css.match_indices("--") {
        let before = css[..at].chars().next_back().unwrap_or(' ');
        if !(before == '{' || before == ';' || before.is_whitespace()) {
            continue;
        }
        let name = &css[at + 2..];
        let end = name
            .find(|c: char| !(c.is_ascii_alphanumeric() || c == '-' || c == '_'))
            .unwrap_or(name.len());
        if end > 0 && name[end..].trim_start().starts_with(':') {
            defined.insert(name[..end].to_string());
        }
    }
    defined
}

/// The custom properties the token stylesheet defines: every palette token and the layout
/// tokens.
pub fn token_names() -> BTreeSet<String> {
    TOKENS
        .iter()
        .copied()
        .chain(LAYOUT.iter().map(|(t, _)| *t))
        .map(str::to_string)
        .collect()
}

/// The custom properties `sheets` use and neither define nor find in [`token_names`].
///
/// ```
/// use ikigai_ui::testkit::lint::undefined_vars;
/// let missing = undefined_vars(&["a{color:var(--cms-mut)} b{color:var(--muted)}"]);
/// assert_eq!(missing.into_iter().collect::<Vec<_>>(), ["cms-mut"]);
/// ```
pub fn undefined_vars(sheets: &[&str]) -> BTreeSet<String> {
    let mut defined = token_names();
    let mut used = BTreeSet::new();
    for sheet in sheets {
        defined.extend(defined_vars(sheet));
        used.extend(used_vars(sheet));
    }
    used.difference(&defined).cloned().collect()
}
