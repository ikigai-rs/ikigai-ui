mod common;

use std::sync::Arc;

use futures::executor::block_on;
use ikigai_core::{
    ArgRef, Capability, ContentId, Error, Fallback, Iri, Kernel, Representation, Request, Verb,
};
use ikigai_ui::{model, screen, sexpr};

fn body() -> ArgRef {
    ArgRef::Content(ContentId::of(b"unreadable body"))
}

fn inline(value: &str) -> ArgRef {
    ArgRef::Inline(value.as_bytes().to_vec())
}

fn read(kernel: &Kernel, iri: &str, args: &[(&str, ArgRef)]) -> Result<Representation, Error> {
    let mut request = Request::new(Verb::Source, Iri::parse(iri).unwrap());
    for (name, value) in args {
        request = request.with_arg(*name, value.clone());
    }
    block_on(kernel.issue(request, &Capability::root()))
}

fn refused(iri: &str, result: Result<Representation, Error>, arg: &str, reason: &str) {
    match result {
        Err(Error::InvalidArgument { name, detail }) => {
            assert_eq!(name, arg, "{iri}: {detail}");
            assert!(detail.contains(reason), "{iri}: {detail}");
        }
        other => panic!(
            "{iri}: expected refusal for {arg}, got {:?}",
            other.map(|r| r.repr_type)
        ),
    }
}

fn assert_face(kernel: &Kernel, iri: &str, default: &str, selected: &str) {
    assert_eq!(
        read(kernel, iri, &[])
            .unwrap_or_else(|error| panic!("{iri}: {error}"))
            .repr_type
            .media_type,
        default,
        "{iri}"
    );
    assert_eq!(
        read(kernel, iri, &[("as", inline(selected))])
            .unwrap_or_else(|error| panic!("{iri}: {error}"))
            .repr_type
            .media_type,
        selected,
        "{iri}"
    );
    for (value, reason) in [(ArgRef::Inline(vec![0xff]), "UTF-8"), (body(), "inline")] {
        refused(iri, read(kernel, iri, &[("as", value)]), "as", reason);
    }
}

#[test]
fn face_selectors_refuse_unreadable_values() {
    let kernel = Kernel::new(Arc::new(ikigai_ui::space()));
    for (iri, default, selected) in [
        (
            "urn:iki:ui:palette:light",
            "text/plain; charset=utf-8",
            "text/turtle",
        ),
        (
            "urn:iki:ui:support",
            "application/json",
            "text/html; charset=utf-8",
        ),
        (
            "urn:iki:ui:criterion:kind:text:target:html",
            "text/turtle",
            "text/plain",
        ),
        (
            "urn:iki:ui:criterion:element:meter:target:gpui",
            "text/turtle",
            "text/plain",
        ),
        (ikigai_ui::GALLERY, model::MEDIA_TYPE, sexpr::MEDIA_TYPE),
    ] {
        assert_face(&kernel, iri, default, selected);
        let reference = ArgRef::Reference(Iri::parse("urn:test:unused").unwrap());
        refused(
            iri,
            read(&kernel, iri, &[("as", reference)]),
            "as",
            "inline",
        );
    }
}

#[test]
fn model_faces_refuse_unreadable_selectors() {
    let kernel = common::host_with(None, vec![Arc::new(common::form_models())]);
    for iri in [
        "urn:iki:ui:example:kind:nav",
        "urn:iki:ui:example:structure:region",
        "urn:iki:ui:form:resource:http://example.org/contact",
        "urn:iki:ui:form:openapi:urn:demo:openapi:operation:contact",
        "urn:iki:ui:face:target:gpui:screen:gallery",
        "urn:iki:ui:face:screen:gallery",
        ikigai_ui::element::SCREEN,
        "urn:iki:ui:screen:name:lens:contact-list:resource:urn:iki:ui:example:graph:contact",
        "urn:iki:ui:view:lens:demo-board:resource:urn:iki:ui:example:graph:lenses",
        "urn:iki:ui:view:resource:http://example.org/contact",
        "urn:iki:ui:view:generated:http://example.org/contact",
    ] {
        assert_face(&kernel, iri, model::MEDIA_TYPE, model::JSON_LD);
    }
}

#[test]
fn compilers_refuse_unreadable_screen_names_and_open_chains() {
    let kernel = Kernel::new(Arc::new(Fallback::new(vec![
        Arc::new(ikigai_ui::space()),
        Arc::new(ikigai_lisp::space()),
    ])));
    for (iri, content) in [
        (screen::COMPILER, "text content=Hello"),
        (sexpr::COMPILER, r#"(text :content "Hello")"#),
        (sexpr::PROGRAM, r#"'(text :content "Hello")"#),
    ] {
        let run = |args: &[(&str, ArgRef)]| {
            let mut all = vec![("content", inline(content))];
            all.extend_from_slice(args);
            read(&kernel, iri, &all)
        };
        let default = run(&[]).unwrap();
        let valid = run(&[
            ("screen", inline("urn:test:screen")),
            ("open", inline("urn:test:parent")),
        ])
        .unwrap();
        assert_eq!(default.bytes, valid.bytes, "{iri}");
        for arg in ["screen", "open"] {
            refused(iri, run(&[(arg, ArgRef::Inline(vec![0xff]))]), arg, "UTF-8");
            refused(iri, run(&[(arg, body())]), arg, "inline");
        }
    }
}

#[test]
fn screens_refuse_unreadable_open_chains() {
    let kernel = Kernel::new(Arc::new(ikigai_ui::space()));
    let iri = ikigai_ui::GALLERY;
    let default = read(&kernel, iri, &[]).unwrap();
    let valid = read(&kernel, iri, &[("open", inline("urn:test:parent"))]).unwrap();
    assert_eq!(default.bytes, valid.bytes);
    refused(
        iri,
        read(&kernel, iri, &[("open", ArgRef::Inline(vec![0xff]))]),
        "open",
        "UTF-8",
    );
    refused(
        iri,
        read(&kernel, iri, &[("open", body())]),
        "open",
        "inline",
    );
}
