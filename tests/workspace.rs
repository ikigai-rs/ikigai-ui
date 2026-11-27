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

    // Every form that writes is an edit form (the "Layout" form, each pane's "Pane" menu, ledger
    // #3952, and its Collapse toggle, ledger #3953), posting to the edit Sink: 1 + 4 × (move,
    // tabs, close) + 4 collapse + the 2 reorders. The selection is in none of their fields. The
    // rest are the four Maximize toggles, GETs of the workspace's own address that keep the
    // selection and write nothing.
    let forms = all(&screen, "form");
    let (edits, views): (Vec<&Node>, Vec<&Node>) =
        forms.into_iter().partition(|f| text(f, "method") == "post");
    assert_eq!(edits.len(), 19);
    for form in edits {
        assert_eq!(text(form, "resource"), format!("{WORKSPACE}:edit"));
        for field in all(form, "field") {
            assert!(!text(field, "value").contains(chosen), "{field:?}");
        }
    }
    assert_eq!(views.len(), 4);
    for form in views {
        assert!(text(form, "label").starts_with("Maximize "), "{form:?}");
        assert!(text(form, "action").starts_with(&format!("/workspace/{WORKSPACE}#pane-")));
        assert!(all(form, "field")
            .iter()
            .any(|f| text(f, "value") == chosen));
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

    // The Layout form, the "Pane" menus' forms (ledger #3952) and the Collapse toggles (ledger
    // #3953): every one an edit. The four Maximize toggles are GETs that write nothing.
    assert_eq!(html.matches("<form").count(), 23, "{html}");
    assert_eq!(html.matches("method=\"get\"").count(), 4, "{html}");
    for (at, _) in html.match_indices("<form") {
        let form = &html[at..];
        let form = &form[..form.find("</form>").unwrap()];
        if form.contains("method=\"get\"") {
            assert!(form.contains("aria-pressed=\"false\">Maximize "), "{form}");
            continue;
        }
        assert!(!form.contains(chosen), "{form}");
        assert!(form.contains("name%3Aitem%3Aedit\""), "{form}");
        assert!(form.contains("name=\"if-version\""), "{form}");
    }
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
        r##"<div class="ui-share ui-share-300"><section class="ui-group ui-pane" id="pane-dashboard" aria-label="Dashboard" data-ui-drop="left right above below tabs" data-ui-collapsed="false">"##,
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
    let form = all(&screen, "form").into_iter().last().unwrap();
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

fn face_html() -> String {
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
    String::from_utf8(rep.bytes).unwrap()
}

/// The splitter (ledger #3951, section 5A "Resize by dragging"): one `role="separator"` between
/// each two children of a split, its value the first child's share, its range leaving each side
/// at least `MIN_SHARE`, its value said in words and naming the two panes it sizes (a tabs
/// group by its active pane, a split by its first). Not focusable in markup: the shell's script
/// puts it in the tab order, and with no script the "Layout" form sizes the panes.
#[test]
fn a_splitter_between_each_two_split_children() {
    let html = face_html();
    assert_eq!(html.matches("role=\"separator\"").count(), 2, "{html}");
    for part in [
        // The root row: Dashboard 300 beside the column split led by Session.
        r##"<div class="ui-share ui-share-300"><section class="ui-group ui-pane" id="pane-dashboard""##,
        r##"</div><div class="ui-splitter" role="separator" data-ui-splitter="0" aria-orientation="vertical" aria-label="Resize Dashboard and Session" aria-controls="pane-dashboard pane-session" aria-valuenow="300" aria-valuemin="50" aria-valuemax="950" aria-valuetext="Dashboard 30%, Session 70%"></div><div class="ui-share ui-share-700">"##,
        // The column: Session 600 above the tabs, whose active pane is Item.
        r##"<div class="ui-splitter" role="separator" data-ui-splitter="0" aria-orientation="horizontal" aria-label="Resize Session and Item" aria-controls="pane-session pane-item" aria-valuenow="600" aria-valuemin="50" aria-valuemax="950" aria-valuetext="Session 60%, Item 40%"></div><div class="ui-share ui-share-400"><div class="ui-tabs""##,
    ] {
        assert!(html.contains(part), "{part}\n{html}");
    }
    let separator = &html[html.find("role=\"separator\"").unwrap()..];
    assert!(!separator[..separator.find('>').unwrap()].contains("tabindex"));
}

fn args(pairs: &[(&str, &str)]) -> BTreeMap<String, String> {
    pairs
        .iter()
        .map(|(k, v)| (k.to_string(), v.to_string()))
        .collect()
}

fn shares(layout: &Layout) -> Vec<u32> {
    match layout {
        Layout::Split { children, .. } => children
            .iter()
            .flat_map(|c| std::iter::once(c.share).chain(shares(&c.node)))
            .collect(),
        _ => vec![],
    }
}

/// One `resize` edit (section 5A): the arguments the "Layout" form posts, and the same the
/// splitter sends at the end of a drag or a second after its last key. It moves shares and
/// nothing else, and a result that breaks a split's sum or the least share is refused.
#[test]
fn a_resize_edit_moves_shares_only() {
    let ws = workspace::validate(ITEM).unwrap();
    assert_eq!(shares(&ws.layout), [300, 700, 600, 400]);
    let done = workspace::resize(
        &ws,
        &args(&[
            ("op", "resize"),
            ("if-version", "7"),
            ("share.0", "450"),
            ("share.1", "550"),
            ("share.1.0", "500"),
            ("share.1.1", "500"),
        ]),
    )
    .unwrap();
    assert_eq!(shares(&done.layout), [450, 550, 500, 500]);
    assert_eq!(done.panes, ws.panes);
    assert_eq!(done.selection, ws.selection);
    assert_eq!(
        workspace::reading_order(&done.layout),
        workspace::reading_order(&ws.layout)
    );
    // A child not named keeps its share; the split must still add up.
    let one = workspace::resize(
        &ws,
        &args(&[("op", "resize"), ("share.1.0", "550"), ("share.1.1", "450")]),
    )
    .unwrap();
    assert_eq!(shares(&one.layout), [300, 700, 550, 450]);
    for (pairs, says) in [
        (vec![("op", "move")], "`move` is not a resize edit"),
        (vec![("share.0", "500")], "no `op`"),
        (
            vec![("op", "resize"), ("share.0", "400")],
            "add up to 1100, not 1000",
        ),
        (
            vec![("op", "resize"), ("share.1.0", "960"), ("share.1.1", "40")],
            "`share.1.1` is 40",
        ),
        (
            vec![("op", "resize"), ("share.0.0", "500")],
            "`share.0.0` names no split child",
        ),
        (
            vec![("op", "resize"), ("share.2", "500")],
            "`share.2` names no split child",
        ),
        (
            vec![("op", "resize"), ("share.0", "half")],
            "not a whole number",
        ),
        (
            vec![("op", "resize"), ("label", "Mine")],
            "`label` is not an argument",
        ),
    ] {
        let err = workspace::resize(&ws, &args(&pairs))
            .unwrap_err()
            .to_string();
        assert!(err.contains(says), "{pairs:?}: {err}");
    }
}

/// The "Layout" form's fields are a resize edit: posted as they stand, they change nothing.
#[test]
fn the_layout_form_is_a_resize_edit() {
    let ws = workspace::validate(ITEM).unwrap();
    let form = workspace::layout_form(&ws, WORKSPACE, 3);
    let posted: BTreeMap<String, String> = all(&form, "field")
        .into_iter()
        .map(|f| (text(f, "name").to_string(), text(f, "value").to_string()))
        .collect();
    assert_eq!(
        posted.keys().map(String::as_str).collect::<Vec<_>>(),
        [
            "if-version",
            "op",
            "share.0",
            "share.1",
            "share.1.0",
            "share.1.1"
        ]
    );
    assert_eq!(workspace::resize(&ws, &posted).unwrap(), ws);
}

// --- proposal 13, second half: a shared workspace shows each viewer only their own reads -----

const ALICE: &str = "urn:cap:test:person:alice";
const BOB: &str = "urn:cap:test:person:bob";
const TRAIL_READ: &str = "urn:cap:test:trail:read";

/// Every request seen, with the viewer it was read for.
type SeenBy = Arc<Mutex<Vec<(Verb, String, &'static str)>>>;

/// One atom, two viewers: every pane answers "{viewer}'s {pane}", from the capability it is
/// read under, and the trail pane needs a grant only Bob holds. Records every request with the
/// viewer it came from.
fn shared_host() -> (Kernel, SeenBy) {
    let seen: SeenBy = Arc::default();
    let who = |cap: &Capability| match (cap.allows(ALICE), cap.allows(BOB)) {
        (true, false) => "Alice",
        (false, true) => "Bob",
        _ => "nobody",
    };
    let pane = |id: &'static str, seen: SeenBy| {
        FnEndpoint::new(id, move |inv: &Invocation<'_>| {
            let viewer = who(inv.capability);
            let target = inv.request.target.as_str().to_string();
            seen.lock()
                .unwrap()
                .push((inv.request.verb, target.clone(), viewer));
            let lens = target
                .trim_start_matches("urn:iki:ui:screen:name:lens:")
                .split(':')
                .next()
                .unwrap_or_default()
                .to_string();
            let node = serde_json::json!({
                "kind": "text", "properties": {"content": format!("{viewer}'s {lens} pane.")}
            });
            Ok(Representation::new(
                ReprType::new(model::MEDIA_TYPE),
                serde_json::to_vec(&node).unwrap(),
            ))
        })
    };
    let described = |id: &'static str| {
        Description::new(id)
            .title(id)
            .summary("Answers the viewer's own reads.")
            .verb(Verb::Source)
            .verb(Verb::Meta)
            .output(model::MEDIA_TYPE)
    };
    let atom_seen = seen.clone();
    let space = EndpointSpace::new()
        .bind(
            Exact::new(WORKSPACE),
            FnEndpoint::new("atom", move |inv: &Invocation<'_>| {
                atom_seen.lock().unwrap().push((
                    inv.request.verb,
                    WORKSPACE.to_string(),
                    who(inv.capability),
                ));
                if inv.request.verb != Verb::Source {
                    return Err(Error::Denied("the atom is read only here".into()));
                }
                Ok(Representation::new(
                    ReprType::new("application/json"),
                    ITEM.to_vec(),
                ))
            })
            .with_description(
                Description::new("atom")
                    .title("atom")
                    .summary("Alice's workspace, shared with Bob.")
                    .verb(Verb::Source)
                    .verb(Verb::Sink)
                    .verb(Verb::Meta)
                    .output("application/json"),
            ),
        )
        .bind(
            UriTemplate::parse("urn:iki:ui:screen:name:lens:trail:{rest}").unwrap(),
            pane("trail", seen.clone()).with_description(described("trail").requires(TRAIL_READ)),
        )
        .bind(
            UriTemplate::parse("urn:iki:ui:screen:name:lens:{rest}").unwrap(),
            pane("panes", seen.clone()).with_description(described("panes")),
        );
    let root = Arc::new(ikigai_core::Fallback::new(vec![
        Arc::new(space) as Arc<dyn ikigai_core::Space>,
        common::root(None, Vec::new()),
    ]));
    (common::kernel(root), seen)
}

/// Proposal 13, second half (section 1, "Shared"): Alice's workspace, read by Alice and by Bob
/// under their own capabilities. Each sees the same layout filled with their own reads: every
/// pane is read under the viewer's capability and answers that viewer's content; the trail,
/// which only Bob may read, shows Bob's and is withheld from Alice; neither page carries a word
/// of the other's; and nobody writes the atom.
#[test]
fn a_shared_workspace_shows_each_viewer_only_their_own_reads() {
    let (kernel, seen) = shared_host();
    let face = format!(
        "urn:iki:ui:html:screen:{}",
        workspace::face_name(WORKSPACE, &BTreeMap::new())
    );
    let page = |scopes: &[&str]| {
        let rep = block_on(kernel.issue(
            Request::new(Verb::Source, Iri::parse(face.as_str()).unwrap()),
            &Capability::scoped(scopes.iter().copied()),
        ))
        .unwrap();
        String::from_utf8(rep.bytes).unwrap()
    };
    let alice = page(&["urn:cap:test:read", ALICE]);
    let bob = page(&["urn:cap:test:read", BOB, TRAIL_READ]);

    // The same layout for both: the same panes, in the same order, with the same splitters.
    let panes = |html: &str| -> Vec<String> {
        html.match_indices(" id=\"pane-")
            .map(|(i, _)| {
                let id = &html[i + 5..];
                id[..id.find('"').unwrap()].to_string()
            })
            .collect()
    };
    assert_eq!(
        panes(&alice),
        ["pane-dashboard", "pane-session", "pane-item", "pane-trail"],
        "{alice}"
    );
    assert_eq!(panes(&alice), panes(&bob));
    assert_eq!(
        alice.matches("role=\"separator\"").count(),
        bob.matches("role=\"separator\"").count()
    );

    // Each sees their own reads, and nothing of the other's.
    for lens in ["dashboard", "session", "item"] {
        assert!(
            alice.contains(&format!("Alice&#39;s {lens} pane.")),
            "{alice}"
        );
        assert!(bob.contains(&format!("Bob&#39;s {lens} pane.")), "{bob}");
    }
    assert!(bob.contains("Bob&#39;s trail pane."), "{bob}");
    assert!(
        !alice.contains("trail pane."),
        "Alice may not read the trail: {alice}"
    );
    assert!(!alice.contains("Bob"), "{alice}");
    assert!(!bob.contains("Alice"), "{bob}");

    // Every read was made under the viewer who asked, none under the other or anyone else; the
    // trail was never answered for Alice; and the atom was only read.
    let seen = seen.lock().unwrap();
    assert!(
        seen.iter().all(|(verb, _, _)| *verb == Verb::Source),
        "{seen:?}"
    );
    assert!(seen.iter().all(|(_, _, who)| *who != "nobody"), "{seen:?}");
    let atom_reads: Vec<_> = seen.iter().filter(|(_, iri, _)| iri == WORKSPACE).collect();
    assert!(atom_reads.iter().any(|(_, _, w)| *w == "Alice"));
    assert!(atom_reads.iter().any(|(_, _, w)| *w == "Bob"));
    assert!(
        !seen
            .iter()
            .any(|(_, iri, w)| iri.contains(":lens:trail:") && *w == "Alice"),
        "{seen:?}"
    );
}

// --- proposal 6: the `addable` filter (ledger #3953) -----------------------------------------

/// What the `{workspace}:addable` list offers in the addable host below.
const ADDABLE: &str = r#"[
  {"label": "Trail again", "content": "lens:trail:resource:{item}", "accepts": "item"},
  {"label": "Notes", "content": "lens:notes:resource:urn:test:notes"},
  {"label": "Retired", "content": "lens:retired:resource:urn:test:notes"},
  {"label": "Saved", "content": "urn:iki:ui:screen:name:saved-question"},
  {"label": "Unsaved", "content": "urn:iki:ui:screen:name:never-saved"}
]"#;

/// The selected item no viewer may read on its own: an `Exists` of it is denied to everyone,
/// so an offer checked against the selected IRI, not the screen it opens, would be left out.
const SELECTED: &str = "urn:test:item:secret";

/// [`shared_host`]'s two viewers with an `addable` list: the trail lens needs a grant only Bob
/// holds; `retired` answers `Exists` Ok with `false`; `saved-question` is a screen with a source
/// (compiled by ikigai-ui's `screens()`, placed last), `never-saved` one without; and the
/// selected item itself is denied to everyone. Records every request with its viewer.
fn addable_host() -> (Kernel, SeenBy) {
    let seen: SeenBy = Arc::default();
    let who = |cap: &Capability| match (cap.allows(ALICE), cap.allows(BOB)) {
        (true, false) => "Alice",
        (false, true) => "Bob",
        _ => "nobody",
    };
    let answering = |id: &'static str, seen: SeenBy, body: &'static str, media: &'static str| {
        FnEndpoint::new(id, move |inv: &Invocation<'_>| {
            seen.lock().unwrap().push((
                inv.request.verb,
                inv.request.target.as_str().to_string(),
                who(inv.capability),
            ));
            if id == "retired" && inv.request.verb == Verb::Exists {
                return Ok(Representation::new(
                    ReprType::new("text/plain"),
                    b"false".to_vec(),
                ));
            }
            if id == "selected" {
                return Err(Error::Denied("nobody reads the selected item alone".into()));
            }
            Ok(Representation::new(
                ReprType::new(media),
                body.as_bytes().to_vec(),
            ))
        })
        .with_description(
            Description::new(id)
                .title(id)
                .summary("Records what it is asked.")
                .verb(Verb::Source)
                .verb(Verb::Meta)
                .output(media),
        )
    };
    let pane = r#"{"kind": "text", "properties": {"content": "A pane."}}"#;
    let space = EndpointSpace::new()
        .bind(
            Exact::new(WORKSPACE),
            answering(
                "atom",
                seen.clone(),
                std::str::from_utf8(ITEM).unwrap(),
                "application/json",
            ),
        )
        .bind(
            Exact::new(workspace::addable_iri(WORKSPACE).as_str()),
            answering("addable", seen.clone(), ADDABLE, "application/json"),
        )
        .bind(
            Exact::new(SELECTED),
            answering("selected", seen.clone(), "", "application/json"),
        )
        .bind(
            Exact::new("urn:iki:ui:screen:name:saved-question:source"),
            answering(
                "saved",
                seen.clone(),
                "group label=Saved\n  text content=Saved\n",
                "text/plain",
            ),
        )
        .bind(
            UriTemplate::parse("urn:iki:ui:screen:name:lens:trail:{rest}").unwrap(),
            answering("trail", seen.clone(), pane, model::MEDIA_TYPE).with_description(
                Description::new("trail")
                    .title("trail")
                    .summary("Only Bob reads the trail.")
                    .verb(Verb::Source)
                    .verb(Verb::Meta)
                    .output(model::MEDIA_TYPE)
                    .requires(TRAIL_READ),
            ),
        )
        .bind(
            UriTemplate::parse("urn:iki:ui:screen:name:lens:retired:{rest}").unwrap(),
            answering("retired", seen.clone(), pane, model::MEDIA_TYPE),
        )
        .bind(
            UriTemplate::parse("urn:iki:ui:screen:name:lens:{rest}").unwrap(),
            answering("panes", seen.clone(), pane, model::MEDIA_TYPE),
        );
    let root = Arc::new(ikigai_core::Fallback::new(vec![
        Arc::new(space) as Arc<dyn ikigai_core::Space>,
        common::root(None, Vec::new()),
        Arc::new(ikigai_ui::screen::screens()),
    ]));
    (common::kernel(root), seen)
}

/// What the workspace screen's "Add pane" form offers to open, by label, under `scopes`.
fn offered_to(kernel: &Kernel, scopes: &[&str]) -> Vec<String> {
    let request = Request::new(
        Verb::Source,
        Iri::parse(workspace::screen(WORKSPACE)).unwrap(),
    )
    .with_arg("select.item", ArgRef::Inline(SELECTED.as_bytes().to_vec()));
    let rep = block_on(kernel.issue(request, &Capability::scoped(scopes.iter().copied()))).unwrap();
    let screen = model::parse(&rep.bytes).unwrap();
    all(&screen, "choice")
        .into_iter()
        .filter(|c| text(c, "name") == "content")
        .flat_map(|c| c.children.iter().map(|o| text(o, "label").to_string()))
        .collect()
}

/// Proposal 6: the `addable` filter never offers what the viewer cannot read, on a kernel
/// whose capability lacks that read. Alice holds no grant for the trail lens, so "Trail again"
/// is not offered to her and is to Bob, who holds it. Readability is asked of the screen an
/// entry would open, not of the selected IRI (denied to both, yet "Trail again" is Bob's). An
/// `Exists` answering Ok with `false` is not readable; a saved screen with no source is not
/// either; and a saved screen with one is, its `Exists` answered from its source's without a
/// compile. Every check is an `Exists`: nothing offered is rendered to decide.
#[test]
fn the_addable_filter_never_offers_what_the_viewer_cannot_read() {
    let (kernel, seen) = addable_host();
    // The saved screen is a real one: it compiles when read.
    let saved = block_on(kernel.issue(
        Request::new(
            Verb::Source,
            Iri::parse("urn:iki:ui:screen:name:saved-question").unwrap(),
        ),
        &Capability::scoped(["urn:cap:test:read", ALICE]),
    ))
    .unwrap();
    assert!(model::parse(&saved.bytes).is_ok());
    seen.lock().unwrap().clear();
    let alice = offered_to(&kernel, &["urn:cap:test:read", ALICE]);
    assert_eq!(alice, ["Notes", "Saved"], "Alice may not read the trail");
    let bob = offered_to(&kernel, &["urn:cap:test:read", BOB, TRAIL_READ]);
    assert_eq!(bob, ["Trail again", "Notes", "Saved"]);

    let seen = seen.lock().unwrap();
    // The trail was asked of the screen it opens, by Exists, and never for Alice past the
    // floor (her Exists was denied before the endpoint ran).
    let trail = format!("urn:iki:ui:screen:name:lens:trail:resource:{SELECTED}");
    assert!(
        seen.iter()
            .any(|(v, iri, w)| *v == Verb::Exists && *iri == trail && *w == "Bob"),
        "{seen:?}"
    );
    assert!(
        !seen
            .iter()
            .any(|(_, iri, w)| iri.contains(":lens:trail:") && *w == "Alice"),
        "{seen:?}"
    );
    // The selected IRI is not what was asked.
    assert!(!seen.iter().any(|(_, iri, _)| iri == SELECTED), "{seen:?}");
    // No offered screen was read with Source to decide: only Exists, and the saved screen's
    // source only by Exists (no compile).
    for (verb, iri, _) in seen.iter() {
        if iri.starts_with("urn:iki:ui:screen:name:") {
            assert_eq!(*verb, Verb::Exists, "{iri} was read to decide: {seen:?}");
        }
    }
    assert!(
        seen.iter().any(|(v, iri, _)| *v == Verb::Exists
            && iri == "urn:iki:ui:screen:name:saved-question:source"),
        "{seen:?}"
    );
}

/// The Layout form's hidden fields (`op`, `if-version`) are hidden inputs and nothing else: no
/// visible `<label>`, no hint or note, so a sighted keyboard user and a screen reader meet no
/// label without a control. The visible share fields keep theirs.
#[test]
fn a_hidden_field_renders_no_label() {
    let html = face_html();
    // The Layout form, the page's last.
    let form = &html[html.rfind("<form").unwrap()..];
    let form = &form[..form.find("</form>").unwrap()];
    assert!(!form.contains(">Edit</label>"), "{form}");
    assert!(!form.contains(">Made against version</label>"), "{form}");
    assert!(
        form.contains("<input type=\"hidden\" name=\"op\" value=\"resize\">"),
        "{form}"
    );
    assert!(
        form.contains("<input type=\"hidden\" name=\"if-version\" value=\"1\">"),
        "{form}"
    );
    // Every label left names a control that is there.
    for (i, _) in form.match_indices("<label for=\"") {
        let id = &form[i + 12..];
        let id = &id[..id.find('"').unwrap()];
        assert!(form.contains(&format!("id=\"{id}\"")), "{id}: {form}");
        assert!(!form.contains(&format!("id=\"{id}\" name=\"op\"")));
    }
    assert_eq!(form.matches("type=\"number\"").count(), 4, "{form}");
}

// --- the workspace as a page (ledger #3967, section 1: `{base}page:workspace:{iri}`) ---------

fn page(kernel: &Kernel, name: &str, args: &[(&str, &str)]) -> Result<String, Error> {
    let mut request = Request::new(Verb::Source, Iri::parse(name).unwrap());
    for (k, v) in args {
        request = request.with_arg(*k, ArgRef::Inline(v.as_bytes().to_vec()));
    }
    let rep = block_on(kernel.issue(request, &Capability::scoped(["urn:cap:test:read"])))?;
    Ok(String::from_utf8(rep.bytes).unwrap())
}

/// `Pages::workspaces(true)` serves `{base}page:workspace:{iri}`: the shell (the family's
/// header, the theme, the status region) over the workspace's html face, titled by the atom's
/// label. The IRI may come percent-encoded, as a door's one segment; `select.{role}=` is the
/// selection; the theme is the shell's.
#[test]
fn a_workspace_is_a_page_in_the_shell() {
    let (kernel, seen) = face_host();
    let html = page(
        &kernel,
        &format!("urn:demo:page:workspace:{WORKSPACE}"),
        &[],
    )
    .unwrap();
    assert!(html.starts_with("<!doctype html>"), "{html}");
    assert!(html.contains("<title>Item 3941</title>"), "{html}");
    assert!(
        html.contains("<div id=\"status\" role=\"status\""),
        "{html}"
    );
    assert!(html.contains("<nav aria-label=\"Site\">"), "{html}");
    let main = &html[html.find("<main id=\"main\" tabindex=\"-1\">").unwrap()..];
    assert!(main.contains("<h1>Item 3941</h1>"), "{main}");
    assert!(main.contains("<div role=\"tablist\""), "{main}");
    assert!(main.contains("role=\"separator\""), "{main}");
    assert!(main.contains("<nav aria-label=\"Panes\">"), "{main}");
    // The "Panes" list links to this page's own address.
    assert!(
        main.contains(&format!("href=\"/workspace/{WORKSPACE}#pane-trail\"")),
        "{main}"
    );
    assert!(seen
        .lock()
        .unwrap()
        .iter()
        .all(|(verb, _)| *verb == Verb::Source));

    // Percent-encoded, as a door may hand it over: the same page.
    let encoded = WORKSPACE.replace(':', "%3A");
    let same = page(&kernel, &format!("urn:demo:page:workspace:{encoded}"), &[]).unwrap();
    assert_eq!(same, html);

    // The selection from the address, and the theme.
    let chosen = "urn:iki:ledger:default:item:3950";
    let selected = page(
        &kernel,
        &format!("urn:demo:page:workspace:{WORKSPACE}"),
        &[("select.item", chosen), ("theme", "dark")],
    )
    .unwrap();
    assert!(selected.contains("data-theme=\"dark\""), "{selected}");
    assert!(
        selected.contains(&format!("Session: follows item: {chosen}")),
        "{selected}"
    );
    assert!(!selected.contains("Session: follows item: urn:iki:ledger:default:item:3941"));
}

/// A workspace page refuses, before reading anything, an IRI that is not a workspace or that
/// carries what would leave the name; and a family that does not turn workspaces on has none.
#[test]
fn a_workspace_page_refuses_what_is_not_a_workspace() {
    let (kernel, seen) = face_host();
    for bad in [
        "urn:iki:ledger:default:item:3941",
        "urn:iki:ui:workspace:person:urn:person:alice:name:item%3Fx",
        "urn:iki:ui:workspace:person:urn:person:alice:name:item%23x",
    ] {
        let refused = page(&kernel, &format!("urn:demo:page:workspace:{bad}"), &[]).unwrap_err();
        assert!(
            matches!(refused, Error::InvalidArgument { .. }),
            "{bad}: {refused:?}"
        );
    }
    assert!(seen.lock().unwrap().is_empty(), "nothing was read");

    let off = ikigai_ui::Pages::new("plain", "urn:plain:")
        .build()
        .unwrap();
    let kernel = common::kernel(Arc::new(ikigai_core::Fallback::new(vec![
        Arc::new(ikigai_ui::space()) as Arc<dyn ikigai_core::Space>,
        Arc::new(off),
    ])));
    assert!(page(
        &kernel,
        &format!("urn:plain:page:workspace:{WORKSPACE}"),
        &[]
    )
    .is_err());
}

// --- workspace 3: rearrange and tabs (ledger #3952, section 5A "Rearrange") -------------------

/// The layout of an atom as JSON, to compare whole trees.
fn tree(ws: &workspace::Workspace) -> serde_json::Value {
    let json: serde_json::Value = serde_json::from_slice(&workspace::atom(ws)).unwrap();
    json["layout"].clone()
}

fn moved(
    ws: &workspace::Workspace,
    pane: &str,
    to: &str,
    place: &str,
) -> (workspace::Workspace, String) {
    workspace::apply(
        ws,
        &args(&[
            ("op", "move"),
            ("if-version", "1"),
            ("pane", pane),
            ("to", to),
            ("place", place),
        ]),
    )
    .unwrap()
}

/// The atom a host stores after an edit reads back as the same workspace.
#[test]
fn the_atom_round_trips() {
    let ws = workspace::validate(ITEM).unwrap();
    assert_eq!(workspace::validate(&workspace::atom(&ws)).unwrap(), ws);
}

/// Each of the five places, from the fixture (Dashboard 300 beside a column of Session 600 over
/// the tabs Item and Trail): beside a pane in a split of the same axis it takes half that pane's
/// share; across the axis the two make a new split; out of tabs it leaves its group (a group of
/// one becomes the pane alone); into tabs it is the active tab. Only the layout changes.
#[test]
fn a_move_to_each_place() {
    let ws = workspace::validate(ITEM).unwrap();
    use serde_json::json;
    let cases = [
        (
            ("session", "dashboard", "right"),
            "Moved Session to the right of Dashboard.",
            json!({"split": "row", "children": [
                {"share": 150, "pane": "dashboard"},
                {"share": 150, "pane": "session"},
                {"share": 700, "tabs": ["item", "trail"], "active": "item"}
            ]}),
        ),
        (
            ("trail", "session", "left"),
            "Moved Trail to the left of Session.",
            json!({"split": "row", "children": [
                {"share": 300, "pane": "dashboard"},
                {"share": 700, "split": "column", "children": [
                    {"share": 600, "split": "row", "children": [
                        {"share": 500, "pane": "trail"}, {"share": 500, "pane": "session"}
                    ]},
                    {"share": 400, "pane": "item"}
                ]}
            ]}),
        ),
        (
            ("item", "session", "below"),
            "Moved Item below Session.",
            json!({"split": "row", "children": [
                {"share": 300, "pane": "dashboard"},
                {"share": 700, "split": "column", "children": [
                    {"share": 300, "pane": "session"},
                    {"share": 300, "pane": "item"},
                    {"share": 400, "pane": "trail"}
                ]}
            ]}),
        ),
        (
            ("item", "dashboard", "above"),
            "Moved Item above Dashboard.",
            json!({"split": "row", "children": [
                {"share": 300, "split": "column", "children": [
                    {"share": 500, "pane": "item"}, {"share": 500, "pane": "dashboard"}
                ]},
                {"share": 700, "split": "column", "children": [
                    {"share": 600, "pane": "session"},
                    {"share": 400, "pane": "trail"}
                ]}
            ]}),
        ),
        (
            ("dashboard", "trail", "tabs"),
            "Put Dashboard in tabs with Trail.",
            json!({"split": "column", "children": [
                {"share": 600, "pane": "session"},
                {"share": 400, "tabs": ["item", "trail", "dashboard"], "active": "dashboard"}
            ]}),
        ),
        (
            ("session", "dashboard", "tabs"),
            "Put Session in tabs with Dashboard.",
            json!({"split": "row", "children": [
                {"share": 300, "tabs": ["dashboard", "session"], "active": "session"},
                {"share": 700, "tabs": ["item", "trail"], "active": "item"}
            ]}),
        ),
    ];
    for ((pane, to, place), sentence, want) in cases {
        let (done, said) = moved(&ws, pane, to, place);
        assert_eq!(said, sentence);
        assert_eq!(tree(&done), want, "{pane} {place} {to}");
        assert_eq!(done.panes, ws.panes, "a move changes no pane");
        assert_eq!(done.selection, ws.selection, "a move changes no selection");
        assert_eq!(done.label, ws.label);
    }
}

fn eight_tabs() -> workspace::Workspace {
    let keys: Vec<String> = (1..=8).map(|n| format!("t{n}")).collect();
    let mut panes = serde_json::Map::new();
    for k in keys
        .iter()
        .map(String::as_str)
        .chain(["solo", "deep", "t-x"])
    {
        panes.insert(k.to_string(), pane(k));
    }
    // `deep` is four deep: root row, column, row, pane.
    let json = serde_json::json!({
        "label": "Bounds",
        "layout": {"split": "row", "children": [
            {"share": 500, "tabs": keys, "active": "t1"},
            {"share": 500, "split": "column", "children": [
                {"share": 500, "pane": "solo"},
                {"share": 500, "split": "row", "children": [
                    {"share": 500, "pane": "deep"},
                    {"share": 500, "pane": "t-x"}
                ]}
            ]}
        ]},
        "panes": panes,
    });
    workspace::validate(&serde_json::to_vec(&json).unwrap()).unwrap()
}

/// What would break a bound is not offered for a drop and is refused by the edit, saying why
/// in words: a ninth tab, and a split that would nest the layout five deep.
#[test]
fn an_over_bound_place_is_not_offered_and_is_refused() {
    let ws = eight_tabs();
    // The full tab group takes no ninth tab; its edges are still offered.
    assert_eq!(
        workspace::offered(&ws, "t3"),
        ["left", "right", "above", "below"]
    );
    // `deep` is four deep in a row: left and right sit beside it, above and below would nest
    // a fifth level.
    assert_eq!(workspace::offered(&ws, "deep"), ["left", "right", "tabs"]);
    assert_eq!(
        workspace::offered(&ws, "solo"),
        ["left", "right", "above", "below", "tabs"]
    );
    let refuse = |pairs: &[(&str, &str)]| {
        let mut all = vec![("op", "move"), ("if-version", "1")];
        all.extend_from_slice(pairs);
        format!("{:?}", workspace::apply(&ws, &args(&all)).unwrap_err())
    };
    let says = refuse(&[("pane", "solo"), ("to", "t1"), ("place", "tabs")]);
    assert!(says.contains("A tab group holds at most 8 panes"), "{says}");
    let says = refuse(&[("pane", "t1"), ("to", "deep"), ("place", "below")]);
    assert!(says.contains("A layout nests at most 4 deep"), "{says}");
    let says = refuse(&[("pane", "solo"), ("to", "solo"), ("place", "left")]);
    assert!(
        says.contains("solo cannot be moved beside itself"),
        "{says}"
    );
    let says = refuse(&[("pane", "t1"), ("to", "t2"), ("place", "tabs")]);
    assert!(says.contains("t1 is already in tabs with t2"), "{says}");
    let says = refuse(&[("pane", "nobody"), ("to", "t2"), ("place", "tabs")]);
    assert!(says.contains("there is no pane `nobody`"), "{says}");
    let says = refuse(&[("pane", "solo"), ("to", "t2"), ("place", "middle")]);
    assert!(says.contains("`middle` is not a place"), "{says}");
    let says = refuse(&[
        ("pane", "solo"),
        ("to", "t2"),
        ("place", "tabs"),
        ("label", "Mine"),
    ]);
    assert!(
        says.contains("`label` is not an argument of a move edit"),
        "{says}"
    );
    // Right of `deep` is fine: it takes half of deep's share.
    let (done, _) = moved(&ws, "solo", "deep", "right");
    assert_eq!(
        workspace::reading_order(&done.layout)[8..],
        ["deep", "solo", "t-x"]
    );
}

/// A `reorder` moves a tab within its group and makes it the active tab.
#[test]
fn a_reorder_moves_a_tab_along_its_list() {
    let ws = workspace::validate(ITEM).unwrap();
    let (done, said) = workspace::apply(
        &ws,
        &args(&[
            ("op", "reorder"),
            ("if-version", "1"),
            ("pane", "trail"),
            ("index", "0"),
        ]),
    )
    .unwrap();
    assert_eq!(said, "Moved Trail to tab 1 of 2.");
    assert_eq!(
        tree(&done)["children"][1]["children"][1],
        serde_json::json!({"share": 400, "tabs": ["trail", "item"], "active": "trail"})
    );
    for (pairs, says) in [
        (
            vec![("pane", "session"), ("index", "0")],
            "Session is not in tabs",
        ),
        (vec![("pane", "trail"), ("index", "2")], "there is no tab 3"),
        (
            vec![("pane", "trail"), ("index", "first")],
            "`index` is not a whole number",
        ),
    ] {
        let mut all = vec![("op", "reorder")];
        all.extend(pairs);
        let err = format!("{:?}", workspace::apply(&ws, &args(&all)).unwrap_err());
        assert!(err.contains(says), "{err}");
    }
    // A close is an edit now (ledger #3953), refused here only for naming no pane.
    let err = format!(
        "{:?}",
        workspace::apply(&ws, &args(&[("op", "close")])).unwrap_err()
    );
    assert!(err.contains("the edit has no `pane`"), "{err}");
    let err = format!(
        "{:?}",
        workspace::apply(&ws, &args(&[("op", "split")])).unwrap_err()
    );
    assert!(
        err.contains("`split` is not an edit this workspace makes"),
        "{err}"
    );
}

/// The "Pane" menu (section 5A "Equivalents"): on every pane, the edit forms that do what a drag
/// does. "Move…" lists the other panes and the five places; "Put in tabs with…" lists the panes
/// not already in its group; a tab has "Move tab earlier" or "later" where there is room. Every
/// form posts to the edit Sink with the version, and its fields are the edit a drag sends.
#[test]
fn every_pane_has_a_pane_menu_of_edits() {
    let ws = workspace::validate(ITEM).unwrap();
    let screen = workspace::compose(&ws, WORKSPACE, 12, &BTreeMap::new()).unwrap();
    model::validate(&screen).unwrap();
    let menus: Vec<&Node> = all(&screen, "group")
        .into_iter()
        .filter(|g| g.hint.contains_key(workspace::MENU))
        .collect();
    assert_eq!(menus.len(), 4);
    let fields = |form: &Node| -> BTreeMap<String, String> {
        form.children
            .iter()
            .filter(|c| c.kind == "field")
            .map(|c| (text(c, "name").to_string(), text(c, "value").to_string()))
            .collect()
    };
    let options = |form: &Node, name: &str| -> Vec<String> {
        all(form, "choice")
            .into_iter()
            .filter(|c| text(c, "name") == name)
            .flat_map(|c| c.children.iter().map(|o| text(o, "value").to_string()))
            .collect()
    };
    let menu = |label: &str| *menus.iter().find(|m| text(m, "label") == label).unwrap();
    let forms = |m: &Node| -> Vec<String> {
        m.children
            .iter()
            .filter(|c| c.kind == "form")
            .map(|f| text(f, "label").to_string())
            .collect()
    };
    assert_eq!(
        forms(menu("Dashboard")),
        [
            "Move Dashboard…",
            "Put Dashboard in tabs with…",
            "Close Dashboard"
        ]
    );
    assert_eq!(
        forms(menu("Item")),
        [
            "Move Item…",
            "Put Item in tabs with…",
            "Move Item tab later",
            "Close Item"
        ]
    );
    assert_eq!(
        forms(menu("Trail")),
        [
            "Move Trail…",
            "Put Trail in tabs with…",
            "Move Trail tab earlier",
            "Close Trail"
        ]
    );
    let item = menu("Item");
    let mv = &item.children[0];
    assert_eq!(text(mv, "resource"), workspace::edit(WORKSPACE));
    assert_eq!(text(mv, "method"), "post");
    assert_eq!(fields(mv)["if-version"], "12");
    assert_eq!(fields(mv)["op"], "move");
    assert_eq!(fields(mv)["pane"], "item");
    assert_eq!(options(mv, "to"), ["dashboard", "session", "trail"]);
    assert_eq!(options(mv, "place"), workspace::PLACES);
    // Into tabs: not Trail, already in Item's group.
    assert_eq!(options(&item.children[1], "to"), ["dashboard", "session"]);
    assert_eq!(fields(&item.children[2])["index"], "1");

    // The Move form's fields, with a choice made, are the edit the drag sends.
    let mut posted = fields(mv);
    posted.insert("to".into(), "dashboard".into());
    posted.insert("place".into(), "below".into());
    assert_eq!(
        workspace::apply(&ws, &posted).unwrap(),
        moved(&ws, "item", "dashboard", "below")
    );

    // Each pane's region says where a drop may land.
    let regions = all(&screen, "region");
    for r in regions {
        assert_eq!(
            r.hint.get(workspace::DROP),
            Some(&Hint::Plain(Value::Text(
                "left right above below tabs".into()
            )))
        );
    }

    // A full tab group is said in words in "Put in tabs with…", not offered.
    let bounds = eight_tabs();
    let screen = workspace::compose(&bounds, WORKSPACE, 1, &BTreeMap::new()).unwrap();
    let solo = all(&screen, "group")
        .into_iter()
        .find(|g| g.hint.contains_key(workspace::MENU) && text(g, "label") == "solo")
        .unwrap();
    let tabs = &solo.children[1];
    assert_eq!(options(tabs, "to"), ["deep", "t-x"]);
    assert!(text(tabs, "description").contains("A tab group holds at most 8 panes"));
}

/// The html face: each pane's section carries its drop places, and its "Pane" menu is a
/// `<details>` whose summary is named for the pane; its forms post to the edit Sink.
#[test]
fn the_html_face_renders_the_pane_menu() {
    let html = face_html();
    assert!(
        html.contains(
            r#"id="pane-session" aria-label="Session" data-ui-drop="left right above below tabs" data-ui-collapsed="false">"#
        ),
        "{html}"
    );
    assert!(html.contains(
        r#"<details class="ui-pane-menu" data-ui-pane-menu="session"><summary>Pane<span class="ui-sr-only"> menu: Session</span></summary>"#
    ));
    assert_eq!(html.matches("<details class=\"ui-pane-menu\"").count(), 4);
    assert!(html.contains("<option value=\"tabs\">Into tabs with it</option>"));
}

/// A host's store may say the version in the atom's type; the edit forms carry it.
#[test]
fn the_version_the_store_answers_is_the_forms_if_version() {
    let atom = FnEndpoint::new("atom", |_: &Invocation<'_>| {
        Ok(Representation::new(
            ReprType::new("application/json").with_param("version", "7"),
            ITEM.to_vec(),
        ))
    });
    let space = EndpointSpace::new().bind(Exact::new(WORKSPACE), atom);
    let kernel = common::host_with(None, vec![Arc::new(space)]);
    let screen = Iri::parse(workspace::screen(WORKSPACE)).unwrap();
    let rep =
        block_on(kernel.issue(Request::new(Verb::Source, screen), &Capability::root())).unwrap();
    let node: Node = serde_json::from_slice(&rep.bytes).unwrap();
    let versions: Vec<&str> = all(&node, "field")
        .into_iter()
        .filter(|f| text(f, "name") == "if-version")
        .map(|f| text(f, "value"))
        .collect();
    assert!(versions.len() > 4);
    assert!(versions.iter().all(|v| *v == "7"), "{versions:?}");
}
