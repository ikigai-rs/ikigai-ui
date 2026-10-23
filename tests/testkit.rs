//! The test kit tests itself: the vectors are ikigai-fn's file verbatim and pass against
//! `ikigai_fn::html`, snapshots write once and then hold, and the lints find what they say.

mod common;

use sha2::{Digest, Sha256};

use ikigai_ui::palette::{Kind, Palette, LIGHT};
use ikigai_ui::testkit::{contrast, lint, snapshot, vectors};

#[test]
fn the_vectors_are_ikigai_fns_file_verbatim() {
    let digest: String = Sha256::digest(vectors::HTML_VECTORS.as_bytes())
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect();
    assert_eq!(digest, vectors::HTML_VECTORS_SHA256);
    for kind in vectors::KINDS {
        assert!(!vectors::cases(kind).is_empty(), "{kind} has cases");
    }
}

#[test]
fn ikigai_fn_passes_every_vector() {
    let failures = vectors::check_rust();
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
fn a_snapshot_is_written_once_then_held() {
    let dir = common::scratch("snap");
    assert_eq!(
        snapshot::check(&dir, "a.html", "<p>one</p>\n").unwrap(),
        snapshot::Outcome::Written
    );
    assert_eq!(
        snapshot::check(&dir, "a.html", "<p>one</p>\n").unwrap(),
        snapshot::Outcome::Same
    );
    assert_eq!(
        snapshot::check(&dir, "a.html", "<p>two</p>\n").unwrap(),
        snapshot::Outcome::Differs {
            line: 1,
            want: "<p>one</p>".to_string(),
            got: "<p>two</p>".to_string()
        }
    );
    assert!(matches!(
        snapshot::check(&dir, "a.html", "<p>one</p>").unwrap(),
        snapshot::Outcome::Differs { line: 2, .. }
    ));
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn the_contrast_check_fails_a_palette_below_the_floor() {
    let mut weak: Palette = LIGHT;
    weak.name = "weak";
    weak.colors[3] = ("muted", "#aaaaaa"); // 2.32:1 on white
    let failing = contrast::failures(&[weak]);
    assert!(failing
        .iter()
        .any(|r| r.fg == "muted" && r.bg == "bg" && r.kind == Kind::Text));
    // A token a palette lacks is a failure, not a silence.
    weak.colors[0] = ("background", "#ffffff");
    assert!(contrast::failures(&[weak])
        .iter()
        .any(|r| r.bg == "bg" && r.ratio == 0.0));
}

#[test]
fn the_lints_find_hex_colors_and_undefined_vars() {
    let css = ".a{color:#c0ffee;background:var(--surface)} .b #bad {border:1px solid var(--nope)}";
    assert_eq!(lint::hex_colors(css), ["#c0ffee"]);
    assert_eq!(
        lint::undefined_vars(&[css]).into_iter().collect::<Vec<_>>(),
        ["nope"]
    );
    assert!(lint::undefined_vars(&[css, ":root{--nope: 1px}"]).is_empty());
}
