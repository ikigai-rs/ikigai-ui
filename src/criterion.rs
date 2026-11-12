//! Accessibility acceptance criteria per node kind, custom element and target, as data (ledger
//! #3541, every kind and this crate's elements ledger #3542).
//!
//! | name | what it is |
//! |---|---|
//! | `urn:iki:ui:criterion:kind:{kind}:target:{target}` | `{kind}`'s criterion on the `{target}` face (`html` or `gpui`): Turtle by default, `as=text/plain` for the same in words |
//! | `urn:iki:ui:criterion:element:{element}:target:{target}` | the same for one of this crate's custom elements ([`ELEMENTS`]) |
//! | `urn:iki:ui:shape:criterion` | the vocabulary and the closed SHACL shapes of a criterion, Turtle ([`SHAPE`]); [`validate`] says the same in Rust |
//!
//! A criterion says what a face must do for keyboard and assistive-technology users: the element
//! or role it emits (`semantics`), its keys and their outcomes (`keyboard`), what a screen reader
//! announces as Name, Role, Group (hint, error and group name, read after the label) and State,
//! its device-settings checks (`setting`: text at 200%, zoom, high contrast) and its other checks
//! (`check`). Every check is SEE (a face or a test can verify it, and its `probe` names how; see
//! [`PROBES`] for the html face and [`GPUI_PROBES`] for the gpui face) or HEAR (it needs
//! assistive technology, and has no probe). A SEE check with a probe is verified by a test; one
//! without is reported as not verified ([`tally`], which the support matrix shows). A check that
//! cannot pass today names what it is `blocked` by ([`BLOCKERS`]): on the gpui face, every HEAR
//! check is blocked by the toolkit, because GPUI 0.2.2 exposes no accessibility tree (ledger
//! #3546). A gpui criterion also says what each desktop platform's screen reader should announce
//! once there is one (`announce`, one per [`PLATFORMS`]: the platform's API role and the reader's
//! words). A kind's descriptor (`urn:iki:ui:kind:name:{kind}`) links each of its criteria by
//! `urn:iki:ui:criterion:term:criterion`.
//!
//! The criteria are written as one closed JSON sheet per kind (`assets/criterion/{kind}.json`),
//! because the library carries no RDF parser; the Turtle face is written from the parsed sheet
//! and `tests/criterion.rs` holds it to [`SHAPE`]. The html wording is seeded from magentaA11y
//! and atomica11y-skills (Apache-2.0, see `NOTICE`). They cover the web, iOS and Android, not
//! desktop-native apps, so the gpui criteria are our own, drawn from each platform's guidance
//! (Apple's, Windows UI Automation's, AT-SPI's). Each criterion names its sources.
//! Neither covers a meter, nor a 3D scene whose replacement is its accessible form, so the html
//! criteria of those two are written from what this repository's `ACCESSIBILITY.md` promises,
//! and name it.
//!
//! ```
//! use ikigai_ui::criterion;
//! let html = criterion::criterion("button", "html").unwrap();
//! assert_eq!(html.role, "Button.");
//! assert!(criterion::criterion("meter", "gpui").is_some()); // an element's
//! assert!(criterion::criterion("rating", "html").is_none()); // an example, not ours
//! ```

use std::fmt::Write as _;
use std::sync::OnceLock;

use ikigai_core::{
    ArgSpec, Description, Error, FnEndpoint, Invocation, ReprType, Representation, Verb,
};
use serde::{Deserialize, Serialize};

use crate::model::Node;

/// The prefix of every criterion's name.
pub const PREFIX: &str = "urn:iki:ui:criterion:";
/// The prefix of the criterion vocabulary's terms.
pub const TERM: &str = "urn:iki:ui:criterion:term:";
/// Where the vocabulary and the closed shapes are served.
pub const SHAPE_IRI: &str = "urn:iki:ui:shape:criterion";
/// The vocabulary and the closed shapes, Turtle.
pub const SHAPE: &str = include_str!("../assets/criterion.ttl");
/// The text face's media type.
pub const TEXT: &str = "text/plain";
/// The targets a criterion is written for, in the order a sheet states them.
pub const TARGETS: [&str; 2] = ["html", "gpui"];

/// How a test verifies a SEE check on the html face (`tests/criterion.rs`). A SEE check with
/// no probe is reported as not verified, never skipped.
///
/// The node's SUBJECT is its HTML's outermost element, or, when that is a `<div>` wrapper (a
/// field, a choice, a meter), the control in it the label names. Its CONTROL is the subject, or
/// a `<details>`'s `<summary>`.
///
/// - `element` (argument: an element name): the subject is that element.
/// - `attribute` (argument: `name=value`): the subject carries the attribute with that value.
/// - `contains` (argument: `tag`, `tag[name]` or `tag[name=value]`): an element in the node's
///   HTML matches.
/// - `focusable`: the control is in the tab order natively: a `<button>`, an `<a href>`, an
///   `<input>`, a `<select>` or a `<summary>`, not `disabled`, with no negative `tabindex`.
/// - `no-tab-stop`: nothing in the node's HTML is in the tab order: no control, no
///   `tabindex` of 0 or more.
/// - `focus-visible`: the token stylesheet draws a `:focus-visible` outline of at least 2px in
///   `--focus`, and no rule takes it from the control.
/// - `name-from-content`: the control's text is the node's `label` (else its `content`), with
///   no `aria-label` over it.
/// - `name-from-label`: a `<label for>` whose text is the node's `label` names the subject's
///   `id`.
/// - `labelled-by`: the subject's `aria-labelledby` names an element in its HTML whose text is
///   the node's `label`.
/// - `aria-label`: the first element in the node's HTML with an `aria-label` is named by the
///   node's `label`, alone or followed by `:` and the keys.
/// - `heading-level`: the subject is `<h{level}>`, the node's `level`.
/// - `content`: the subject's text is the node's `content`, as text (markup in it escaped).
/// - `caption`: the table's `<caption>` says the node's `caption`.
/// - `header-scope`: the table has header cells, and every `<th>` has `scope` `row` or `col`.
/// - `items`: one `<li>` per child of the node.
/// - `alt`: the subject's `alt` is the node's `alt`, not empty.
/// - `value`: the subject's `value` is the node's `value`.
/// - `quiet`: nothing in the node's HTML is a live region: no `aria-live`, no `role` of
///   `status`, `alert` or `log` (the page's one live region is the shell's `#status`).
/// - `open-state`: the `<details>` is `open` exactly when the node's `open` is true.
/// - `range`: the `<meter>`'s `value`, `min` and `max` are the node's (`min` 0 and `max` 1 by
///   default).
/// - `replacement`: a link in the node's HTML points (`#id`) at an element after the
///   `<canvas>`, which holds the replacement, and the canvas is `hidden` until it has drawn.
/// - `href`: the link's `href` is the node's.
/// - `described-by`: `aria-describedby` names the hint and then the error, both in the node's
///   HTML, and each holds the node's text for it.
/// - `required`: `aria-required` is `true` exactly when the node is required.
/// - `invalid`: `aria-invalid` is `true` exactly when the node carries an `error`.
/// - `text-scales`: no stylesheet the page loads sets a font size in `px`, so text follows the
///   reader's text size.
pub const PROBES: [&str; 26] = [
    "element",
    "attribute",
    "contains",
    "focusable",
    "no-tab-stop",
    "focus-visible",
    "name-from-content",
    "name-from-label",
    "labelled-by",
    "aria-label",
    "heading-level",
    "content",
    "caption",
    "header-scope",
    "items",
    "alt",
    "value",
    "quiet",
    "open-state",
    "range",
    "replacement",
    "href",
    "described-by",
    "required",
    "invalid",
    "text-scales",
];

/// How a window test verifies a SEE check on the gpui face, on GPUI's test platform
/// (`gpui/src/native.rs`, `every_gpui_see_check_with_a_probe_holds`): the gallery's and the
/// elements screen's windows, each node of the kind or element. Those tests run where GPUI's
/// `test-support` is taken (macOS, and Linux by hand), not in CI (`gpui/README.md`).
///
/// - `tab-stop`: the node is one stop in the window's tab order, shown, and focusing it puts
///   the keyboard focus there.
/// - `no-tab-stop`: the node adds no stop of its own to the window's tab order.
/// - `toggles`: Space on the focused node opens it and Enter closes it again, its model
///   unchanged (the state is the window's).
pub const GPUI_PROBES: [&str; 3] = ["tab-stop", "no-tab-stop", "toggles"];

/// What a check can be blocked by. `toolkit`: the face's toolkit cannot do it yet (GPUI 0.2.2
/// exposes no accessibility tree and reports no system text size or high contrast), so the
/// check fails on that face until the toolkit can, whatever the face does.
pub const BLOCKERS: [&str; 1] = ["toolkit"];

/// The desktop platforms a gpui criterion says the announcement on, in this order.
pub const PLATFORMS: [&str; 3] = ["macos", "windows", "linux"];

/// The probes that take an argument.
const PROBES_WITH_ARGUMENT: [&str; 3] = ["element", "attribute", "contains"];

/// `target`'s probes.
pub fn probes(target: &str) -> &'static [&'static str] {
    match target {
        "html" => &PROBES,
        "gpui" => &GPUI_PROBES,
        _ => &[],
    }
}

/// This crate's custom elements that have criteria (`urn:iki:ui:element:name:{name}`). The
/// `rating` example is a test's, not a face's, and has none.
pub const ELEMENTS: [&str; 5] = ["card", "key-value", "tag-list", "disclosure", "meter"];

/// Every kind's sheet, in [`crate::model::KINDS`] order, then every element's, in
/// [`ELEMENTS`] order.
const SHEETS: [(&str, &str); 21] = [
    ("heading", include_str!("../assets/criterion/heading.json")),
    ("text", include_str!("../assets/criterion/text.json")),
    ("field", include_str!("../assets/criterion/field.json")),
    ("choice", include_str!("../assets/criterion/choice.json")),
    ("option", include_str!("../assets/criterion/option.json")),
    ("button", include_str!("../assets/criterion/button.json")),
    ("link", include_str!("../assets/criterion/link.json")),
    ("list", include_str!("../assets/criterion/list.json")),
    ("table", include_str!("../assets/criterion/table.json")),
    ("form", include_str!("../assets/criterion/form.json")),
    ("nav", include_str!("../assets/criterion/nav.json")),
    ("status", include_str!("../assets/criterion/status.json")),
    ("image", include_str!("../assets/criterion/image.json")),
    ("group", include_str!("../assets/criterion/group.json")),
    ("visual", include_str!("../assets/criterion/visual.json")),
    ("scene", include_str!("../assets/criterion/scene.json")),
    ("card", include_str!("../assets/criterion/card.json")),
    (
        "key-value",
        include_str!("../assets/criterion/key-value.json"),
    ),
    (
        "tag-list",
        include_str!("../assets/criterion/tag-list.json"),
    ),
    (
        "disclosure",
        include_str!("../assets/criterion/disclosure.json"),
    ),
    ("meter", include_str!("../assets/criterion/meter.json")),
];

/// One kind's or element's criteria, one per target.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Sheet {
    /// The kind's name, or the element's (its IRI's last segment).
    pub kind: String,
    /// Its criteria, one per target, in [`TARGETS`] order.
    pub criteria: Vec<Criterion>,
}

/// A kind's criterion on one target.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Criterion {
    /// `html` or `gpui`.
    pub target: String,
    /// The pages its wording was seeded from.
    pub source: Vec<String>,
    /// The element or role the face emits.
    pub semantics: String,
    /// Keys and their outcomes.
    pub keyboard: Vec<Keystroke>,
    /// What a screen reader announces as the name.
    pub name: String,
    /// … as the role.
    pub role: String,
    /// … after the label: hint, error and group name.
    #[serde(default)]
    pub group: Option<String>,
    /// … as the state.
    #[serde(default)]
    pub state: Option<String>,
    /// Device-settings checks, all SEE.
    pub setting: Vec<Check>,
    /// The other checks.
    pub check: Vec<Check>,
    /// What each desktop platform's screen reader should announce, one per [`PLATFORMS`] on
    /// the gpui face; none on the html face, whose announcement is the browser's.
    #[serde(default)]
    pub announce: Vec<Announcement>,
    /// A known gap between the face and this criterion.
    #[serde(default)]
    pub note: Option<String>,
}

/// A key and its outcome.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Keystroke {
    /// The key, as a user names it.
    pub key: String,
    /// What it does.
    pub outcome: String,
}

/// What one desktop platform's screen reader should announce for the face's control.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Announcement {
    /// One of [`PLATFORMS`].
    pub platform: String,
    /// The screen reader, and the accessibility API it reads.
    pub reader: String,
    /// The role and properties the control must expose in that API.
    pub api: String,
    /// What the reader says on reaching it, in its words.
    pub says: String,
}

/// SEE or HEAR.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Mode {
    /// A face or a test can verify it.
    See,
    /// It needs assistive technology.
    Hear,
}

impl Mode {
    /// `see` or `hear`.
    pub fn as_str(self) -> &'static str {
        match self {
            Mode::See => "see",
            Mode::Hear => "hear",
        }
    }
}

/// One check.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Check {
    /// SEE or HEAR.
    pub mode: Mode,
    /// The check in words.
    pub statement: String,
    /// How a test verifies a SEE check ([`PROBES`] on html, [`GPUI_PROBES`] on gpui).
    #[serde(default)]
    pub probe: Option<String>,
    /// The probe's argument.
    #[serde(default)]
    pub argument: Option<String>,
    /// What keeps it from passing today ([`BLOCKERS`]).
    #[serde(default)]
    pub blocked: Option<String>,
}

impl Criterion {
    /// Every check, the settings first, each with the property it is under (`setting` or
    /// `check`) and its 1-based place there.
    pub fn checks(&self) -> impl Iterator<Item = (&'static str, usize, &Check)> {
        let settings = self
            .setting
            .iter()
            .enumerate()
            .map(|(i, c)| ("setting", i + 1, c));
        let checks = self
            .check
            .iter()
            .enumerate()
            .map(|(i, c)| ("check", i + 1, c));
        settings.chain(checks)
    }
}

/// A sheet read from JSON and validated: what the closed shapes say, in Rust.
pub fn parse(json: &str) -> Result<Sheet, String> {
    let sheet: Sheet = serde_json::from_str(json).map_err(|e| e.to_string())?;
    validate(&sheet)?;
    Ok(sheet)
}

/// What [`SHAPE`] says of a sheet's criteria, plus what a shape cannot say: one criterion per
/// target in [`TARGETS`] order, every setting a SEE check, a probe only on a SEE check and one
/// of its target's, an argument exactly when its probe takes one, no probe on a blocked check,
/// and an announcement per [`PLATFORMS`], in order, on the gpui face and none on the html face.
pub fn validate(sheet: &Sheet) -> Result<(), String> {
    let kind = &sheet.kind;
    if crate::model::kind(kind).is_none() && !ELEMENTS.contains(&kind.as_str()) {
        return Err(format!(
            "{kind}: not a kind or one of this crate's elements"
        ));
    }
    let targets: Vec<&str> = sheet.criteria.iter().map(|c| c.target.as_str()).collect();
    if targets != TARGETS {
        return Err(format!(
            "{kind}: one criterion per target, in the order {TARGETS:?}, not {targets:?}"
        ));
    }
    for c in &sheet.criteria {
        let at = format!("{kind} on {}", c.target);
        let nonempty = |what: &str, s: &str| {
            if s.trim().is_empty() {
                Err(format!("{at}: {what} is empty"))
            } else {
                Ok(())
            }
        };
        nonempty("semantics", &c.semantics)?;
        nonempty("name", &c.name)?;
        nonempty("role", &c.role)?;
        for s in [&c.group, &c.state, &c.note].into_iter().flatten() {
            nonempty("an announcement or note", s)?;
        }
        if c.source.is_empty() {
            return Err(format!("{at}: no source"));
        }
        for s in &c.source {
            if !s.starts_with("https://") || s.contains(['<', '>', '"', ' ']) {
                return Err(format!("{at}: source {s} is not an https page"));
            }
        }
        if c.keyboard.is_empty() {
            return Err(format!("{at}: no keyboard behavior"));
        }
        for k in &c.keyboard {
            nonempty("a key", &k.key)?;
            nonempty("a key's outcome", &k.outcome)?;
        }
        let platforms: Vec<&str> = c.announce.iter().map(|a| a.platform.as_str()).collect();
        let want: &[&str] = if c.target == "gpui" { &PLATFORMS } else { &[] };
        if platforms != want {
            return Err(format!(
                "{at}: an announcement per platform, in the order {want:?}, not {platforms:?}"
            ));
        }
        for a in &c.announce {
            nonempty("a reader", &a.reader)?;
            nonempty("an API role", &a.api)?;
            nonempty("an announcement", &a.says)?;
        }
        if c.setting.is_empty() || c.check.is_empty() {
            return Err(format!("{at}: a setting and a check, at least one each"));
        }
        for (under, n, check) in c.checks() {
            let at = format!("{at}, {under} {n}");
            nonempty("a statement", &check.statement)?;
            if under == "setting" && check.mode != Mode::See {
                return Err(format!("{at}: a device-settings check is SEE"));
            }
            match (&check.probe, check.mode) {
                (Some(_), Mode::Hear) => {
                    return Err(format!(
                        "{at}: a HEAR check needs assistive technology, no probe"
                    ))
                }
                (Some(p), Mode::See) if !probes(&c.target).contains(&p.as_str()) => {
                    return Err(format!(
                        "{at}: {p} is not a {} probe ({:?})",
                        c.target,
                        probes(&c.target)
                    ))
                }
                _ => {}
            }
            match (&check.blocked, &check.probe) {
                (Some(b), _) if !BLOCKERS.contains(&b.as_str()) => {
                    return Err(format!("{at}: {b} is not a blocker ({BLOCKERS:?})"))
                }
                (Some(_), Some(_)) => {
                    return Err(format!("{at}: a blocked check has no probe to pass"))
                }
                _ => {}
            }
            let takes = check
                .probe
                .as_deref()
                .is_some_and(|p| PROBES_WITH_ARGUMENT.contains(&p));
            match (&check.argument, takes) {
                (None, true) => return Err(format!("{at}: its probe takes an argument")),
                (Some(_), false) => return Err(format!("{at}: an argument with no probe for it")),
                (Some(a), true) => nonempty("an argument", a)?,
                (None, false) => {}
            }
        }
    }
    Ok(())
}

/// Every sheet, parsed once.
fn sheets() -> &'static [Sheet] {
    static SHEETS_READ: OnceLock<Vec<Sheet>> = OnceLock::new();
    SHEETS_READ.get_or_init(|| {
        SHEETS
            .iter()
            .map(|(name, json)| {
                let sheet = parse(json).unwrap_or_else(|e| panic!("the {name} criteria: {e}"));
                assert_eq!(&sheet.kind, name, "a sheet is its kind's");
                sheet
            })
            .collect()
    })
}

/// The kinds and elements that have criteria: every kind, then [`ELEMENTS`].
pub fn kinds() -> impl Iterator<Item = &'static str> {
    SHEETS.iter().map(|(name, _)| *name)
}

/// Whether `name` is one of [`ELEMENTS`], not a kind.
pub fn is_element(name: &str) -> bool {
    ELEMENTS.contains(&name)
}

/// `kind`'s criteria, if it has any.
pub fn sheet(kind: &str) -> Option<&'static Sheet> {
    sheets().iter().find(|s| s.kind == kind)
}

/// `kind`'s criterion on `target`, if it has one.
pub fn criterion(kind: &str, target: &str) -> Option<&'static Criterion> {
    sheet(kind)?.criteria.iter().find(|c| c.target == target)
}

/// A criterion's name: `urn:iki:ui:criterion:kind:{kind}:target:{target}`, or
/// `urn:iki:ui:criterion:element:{element}:target:{target}` for one of [`ELEMENTS`].
pub fn iri(kind: &str, target: &str) -> String {
    let what = if is_element(kind) { "element" } else { "kind" };
    format!("{PREFIX}{what}:{kind}:target:{target}")
}

/// What a kind's (or element's) criterion on one target leaves to be verified: the support
/// matrix's accessibility column.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct Tally {
    /// Its SEE checks, settings included.
    pub see: usize,
    /// … of which a test verifies (those with a probe).
    pub see_verified: usize,
    /// … of which cannot pass today, whatever the face does ([`BLOCKERS`]).
    pub see_blocked: usize,
    /// Its HEAR checks, none of which a test verifies: each needs a person with a screen
    /// reader.
    pub hear_untested: usize,
}

/// `kind`'s [`Tally`] on `target`, if it has a criterion there.
pub fn tally(kind: &str, target: &str) -> Option<Tally> {
    let c = criterion(kind, target)?;
    let see = || c.checks().filter(|(_, _, k)| k.mode == Mode::See);
    Some(Tally {
        see: see().count(),
        see_verified: see().filter(|(_, _, k)| k.probe.is_some()).count(),
        see_blocked: see().filter(|(_, _, k)| k.blocked.is_some()).count(),
        hear_untested: c.checks().filter(|(_, _, k)| k.mode == Mode::Hear).count(),
    })
}

/// The nodes of a screen a criterion is checked on: each node of `face` (the screen as a
/// target's face projected it) whose kind has criteria, and each node of `source` (the same
/// screen before its elements were expanded) that is one of [`ELEMENTS`], with the face's node
/// at its place. Each comes with its child indices from the root and the criterion's name.
///
/// An element's own nodes are its, and a table's rows its, not checked again as kinds; an
/// `option` is checked only in its `choice` (on its own, a face draws it as text). The walk
/// follows a node's children only where the source and the face have the same number of them.
pub fn subjects<'a>(source: &Node, face: &'a Node) -> Vec<(Vec<usize>, &'static str, &'a Node)> {
    type Found<'a> = Vec<(Vec<usize>, &'static str, &'a Node)>;
    fn walk<'a>(
        source: &Node,
        face: &'a Node,
        parent: &str,
        at: &mut Vec<usize>,
        out: &mut Found<'a>,
    ) {
        let element = source
            .kind
            .strip_prefix(crate::element::PREFIX)
            .and_then(|name| ELEMENTS.iter().find(|e| **e == name));
        if let Some(name) = element {
            out.push((at.clone(), name, face));
            return;
        }
        let kind = SHEETS.iter().find(|(name, _)| *name == face.kind);
        if let Some((name, _)) = kind.filter(|(name, _)| *name != "option" || parent == "choice") {
            out.push((at.clone(), name, face));
        }
        if face.kind != "table" && source.children.len() == face.children.len() {
            for (i, (s, f)) in source.children.iter().zip(&face.children).enumerate() {
                at.push(i);
                walk(s, f, &face.kind, at, out);
                at.pop();
            }
        }
    }
    let mut out = Vec::new();
    walk(source, face, "", &mut Vec::new(), &mut out);
    out
}

/// A Turtle string literal.
fn literal(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

/// `kind`'s criterion `c` as Turtle: the criterion, its keystrokes (`{iri}:key:{n}`) and its
/// checks (`{iri}:setting:{n}`, `{iri}:check:{n}`), every node named.
pub fn turtle(kind: &str, c: &Criterion) -> String {
    let me = iri(kind, &c.target);
    let mut out = format!("@prefix c: <{TERM}> .\n\n<{me}> a c:Criterion ;\n");
    let prefix = if is_element(kind) {
        crate::element::PREFIX
    } else {
        crate::model::KIND_PREFIX
    };
    let _ = writeln!(out, "  c:kind <{prefix}{kind}> ;");
    let _ = writeln!(out, "  c:target <urn:iki:ui:target:name:{}> ;", c.target);
    for s in &c.source {
        let _ = writeln!(out, "  c:source <{s}> ;");
    }
    let _ = writeln!(out, "  c:semantics {} ;", literal(&c.semantics));
    for n in 1..=c.keyboard.len() {
        let _ = writeln!(out, "  c:keyboard <{me}:key:{n}> ;");
    }
    let _ = writeln!(out, "  c:name {} ;", literal(&c.name));
    let _ = writeln!(out, "  c:role {} ;", literal(&c.role));
    if let Some(g) = &c.group {
        let _ = writeln!(out, "  c:group {} ;", literal(g));
    }
    if let Some(s) = &c.state {
        let _ = writeln!(out, "  c:state {} ;", literal(s));
    }
    if let Some(note) = &c.note {
        let _ = writeln!(out, "  c:note {} ;", literal(note));
    }
    for (under, n, _) in c.checks() {
        let _ = writeln!(out, "  c:{under} <{me}:{under}:{n}> ;");
    }
    for a in &c.announce {
        let _ = writeln!(out, "  c:announce <{me}:announce:{}> ;", a.platform);
    }
    out.truncate(out.len() - 2);
    out.push_str(".\n");
    for (i, k) in c.keyboard.iter().enumerate() {
        let _ = write!(
            out,
            "\n<{me}:key:{}> a c:Keystroke ;\n  c:key {} ;\n  c:outcome {} .\n",
            i + 1,
            literal(&k.key),
            literal(&k.outcome)
        );
    }
    for (under, n, check) in c.checks() {
        let _ = write!(
            out,
            "\n<{me}:{under}:{n}> a c:Check ;\n  c:mode {} ;\n  c:statement {}",
            literal(check.mode.as_str()),
            literal(&check.statement)
        );
        if let Some(p) = &check.probe {
            let _ = write!(out, " ;\n  c:probe {}", literal(p));
        }
        if let Some(a) = &check.argument {
            let _ = write!(out, " ;\n  c:argument {}", literal(a));
        }
        if let Some(b) = &check.blocked {
            let _ = write!(out, " ;\n  c:blocked {}", literal(b));
        }
        out.push_str(" .\n");
    }
    for a in &c.announce {
        let _ = write!(
            out,
            "\n<{me}:announce:{}> a c:Announcement ;\n  c:platform {} ;\n  c:reader {} ;\n  c:api {} ;\n  c:says {} .\n",
            a.platform,
            literal(&a.platform),
            literal(&a.reader),
            literal(&a.api),
            literal(&a.says)
        );
    }
    out
}

/// `kind`'s criterion `c` in words: what a person reads before testing.
pub fn text(kind: &str, c: &Criterion) -> String {
    let mut out = format!(
        "{kind} on {}: accessibility acceptance criteria\n",
        c.target
    );
    let _ = writeln!(out, "\nSemantics\n  {}", c.semantics);
    out.push_str("\nKeyboard\n");
    for k in &c.keyboard {
        let _ = writeln!(out, "  {}: {}", k.key, k.outcome);
    }
    out.push_str("\nScreen reader\n");
    let _ = writeln!(out, "  Name: {}", c.name);
    let _ = writeln!(out, "  Role: {}", c.role);
    if let Some(g) = &c.group {
        let _ = writeln!(out, "  Group: {g}");
    }
    if let Some(s) = &c.state {
        let _ = writeln!(out, "  State: {s}");
    }
    for (title, list) in [("Device settings", &c.setting), ("Checks", &c.check)] {
        let _ = writeln!(out, "\n{title}");
        for check in list {
            let mode = check.mode.as_str().to_uppercase();
            let blocked = match &check.blocked {
                Some(b) => format!(", blocked by the {b}"),
                None => String::new(),
            };
            let _ = writeln!(out, "  [{mode}{blocked}] {}", check.statement);
        }
    }
    if !c.announce.is_empty() {
        out.push_str("\nOnce there is an accessibility tree, each platform's screen reader\n");
        for a in &c.announce {
            let platform = match a.platform.as_str() {
                "macos" => "macOS",
                "windows" => "Windows",
                _ => "Linux",
            };
            let _ = writeln!(out, "  {platform} ({}): {}", a.reader, a.says);
            let _ = writeln!(out, "    exposes {}", a.api);
        }
    }
    if let Some(note) = &c.note {
        let _ = writeln!(out, "\nKnown gap\n  {note}");
    }
    out.push_str("\nSources\n");
    for s in &c.source {
        let _ = writeln!(out, "  {s}");
    }
    out
}

/// `urn:iki:ui:criterion:kind:{kind}:target:{target}`.
pub(crate) fn endpoint() -> FnEndpoint {
    endpoint_for(false)
}

/// `urn:iki:ui:criterion:element:{element}:target:{target}`.
pub(crate) fn element_endpoint() -> FnEndpoint {
    endpoint_for(true)
}

/// The kinds' endpoint (`ui-criterion`, bound `kind`) or the elements' (`ui-criterion-element`,
/// bound `element`): the same criteria, each under its own name only.
fn endpoint_for(elements: bool) -> FnEndpoint {
    let (id, binding, what) = if elements {
        ("ui-criterion-element", "element", "custom element")
    } else {
        ("ui-criterion", "kind", "node kind")
    };
    let names = move || kinds().filter(move |k| is_element(k) == elements);
    FnEndpoint::new(id, move |inv: &Invocation<'_>| {
        let kind = inv.bindings.get(binding).unwrap_or_default();
        let target = inv.bindings.get("target").unwrap_or_default();
        let c = criterion(kind, target)
            .filter(|_| is_element(kind) == elements)
            .ok_or_else(|| {
                Error::NotFound(format!(
                    "{}: no criterion (the {what}s with criteria are {}, the targets {})",
                    inv.request.target.as_str(),
                    names().collect::<Vec<_>>().join(", "),
                    TARGETS.join(", ")
                ))
            })?;
        let (media_type, body) = if crate::wants(inv, TEXT) {
            (TEXT, text(kind, c))
        } else {
            (crate::TURTLE, turtle(kind, c))
        };
        Ok(Representation::new(ReprType::new(media_type), body.into_bytes()).cacheable())
    })
    .with_description(
        Description::new(id)
            .title(format!("A UI {what}'s accessibility acceptance criterion"))
            .summary(format!(
                "What one {what}'s face on one target must do for keyboard and \
                 assistive-technology users: semantics, keys, what a screen reader announces \
                 (name, role, group, state), device-settings checks and SEE and HEAR checks. \
                 Turtle by default, as=text/plain in words."
            ))
            .verb(Verb::Source)
            .verb(Verb::Meta)
            .input(
                ArgSpec::new(binding)
                    .summary(format!("the {what}"))
                    .class(crate::XSD_STRING)
                    .binding()
                    .one_of(names()),
            )
            .input(
                ArgSpec::new("target")
                    .summary("the target whose face it is for")
                    .class(crate::XSD_STRING)
                    .binding()
                    .one_of(TARGETS),
            )
            .output(crate::TURTLE)
            .output(TEXT),
    )
}

/// `urn:iki:ui:shape:criterion`.
pub(crate) fn shape_endpoint() -> FnEndpoint {
    crate::fixed("ui-shape-criterion", crate::TURTLE, SHAPE).with_description(
        Description::new("ui-shape-criterion")
            .title("The accessibility acceptance criterion vocabulary")
            .summary(
                "The criterion vocabulary (urn:iki:ui:criterion:term:) and its closed SHACL \
                 shapes: a Criterion, its Keystrokes and its Checks.",
            )
            .verb(Verb::Source)
            .verb(Verb::Meta)
            .output(crate::TURTLE),
    )
}
