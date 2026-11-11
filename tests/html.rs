//! The html target (ledger #3355): every kind through its compose template, header cells in a
//! table, values escaped, and a generated form that submits to its action with htmx.

mod common;

use ikigai_ui::model::Value;
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
    // The contact contract at an http URL: the form's action is that URL.
    let kernel = common::form_host(common::CONTACT);
    let html = render::source(&kernel, "urn:iki:ui:html:screen:contact", &[]).unwrap();
    assert!(
        html.starts_with("<form id=\"ui-n\" class=\"ui-form\" method=\"post\""),
        "{html}"
    );
    assert_eq!(common::attribute(&html, "action"), common::CONTACT);
    assert_eq!(common::attribute(&html, "hx-post"), common::CONTACT);
    assert!(html.contains("<input id=\"ui-n.0\""), "{html}");
    assert!(html.contains("<button type=\"submit\""), "{html}");
}

#[test]
fn a_generated_choice_selects_the_contracts_default() {
    // The second option is the contract's default; the browser would choose the first
    // option if the face dropped the choice's value.
    let kernel = common::form_host(common::CONTACT);
    let html = render::source(&kernel, "urn:iki:ui:html:screen:contact", &[]).unwrap();
    assert!(
        html.contains("<option value=\"support\" selected>support</option>"),
        "{html}"
    );
    assert!(
        html.contains("<option value=\"sales\">sales</option>"),
        "{html}"
    );
    assert_eq!(html.matches(" selected").count(), 1, "{html}");
}

#[test]
fn a_form_from_a_urn_contract_posts_to_the_door_url_for_its_resource() {
    // No action: the resource reaches the door through the html descriptor's submit rule.
    let kernel = common::form_host(common::CONTACT_URN);
    let html = render::source(&kernel, "urn:iki:ui:html:screen:contact", &[]).unwrap();
    let door = "/resolution/iri/urn%3Ademo%3Acontact";
    assert!(html.starts_with("<form id=\"ui-n\""), "{html}");
    assert_eq!(common::attribute(&html, "method"), "post");
    assert_eq!(common::attribute(&html, "action"), door);
    assert_eq!(common::attribute(&html, "hx-post"), door);
}

#[test]
fn a_host_overrides_the_submit_rule_from_its_directory() {
    let dir = common::scratch("submit-rule");
    let rule = include_str!("../assets/target/html.json")
        .replace("/resolution/iri/{resource}", "/door/{+resource}");
    std::fs::create_dir_all(dir.join("target")).unwrap();
    std::fs::write(dir.join("target/html.json"), rule).unwrap();
    let kernel = common::form_host_in(Some(&dir), common::CONTACT_URN);
    let html = render::source(&kernel, "urn:iki:ui:html:screen:contact", &[]).unwrap();
    assert_eq!(common::attribute(&html, "action"), "/door/urn:demo:contact");
}

#[test]
fn the_html_face_refuses_a_form_verb_it_has_no_method_for() {
    for verb in ["new", "delete"] {
        let node = ikigai_ui::model::Node {
            kind: "form".into(),
            replacement: None,
            hint: Default::default(),
            properties: [
                ("label".to_string(), Value::Text("Make".into())),
                ("resource".to_string(), Value::Text("urn:demo:thing".into())),
                ("verb".to_string(), Value::Text(verb.into())),
            ]
            .into_iter()
            .collect(),
            children: Vec::new(),
        };
        let err = ikigai_ui::html::form_method(&node).unwrap_err();
        assert!(
            matches!(&err, ikigai_core::Error::InvalidArgument { name, detail } if name == "verb" && detail.contains(verb)),
            "{err}"
        );
    }
}

#[test]
fn the_shell_lands_a_refused_form_on_its_field() {
    let js =
        std::str::from_utf8(ikigai_ui::artifact("urn:iki:ui:shell.js").unwrap().bytes).unwrap();
    assert!(js.contains("function fieldOf(form, text)"));
    assert!(js.contains("noteAt(named.field, elt, named.text, code)"));
    assert!(js.contains("focusOn(named.field)"));
}
