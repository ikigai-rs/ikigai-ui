//! The html target (ledger #3355): every kind through its compose template, header cells in a
//! table, values escaped, and a generated form that submits to its action with htmx.

mod common;

use std::sync::Arc;

use ikigai_core::{
    AsyncFnEndpoint, EndpointSpace, Exact, Invocation, InvokeFuture, Iri, Request, Verb,
};
use ikigai_ui::testkit::render;

#[test]
fn gallery_renders_every_kind_through_templates() {
    let kernel = common::host(None);
    let html = render::source(&kernel, "urn:iki:ui:html:screen:gallery", &[]).unwrap();
    for needle in [
        "<h2>Text</h2>",
        "<form id=\"ui-n.1.1\"",
        "<label for=\"ui-n.1.2\">Email</label>",
        "<select id=\"ui-n.1.3\" name=\"size\"",
        "<option value=\"s\">Small</option>",
        "<a href=\"/help\">Help</a>",
        "<nav aria-label=\"Site\"><ul><li>",
        "<ul class=\"ui-list\" aria-label=\"Steps\"><li>",
        "<img src=\"/img/lighthouse.png\" alt=\"A lighthouse at dusk\">",
        "<figure class=\"ui-visual\">",
        // A refused field's message is on the field, and the field says it is invalid.
        "aria-invalid=\"true\" aria-describedby=\"ui-n.1.2-description ui-n.1.2-error\"",
        "<p class=\"ui-note\" id=\"ui-n.1.2-error\">An email address has an @ in it.</p>",
        // Model text is text.
        "&lt;b&gt; stays text.",
    ] {
        assert!(html.contains(needle), "missing {needle} in {html}");
    }
}

#[test]
fn a_table_has_header_cells() {
    let kernel = common::host(None);
    let html = render::source(&kernel, "urn:iki:ui:html:screen:gallery", &[]).unwrap();
    assert!(html.contains(
        "<caption>Totals</caption><tbody><tr><th scope=\"col\"><p>Fruit</p></th>\
         <th scope=\"col\"><p>Count</p></th></tr><tr><th scope=\"row\"><p>Apples</p></th>\
         <td><p>3</p></td></tr>"
    ));
}

#[test]
fn every_kind_has_a_template() {
    let kernel = common::host(None);
    for kind in ikigai_ui::model::KINDS {
        let template = render::source(
            &kernel,
            &format!("urn:iki:ui:template:kind:{}", kind.name),
            &[],
        )
        .unwrap();
        assert!(template.starts_with('<'), "{}: {template}", kind.name);
        // Values splice escaped; only composed child views splice raw.
        for (i, _) in template.match_indices("$r{") {
            assert!(
                template[i..].starts_with("$r{urn:iki:ui:html:screen:{screen}:node:{node}:child}")
            );
        }
        assert!(!template.contains("$a{"), "{}: no expansion", kind.name);
    }
}

#[test]
fn a_generated_form_renders_and_posts_to_its_action() {
    // A screen that is the form generated from the contact contract.
    let screen = AsyncFnEndpoint::new("test-screen", |inv: &Invocation<'_>| -> InvokeFuture<'_> {
        Box::pin(async move {
            let iri =
                Iri::parse(format!("{}{}", ikigai_ui::FORM_RESOURCE, common::CONTACT)).unwrap();
            inv.issue(Request::new(Verb::Source, iri)).await
        })
    });
    let extra = EndpointSpace::new().bind(Exact::new("urn:iki:ui:screen:name:contact"), screen);
    let kernel = common::kernel(common::root(
        None,
        vec![Arc::new(extra), Arc::new(common::form_models())],
    ));
    let html = render::source(&kernel, "urn:iki:ui:html:screen:contact", &[]).unwrap();
    assert!(
        html.starts_with("<form id=\"ui-n\" class=\"ui-form\" method=\"post\""),
        "{html}"
    );
    assert!(html.contains("hx-post=\""), "{html}");
    assert!(html.contains("<input id=\"ui-n.0\""), "{html}");
    assert!(html.contains("<button type=\"submit\""), "{html}");
}

#[test]
fn the_shell_lands_a_refused_form_on_its_field() {
    let js =
        std::str::from_utf8(ikigai_ui::artifact("urn:iki:ui:shell.js").unwrap().bytes).unwrap();
    assert!(js.contains("function fieldOf(form, text)"));
    assert!(js.contains("noteAt(named.field, elt, named.text, code)"));
    assert!(js.contains("focusOn(named.field)"));
}
