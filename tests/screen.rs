//! Screens as outlines (ledger #3365): the outline parser and its bounds, the compiler and its
//! overrides, references to other outlines, cycles, the nodes-produced bound, prefixed names
//! through ikigai-fn's prefix spaces, and the gallery compiled from its outline equal to the
//! model it was assembled into before.

use std::collections::BTreeMap;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use futures::executor::block_on;
use ikigai_core::{
    Capability, Description, EndpointSpace, Error, Exact, Fallback, FnEndpoint, Invocation, Iri,
    Kernel, ReprType, Representation, Request, Scope, Space, UriTemplate, Verb,
};
use ikigai_ui::model::{self, Node, Value};
use ikigai_ui::screen;

type Sources = Arc<Mutex<BTreeMap<String, String>>>;

/// A host: ikigai-ui, a source per screen from `sources` (counting reads), then the screens.
fn host(sources: Sources, reads: Arc<AtomicUsize>) -> Kernel {
    let endpoint = FnEndpoint::new("test-screen-source", move |inv: &Invocation<'_>| {
        reads.fetch_add(1, Ordering::SeqCst);
        let name = inv.bindings.get("screen").unwrap_or_default();
        let text = sources
            .lock()
            .unwrap()
            .get(name)
            .cloned()
            .ok_or_else(|| Error::NotFound(format!("no screen `{name}`")))?;
        Ok(Representation::new(ReprType::new(screen::OUTLINE), text.into_bytes()).cacheable())
    })
    .with_description(
        Description::new("test-screen-source")
            .verb(Verb::Source)
            .output(screen::OUTLINE),
    );
    let sources = EndpointSpace::new().bind(
        UriTemplate::parse("urn:iki:ui:screen:name:{screen}:source").unwrap(),
        endpoint,
    );
    Kernel::new(Arc::new(Fallback::new(vec![
        Arc::new(ikigai_ui::space()),
        Arc::new(sources),
        Arc::new(screen::screens()),
    ])))
}

fn with(sources: &[(&str, &str)]) -> (Kernel, Sources, Arc<AtomicUsize>) {
    let map: Sources = Arc::new(Mutex::new(
        sources
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect(),
    ));
    let reads = Arc::new(AtomicUsize::new(0));
    (host(map.clone(), reads.clone()), map, reads)
}

fn request(name: &str) -> Request {
    Request::new(
        Verb::Source,
        Iri::parse(format!("{}{name}", screen::SCREEN_PREFIX)).unwrap(),
    )
}

fn read_in(kernel: &Kernel, name: &str, scope: Scope) -> Result<Node, Error> {
    let rep = block_on(kernel.issue_in(request(name), &Capability::root(), scope))?;
    assert_eq!(rep.repr_type.media_type, model::MEDIA_TYPE);
    Ok(model::parse(&rep.bytes).unwrap())
}

fn read(kernel: &Kernel, name: &str) -> Result<Node, Error> {
    read_in(kernel, name, Scope::empty())
}

fn example(kind: &str) -> Node {
    serde_json::from_str(model::kind(kind).unwrap().example).unwrap()
}

/// The refusal's detail, which must be InvalidArgument.
fn refused(result: Result<Node, Error>) -> String {
    match result {
        Err(Error::InvalidArgument { detail, .. }) => detail,
        other => panic!("not refused as InvalidArgument: {other:?}"),
    }
}

fn text(v: &str) -> Value {
    Value::Text(v.into())
}

#[test]
fn a_kind_line_and_a_reference_compile_to_the_model() {
    let (kernel, _, _) = with(&[(
        "one",
        "# a comment\n\ngroup label=\"A screen\"\n  heading content=Hello level=1\n  urn:iki:ui:example:kind:status\n",
    )]);
    let node = read(&kernel, "one").unwrap();
    assert_eq!(node.kind, "group");
    assert_eq!(node.properties["label"], text("A screen"));
    assert_eq!(node.children[0].kind, "heading");
    assert_eq!(node.children[0].properties["level"], Value::Number(1));
    assert_eq!(node.children[1], example("status"));
}

#[test]
fn an_override_relabels_a_reference_and_its_children_follow_its_own() {
    let (kernel, _, _) = with(&[(
        "form",
        "urn:iki:ui:example:kind:form label=\"Write to us\" description=\"We answer within a day\"\n  button label=Again\n",
    )]);
    let node = read(&kernel, "form").unwrap();
    let mut expected = example("form");
    expected
        .properties
        .insert("label".into(), text("Write to us"));
    expected
        .properties
        .insert("description".into(), text("We answer within a day"));
    expected.children.push(Node {
        kind: "button".into(),
        hint: Default::default(),
        replacement: None,
        properties: [("label".to_string(), text("Again"))].into(),
        children: Vec::new(),
    });
    assert_eq!(node, expected);
}

#[test]
fn an_override_the_kind_does_not_have_is_refused_naming_the_line() {
    let (kernel, _, _) = with(&[(
        "bad",
        "group label=G\n  urn:iki:ui:example:kind:form colour=red\n",
    )]);
    let detail = refused(read(&kernel, "bad"));
    assert!(
        detail.contains("urn:iki:ui:screen:name:bad line 2: a form has no property `colour`"),
        "{detail}"
    );
    // A value of the wrong type is refused the same way.
    let (kernel, _, _) = with(&[("level", "group label=G\n  heading content=H level=9\n")]);
    let detail = refused(read(&kernel, "level"));
    assert!(
        detail.contains("line 2: `level` is not a valid Level"),
        "{detail}"
    );
    // So is a missing required property, and a child the kind does not take.
    let (kernel, _, _) = with(&[("missing", "group label=G\n\n  heading content=H\n")]);
    assert!(refused(read(&kernel, "missing")).contains("line 3: a heading needs `level`"));
    let (kernel, _, _) = with(&[("nav", "nav label=N\n  text content=T\n")]);
    assert!(refused(read(&kernel, "nav")).contains("line 1: a nav takes only link children"));
}

#[test]
fn a_reference_to_another_outline_compiles_it_in() {
    let (kernel, _, _) = with(&[
        (
            "outer",
            "group label=Outer\n  urn:iki:ui:screen:name:inner label=Renamed\n",
        ),
        ("inner", "group label=Inner\n  text content=Within\n"),
    ]);
    let node = read(&kernel, "outer").unwrap();
    let inner = &node.children[0];
    assert_eq!(inner.properties["label"], text("Renamed"));
    assert_eq!(inner.children[0].properties["content"], text("Within"));
}

#[test]
fn a_cycle_of_screens_is_refused_naming_the_line() {
    let (kernel, _, _) = with(&[
        ("a", "group label=A\n  urn:iki:ui:screen:name:b\n"),
        (
            "b",
            "group label=B\n  text content=x\n  urn:iki:ui:screen:name:a\n",
        ),
        ("self", "group label=S\n  urn:iki:ui:screen:name:self\n"),
    ]);
    let detail = refused(read(&kernel, "a"));
    assert!(
        detail.contains("urn:iki:ui:screen:name:b line 3: a cycle of screens: a → b → a"),
        "{detail}"
    );
    assert!(refused(read(&kernel, "self")).contains("line 2: a cycle of screens: self → self"));
}

#[test]
fn a_chain_of_screens_past_the_depth_bound_is_refused() {
    let names: Vec<String> = (0..=screen::MAX_DEPTH).map(|i| format!("d{i}")).collect();
    let sources: Vec<(String, String)> = names
        .iter()
        .enumerate()
        .map(|(i, n)| {
            let body = match names.get(i + 1) {
                Some(next) => format!("group label={n}\n  urn:iki:ui:screen:name:{next}\n"),
                None => "text content=bottom\n".to_string(),
            };
            (n.clone(), body)
        })
        .collect();
    let borrowed: Vec<(&str, &str)> = sources
        .iter()
        .map(|(a, b)| (a.as_str(), b.as_str()))
        .collect();
    let (kernel, _, _) = with(&borrowed);
    assert!(refused(read(&kernel, "d0")).contains("screens nested past the bound"));
    // A shorter chain compiles.
    assert_eq!(
        read(&kernel, &names[screen::MAX_DEPTH - 2])
            .unwrap()
            .children[0]
            .children[0]
            .kind,
        "text"
    );
}

#[test]
fn a_fan_out_past_the_nodes_produced_bound_is_refused() {
    // 100 lines of one reference: read once, but each line produces its nodes.
    let hundred = |r: &str| {
        let mut s = "group label=Wide\n".to_string();
        for _ in 0..100 {
            s.push_str(&format!("  {r}\n"));
        }
        s
    };
    let leaf = hundred("text content=leaf");
    let wide = hundred("urn:iki:ui:screen:name:leaf");
    let (kernel, _, reads) = with(&[("leaf", &leaf), ("wide", &wide)]);
    assert_eq!(read(&kernel, "leaf").unwrap().children.len(), 100);
    let detail = refused(read(&kernel, "wide"));
    assert!(
        detail.contains(&format!("produces more than {} nodes", screen::MAX_NODES)),
        "{detail}"
    );
    // The source of `leaf` was read once for both.
    assert_eq!(reads.load(Ordering::SeqCst), 2);
}

#[test]
fn the_source_is_bounded_before_it_is_parsed() {
    let big = vec![b'#'; screen::MAX_SOURCE_BYTES + 1];
    let Err(Error::InvalidArgument { detail, .. }) = screen::parse("s", &big) else {
        panic!("not refused");
    };
    assert!(detail.contains("past the bound"), "{detail}");
    let many = "#\n".repeat(screen::MAX_LINES + 1);
    assert!(screen::parse("s", many.as_bytes()).is_err());
    let deep = format!(
        "group label=G\n{}text content=t\n",
        " ".repeat(screen::MAX_INDENT + 1)
    );
    assert!(screen::parse("s", deep.as_bytes()).is_err());
    // Nesting past the bound, each line one column deeper.
    let mut nested = String::new();
    for i in 0..=screen::MAX_NESTING {
        nested.push_str(&format!("{}group label=g\n", " ".repeat(i)));
    }
    let Err(Error::InvalidArgument { detail, .. }) = screen::parse("s", nested.as_bytes()) else {
        panic!("not refused");
    };
    assert!(detail.contains("nested past the bound"), "{detail}");
}

#[test]
fn parse_errors_name_the_line() {
    let cases = [
        (
            "group label=G\n\ttext content=t\n",
            "line 2: indent with spaces",
        ),
        (
            "group label=G\n    text content=a\n  text content=b\n",
            "line 3: the indentation matches no line",
        ),
        (
            "text content=a\ntext content=b\n",
            "line 2: an outline has one root line",
        ),
        ("bogus label=x\n", "line 1: `bogus` is not a kind"),
        (
            "text content=\"open\n",
            "line 1: `content`: the quote is not closed",
        ),
        ("text content\n", "line 1: `content` is not key=value"),
        (
            "text content=a content=b\n",
            "line 1: `content` is given twice",
        ),
        (
            "text content=a\n@prefixes urn:x:p\n",
            "line 2: @prefixes comes once",
        ),
        ("", "line 0: the outline has no node"),
    ];
    for (source, expected) in cases {
        let Err(Error::InvalidArgument { detail, .. }) = screen::parse("s", source.as_bytes())
        else {
            panic!("{source:?} parsed");
        };
        assert!(detail.contains(expected), "{source:?}: {detail}");
    }
    let quoted = screen::parse("s", br#"text content="say \"hi\" \\ there""#).unwrap();
    assert_eq!(quoted.root.properties[0].1.text, r#"say "hi" \ there"#);
}

#[test]
fn an_element_line_stays_an_element_node_with_its_arguments_typed() {
    let (kernel, _, _) = with(&[(
        "rated",
        "group label=R\n  urn:iki:ui:element:name:rating fallback=\"Rated 4\" label=Service score=4\n",
    )]);
    let node = read(&kernel, "rated").unwrap();
    let expected: Node = serde_json::from_str(ikigai_ui::element::EXAMPLES[0]).unwrap();
    let mut expected = expected;
    expected
        .properties
        .insert("fallback".into(), text("Rated 4"));
    assert_eq!(node.children[0], expected);
    assert_eq!(node.children[0].properties["score"], Value::Number(4));
}

#[test]
fn editing_one_source_recompiles_exactly_that_screen() {
    let (kernel, sources, reads) = with(&[
        ("first", "text content=Before\n"),
        ("second", "text content=Other\n"),
    ]);
    assert_eq!(
        read(&kernel, "first").unwrap().properties["content"],
        text("Before")
    );
    read(&kernel, "second").unwrap();
    read(&kernel, "first").unwrap();
    read(&kernel, "second").unwrap();
    assert_eq!(reads.load(Ordering::SeqCst), 2, "both cached");
    sources
        .lock()
        .unwrap()
        .insert("first".into(), "text content=After\n".into());
    kernel.cut("urn:iki:ui:screen:name:first:source");
    assert_eq!(
        read(&kernel, "first").unwrap().properties["content"],
        text("After")
    );
    read(&kernel, "second").unwrap();
    assert_eq!(
        reads.load(Ordering::SeqCst),
        3,
        "only `first` was read again"
    );
}

const SHADOW: &str = r#"
@prefix ik: <https://ikigai-rs.dev/ns#> .
@prefix rdf: <http://www.w3.org/1999/02/22-rdf-syntax-ns#> .
<urn:test:shadow> a ik:Alias ; ik:maxHops 8 ;
    ik:rewrites <urn:test:shadow:rule:1> ; ik:space <urn:test:shadow:space> .
<urn:test:shadow:rule:1> a ik:RewriteRule ; ik:ruleKind "prefix" ;
    ik:logical "urn:" ; ik:canonical "elsewhere:" ; ik:resolveAt "root" .
<urn:test:shadow:space> a ik:EndpointSpace ; ik:doors rdf:nil .
"#;

/// The default table's corridor, bound from `urn:iki:ui:prefixes` through the kernel.
fn default_corridor(kernel: &Kernel) -> Scope {
    let space = screen::prefix_space(screen::PREFIXES);
    block_on(space.refresh(kernel, &Capability::root())).unwrap();
    Scope::empty().with(Arc::new(space) as Arc<dyn Space>)
}

#[test]
fn a_prefixed_reference_resolves_inside_the_corridor_and_nowhere_else() {
    let short = "@prefixes urn:iki:ui:prefixes\ngroup label=Short\n  ui:example:kind:status content=Saved\n";
    let (kernel, _, _) = with(&[("short", short)]);
    let corridor = default_corridor(&kernel);
    let node = read_in(&kernel, "short", corridor).unwrap();
    let mut status = example("status");
    status.properties.insert("content".into(), text("Saved"));
    assert_eq!(node.children[0], status);
    // Outside the corridor its table is not in force, and the screen says so at its line.
    let detail = refused(read(&kernel, "short"));
    assert!(
        detail.contains("line 1: the prefix table `urn:iki:ui:prefixes` is not in force"),
        "{detail}"
    );
    // Without `@prefixes`, the short name is simply not a name outside the corridor.
    let (kernel, _, _) = with(&[("bare", "ui:example:kind:status\n")]);
    assert!(matches!(read(&kernel, "bare"), Err(Error::Unresolved(_))));
    let corridor = default_corridor(&kernel);
    assert_eq!(
        read_in(&kernel, "bare", corridor).unwrap(),
        example("status")
    );
}

#[test]
fn a_prefix_that_would_shadow_urn_is_refused_by_prefix_check() {
    let tables = EndpointSpace::new().bind(
        Exact::new("urn:test:shadow"),
        FnEndpoint::new("test-shadow", |_: &Invocation<'_>| {
            Ok(Representation::new(
                ReprType::new("text/turtle"),
                SHADOW.as_bytes().to_vec(),
            ))
        }),
    );
    let kernel = Kernel::new(Arc::new(tables));
    let space = screen::prefix_space("urn:test:shadow");
    let refusal = block_on(space.refresh(&kernel, &Capability::root())).unwrap_err();
    assert!(
        refusal.to_string().contains("reserved scheme `urn:`"),
        "{refusal}"
    );
    // Refused, the space binds nothing: injected anyway, it rewrites no name, and a screen's
    // full names resolve as they would without it.
    assert!(space.status().in_force.is_none());
    let (kernel, _, _) = with(&[(
        "shadowed",
        "@prefixes urn:test:shadow\nurn:iki:ui:example:kind:status\n",
    )]);
    let corridor = Scope::empty().with(Arc::new(space) as Arc<dyn Space>);
    assert_eq!(
        read_in(&kernel, "shadowed", corridor).unwrap(),
        example("status")
    );
}

/// The gallery as it was assembled before its outline (ledger #3352): a group per section of
/// `model::GALLERY`, each a heading and the section's examples.
fn assembled_gallery() -> Node {
    let gallery: serde_json::Value = serde_json::from_str(model::GALLERY).unwrap();
    let group = |label: &str, children: Vec<Node>| Node {
        kind: "group".into(),
        hint: Default::default(),
        replacement: None,
        properties: [("label".to_string(), text(label))].into(),
        children,
    };
    let sections = gallery["section"]
        .as_array()
        .unwrap()
        .iter()
        .map(|section| {
            let label = section["label"].as_str().unwrap();
            let mut children = vec![Node {
                kind: "heading".into(),
                hint: Default::default(),
                replacement: None,
                properties: [
                    ("content".to_string(), text(label)),
                    ("level".to_string(), Value::Number(2)),
                ]
                .into(),
                children: Vec::new(),
            }];
            for kind in section["kind"].as_array().unwrap() {
                children.push(example(kind.as_str().unwrap()));
            }
            group(label, children)
        })
        .collect();
    group(gallery["title"].as_str().unwrap(), sections)
}

#[test]
fn the_gallery_outline_compiles_to_the_model_it_was_assembled_into() {
    let kernel = Kernel::new(Arc::new(ikigai_ui::space()));
    let rep = block_on(kernel.issue(
        Request::new(Verb::Source, Iri::parse(ikigai_ui::GALLERY).unwrap()),
        &Capability::root(),
    ))
    .unwrap();
    assert_eq!(model::parse(&rep.bytes).unwrap(), assembled_gallery());
    assert_eq!(
        screen::SOURCES[0].bytes,
        include_bytes!("../assets/screen/gallery.outline")
    );
}

#[test]
fn the_default_prefix_table_passes_prefix_check() {
    let kernel = Kernel::new(Arc::new(ikigai_ui::space()));
    let space = screen::prefix_space(screen::PREFIXES);
    block_on(space.refresh(&kernel, &Capability::root())).unwrap();
    assert!(space.status().in_force.is_some());
    assert_eq!(
        screen::prefix_space_name("urn:iki:uidemo:prefixes").as_deref(),
        Some("ui-prefixes:iki:uidemo:prefixes")
    );
    assert_eq!(screen::prefix_space_name("http://x/p"), None);
}
