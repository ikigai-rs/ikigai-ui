//! D17: two deliveries, the same names. A directory bound at `urn:iki:ui:dir:` overrides an
//! artifact per name, falls back to the compiled-in bytes only on the directory atom's own
//! absence, and is golden-threaded: a write to the file recomputes everything over it.

mod common;

use std::sync::Arc;

use common::*;
use ikigai_core::{Description, EndpointSpace, Error, FnEndpoint, Invocation, UriTemplate, Verb};

/// Every name a page is built from, plus a page in each theme.
fn everything(kernel: &ikigai_core::Kernel) -> Vec<(String, Vec<u8>, String)> {
    let mut names: Vec<(String, Vec<(&str, &str)>)> = ikigai_ui::ARTIFACTS
        .iter()
        .map(|a| (a.iri.to_string(), Vec::new()))
        .collect();
    names.push((ikigai_ui::TOKENS_CSS.to_string(), Vec::new()));
    for theme in ikigai_ui::THEMES {
        names.push(("urn:demo:page:home".to_string(), vec![("theme", theme)]));
    }
    names.push(("urn:demo:page:about".to_string(), Vec::new()));
    names
        .into_iter()
        .map(|(iri, args)| {
            let args: Vec<(&str, &[u8])> = args.iter().map(|(k, v)| (*k, v.as_bytes())).collect();
            let rep =
                issue(kernel, Verb::Source, &iri, &args).unwrap_or_else(|e| panic!("{iri}: {e}"));
            (iri, rep.bytes, rep.repr_type.media_type)
        })
        .collect()
}

#[test]
fn both_deliveries_render_the_same_bytes() {
    let dir = mirror("same");
    let compiled = everything(&host(None));
    let directory = everything(&host(Some(&dir)));
    assert_eq!(compiled.len(), directory.len());
    for ((iri, a, ta), (_, b, tb)) in compiled.iter().zip(&directory) {
        assert_eq!(ta, tb, "{iri}: the same media type from both deliveries");
        assert!(
            a == b,
            "{iri}: the directory delivery differs from the compiled-in one"
        );
    }
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn an_empty_directory_is_the_compiled_in_set() {
    let dir = scratch("empty");
    assert_eq!(everything(&host(Some(&dir))), everything(&host(None)));
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn a_directory_overrides_one_artifact_by_name() {
    let dir = scratch("one");
    std::fs::write(dir.join("shell.js"), "// the host's own shell\n").unwrap();
    let kernel = host(Some(&dir));
    let js = source(&kernel, "urn:iki:ui:shell.js").unwrap();
    assert_eq!(js.bytes, b"// the host's own shell\n");
    assert_eq!(
        js.repr_type.media_type, "text/javascript",
        "the name fixes the type"
    );
    // Every other artifact is still compiled in.
    let htmx = source(&kernel, "urn:iki:ui:htmx").unwrap();
    assert_eq!(
        htmx.bytes,
        ikigai_ui::artifact("urn:iki:ui:htmx").unwrap().bytes
    );
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn an_overridden_template_reaches_every_page_over_it() {
    let dir = scratch("template");
    let shell = String::from_utf8(
        ikigai_ui::artifact(ikigai_ui::SHELL)
            .unwrap()
            .bytes
            .to_vec(),
    )
    .unwrap()
    .replace("<body>", "<body class=\"host\">");
    std::fs::write(dir.join("shell.html"), shell).unwrap();
    let page = text(&host(Some(&dir)), "urn:demo:page:about", &[]);
    assert!(page.contains("<body class=\"host\">"), "{page}");
    std::fs::remove_dir_all(&dir).unwrap();
}

/// The golden thread, end to end: the compiled-in answer is cached under the directory
/// atom's thread, a Sink to the file through the kernel cuts it, and a Delete cuts it back.
#[test]
fn a_write_to_the_directory_recomputes_the_artifact_and_the_stylesheet() {
    let dir = scratch("thread");
    let kernel = host(Some(&dir));
    let compiled = ikigai_ui::artifact(ikigai_ui::TOKENS_TEMPLATE)
        .unwrap()
        .bytes;
    let before = text(&kernel, ikigai_ui::TOKENS_CSS, &[]);
    assert!(before.contains("/* The shell's own rules"));
    assert_eq!(
        source(&kernel, ikigai_ui::TOKENS_TEMPLATE).unwrap().bytes,
        compiled
    );

    let mine = ":root { --bg: $h{urn:iki:ui:palette:dark} }\n";
    issue(
        &kernel,
        Verb::Sink,
        "urn:iki:ui:dir:tokens.template.css",
        &[("content", mine.as_bytes())],
    )
    .unwrap();
    assert_eq!(
        source(&kernel, ikigai_ui::TOKENS_TEMPLATE).unwrap().bytes,
        mine.as_bytes(),
        "the Sink cut the thread the compiled-in answer hung from"
    );
    let after = text(&kernel, ikigai_ui::TOKENS_CSS, &[]);
    assert!(
        after.starts_with(":root { --bg:   --bg: #0d1117;"),
        "{after}"
    );

    issue(
        &kernel,
        Verb::Delete,
        "urn:iki:ui:dir:tokens.template.css",
        &[],
    )
    .unwrap();
    assert_eq!(text(&kernel, ikigai_ui::TOKENS_CSS, &[]), before);
    std::fs::remove_dir_all(&dir).unwrap();
}

/// Fall back on the directory atom's own absence ONLY: a refusal is the answer.
#[test]
fn a_refusal_from_the_directory_is_not_a_fallback() {
    let refusing = FnEndpoint::new("refusing-dir", |_: &Invocation<'_>| {
        Err(Error::Denied("needs urn:cap:example:ui-dir".to_string()))
    })
    .with_description(
        Description::new("refusing-dir")
            .title("A directory that refuses")
            .summary("Stands in for a directory the caller may not read.")
            .verb(Verb::Source)
            .verb(Verb::Meta),
    );
    let dir = EndpointSpace::new().bind(
        UriTemplate::parse("urn:iki:ui:dir:{path}").unwrap(),
        refusing,
    );
    let kernel = host_with(None, vec![Arc::new(dir)]);
    match source(&kernel, "urn:iki:ui:shell.js") {
        Err(Error::Denied(why)) => assert!(why.contains("urn:cap:example:ui-dir"), "{why}"),
        other => panic!("a denial passes through, got {other:?}"),
    }
}

/// Sealed: a directory replaces the named artifacts and adds none.
#[test]
fn a_directory_cannot_add_a_name() {
    let dir = mirror("sealed");
    std::fs::write(dir.join("evil.js"), "alert(1)").unwrap();
    std::fs::write(dir.join("csp"), "default-src *").unwrap();
    let kernel = host(Some(&dir));
    for iri in ["urn:iki:ui:evil.js", "urn:iki:ui:csp", "urn:iki:ui:headers"] {
        assert!(
            matches!(source(&kernel, iri), Err(Error::Unresolved(_))),
            "{iri} is not a UI name"
        );
    }
    std::fs::remove_dir_all(&dir).unwrap();
}

/// A host's own artifacts get the same two deliveries (`ikigai_ui::artifacts`): compiled in,
/// a file under the host's directory prefix wins, and a write to it recomputes the answer.
#[test]
fn a_host_s_own_artifacts_have_both_deliveries() {
    const MINE: [ikigai_ui::Artifact; 2] = [
        ikigai_ui::Artifact {
            iri: "urn:app:app.css",
            id: "app-css",
            file: "app.css",
            media_type: "text/css",
            bytes: b"main { margin: 0; }\n",
        },
        ikigai_ui::Artifact {
            iri: "urn:app:app.js",
            id: "app-js",
            file: "app.js",
            media_type: "text/javascript",
            bytes: b"// compiled in\n",
        },
    ];
    let dir = scratch("host-artifacts");
    let kernel = ikigai_core::Kernel::new(Arc::new(ikigai_core::Fallback::new(vec![
        Arc::new(ikigai_ui::artifacts(&MINE, "urn:app:dir:").unwrap())
            as Arc<dyn ikigai_core::Space>,
        Arc::new(EndpointSpace::new().bind(
            UriTemplate::parse("urn:app:dir:{path}").unwrap(),
            ikigai_fs::FileEndpoint::new(dir.clone()).cacheable(),
        )),
    ])));
    assert_eq!(
        source(&kernel, "urn:app:app.css").unwrap().bytes,
        MINE[0].bytes
    );
    issue(
        &kernel,
        Verb::Sink,
        "urn:app:dir:app.css",
        &[("content", b"main { margin: 1rem; }\n")],
    )
    .unwrap();
    let css = source(&kernel, "urn:app:app.css").unwrap();
    assert_eq!(
        css.bytes, b"main { margin: 1rem; }\n",
        "the Sink cut the thread"
    );
    assert_eq!(
        css.repr_type.media_type, "text/css",
        "the name fixes the type"
    );
    assert_eq!(
        source(&kernel, "urn:app:app.js").unwrap().bytes,
        MINE[1].bytes
    );
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn a_host_s_artifact_list_is_checked() {
    let one = ikigai_ui::ARTIFACTS[0];
    assert!(ikigai_ui::artifacts(&[], "urn:app:dir:").is_err(), "empty");
    assert!(
        ikigai_ui::artifacts(&[one, one], "urn:app:dir:").is_err(),
        "a duplicate IRI"
    );
    let mut bad = one;
    bad.iri = "not an iri";
    assert!(
        ikigai_ui::artifacts(&[bad], "urn:app:dir:").is_err(),
        "a bad IRI"
    );
    assert!(
        ikigai_ui::artifacts(&[one], "").is_err(),
        "no scheme in the override"
    );
}
