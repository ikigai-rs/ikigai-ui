//! D1, D8, D13: a page is the shell over the host's fragment. Snapshots hold the bytes, and
//! the rest holds the rule that makes `$r` safe here: which fragment a page splices is the
//! host's decision, never the request's.

mod common;

use std::sync::Arc;

use common::*;
use ikigai_core::Error;
use ikigai_ui::testkit::snapshot::{check, Outcome};

#[test]
fn the_pages_match_their_snapshots() {
    let kernel = host(None);
    let mut rendered: Vec<(String, String)> = ikigai_ui::THEMES
        .iter()
        .map(|theme| {
            (
                format!("home-{theme}.html"),
                text(&kernel, "urn:demo:page:home", &[("theme", theme)]),
            )
        })
        .collect();
    rendered.push((
        "about.html".into(),
        text(&kernel, "urn:demo:page:about", &[]),
    ));
    rendered.push((
        "tokens.css".into(),
        text(&kernel, ikigai_ui::TOKENS_CSS, &[]),
    ));
    // Every snapshot is checked (and a missing one written) before any failure is reported.
    let outcomes: Vec<_> = rendered
        .iter()
        .map(|(name, page)| (name, check(&snapshots(), name, page).unwrap()))
        .collect();
    let bad: Vec<String> = outcomes
        .iter()
        .filter(|(_, o)| *o != Outcome::Same)
        .map(|(name, o)| format!("{name}: {o:?}"))
        .collect();
    assert!(
        bad.is_empty(),
        "{}\n(a written snapshot is new: review it and commit it; to accept a change, delete \
         the file and run again)",
        bad.join("\n")
    );
}

#[test]
fn the_shell_has_its_landmarks_and_one_status_region() {
    let page = text(&host(None), "urn:demo:page:about", &[]);
    for part in [
        "<a class=\"ui-skip\" href=\"#main\">Skip to content</a>",
        "<header>\n",
        "<div id=\"status\" role=\"status\" aria-live=\"polite\"></div>",
        "<main id=\"main\" tabindex=\"-1\">\n<h1>About</h1>",
        "<link rel=\"stylesheet\" href=\"/ui/tokens.css\">",
        "<script src=\"/ui/shell.js\"></script>",
    ] {
        assert!(page.contains(part), "`{part}` in\n{page}");
    }
    assert_eq!(page.matches("role=\"status\"").count(), 1);
    assert_eq!(page.matches("<main").count(), 1);
    // The shell script is not deferred, and comes before htmx: the theme is applied before
    // the first paint, and its handlers are in place before htmx runs.
    assert!(page.find("/ui/shell.js").unwrap() < page.find("/ui/htmx").unwrap());
    assert!(!page.contains("style="), "no inline style (D6)");
    assert_eq!(
        page.matches("<script").count(),
        2,
        "only the two scripts, both by URL"
    );
}

#[test]
fn a_title_is_text_and_an_unknown_theme_is_auto() {
    let kernel = host(None);
    let page = text(&kernel, "urn:demo:page:home", &[("theme", "\"><script>")]);
    assert!(page.contains("<html lang=\"en\" data-theme=\"auto\">"));
    assert!(page.contains("<title>Home &amp; &lt;welcome&gt;</title>"));
}

#[test]
fn a_request_cannot_choose_what_a_page_splices() {
    let kernel = host(None);
    let plain = text(&kernel, "urn:demo:page:about", &[]);
    // The shell's variables are the page's: request arguments of the same names lose.
    let tried = text(
        &kernel,
        "urn:demo:page:about",
        &[
            ("family", "evil"),
            ("name", "home"),
            ("main", "urn:ui:shell.js"),
        ],
    );
    assert_eq!(plain, tried);
    // And a page the host does not have is not found, loudly, rather than an empty shell.
    match source(&kernel, "urn:demo:page:nowhere") {
        Err(Error::NotFound(_)) => {}
        other => panic!("a missing fragment fails the page, got {other:?}"),
    }
}

#[test]
fn two_families_share_one_kernel() {
    // A second family, whose host has bound none of its names yet.
    let kernel = host_with(
        None,
        vec![Arc::new(ikigai_ui::pages("other", "urn:other:").unwrap())],
    );
    // The demo family is untouched by the second one…
    assert!(text(&kernel, "urn:demo:page:about", &[]).contains("<h1>About</h1>"));
    // …and the other family's page reaches only the other family's names, none of them bound.
    match source(&kernel, "urn:other:page:about") {
        Err(Error::Unresolved(t)) => assert!(t.as_str().starts_with("urn:other:"), "{t}"),
        other => panic!("the other family's names are unbound, got {other:?}"),
    }
}

#[test]
fn a_page_name_reaches_the_fragment_as_one_segment() {
    // Pinned on purpose: compose percent-encodes a template argument in an IRI as one segment,
    // so `a:b` arrives as `a%3Ab` (see `ikigai_ui::pages`).
    let kernel = host(None);
    match source(&kernel, "urn:demo:page:a:b") {
        Err(Error::NotFound(why)) => assert!(why.contains("a%3Ab"), "{why}"),
        other => panic!("{other:?}"),
    }
}

#[test]
fn a_family_and_a_base_are_checked() {
    assert!(ikigai_ui::pages("Demo", "urn:demo:").is_err());
    assert!(ikigai_ui::pages("", "urn:demo:").is_err());
    assert!(ikigai_ui::pages("a:b", "urn:demo:").is_err());
    assert!(ikigai_ui::pages("demo", "urn:demo").is_err());
    assert!(ikigai_ui::pages("demo-2", "urn:demo:").is_ok());
}
