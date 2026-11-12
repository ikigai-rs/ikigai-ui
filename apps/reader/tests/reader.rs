//! The reader against a LOCAL layer host in this process (never the live edge): a stranger's
//! join from the app's own fresh certificate, a public feed read newest first, Markdown as text
//! kinds with raw HTML dropped, and the screens composed and drawn by the HTML face.
#![cfg(not(target_arch = "wasm32"))]

use std::path::Path;
use std::sync::Arc;

use futures::executor::block_on;
use ikigai_core::{Fallback, Iri, Kernel, Request, Verb};
use ikigai_reader::{Descriptor, Identity};

mod common;
use common::{host, publish, reader, Scratch, DESCRIPTOR};

#[test]
fn the_descriptor_lists_public_feeds_and_refuses_a_bad_name() {
    let d = Descriptor::parse(DESCRIPTOR.as_bytes()).unwrap();
    assert_eq!(d.join, "urn:layer:join:say");
    assert_eq!(d.feeds[0].name, "news");
    assert_eq!(d.feeds[0].label, "News");
    let bad = r#"{"join":"urn:layer:join:say","public_layers":[{"name":"a:b"}]}"#;
    assert!(Descriptor::parse(bad.as_bytes()).is_err());
}

#[cfg(unix)]
#[test]
fn the_identity_is_made_fresh_and_kept_owner_only() {
    use std::os::unix::fs::PermissionsExt;
    let home = Scratch::new("home-perm");
    let id = Identity::load_or_create(&home.0.join("reader")).unwrap();
    let mode = |p: &Path| std::fs::metadata(p).unwrap().permissions().mode() & 0o777;
    assert_eq!(mode(&home.0.join("reader")), 0o700);
    assert_eq!(mode(&id.key_path), 0o600);
    assert_eq!(id.fingerprint.len(), 64);
    // The same identity on the next start.
    assert_eq!(
        Identity::load_or_create(&home.0.join("reader"))
            .unwrap()
            .fingerprint,
        id.fingerprint
    );
    // A key others can read is refused.
    std::fs::set_permissions(&id.key_path, std::fs::Permissions::from_mode(0o644)).unwrap();
    assert!(Identity::load_or_create(&home.0.join("reader")).is_err());
}

#[test]
fn a_stranger_joins_and_reads_the_feed_newest_first() {
    let local = host();
    publish(&local, "First", "Hello");
    publish(&local, "Second", "# Big\n\nA *para* <script>alert(1)</script> [site](https://ikigai-rs.dev) [bad](javascript:x)\n\n- one\n- two\n");
    let home = Scratch::new("home-join");
    let reader = reader(&local, &home.0.join("reader"));
    let granted = reader.join().expect("the public join");
    assert!(granted.iter().any(|l| l.contains("news")), "{granted:?}");
    let entries = reader.entries("news").expect("the feed reads");
    let titles: Vec<_> = entries.iter().map(|e| e.title.clone().unwrap()).collect();
    assert_eq!(titles, ["Second", "First"]);
    // A layer the descriptor does not offer is never asked for.
    assert!(reader.entries("internal").is_err());
}

#[test]
fn markdown_becomes_text_kinds_and_raw_html_never_passes() {
    let nodes = ikigai_reader::markdown("# Big\n\nA *para* <b>x</b> [site](https://ikigai-rs.dev) [bad](javascript:x)\n\n<div>block</div>\n\n- one\n- two\n");
    let text = serde_json::to_string(&nodes).unwrap();
    assert!(!text.contains('<'), "{text}");
    assert!(!text.contains("javascript:"), "{text}");
    let kinds: Vec<_> = nodes.iter().map(|n| n["kind"].as_str().unwrap()).collect();
    assert_eq!(kinds, ["heading", "text", "link", "list"]);
}

#[test]
fn the_screens_compose_over_the_apps_resources_and_validate() {
    let local = host();
    publish(&local, "Older", "Body **bold**");
    publish(
        &local,
        "Only",
        "# Inside\n\nBody <script>x</script> **bold**",
    );
    let home = Scratch::new("home-screen");
    let reader = reader(&local, &home.0.join("reader"));
    let root = Arc::new(Fallback::new(vec![
        Arc::new(ikigai_reader::space(reader)),
        Arc::new(ikigai_ui::space()),
        Arc::new(ikigai_ui::screen::screens()),
    ]));
    let kernel = Kernel::with_meta_renderer(root, Arc::new(ikigai_vocab::TurtleRenderer));
    let html_of = |screen: &str| {
        let iri = format!("urn:iki:ui:html:screen:{screen}");
        let rep = block_on(kernel.issue(
            Request::new(Verb::Source, Iri::parse(&iri).unwrap()),
            &ikigai_reader::capability(),
        ))
        .unwrap_or_else(|e| panic!("{iri}: {e}"));
        String::from_utf8(rep.bytes.clone()).unwrap()
    };
    // The feed, newest first, each entry a link to its own screen.
    let feed = html_of("feed-news");
    let (newer, older) = (feed.find("Only").unwrap(), feed.find("Older").unwrap());
    assert!(newer < older, "{feed}");
    assert!(feed.contains("href=\"/page/entry-news-2\""), "{feed}");
    // The entry's screen: its Markdown as text kinds, raw HTML dropped, a way back.
    let entry = html_of("entry-news-2");
    assert!(
        entry.contains("Inside") && entry.contains("bold"),
        "{entry}"
    );
    assert!(!entry.contains("<script"), "{entry}");
    assert!(entry.contains("href=\"/page/feed-news\""), "{entry}");
    // An entry of a feed the descriptor does not offer, or one that is not there, is no screen.
    for screen in ["entry-internal-1", "entry-news-9", "entry-news-x"] {
        let iri = format!("urn:iki:ui:html:screen:{screen}");
        let rep = block_on(kernel.issue(
            Request::new(Verb::Source, Iri::parse(&iri).unwrap()),
            &ikigai_reader::capability(),
        ));
        let shown = rep
            .map(|r| String::from_utf8(r.bytes).unwrap())
            .unwrap_or_default();
        assert!(
            !shown.contains("Older") && !shown.contains("Inside"),
            "{screen}: {shown}"
        );
    }
    for (screen, want) in [
        ("feeds", "News"),
        ("feed-news", "Only"),
        ("entry-news-1", "Older"),
    ] {
        let iri = format!("urn:iki:ui:html:screen:{screen}");
        let rep = block_on(kernel.issue(
            Request::new(Verb::Source, Iri::parse(&iri).unwrap()),
            &ikigai_reader::capability(),
        ))
        .unwrap_or_else(|e| panic!("{iri}: {e}"));
        let html = String::from_utf8(rep.bytes.clone()).unwrap();
        assert!(html.contains(want), "{screen}: {html}");
        assert!(!html.contains("could not be read"), "{screen}: {html}");
    }
}
