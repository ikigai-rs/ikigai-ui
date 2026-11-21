//! The workspace (ledger #3950, `docs/design/workspace.md`). The two refusal tests of section 4
//! come first, per decision 10: a pane's form naming a Sink the manifold did not offer is
//! refused, and a selection never writes. Then the atom's shape and the workspace screen.
mod common;

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};

use common::lens_actions::{issue, operations_host, CALL, INVOKE};
use futures::executor::block_on;
use ikigai_core::{
    ArgRef, Capability, Description, EndpointSpace, Error, Exact, FnEndpoint, Invocation, Iri,
    Kernel, ReprType, Representation, Request, UriTemplate, Verb,
};
use ikigai_ui::lens::actions;
use ikigai_ui::model::{self, Hint, Node, Value};
use ikigai_ui::workspace::{self, Layout};

const ITEM: &[u8] = include_bytes!("fixtures/workspace/item.json");
const WORKSPACE: &str = "urn:iki:ui:workspace:person:urn:person:alice:name:item";

fn kinds<'a>(n: &'a Node, kind: &str, out: &mut Vec<&'a Node>) {
    if n.kind == kind {
        out.push(n);
    }
    for c in &n.children {
        kinds(c, kind, out);
    }
}

fn all<'a>(n: &'a Node, kind: &str) -> Vec<&'a Node> {
    let mut out = Vec::new();
    kinds(n, kind, &mut out);
    out
}

fn text<'a>(n: &'a Node, key: &str) -> &'a str {
    match n.properties.get(key) {
        Some(Value::Text(s)) => s,
        _ => "",
    }
}

// --- section 4, first: a workspace adds no authority -----------------------------------------

/// A pane's form is only ever a generated form through `urn:iki:ui:lens:operation:`. Naming a
/// Sink the viewer's manifold did not offer is refused at submission, and the Sink is never
/// called, whatever the form said.
#[test]
fn a_panes_form_naming_a_sink_the_manifold_did_not_offer_is_refused() {
    let (kernel, calls) = operations_host();
    let wrapper = format!("{}{CALL}", actions::OPERATION);
    let args = [("person", "urn:test:Alice"), ("times", "3")];

    // The viewer holds no grant for the Sink: the manifold does not offer it.
    let viewer = Capability::scoped(["urn:cap:test:read"]);
    let refused = issue(&kernel, &viewer, Verb::Sink, &wrapper, &args).unwrap_err();
    assert!(matches!(refused, Error::Denied(_)), "{refused:?}");
    // A Sink nobody binds is not offered either.
    let nowhere = format!("{}urn:test:not-offered", actions::OPERATION);
    let cap = Capability::scoped([INVOKE]);
    assert!(issue(&kernel, &cap, Verb::Sink, &nowhere, &args).is_err());
    assert!(calls.lock().unwrap().is_empty(), "no Sink was called");

    // The same form, under a viewer whose manifold offers the Sink, goes through.
    issue(&kernel, &cap, Verb::Sink, &wrapper, &args).unwrap();
    assert_eq!(calls.lock().unwrap().len(), 1);
}

type Seen = Arc<Mutex<Vec<(Verb, String)>>>;

/// Records every request it answers; as a model endpoint it answers a small screen.
fn recorder(id: &'static str, seen: Seen, body: Vec<u8>, media: &'static str) -> FnEndpoint {
    FnEndpoint::new(id, move |inv: &Invocation<'_>| {
        seen.lock()
            .unwrap()
            .push((inv.request.verb, inv.request.target.as_str().to_string()));
        if inv.request.verb != Verb::Source {
            return Err(Error::Denied("this test answers reads only".into()));
        }
        Ok(Representation::new(ReprType::new(media), body.clone()).cacheable())
    })
    .with_description(
        Description::new(id)
            .title(id)
            .summary("Records what it is asked.")
            .verb(Verb::Source)
            .verb(Verb::Sink)
            .verb(Verb::Meta)
            .output(media),
    )
}

fn workspace_host() -> (Kernel, Seen) {
    let seen = Seen::default();
    let pane = serde_json::to_vec(&serde_json::json!({
        "kind": "text", "properties": {"content": "A pane's content."}
    }))
    .unwrap();
    let space = EndpointSpace::new()
        .bind(
            Exact::new(WORKSPACE),
            recorder("atom", seen.clone(), ITEM.to_vec(), "application/json"),
        )
        .bind(
            UriTemplate::parse("urn:iki:ui:screen:name:lens:{rest}").unwrap(),
            recorder("panes", seen.clone(), pane, model::MEDIA_TYPE),
        );
    (common::host_with(None, vec![Arc::new(space)]), seen)
}

/// [`workspace_host`] with its spaces AHEAD of ikigai-ui's, so the panes' lensed screens are
/// the recorder's and not ikigai-ui's lensed door: the face's reads of them are seen.
fn face_host() -> (Kernel, Seen) {
    let seen = Seen::default();
    let pane = serde_json::to_vec(&serde_json::json!({
        "kind": "text", "properties": {"content": "A pane's content."}
    }))
    .unwrap();
    let space = EndpointSpace::new()
        .bind(
            Exact::new(WORKSPACE),
            recorder("atom", seen.clone(), ITEM.to_vec(), "application/json"),
        )
        .bind(
            UriTemplate::parse("urn:iki:ui:screen:name:lens:{rest}").unwrap(),
            recorder("panes", seen.clone(), pane, model::MEDIA_TYPE),
        );
    let root = Arc::new(ikigai_core::Fallback::new(vec![
        Arc::new(space) as Arc<dyn ikigai_core::Space>,
        common::root(None, Vec::new()),
    ]));
    (common::kernel(root), seen)
}

fn workspace_screen(kernel: &Kernel, select: &[(&str, &str)]) -> Result<Node, Error> {
    let mut request = Request::new(
        Verb::Source,
        Iri::parse(workspace::screen(WORKSPACE)).unwrap(),
    );
    for (role, iri) in select {
        request = request.with_arg(
            format!("select.{role}"),
            ArgRef::Inline(iri.as_bytes().to_vec()),
        );
    }
    let rep = block_on(kernel.issue(request, &Capability::scoped(["urn:cap:test:read"])))?;
    model::parse(&rep.bytes)
}

/// A selection is a GET: composing a workspace with a new selection reads the atom and the
/// following panes' content, and issues nothing but reads. The atom is not written, and no
/// selected IRI is put into an action's arguments behind the form's back.
#[test]
fn a_selection_never_writes() {
    let (kernel, seen) = workspace_host();
    let chosen = "urn:iki:ledger:default:item:3950";
    let screen = workspace_screen(&kernel, &[("item", chosen)]).unwrap();

    let seen = seen.lock().unwrap();
    assert!(!seen.is_empty());
    assert!(
        seen.iter().all(|(verb, _)| *verb == Verb::Source),
        "only reads: {seen:?}"
    );
    assert!(seen.iter().any(|(_, iri)| iri == WORKSPACE));
    // The following panes moved to the selection, by reference: the face reads each under
    // the viewer's capability inside its region. The pinned pane did not move.
    let refs: Vec<&str> = all(&screen, "reference")
        .iter()
        .map(|r| text(r, "resource"))
        .collect();
    assert!(
        refs.contains(&format!("urn:iki:ui:screen:name:lens:session:resource:{chosen}").as_str())
    );
    assert!(refs.contains(&"urn:iki:ui:screen:name:lens:dashboard:resource:urn:iki:ledger:default"));

    // The only form is the "Layout" form, posting to the edit Sink. The selection is in none
    // of its fields.
    let forms = all(&screen, "form");
    assert_eq!(forms.len(), 1);
    assert_eq!(text(forms[0], "resource"), format!("{WORKSPACE}:edit"));
    for field in all(forms[0], "field") {
        assert!(!text(field, "value").contains(chosen), "{field:?}");
    }
}

/// The same through the html face (round 2): the workspace screen reached as a named screen,
/// `urn:iki:ui:html:screen:workspace:resource:{iri}`, with the browser's `select.item=` folded
/// in. The face resolves each pane's reference inside its region, so the pane reads it makes are
/// covered too: every request is a read, the following panes read the selected item, and the
/// page's one form is the Layout form, the selection in none of its fields.
#[test]
fn a_selection_never_writes_through_the_face() {
    let (kernel, seen) = face_host();
    let chosen = "urn:iki:ledger:default:item:3950";
    let face = format!(
        "urn:iki:ui:html:screen:{}",
        workspace::face_name(WORKSPACE, &BTreeMap::new())
    );
    let request = Request::new(Verb::Source, Iri::parse(face.as_str()).unwrap())
        .with_arg("select.item", ArgRef::Inline(chosen.as_bytes().to_vec()));
    let rep = block_on(kernel.issue(request, &Capability::scoped(["urn:cap:test:read"]))).unwrap();
    let html = String::from_utf8(rep.bytes).unwrap();

    let seen = seen.lock().unwrap();
    assert!(
        seen.iter().all(|(verb, _)| *verb == Verb::Source),
        "only reads: {seen:?}"
    );
    assert!(seen.iter().any(|(_, iri)| iri == WORKSPACE));
    // The references the face resolved: the following panes at the selection, the pinned one
    // where it was.
    let read = |iri: String| seen.iter().any(|(_, i)| *i == iri);
    assert!(read(format!(
        "urn:iki:ui:screen:name:lens:session:resource:{chosen}"
    )));
    assert!(read(format!(
        "urn:iki:ui:screen:name:lens:item:resource:{chosen}"
    )));
    assert!(read(
        "urn:iki:ui:screen:name:lens:dashboard:resource:urn:iki:ledger:default".to_string()
    ));
    assert_eq!(html.matches("A pane&#39;s content.").count(), 4, "{html}");

    assert_eq!(html.matches("<form").count(), 1, "{html}");
    let form = &html[html.find("<form").unwrap()..];
    let form = &form[..form.find("</form>").unwrap()];
    assert!(!form.contains(chosen), "{form}");
    assert!(form.contains("name%3Aitem%3Aedit\""), "{form}");
    assert!(form.contains("name=\"if-version\""), "{form}");
}

/// Proposal 3, the html face for the layout hints: a split is a flex row or column sized by
/// its shares (classes, since the CSP allows no inline style), tabs a real tab list with the
/// active key from the hint, each pane a section with its fragment id; and a face that does
/// not claim the layout hints gets the linear form.
#[test]
fn the_html_face_renders_the_layout_hints() {
    let (kernel, _) = face_host();
    let face = format!(
        "urn:iki:ui:html:screen:{}",
        workspace::face_name(WORKSPACE, &BTreeMap::new())
    );
    let rep = block_on(kernel.issue(
        Request::new(Verb::Source, Iri::parse(face.as_str()).unwrap()),
        &Capability::scoped(["urn:cap:test:read"]),
    ))
    .unwrap();
    let html = String::from_utf8(rep.bytes).unwrap();
    for part in [
        r##"<div class="ui-split ui-split-row" role="group" aria-label="Side by side: Dashboard, Session">"##,
        r##"<div class="ui-share ui-share-300"><section class="ui-group ui-pane" id="pane-dashboard" aria-label="Dashboard">"##,
        r##"<div class="ui-share ui-share-700"><div class="ui-split ui-split-column""##,
        r##"<div class="ui-tabs" data-ui-tabs role="group""##,
        r##"<div role="tablist" aria-label="Tabs: Item, Trail">"##,
        r##"<a role="tab" id="tab-item" href="#pane-item" aria-controls="panel-item" aria-selected="true">Item</a>"##,
        r##"<a role="tab" id="tab-trail" href="#pane-trail" aria-controls="panel-trail" aria-selected="false">Trail</a>"##,
        r##"<div role="tabpanel" id="panel-trail" aria-labelledby="tab-trail" class="ui-tabpanel ui-tabpanel-inactive"><section class="ui-group ui-pane" id="pane-trail""##,
        r##"<nav aria-label="Panes">"##,
        r##"#pane-trail">Trail: follows item"##,
    ] {
        assert!(html.contains(part), "{part}\n{html}");
    }
    // The pane list comes before the layout: it is the skip list into the panes.
    assert!(html.find("aria-label=\"Panes\"") < html.find("ui-split"));
}

/// The linear fallback: a face that does not claim `urn:iki:ui:hint:layout:` drops the hints
/// (the same `without_hint_namespace` the choice and group hints take), and the workspace is
/// its panes in reading order, each still a labeled section with its heading.
#[test]
fn without_the_layout_hints_the_workspace_is_linear() {
    let ws = workspace::validate(ITEM).unwrap();
    let screen = workspace::compose(&ws, WORKSPACE, 1, &BTreeMap::new()).unwrap();
    let linear = ikigai_ui::html::without_layout_hints(screen);
    fn hinted(n: &Node) -> bool {
        n.hint.keys().any(|k| k.starts_with(workspace::HINTS)) || n.children.iter().any(hinted)
    }
    assert!(!hinted(&linear));
    model::validate(&linear).unwrap();
    let headings: Vec<&str> = all(&linear, "heading")
        .iter()
        .map(|h| text(h, "content"))
        .collect();
    assert_eq!(
        headings,
        ["Item 3941", "Dashboard", "Session", "Item", "Trail"]
    );
}

/// A workspace face name round-trips its selection, percent-encoded so no `:` splits it.
#[test]
fn the_face_name() {
    let mut chosen = BTreeMap::new();
    assert_eq!(
        workspace::face_name(WORKSPACE, &chosen),
        format!("workspace:resource:{WORKSPACE}")
    );
    chosen.insert(
        "item".to_string(),
        "urn:iki:ledger:default:item:3950".to_string(),
    );
    let name = workspace::face_name(WORKSPACE, &chosen);
    assert_eq!(
        name,
        format!(
            "workspace:select:item=urn%3Aiki%3Aledger%3Adefault%3Aitem%3A3950:resource:{WORKSPACE}"
        )
    );
    let (iri, back) = workspace::parse_face_name(name.strip_prefix("workspace:").unwrap())
        .unwrap()
        .unwrap();
    assert_eq!((iri.as_str(), back), (WORKSPACE, chosen));
    assert!(workspace::parse_face_name("select:item:resource:urn:x")
        .unwrap()
        .is_err());
    assert!(workspace::parse_face_name("nothing").is_none());
}

/// A selection that is not one absolute IRI is refused before anything is read.
#[test]
fn a_selection_is_one_iri() {
    let (kernel, seen) = workspace_host();
    for bad in ["not an iri", "urn:a{item}", ""] {
        assert!(
            workspace_screen(&kernel, &[("item", bad)]).is_err(),
            "{bad}"
        );
    }
    assert!(workspace_screen(&kernel, &[("Item!", "urn:x")]).is_err());
    assert!(seen.lock().unwrap().iter().all(|(v, _)| *v == Verb::Source));
}

// --- section 1: the atom ----------------------------------------------------------------------

#[test]
fn the_fixture_is_a_workspace() {
    let ws = workspace::validate(ITEM).unwrap();
    assert_eq!(ws.label, "Item 3941");
    assert_eq!(ws.panes.len(), 4);
    assert_eq!(
        workspace::reading_order(&ws.layout),
        ["dashboard", "session", "item", "trail"]
    );
    let Layout::Split { children, .. } = &ws.layout else {
        panic!("a split")
    };
    assert_eq!(children[0].share, 300);
}

#[test]
fn the_names() {
    let iri = workspace::name("urn:person:alice", "item").unwrap();
    assert_eq!(iri, WORKSPACE);
    assert_eq!(
        workspace::parse_name(&iri),
        Some(("urn:person:alice", "item"))
    );
    assert_eq!(workspace::version(&iri, 3), format!("{iri}:version:3"));
    assert_eq!(
        workspace::as_of(&iri, "2026-11-21T12:00:00Z").unwrap(),
        format!("{iri}:as-of:2026-11-21T12:00:00Z")
    );
    assert!(workspace::as_of(&iri, "yesterday").is_err());
    assert_eq!(
        workspace::screen(&iri),
        format!("urn:iki:ui:screen:workspace:{iri}")
    );
    assert!(workspace::name("urn:person:alice", "Item").is_err());
    assert!(workspace::name("alice bob", "item").is_err());
    assert_eq!(workspace::parse_name(&format!("{iri}:edit")), None);
    assert_eq!(workspace::SHAPE, "urn:iki:ui:shape:workspace");
    assert!(workspace::SHAPE_TTL.contains("sh:closed true"));
}

fn refused(json: serde_json::Value, says: &str) {
    let bytes = serde_json::to_vec(&json).unwrap();
    match workspace::validate(&bytes) {
        Err(Error::InvalidArgument { detail, .. }) => {
            assert!(detail.contains(says), "`{detail}` does not say `{says}`")
        }
        other => panic!("not refused ({says}): {other:?}"),
    }
}

fn pane(label: &str) -> serde_json::Value {
    serde_json::json!({"label": label, "content": "lens:item:resource:urn:x"})
}

#[test]
fn the_atom_is_closed_and_bounded() {
    let base: serde_json::Value = serde_json::from_slice(ITEM).unwrap();
    let with = |f: &dyn Fn(&mut serde_json::Value)| {
        let mut v = base.clone();
        f(&mut v);
        v
    };
    refused(
        with(&|v| v["theme"] = "dark".into()),
        "unknown field `theme`",
    );
    refused(
        with(&|v| v["panes"]["item"]["width"] = 300.into()),
        "unknown field `width`",
    );
    refused(
        with(&|v| v["layout"]["children"][0]["share"] = 400.into()),
        "add up to 1100",
    );
    refused(
        with(&|v| v["layout"]["children"][0]["share"] = 20.into()),
        "from 50 to 1000",
    );
    refused(
        with(&|v| v["layout"]["children"][0]["pane"] = "item".into()),
        "more than once",
    );
    refused(
        with(&|v| v["panes"]["extra"] = pane("Extra")),
        "`extra` is not in the layout",
    );
    refused(
        with(&|v| v["layout"]["children"][0]["pane"] = "ghost".into()),
        "not in `panes`",
    );
    refused(
        with(&|v| v["panes"]["trail"]["follows"] = serde_json::Value::Null),
        "a role it does not follow",
    );
    refused(
        with(&|v| v["panes"]["item"]["content"] = "https://example.com/".into()),
        "not a screen name",
    );
    refused(
        with(&|v| v["panes"]["item"]["label"] = " ".into()),
        "needs a `label`",
    );
    refused(
        with(&|v| v["layout"]["children"][1]["children"][1]["active"] = "dashboard".into()),
        "not one of its tabs",
    );
    refused(
        with(&|v| v["selection"]["item"] = "not an iri".into()),
        "not one absolute IRI",
    );
    refused(
        with(&|v| v["layout"]["share"] = 1000.into()),
        "root has no `share`",
    );

    // Bounds, said in words.
    let many: serde_json::Map<_, _> = (0..17).map(|i| (format!("p{i}"), pane("P"))).collect();
    refused(
        serde_json::json!({"label": "Many", "layout": {"pane": "p0"}, "panes": many}),
        "at most 16 panes",
    );
    let tabs: Vec<String> = (0..9).map(|i| format!("p{i}")).collect();
    let panes: serde_json::Map<_, _> = tabs.iter().map(|k| (k.clone(), pane("P"))).collect();
    refused(
        serde_json::json!({"label": "Tabs", "layout": {"tabs": tabs}, "panes": panes}),
        "1 to 8 panes",
    );
    let mut deep = serde_json::json!({"pane": "p4"});
    for i in (0..4).rev() {
        deep = serde_json::json!({"split": "row", "children": [
            {"share": 500, "pane": format!("p{i}")}, {"share": 500, "split": "row", "children": [
                {"share": 500, "pane": format!("q{i}")}, ({let mut d = deep.clone(); d["share"] = 500.into(); d})
            ]}
        ]});
    }
    let mut panes = serde_json::Map::new();
    for i in 0..5 {
        panes.insert(format!("p{i}"), pane("P"));
        panes.insert(format!("q{i}"), pane("Q"));
    }
    refused(
        serde_json::json!({"label": "Deep", "layout": deep, "panes": panes}),
        "at most 4 deep",
    );
}

// --- section 1 and decision 1: the workspace screen -------------------------------------------

#[test]
fn the_workspace_is_an_ordinary_screen() {
    let ws = workspace::validate(ITEM).unwrap();
    let screen = workspace::compose(&ws, WORKSPACE, 12, &BTreeMap::new()).unwrap();
    model::validate(&screen).unwrap();
    assert_eq!(screen.kind, "group");
    assert_eq!(text(&screen, "label"), "Item 3941");

    // The pane list first, naming every pane in reading order with a link to its region.
    let nav = &all(&screen, "nav")[0];
    assert_eq!(text(nav, "label"), "Panes");
    let hrefs: Vec<&str> = nav.children.iter().map(|l| text(l, "href")).collect();
    let page = format!("/workspace/{WORKSPACE}");
    assert_eq!(
        hrefs,
        ["dashboard", "session", "item", "trail"].map(|k| format!("{page}#pane-{k}"))
    );
    assert!(text(&nav.children[1], "label").contains("live"));

    // Splits and tabs are groups with the layout hints.
    let groups = all(&screen, "group");
    let split = |v: &str| Hint::Plain(Value::Text(v.into()));
    assert!(groups
        .iter()
        .any(|g| g.hint.get(workspace::SPLIT) == Some(&split("row 300 700"))));
    assert!(groups
        .iter()
        .any(|g| g.hint.get(workspace::SPLIT) == Some(&split("column 600 400"))));
    assert!(groups
        .iter()
        .any(|g| g.hint.get(workspace::TABS) == Some(&split("item"))));

    // Each pane is a region allowed to read only its content screen, its selection put in.
    let regions = all(&screen, "region");
    assert_eq!(regions.len(), 4);
    let session = regions
        .iter()
        .find(|r| text(r, "label") == "Session")
        .unwrap();
    let content = "urn:iki:ui:screen:name:lens:session:resource:urn:iki:ledger:default:item:3941";
    assert_eq!(text(session, "allow"), content);
    assert_eq!(text(all(session, "reference")[0], "resource"), content);
    assert_eq!(
        text(all(session, "link")[0], "href"),
        "/lens/name/session/resource/urn:iki:ledger:default:item:3941"
    );

    // The address's selection is over the saved one.
    let moved: BTreeMap<_, _> = [("item".to_string(), "urn:x:other".to_string())].into();
    let screen = workspace::compose(&ws, WORKSPACE, 12, &moved).unwrap();
    let refs: Vec<&str> = all(&screen, "reference")
        .iter()
        .map(|r| text(r, "resource"))
        .collect();
    assert!(refs.contains(&"urn:iki:ui:screen:name:lens:trail:resource:urn:x:other"));
    assert!(refs.contains(&"urn:iki:ui:screen:name:lens:dashboard:resource:urn:iki:ledger:default"));

    // The "Layout" form posts one resize to the edit Sink, with if-version.
    let form = &all(&screen, "form")[0];
    assert_eq!(text(form, "label"), "Layout");
    let fields: BTreeMap<&str, &str> = all(form, "field")
        .iter()
        .map(|f| (text(f, "name"), text(f, "value")))
        .collect();
    assert_eq!(fields["if-version"], "12");
    assert_eq!(fields["op"], "resize");
    assert_eq!(fields["share.0"], "300");
    assert_eq!(fields["share.1.1"], "400");
}

#[test]
fn a_pane_waiting_for_a_selection_says_so_and_reads_nothing() {
    let mut json: serde_json::Value = serde_json::from_slice(ITEM).unwrap();
    json.as_object_mut().unwrap().remove("selection");
    let ws = workspace::validate(&serde_json::to_vec(&json).unwrap()).unwrap();
    let screen = workspace::compose(&ws, WORKSPACE, 1, &BTreeMap::new()).unwrap();
    model::validate(&screen).unwrap();
    // Only the pinned dashboard references its content.
    assert_eq!(all(&screen, "reference").len(), 1);
    assert!(all(&screen, "text")
        .iter()
        .any(|t| text(t, "content").starts_with("Nothing is selected for item.")));
}

#[test]
fn a_panes_scopes_narrow_its_region() {
    let mut json: serde_json::Value = serde_json::from_slice(ITEM).unwrap();
    json["panes"]["trail"]["scopes"] = "urn:cap:ledger:read".into();
    let ws = workspace::validate(&serde_json::to_vec(&json).unwrap()).unwrap();
    let screen = workspace::compose(&ws, WORKSPACE, 1, &BTreeMap::new()).unwrap();
    let trail = all(&screen, "region")
        .into_iter()
        .find(|r| text(r, "label") == "Trail")
        .unwrap();
    assert_eq!(text(trail, "scopes"), "urn:cap:ledger:read");
}
