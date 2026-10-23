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
    for name in ["about", "live"] {
        rendered.push((
            format!("{name}.html"),
            text(&kernel, &format!("urn:demo:page:{name}"), &[]),
        ));
    }
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
            ("main", "urn:iki:ui:shell.js"),
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
fn a_page_name_reaches_the_fragment_whole() {
    // `{+name}` (ikigai-fn 0.5.1): `page:a:b` shows `fragment:a:b` and `title:a:b`, in the
    // host's own spelling, with no percent-encoding to undo (ledger #2140).
    let fixed = |id: &str, body: &'static str| {
        ikigai_core::FnEndpoint::new(id, move |_| {
            Ok(ikigai_core::Representation::new(
                ikigai_core::ReprType::new(HTML),
                body.as_bytes().to_vec(),
            )
            .cacheable())
        })
    };
    let item = ikigai_core::EndpointSpace::new()
        .bind(
            ikigai_core::Exact::new("urn:demo:title:a:b"),
            fixed("t", "A colon"),
        )
        .bind(
            ikigai_core::Exact::new("urn:demo:fragment:a:b"),
            fixed("f", "<h1>a:b</h1>"),
        );
    let kernel = host_with(None, vec![Arc::new(item)]);
    let page = text(&kernel, "urn:demo:page:a:b", &[]);
    assert!(page.contains("<title>A colon</title>"), "{page}");
    assert!(
        page.contains("<main id=\"main\" tabindex=\"-1\">\n<h1>a:b</h1>"),
        "{page}"
    );
}

#[test]
fn a_page_name_is_bounded_before_anything_is_read() {
    // The name is captured from the REQUESTED IRI and reaches the host's names under
    // `{+name}`, so anything that could leave `{base}fragment:{name}` is refused.
    let kernel = host(None);
    for name in [
        "a/b", "a#b", "a/../b", "a%2Fb", "a%3Ab", "a?b", "a@b", "a!b", "a$b", "a&b", "a=b", "a+b",
        "a,b",
    ] {
        let iri = format!("urn:demo:page:{name}");
        match source(&kernel, &iri) {
            Err(Error::InvalidArgument { name: arg, detail }) => {
                assert_eq!(arg, "name");
                assert!(detail.contains(name), "{detail}");
            }
            other => panic!("{iri}: {other:?}"),
        }
    }
    // The unreserved characters and `:` are a name.
    match source(&kernel, "urn:demo:page:Item_5.v~2-x:y") {
        Err(Error::NotFound(why)) => assert!(why.contains("Item_5.v~2-x:y"), "{why}"),
        other => panic!("{other:?}"),
    }
}

#[test]
fn a_family_s_head_holds_what_the_host_fixed_in_order() {
    let page = text(&host(None), "urn:demo:page:about", &[]);
    let head = &page[..page.find("</head>").unwrap()];
    let htmx = head.find("/ui/htmx").unwrap();
    let css = head
        .find("\n<link rel=\"stylesheet\" href=\"/demo/demo.css\">")
        .unwrap();
    let push = head
        .find("\n<meta name=\"ikigai-ui-push\" content=\"/_push\">")
        .unwrap();
    assert!(htmx < css && css < push, "{head}");
    // A request argument of the head's own name changes nothing.
    assert_eq!(
        page,
        text(
            &host(None),
            "urn:demo:page:about",
            &[("head", "<script>"), ("push", "/evil")]
        )
    );
}

#[test]
fn a_family_with_no_head_renders_the_plain_shell() {
    // `pages(…)` over the demo app's names, with no head additions.
    let kernel = ikigai_core::Kernel::new(Arc::new(ikigai_core::Fallback::new(vec![
        Arc::new(ikigai_ui::space()) as Arc<dyn ikigai_core::Space>,
        Arc::new(ikigai_ui::pages("plain", "urn:demo:").unwrap()),
        Arc::new(demo_app()),
    ])));
    let page = text(&kernel, "urn:demo:page:about", &[]);
    assert!(
        page.contains("defer></script>\n</head>"),
        "nothing between htmx and </head>: {page}"
    );
    assert!(!page.contains("ikigai-ui-push"));
    assert_eq!(text(&kernel, "urn:iki:ui:head:plain", &[]), "");
}

#[test]
fn a_head_url_is_checked_and_escaped() {
    let base = || ikigai_ui::Pages::new("app", "urn:app:");
    for bad in [
        "javascript:alert(1)",
        "//evil.example/x.css",
        "app.css",
        " /app.css",
        "data:text/css,x",
    ] {
        assert!(base().stylesheet(bad).build().is_err(), "stylesheet {bad}");
        assert!(base().script(bad).build().is_err(), "script {bad}");
        assert!(base().push(bad).build().is_err(), "push {bad}");
    }
    assert!(base()
        .stylesheet("https://cdn.example/a.css")
        .script("/a.js")
        .push("/_ikigai/push?x=1")
        .build()
        .is_ok());
    // A quote in a URL stays inside its attribute.
    let app = ikigai_core::EndpointSpace::new();
    let kernel = ikigai_core::Kernel::new(Arc::new(ikigai_core::Fallback::new(vec![
        Arc::new(base().script("/a\"b.js").build().unwrap()) as Arc<dyn ikigai_core::Space>,
        Arc::new(app),
    ])));
    let head = text(&kernel, "urn:iki:ui:head:app", &[]);
    assert_eq!(head, "\n<script src=\"/a&quot;b.js\" defer></script>");
}

#[test]
fn a_family_and_a_base_are_checked() {
    assert!(ikigai_ui::pages("Demo", "urn:demo:").is_err());
    assert!(ikigai_ui::pages("", "urn:demo:").is_err());
    assert!(ikigai_ui::pages("a:b", "urn:demo:").is_err());
    assert!(ikigai_ui::pages("demo", "urn:demo").is_err());
    assert!(ikigai_ui::pages("demo-2", "urn:demo:").is_ok());
}
