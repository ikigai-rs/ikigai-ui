//! The demo's screens as outlines (ledger #3365): `contact` compiles to the form it aliased
//! before, and `write`, in short names, compiles inside the corridor the host injects.

use std::sync::Arc;

use futures::executor::block_on;
use ikigai_core::{Capability, Error, Iri, Request, Scope, Verb};
use ikigai_ui::model::{self, Node, Value};

fn read(kernel: &ikigai_core::Kernel, iri: &str, scope: Scope) -> Result<Node, Error> {
    let request = Request::new(Verb::Source, Iri::parse(iri).unwrap());
    let rep = block_on(kernel.issue_in(request, &Capability::root(), scope))?;
    Ok(model::parse(&rep.bytes).unwrap())
}

#[test]
fn the_contact_outline_compiles_to_the_generated_form() {
    let kernel = ikigai_ui_demo::kernel(ikigai_ui_demo::root(Arc::default()));
    let form = format!("{}{}", ikigai_ui::FORM_RESOURCE, ikigai_ui_demo::CONTACT);
    let screen = read(&kernel, "urn:iki:ui:screen:name:contact", Scope::empty()).unwrap();
    assert_eq!(screen, read(&kernel, &form, Scope::empty()).unwrap());
}

#[test]
fn the_write_outline_compiles_in_short_names_inside_the_corridor() {
    let kernel = ikigai_ui_demo::kernel(ikigai_ui_demo::root(Arc::default()));
    let corridor = block_on(ikigai_ui_demo::prefixes(&kernel)).unwrap();
    let screen = read(&kernel, "urn:iki:ui:screen:name:write", corridor).unwrap();
    assert_eq!(screen.kind, "group");
    assert_eq!(screen.children[0].kind, "heading");
    let form = &screen.children[1];
    assert_eq!(form.kind, "form");
    assert_eq!(
        form.properties["description"],
        Value::Text("We answer within a day".into())
    );
    assert_eq!(
        form.properties["resource"],
        Value::Text(ikigai_ui_demo::CONTACT.into())
    );
    assert_eq!(screen.children[2].kind, "urn:iki:ui:element:name:rating");
    // Outside the corridor the screen's table is not in force, and it says so at its line.
    let Err(Error::InvalidArgument { detail, .. }) =
        read(&kernel, "urn:iki:ui:screen:name:write", Scope::empty())
    else {
        panic!("compiled without its corridor");
    };
    assert!(
        detail.contains("line 3: the prefix table `urn:iki:uidemo:prefixes` is not in force"),
        "{detail}"
    );
}
