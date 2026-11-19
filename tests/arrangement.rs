//! **A host's arrangement round-trips with ikigai-ui in it** (ledger #3814).
//!
//! A host (ikigai-cli's `urn:iki:host:arrangement`) writes its root out as a declaration and
//! builds it again with core's [`build`], from a registry it HARVESTS: every door's own
//! pattern is resolved in the space that holds it (a template expanded with a probe value),
//! and the endpoint that answers is registered under its name. A door core cannot rebuild
//! from text (`ik:matchKind "custom"`) is refused by `build`, and cannot be probed either, so
//! one such door in [`ikigai_ui::space`] makes every host's arrangement undeclarable. 0.5.1's
//! `ui-lens-title` and `ui-screen-lens` doors were two.
//!
//! This test does what such a host does, over a kernel holding `ikigai_ui::space()` (and the
//! lensed page family [`ikigai_ui::pages`] builds), and then resolves one name from each door
//! in the rebuilt space, requiring the same endpoint as the original.

use std::sync::Arc;

use ikigai_core::{
    build, Bindings, Door, Endpoint, Iri, Kernel, MatchKind, Registry, Request, Resolution, Scope,
    Space, SpaceKind, Topology, UriTemplate, Verb,
};

/// The value each `{var}` takes in a probe, as ikigai-embedded's harvest uses.
const PROBE: &str = "ikigai-arrangement-probe";

fn doors(node: &Topology, out: &mut Vec<Door>) {
    if let SpaceKind::EndpointSpace { doors } = &node.kind {
        out.extend(doors.iter().cloned());
    }
    for child in &node.children {
        doors(child, out);
    }
}

/// The name a door's probe resolves: the exact IRI, or the template with every variable
/// [`PROBE`]. `None` for a kind core cannot rebuild.
fn probe_name(door: &Door) -> Option<String> {
    match door.kind {
        MatchKind::Exact => Some(door.pattern.clone()),
        MatchKind::Template => {
            let template = UriTemplate::parse(&door.pattern).ok()?;
            let mut bindings = Bindings::new();
            for var in template.variables() {
                bindings.insert(var, PROBE);
            }
            template.expand(&bindings)
        }
        _ => None,
    }
}

fn resolve(space: &dyn Space, name: &str) -> Option<Arc<dyn Endpoint>> {
    let request = Request::new(Verb::Meta, Iri::parse(name).ok()?);
    match space.resolve(&request, &Scope::empty()) {
        Resolution::Hit(hit) => Some(hit.endpoint),
        _ => None,
    }
}

fn round_trip(root: Arc<dyn Space>, more: &[(&str, &str)]) {
    let kernel = Kernel::new(Arc::clone(&root));
    let topology = kernel.root_topology();
    let mut all = Vec::new();
    doors(&topology, &mut all);
    assert!(!all.is_empty(), "the arrangement has doors");

    // Harvest: every door's endpoint, found by its own pattern. A door that cannot be is
    // a door no host can declare.
    let mut registry = Registry::new();
    let mut unreached = Vec::new();
    for door in &all {
        let found = probe_name(door).and_then(|name| resolve(root.as_ref(), &name));
        match found {
            Some(endpoint) if endpoint.name() == door.endpoint => {
                let _ = registry.register(endpoint);
            }
            _ => unreached.push(format!(
                "`{}` ({}) binding `{}`",
                door.pattern,
                door.kind.keyword(),
                door.endpoint
            )),
        }
    }
    assert!(
        unreached.is_empty(),
        "doors a host cannot recover from its own arrangement:\n{}",
        unreached.join("\n")
    );

    // Dump, load, build.
    let turtle = topology.to_turtle();
    let declared = Topology::from_turtle(&turtle).expect("the dumped arrangement parses");
    let rebuilt = build(&declared, &registry)
        .unwrap_or_else(|e| panic!("the dumped arrangement cannot be built: {e}"));
    assert_eq!(
        Kernel::new(Arc::clone(&rebuilt))
            .root_topology()
            .to_turtle(),
        turtle,
        "the rebuilt arrangement is the one dumped"
    );

    // One name from each door resolves to the same endpoint in both.
    for door in &all {
        let name = probe_name(door).expect("every door probed above");
        let before = resolve(root.as_ref(), &name).map(|e| e.name().to_string());
        let after = resolve(rebuilt.as_ref(), &name).map(|e| e.name().to_string());
        assert_eq!(after, before, "`{name}` from the door `{}`", door.pattern);
    }
    // And names no probe spells: a lensed name with facets reaches its endpoint in both.
    for (name, endpoint) in more {
        for space in [&root, &rebuilt] {
            let found = resolve(space.as_ref(), name).map(|e| e.name().to_string());
            assert_eq!(found.as_deref(), Some(*endpoint), "`{name}`");
        }
    }
}

#[test]
fn the_ui_space_round_trips_as_an_arrangement() {
    round_trip(
        Arc::new(ikigai_ui::space()),
        &[
            (
                "urn:iki:ui:lens:title:lens:contact-list:facets:selected=a%3Ab:resource:urn:x:y",
                "ui-lens-title",
            ),
            (
                "urn:iki:ui:screen:name:lens:contact-list:facets:selected=a:resource:urn:x:y",
                "ui-screen-lens",
            ),
        ],
    );
}

#[test]
fn a_lensed_page_family_round_trips_as_an_arrangement() {
    let family = ikigai_ui::Pages::new("demo", "urn:demo:")
        .lenses(true)
        .build()
        .expect("a valid family");
    let root = ikigai_core::Fallback::new(vec![
        Arc::new(family) as Arc<dyn Space>,
        Arc::new(ikigai_ui::space()),
    ]);
    round_trip(
        Arc::new(root),
        &[(
            "urn:demo:page:lens:contact-list:facets:selected=a:resource:urn:x:y",
            "ui-page-lens-demo",
        )],
    );
}
