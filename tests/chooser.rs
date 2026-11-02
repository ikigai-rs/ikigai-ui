//! The shell's header chooser, `urn:iki:ui:chooser:{family}:{theme}`, for a page composed over
//! the shell directly, with no [`ikigai_ui::Pages`] wiring (ledger #2885): the shape a no-Rust
//! host's names file gives a page (`urn:iki:fn:compose src=<urn:iki:ui:template:shell>
//! family=… name=… theme=…`). Such a page names its header, head, title and main, and nothing
//! else; the chooser is ikigai-ui's, and a family turns it off by binding its override.

mod common;

use std::sync::Arc;

use common::*;
use ikigai_core::{
    Description, EndpointSpace, Exact, FnEndpoint, Invocation, Kernel, ReprType, Representation,
    Space, Verb,
};

fn fixed(id: &str, body: &'static str) -> FnEndpoint {
    FnEndpoint::new(id, move |_: &Invocation<'_>| {
        Ok(Representation::new(ReprType::new(HTML), body.as_bytes().to_vec()).cacheable())
    })
    .with_description(
        Description::new(id)
            .title(id)
            .verb(Verb::Source)
            .verb(Verb::Meta)
            .output(HTML),
    )
}

/// What a names file binds for the `quakes` family: the shell's own names, nothing more.
fn quakes_names() -> EndpointSpace {
    EndpointSpace::new()
        .bind(
            Exact::new("urn:iki:ui:header:quakes"),
            fixed(
                "quakes-header",
                "<nav aria-label=\"Site\"><a href=\"/quakes\">Quakes</a></nav>",
            ),
        )
        .bind(
            Exact::new("urn:iki:ui:head:quakes"),
            fixed("quakes-head", ""),
        )
        .bind(
            Exact::new("urn:iki:ui:title:quakes:home"),
            fixed("quakes-title", "Quakes"),
        )
        .bind(
            Exact::new("urn:iki:ui:main:quakes:home"),
            fixed("quakes-main", "<h1>Quakes</h1>"),
        )
}

/// A host the way the cli's door builds one: ikigai-ui's space FIRST, then the function
/// library, then the names (a names layer is placed last: it adds names, never changes one).
fn host_of(names: EndpointSpace) -> Kernel {
    Kernel::new(Arc::new(ikigai_core::Fallback::new(vec![
        Arc::new(ikigai_ui::space()) as Arc<dyn Space>,
        Arc::new(ikigai_fn::space()),
        Arc::new(names),
    ])))
}

fn composed(kernel: &Kernel, theme: &str) -> ikigai_core::Result<String> {
    ikigai_ui::testkit::render::source(
        kernel,
        "urn:iki:fn:compose",
        &[
            ("src", ikigai_ui::SHELL),
            ("family", "quakes"),
            ("name", "home"),
            ("theme", theme),
        ],
    )
}

fn header(page: &str) -> &str {
    &page[page.find("<header>").unwrap()..page.find("</header>").unwrap()]
}

#[test]
fn a_page_composed_over_the_shell_has_the_chooser_with_no_wiring() {
    let kernel = host_of(quakes_names());
    for theme in ikigai_ui::THEMES {
        let page = composed(&kernel, theme)
            .unwrap_or_else(|e| panic!("a names-file page over the shell, theme {theme}: {e}"));
        assert!(
            header(&page).contains(&ikigai_ui::theme_chooser(theme).unwrap()),
            "{page}"
        );
        assert_eq!(page.matches("data-ui-theme").count(), 1, "{page}");
    }
    // The chooser's name is there for ANY family, an alias of the theme chooser.
    assert_eq!(
        text(&kernel, "urn:iki:ui:chooser:anything:dark", &[]),
        ikigai_ui::theme_chooser("dark").unwrap()
    );
}

#[test]
fn a_names_file_turns_the_chooser_off_with_its_override() {
    // An override bound AFTER ikigai-ui's space (where a names layer sits) still wins.
    let names = quakes_names().bind(
        Exact::new(format!("{}quakes:auto", ikigai_ui::CHOOSER_OVERRIDE)),
        fixed("quakes-no-chooser", ""),
    );
    let kernel = host_of(names);
    let page = composed(&kernel, "auto").unwrap();
    assert!(!page.contains("data-ui-theme"), "{page}");
    assert!(
        header(&page)
            .ends_with("<nav aria-label=\"Site\"><a href=\"/quakes\">Quakes</a></nav>\n\n"),
        "{page}"
    );
    // Another family keeps the shell's chooser.
    assert_eq!(
        text(&kernel, "urn:iki:ui:chooser:other:auto", &[]),
        ikigai_ui::theme_chooser("auto").unwrap()
    );
}

#[test]
fn a_family_override_can_put_its_own_markup_there() {
    let names = quakes_names().bind(
        Exact::new(format!("{}quakes:dark", ikigai_ui::CHOOSER_OVERRIDE)),
        fixed("quakes-own-chooser", "<p class=\"mine\">Theme: dark</p>"),
    );
    let page = composed(&host_of(names), "dark").unwrap();
    assert!(
        header(&page).contains("<p class=\"mine\">Theme: dark</p>"),
        "{page}"
    );
    assert!(!page.contains("data-ui-theme"), "{page}");
}

#[test]
fn the_chooser_refuses_an_unknown_theme_or_family_name() {
    let kernel = host_of(quakes_names());
    for iri in [
        "urn:iki:ui:chooser:quakes:sepia",
        "urn:iki:ui:chooser:Quakes:dark",
    ] {
        match source(&kernel, iri) {
            Err(ikigai_core::Error::NotFound(_)) => {}
            other => panic!("{iri}: {other:?}"),
        }
    }
}

#[test]
fn a_pages_family_turned_off_stays_off_wherever_the_host_puts_it() {
    // Pages::theme_chooser(false) binds the override, so its place in the Fallback does not
    // matter: before or after ikigai-ui's space, the chooser is off for that family only.
    let off = || {
        ikigai_ui::Pages::new("off", "urn:demo:")
            .theme_chooser(false)
            .build()
            .unwrap()
    };
    for (name, spaces) in [
        (
            "space first",
            vec![
                Arc::new(ikigai_ui::space()) as Arc<dyn Space>,
                Arc::new(off()),
                Arc::new(demo_app()),
            ],
        ),
        (
            "family first",
            vec![
                Arc::new(off()) as Arc<dyn Space>,
                Arc::new(ikigai_ui::space()),
                Arc::new(demo_app()),
            ],
        ),
    ] {
        let kernel = Kernel::new(Arc::new(ikigai_core::Fallback::new(spaces)));
        let page = text(&kernel, "urn:demo:page:about", &[("theme", "dark")]);
        assert!(!page.contains("data-ui-theme"), "{name}: {page}");
        assert_eq!(
            text(&kernel, "urn:iki:ui:chooser:off:dark", &[]),
            "",
            "{name}"
        );
        assert_eq!(
            text(&kernel, "urn:iki:ui:chooser:demo:dark", &[]),
            ikigai_ui::theme_chooser("dark").unwrap(),
            "{name}"
        );
    }
}
