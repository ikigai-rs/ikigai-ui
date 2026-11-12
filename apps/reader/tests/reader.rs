//! The reader against a LOCAL layer host in this process (never the live edge): a stranger's
//! join from the app's own fresh certificate, a public feed read newest first, Markdown as text
//! kinds with raw HTML dropped, and the screens composed and drawn by the HTML face.
#![cfg(not(target_arch = "wasm32"))]

use std::path::{Path, PathBuf};
use std::sync::Arc;

use futures::executor::block_on;
use ikigai_core::{ArgRef, Capability, Fallback, Iri, Kernel, Request, Verb};
use ikigai_reader::{Descriptor, Identity, LocalDoor, Reader};

const DOOR: &str = "local";

struct Scratch(PathBuf);
impl Scratch {
    fn new(label: &str) -> Self {
        let p = std::env::temp_dir().join(format!(
            "ikigai-reader-{label}-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        let _ = std::fs::remove_dir_all(&p);
        std::fs::create_dir_all(&p).unwrap();
        Scratch(p)
    }
    fn write(&self, rel: &str, text: &str) {
        let p = self.0.join(rel);
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(p, text).unwrap();
    }
}
impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

const SHAPES: &str = r#"@prefix sh: <http://www.w3.org/ns/shacl#> .
@prefix xsd: <http://www.w3.org/2001/XMLSchema#> .
@prefix ik: <https://ikigai-rs.dev/ns#> .
@prefix prov: <http://www.w3.org/ns/prov#> .
@prefix dcterms: <http://purl.org/dc/terms/> .
@prefix rdf: <http://www.w3.org/1999/02/22-rdf-syntax-ns#> .
<urn:example:shape:post> a sh:NodeShape ;
  sh:targetClass ik:Message ;
  sh:closed true ;
  sh:ignoredProperties ( rdf:type ) ;
  sh:property [ sh:path dcterms:title ; sh:minCount 1 ; sh:maxCount 1 ; sh:datatype xsd:string ] ,
              [ sh:path ik:body ; sh:maxCount 1 ; sh:datatype xsd:string ] ,
              [ sh:path dcterms:subject ; sh:nodeKind sh:IRI ] ,
              [ sh:path prov:wasAttributedTo ; sh:minCount 1 ; sh:maxCount 1 ; sh:nodeKind sh:IRI ] ,
              [ sh:path dcterms:created ; sh:minCount 1 ; sh:maxCount 1 ; sh:datatype xsd:dateTime ] ,
              [ sh:path ik:seq ; sh:minCount 1 ; sh:maxCount 1 ; sh:datatype xsd:integer ] .
"#;
const CONTEXT: &str = r#"{ "@context": { "dcterms": "http://purl.org/dc/terms/", "title": "dcterms:title", "body": "https://ikigai-rs.dev/ns#body" } }"#;

struct Local {
    _config: Scratch,
    _state: Scratch,
    host: Arc<ikigai_layers::Host>,
}

fn host() -> Local {
    let config = Scratch::new("config");
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/host/layers");
    for f in ["join.toml", "join.shapes.ttl", "join.context.jsonld"] {
        config.write(
            &format!("layers/{f}"),
            &std::fs::read_to_string(dir.join(f)).unwrap(),
        );
    }
    config.write("layers/post.shapes.ttl", SHAPES);
    config.write("layers/post.context.jsonld", CONTEXT);
    config.write(
        "layers/news.toml",
        "model = \"channel\"\nlabel = \"News\"\nshapes = \"post.shapes.ttl\"\ncontext = \"post.context.jsonld\"\nfeed = true\npublishers = [\"editor\"]\nscheme = \"post.scheme.ttl\"\npublic = true\n",
    );
    config.write(
        "layers/post.scheme.ttl",
        "@prefix skos: <http://www.w3.org/2004/02/skos/core#> .\n<urn:example:scheme:news> a skos:ConceptScheme .\n<urn:example:concept:release> a skos:Concept ; skos:prefLabel \"Release\" ; skos:notation \"1\" .\n",
    );
    config.write(
        "participants.ttl",
        "@prefix foaf: <http://xmlns.com/foaf/0.1/> .\n@prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> .\n<urn:collab:participant:editor> a foaf:Person ; rdfs:label \"A test editor\" .\n",
    );
    let pair = rcgen::KeyPair::generate_for(&rcgen::PKCS_ED25519).unwrap();
    config.write("join.pem", &pair.serialize_pem());
    config.write("layers.toml", "[join]\nhost_key = \"join.pem\"\n");
    use age::secrecy::ExposeSecret;
    config.write(
        "state.key",
        &format!(
            "{}\n",
            age::x25519::Identity::generate()
                .to_string()
                .expose_secret()
        ),
    );
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(
            config.0.join("state.key"),
            std::fs::Permissions::from_mode(0o600),
        )
        .unwrap();
    }
    let key = ikigai_layers::at_rest::StateKey::load(&config.0.join("state.key")).unwrap();
    let state = Scratch::new("state");
    let host = ikigai_layers::Host::open(&config.0, &state.0, &key).expect("the local host");
    Local {
        _config: config,
        _state: state,
        host,
    }
}

fn publish(local: &Local, title: &str, body: &str) {
    let root = Arc::new(Fallback::new(vec![Arc::new(ikigai_layers::space(
        &local.host,
    ))]));
    let kernel = Kernel::with_meta_renderer(root, Arc::new(ikigai_vocab::TurtleRenderer))
        .with_clock(Arc::new(ikigai_core::SystemClock));
    let editor = Capability::scoped(["urn:cap:layer:say:news".to_string()])
        .with_principal("urn:collab:participant:editor")
        .unwrap();
    let esc = |s: &str| {
        s.replace('\\', "\\\\")
            .replace('"', "\\\"")
            .replace('\n', "\\n")
    };
    let content = format!(
        "@prefix ik: <https://ikigai-rs.dev/ns#> .\n@prefix dcterms: <http://purl.org/dc/terms/> .\n<urn:layer:new> a ik:Message ; dcterms:title \"{}\" ; ik:body \"{}\" .\n",
        esc(title),
        esc(body)
    );
    let request = Request::new(Verb::Sink, Iri::parse("urn:layer:news:say").unwrap())
        .with_arg("content", ArgRef::Inline(content.into_bytes()));
    block_on(kernel.issue(request, &editor)).expect("the editor publishes");
}

const DESCRIPTOR: &str = r#"{"door":"quic://127.0.0.1:4433","join":"urn:layer:join:say","public_layers":[{"name":"news","type":"ik:Feed","read_only":true,"label":"News"}]}"#;

fn reader(local: &Local, home: &Path) -> Arc<Reader> {
    let identity = Identity::load_or_create(home).expect("the app's own identity");
    let door = Arc::new(LocalDoor::new(
        local.host.clone(),
        DOOR,
        &identity.fingerprint,
    ));
    Arc::new(Reader::new(
        door,
        identity,
        Descriptor::parse(DESCRIPTOR.as_bytes()).unwrap(),
    ))
}

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
    publish(&local, "Only", "Body **bold**");
    let home = Scratch::new("home-screen");
    let reader = reader(&local, &home.0.join("reader"));
    let root = Arc::new(Fallback::new(vec![
        Arc::new(ikigai_reader::space(reader)),
        Arc::new(ikigai_ui::space()),
        Arc::new(ikigai_ui::screen::screens()),
    ]));
    let kernel = Kernel::with_meta_renderer(root, Arc::new(ikigai_vocab::TurtleRenderer));
    for (screen, want) in [("feeds", "News"), ("feed-news", "Only")] {
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
