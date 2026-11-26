//! Workspace 4 (ledger #3953, `docs/design/workspace.md` section 5A "Collapse, maximize and
//! restore" and "Close and add", section 7 proposal 6): the `collapse`, `expand`, `close` and
//! `add` edits, maximize in the address, and the forms and links that make each by keyboard
//! and by one pointer with no script.
use std::collections::BTreeMap;

use ikigai_ui::model::{Node, Value};
use ikigai_ui::workspace::{self, Addable, Layout};

const ITEM: &[u8] = include_bytes!("fixtures/workspace/item.json");
const WS: &str = "urn:iki:ui:workspace:person:urn:person:alice:name:item";

fn args(pairs: &[(&str, &str)]) -> BTreeMap<String, String> {
    pairs
        .iter()
        .map(|(k, v)| (k.to_string(), v.to_string()))
        .collect()
}

fn item() -> workspace::Workspace {
    workspace::validate(ITEM).unwrap()
}

fn list() -> Vec<Addable> {
    workspace::addable(
        br#"[{"label": "Review", "content": "lens:review:resource:{item}", "accepts": "item"},
             {"label": "Ledger", "content": "lens:dashboard:resource:urn:iki:ledger:other"}]"#,
    )
    .unwrap()
}

fn walk<'a>(n: &'a Node, out: &mut Vec<&'a Node>) {
    out.push(n);
    for c in &n.children {
        walk(c, out);
    }
}

fn nodes(n: &Node) -> Vec<&Node> {
    let mut out = Vec::new();
    walk(n, &mut out);
    out
}

fn prop<'a>(n: &'a Node, key: &str) -> &'a str {
    match n.properties.get(key) {
        Some(Value::Text(s)) => s,
        _ => "",
    }
}

fn labels(n: &Node, kind: &str) -> Vec<String> {
    nodes(n)
        .into_iter()
        .filter(|c| c.kind == kind)
        .map(|c| prop(c, "label").to_string())
        .collect()
}

fn regions(n: &Node) -> Vec<String> {
    labels(n, ikigai_ui::region::KIND)
}

#[test]
fn before_this_item_the_edits_were_refused() {
    // The claim reproduced: `apply` (with no addable list) still refuses an `add`, and every
    // other new op is now an edit.
    let ws = item();
    let e = workspace::apply(
        &ws,
        &args(&[("op", "add"), ("content", "lens:x:resource:urn:x")]),
    )
    .unwrap_err();
    assert!(
        format!("{e:?}").contains("not something this workspace can open"),
        "{e:?}"
    );
    for op in ["collapse", "close"] {
        workspace::apply(&ws, &args(&[("op", op), ("pane", "dashboard")])).unwrap();
    }
}

#[test]
fn collapse_keeps_the_share_and_expand_gives_it_back() {
    let ws = item();
    let (collapsed, said) =
        workspace::apply(&ws, &args(&[("op", "collapse"), ("pane", "dashboard")])).unwrap();
    assert_eq!(said, "Collapsed Dashboard.");
    let Layout::Split { children, .. } = &collapsed.layout else {
        panic!()
    };
    assert!(children[0].collapsed);
    assert_eq!(children[0].share, 300, "it keeps its share");
    // Saved: the atom says `collapsed`, and reads back the same.
    let bytes = workspace::atom(&collapsed);
    assert!(String::from_utf8_lossy(&bytes).contains("\"collapsed\":true"));
    assert_eq!(workspace::validate(&bytes).unwrap(), collapsed);
    // Composed: the pane keeps its heading and loses its content; its menu says Expand.
    let screen = workspace::compose(&collapsed, WS, 2, &BTreeMap::new()).unwrap();
    let forms = labels(&screen, "form");
    assert!(forms.contains(&"Expand Dashboard".to_string()), "{forms:?}");
    assert!(!forms.contains(&"Collapse Dashboard".to_string()));
    let (back, said) = workspace::apply(
        &collapsed,
        &args(&[("op", "expand"), ("pane", "dashboard")]),
    )
    .unwrap();
    assert_eq!(said, "Expanded Dashboard.");
    assert_eq!(back, ws);
    // A tab collapses its whole tab group.
    let (tabs, _) = workspace::apply(&ws, &args(&[("op", "collapse"), ("pane", "trail")])).unwrap();
    assert!(String::from_utf8_lossy(&workspace::atom(&tabs)).contains("\"collapsed\":true"));
}

#[test]
fn collapse_is_refused_where_nothing_would_take_the_space() {
    let ws = item();
    let (one, _) =
        workspace::apply(&ws, &args(&[("op", "collapse"), ("pane", "session")])).unwrap();
    // Item is now the last open child of its split: it stays open, and its menu offers no
    // Collapse.
    let e = workspace::apply(&one, &args(&[("op", "collapse"), ("pane", "item")])).unwrap_err();
    assert!(format!("{e:?}").contains("one stays open"), "{e:?}");
    let screen = workspace::compose(&one, WS, 2, &BTreeMap::new()).unwrap();
    assert!(!labels(&screen, "form").contains(&"Collapse Item".to_string()));
    let e = workspace::apply(&one, &args(&[("op", "collapse"), ("pane", "session")])).unwrap_err();
    assert!(format!("{e:?}").contains("already collapsed"), "{e:?}");
    let e = workspace::apply(&ws, &args(&[("op", "expand"), ("pane", "session")])).unwrap_err();
    assert!(format!("{e:?}").contains("not collapsed"), "{e:?}");
    let e =
        workspace::apply(&ws, &args(&[("op", "collapse"), ("pane", "x"), ("y", "z")])).unwrap_err();
    assert!(format!("{e:?}").contains("`y` is not an argument"), "{e:?}");
}

#[test]
fn close_takes_the_pane_out_and_names_the_version_that_has_it() {
    let ws = item();
    let edited = workspace::apply_with(
        &ws,
        &args(&[("op", "close"), ("pane", "trail"), ("if-version", "12")]),
        &[],
    )
    .unwrap();
    assert_eq!(
        edited.said,
        "Closed Trail. Revert to version 12 brings it back."
    );
    assert!(!edited.workspace.panes.contains_key("trail"));
    assert_eq!(
        workspace::reading_order(&edited.workspace.layout),
        ["dashboard", "session", "item"]
    );
    // Focus goes to the pane that took its place.
    assert_eq!(edited.pane.as_deref(), Some("item"));
    // The first pane's heir is the one after it.
    let first =
        workspace::apply_with(&ws, &args(&[("op", "close"), ("pane", "dashboard")]), &[]).unwrap();
    assert_eq!(first.pane.as_deref(), Some("session"));
    assert_eq!(first.said, "Closed Dashboard.");
    // Down to one pane: that one cannot be closed, and its menu offers no Close.
    let mut w = ws.clone();
    for k in ["dashboard", "session", "trail"] {
        w = workspace::apply(&w, &args(&[("op", "close"), ("pane", k)]))
            .unwrap()
            .0;
    }
    let e = workspace::apply(&w, &args(&[("op", "close"), ("pane", "item")])).unwrap_err();
    assert!(format!("{e:?}").contains("a workspace keeps one"), "{e:?}");
    let screen = workspace::compose(&w, WS, 5, &BTreeMap::new()).unwrap();
    assert!(!labels(&screen, "form")
        .iter()
        .any(|l| l.starts_with("Close")));
}

#[test]
fn add_opens_only_what_the_list_offers() {
    let ws = item();
    let list = list();
    let edited = workspace::apply_with(
        &ws,
        &args(&[
            ("op", "add"),
            (
                "content",
                "lens:review:resource:urn:iki:ledger:default:item:7",
            ),
            ("to", "session"),
            ("place", "right"),
        ]),
        &list,
    )
    .unwrap();
    assert_eq!(edited.said, "Opened Review to the right of Session.");
    assert_eq!(edited.pane.as_deref(), Some("review"));
    let pane = &edited.workspace.panes["review"];
    assert_eq!(
        pane.content,
        "lens:review:resource:urn:iki:ledger:default:item:7"
    );
    assert!(pane.follows.is_none(), "an added pane is pinned");
    assert_eq!(
        workspace::reading_order(&edited.workspace.layout),
        ["dashboard", "session", "review", "item", "trail"]
    );
    // A second one gets its own key.
    let again = workspace::apply_with(
        &edited.workspace,
        &args(&[("op", "add"), ("content", "lens:review:resource:urn:x:y")]),
        &list,
    )
    .unwrap();
    assert_eq!(again.pane.as_deref(), Some("review-2"));
    // Anything the list does not offer is refused: another lens, a placeholder left in, or a
    // second IRI smuggled in.
    for content in [
        "lens:secret:resource:urn:x:y",
        "lens:review:resource:{item}",
        "lens:review:resource:urn:x y",
        "lens:dashboard:resource:urn:iki:ledger:default",
    ] {
        let e = workspace::apply_with(&ws, &args(&[("op", "add"), ("content", content)]), &list)
            .unwrap_err();
        assert!(
            format!("{e:?}").contains("not something"),
            "{content}: {e:?}"
        );
    }
    // At the bound: refused, saying so.
    let mut w = ws.clone();
    while w.panes.len() < workspace::MAX_PANES {
        w = workspace::apply_with(
            &w,
            &args(&[
                ("op", "add"),
                ("content", "lens:dashboard:resource:urn:iki:ledger:other"),
                ("place", "tabs"),
                ("to", "dashboard"),
            ]),
            &list,
        )
        .or_else(|_| {
            workspace::apply_with(
                &w,
                &args(&[
                    ("op", "add"),
                    ("content", "lens:dashboard:resource:urn:iki:ledger:other"),
                    ("place", "tabs"),
                    ("to", "session"),
                ]),
                &list,
            )
        })
        .unwrap()
        .workspace;
    }
    let e = workspace::apply_with(
        &w,
        &args(&[
            ("op", "add"),
            ("content", "lens:dashboard:resource:urn:iki:ledger:other"),
        ]),
        &list,
    )
    .unwrap_err();
    assert!(format!("{e:?}").contains("at most 16 panes"), "{e:?}");
}

#[test]
fn the_offers_follow_the_selection() {
    let list = list();
    let mut sel = BTreeMap::new();
    assert_eq!(
        workspace::offers(&list, &sel).len(),
        1,
        "nothing selected: the pinned one"
    );
    sel.insert(
        "item".to_string(),
        "urn:iki:ledger:default:item:3941".to_string(),
    );
    let offers = workspace::offers(&list, &sel);
    assert_eq!(
        offers[0].content,
        "lens:review:resource:urn:iki:ledger:default:item:3941"
    );
    assert_eq!(offers[0].role.as_deref(), Some("item"));
    // A list is closed, and a placeholder needs the role it accepts.
    assert!(
        workspace::addable(br#"[{"label": "A", "content": "lens:a:resource:{item}"}]"#).is_err()
    );
    assert!(workspace::addable(br#"[{"label": "A", "content": "x", "secret": 1}]"#).is_err());
}

#[test]
fn the_add_form_and_open_in_a_new_pane() {
    let ws = item();
    let sel = ws.selection.clone();
    let offers = workspace::offers(&list(), &sel);
    let screen = workspace::compose_with(&ws, WS, 3, &BTreeMap::new(), None, &offers).unwrap();
    let forms = labels(&screen, "form");
    assert!(forms.contains(&"Add pane".to_string()), "{forms:?}");
    // In each pane that emits `item`, and only there.
    let open = "Open this selection in a new pane: Review".to_string();
    assert_eq!(forms.iter().filter(|l| **l == open).count(), 2, "{forms:?}");
    // With nothing to offer, no Add form.
    let bare = workspace::compose(&ws, WS, 3, &BTreeMap::new()).unwrap();
    assert!(!labels(&bare, "form").contains(&"Add pane".to_string()));
    ikigai_ui::model::validate(&screen).unwrap();
}

#[test]
fn maximize_is_a_view_in_the_address() {
    let ws = item();
    let screen =
        workspace::compose_with(&ws, WS, 3, &BTreeMap::new(), Some("session"), &[]).unwrap();
    ikigai_ui::model::validate(&screen).unwrap();
    // Only the maximized pane is in the layout: the others are not in the tab order or the
    // accessibility tree.
    assert_eq!(regions(&screen), ["Session"]);
    // "Restore every pane" is the Maximize toggle's place: a GET of the address without
    // `maximize`, its button pressed; and no Collapse while maximized.
    let restore = nodes(&screen)
        .into_iter()
        .find(|n| n.kind == "form" && prop(n, "label") == "Restore every pane")
        .expect("a Restore toggle");
    assert_eq!(prop(restore, "method"), "get");
    assert_eq!(
        prop(restore, "action"),
        format!("/workspace/{WS}#pane-session")
    );
    assert!(restore
        .children
        .iter()
        .all(|f| prop(f, "name") != "maximize"));
    assert_eq!(
        restore.hint.get(workspace::TOGGLE),
        Some(&ikigai_ui::model::Hint::Plain(
            ikigai_ui::model::Value::Text("pressed true".into())
        ))
    );
    let forms = labels(&screen, "form");
    assert!(
        !forms.iter().any(|l| l.starts_with("Collapse")),
        "{forms:?}"
    );
    // The pane list still lists every pane, the hidden ones said in words, each linking back
    // to every pane at itself.
    let nav = nodes(&screen)
        .into_iter()
        .find(|n| n.kind == "nav")
        .unwrap();
    let listed: Vec<(&str, &str)> = nav
        .children
        .iter()
        .map(|l| (prop(l, "label"), prop(l, "href")))
        .collect();
    assert_eq!(listed.len(), 4);
    assert!(
        listed[0].0.ends_with("hidden while Session is maximized"),
        "{listed:?}"
    );
    assert_eq!(listed[0].1, format!("/workspace/{WS}#pane-dashboard"));
    assert!(!listed[1].0.contains("hidden"));
    let said = nodes(&screen)
        .into_iter()
        .any(|n| prop(n, "content") == "Session maximized. Restore to see every pane.");
    assert!(said);
    // Not maximized: each pane links to its maximize address, the selection kept.
    let mut address = BTreeMap::new();
    address.insert(
        "item".to_string(),
        "urn:iki:ledger:default:item:7".to_string(),
    );
    let every = workspace::compose_with(&ws, WS, 3, &address, None, &[]).unwrap();
    let max = nodes(&every)
        .into_iter()
        .find(|n| n.kind == "form" && prop(n, "label") == "Maximize Trail")
        .expect("a Maximize toggle");
    assert_eq!(prop(max, "method"), "get");
    assert_eq!(prop(max, "action"), format!("/workspace/{WS}#pane-trail"));
    let fields: Vec<(&str, &str)> = max
        .children
        .iter()
        .map(|f| (prop(f, "name"), prop(f, "value")))
        .collect();
    assert_eq!(
        fields,
        [
            ("select.item", "urn:iki:ledger:default:item:7"),
            ("maximize", "trail")
        ]
    );
    assert_eq!(regions(&every).len(), 4);
    // An unknown pane is refused, naming it.
    let e = workspace::compose_with(&ws, WS, 3, &BTreeMap::new(), Some("nope"), &[]).unwrap_err();
    assert!(format!("{e:?}").contains("no pane `nope`"), "{e:?}");
    // Maximize never writes: it is not an edit.
    let e = workspace::apply(&ws, &args(&[("op", "maximize"), ("pane", "session")])).unwrap_err();
    assert!(format!("{e:?}").contains("not an edit"), "{e:?}");
}

#[test]
fn the_face_name_carries_maximize() {
    let mut sel = BTreeMap::new();
    sel.insert("item".to_string(), "urn:a:b".to_string());
    for (s, m) in [
        (BTreeMap::new(), Some("session")),
        (sel.clone(), Some("trail")),
        (sel, None),
    ] {
        let name = workspace::face_name_with(WS, &s, m);
        let rest = name.strip_prefix("workspace:").unwrap();
        let (iri, back, max) = workspace::parse_face(rest).unwrap().unwrap();
        assert_eq!((iri.as_str(), &back, max.as_deref()), (WS, &s, m), "{name}");
        // The older reading drops it.
        assert_eq!(
            workspace::parse_face_name(rest).unwrap().unwrap(),
            (iri, back)
        );
    }
    assert!(workspace::parse_face(&format!("maximize:NO:resource:{WS}"))
        .unwrap()
        .is_err());
}
