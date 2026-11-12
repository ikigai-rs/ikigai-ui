//! The local layer host the reader's tests run against, in this process (never the live
//! edge): a public feed `news`, a test editor who publishes to it, and the descriptor offering it.
#![allow(dead_code)]

use std::path::{Path, PathBuf};
use std::sync::Arc;

use futures::executor::block_on;
use ikigai_core::{ArgRef, Capability, Fallback, Iri, Kernel, Request, Verb};
use ikigai_reader::{Descriptor, Identity, LocalDoor, Reader};

pub const DOOR: &str = "local";

pub struct Scratch(pub PathBuf);
impl Scratch {
    pub fn new(label: &str) -> Self {
        let p = std::env::temp_dir().join(format!(
            "ikigai-reader-{label}-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        let _ = std::fs::remove_dir_all(&p);
        std::fs::create_dir_all(&p).unwrap();
        Scratch(p)
    }
    pub fn write(&self, rel: &str, text: &str) {
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

pub struct Local {
    _config: Scratch,
    _state: Scratch,
    pub host: Arc<ikigai_layers::Host>,
}

pub fn host() -> Local {
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
    let key = ikigai_layers::at_rest::StateKey::load(config.0.join("state.key")).unwrap();
    let state = Scratch::new("state");
    let host = ikigai_layers::Host::open(&config.0, &state.0, &key).expect("the local host");
    Local {
        _config: config,
        _state: state,
        host,
    }
}

pub fn publish(local: &Local, title: &str, body: &str) {
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

pub const DESCRIPTOR: &str = r#"{"door":"quic://127.0.0.1:4433","join":"urn:layer:join:say","public_layers":[{"name":"news","type":"ik:Feed","read_only":true,"label":"News"}]}"#;

pub fn reader(local: &Local, home: &Path) -> Arc<Reader> {
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
