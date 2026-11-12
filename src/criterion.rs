//! Accessibility acceptance criteria per node kind and target, as data (ledger #3541).
//!
//! | name | what it is |
//! |---|---|
//! | `urn:iki:ui:criterion:kind:{kind}:target:{target}` | `{kind}`'s criterion on the `{target}` face (`html` or `gpui`): Turtle by default, `as=text/plain` for the same in words |
//! | `urn:iki:ui:shape:criterion` | the vocabulary and the closed SHACL shapes of a criterion, Turtle ([`SHAPE`]); [`validate`] says the same in Rust |
//!
//! A criterion says what a face must do for keyboard and assistive-technology users: the element
//! or role it emits (`semantics`), its keys and their outcomes (`keyboard`), what a screen reader
//! announces as Name, Role, Group (hint, error and group name, read after the label) and State,
//! its device-settings checks (`setting`: text at 200%, zoom) and its other checks (`check`).
//! Every check is SEE (a face or a test can verify it, and its `probe` names how; see
//! [`PROBES`]) or HEAR (it needs assistive technology, and has no probe). A kind's descriptor
//! (`urn:iki:ui:kind:name:{kind}`) links each of its criteria by
//! `urn:iki:ui:criterion:term:criterion`.
//!
//! The criteria are written as one closed JSON sheet per kind (`assets/criterion/{kind}.json`),
//! because the library carries no RDF parser; the Turtle face is written from the parsed sheet
//! and `tests/criterion.rs` holds it to [`SHAPE`]. The wording is seeded from magentaA11y and
//! atomica11y-skills (Apache-2.0, see `NOTICE`), and each criterion names its sources.
//!
//! ```
//! use ikigai_ui::criterion;
//! let html = criterion::criterion("button", "html").unwrap();
//! assert_eq!(html.role, "Button.");
//! assert!(criterion::criterion("heading", "html").is_none()); // a follow-up's
//! ```

use std::fmt::Write as _;
use std::sync::OnceLock;

use ikigai_core::{
    ArgSpec, Description, Error, FnEndpoint, Invocation, ReprType, Representation, Verb,
};
use serde::Deserialize;

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

/// How a test verifies a SEE check on the html face. A SEE check with no probe is reported as
/// not verified, never skipped.
///
/// - `element` (argument: an element name): the node's control is that element.
/// - `attribute` (argument: `name=value`): the control carries the attribute with that value.
/// - `focusable`: the control is in the tab order natively: a `<button>`, an `<a href>` or an
///   `<input>`, not `disabled`, with no negative `tabindex`.
/// - `focus-visible`: the token stylesheet draws a `:focus-visible` outline of at least 2px in
///   `--focus`.
/// - `name-from-content`: the control's text is the node's `label`.
/// - `name-from-label`: a `<label for>` whose text is the node's `label` names the control's
///   `id`.
/// - `href`: the link's `href` is the node's.
/// - `described-by`: `aria-describedby` names the hint and then the error, both in the node's
///   HTML, and each holds the node's text for it.
/// - `required`: `aria-required` is `true` exactly when the node is required.
/// - `invalid`: `aria-invalid` is `true` exactly when the node carries an `error`.
/// - `text-scales`: no stylesheet the page loads sets a font size in `px`, so text follows the
///   reader's text size.
pub const PROBES: [&str; 11] = [
    "element",
    "attribute",
    "focusable",
    "focus-visible",
    "name-from-content",
    "name-from-label",
    "href",
    "described-by",
    "required",
    "invalid",
    "text-scales",
];

/// The probes that take an argument.
const PROBES_WITH_ARGUMENT: [&str; 2] = ["element", "attribute"];

/// The kinds that have criteria, with their sheets. The rest are a follow-up's (ledger #3541).
const SHEETS: [(&str, &str); 3] = [
    ("button", include_str!("../assets/criterion/button.json")),
    ("field", include_str!("../assets/criterion/field.json")),
    ("link", include_str!("../assets/criterion/link.json")),
];

/// One kind's criteria, one per target.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Sheet {
    /// The kind's name.
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
    /// How a test verifies a SEE check ([`PROBES`]).
    #[serde(default)]
    pub probe: Option<String>,
    /// The probe's argument.
    #[serde(default)]
    pub argument: Option<String>,
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
/// target in [`TARGETS`] order, every setting a SEE check, a probe only on a SEE check and an
/// argument exactly when its probe takes one.
pub fn validate(sheet: &Sheet) -> Result<(), String> {
    let kind = &sheet.kind;
    if crate::model::kind(kind).is_none() {
        return Err(format!("{kind}: not a kind"));
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
            if !s.starts_with("https://github.com/") || s.contains(['<', '>', '"', ' ']) {
                return Err(format!("{at}: source {s} is not a GitHub page"));
            }
        }
        if c.keyboard.is_empty() {
            return Err(format!("{at}: no keyboard behavior"));
        }
        for k in &c.keyboard {
            nonempty("a key", &k.key)?;
            nonempty("a key's outcome", &k.outcome)?;
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
                (Some(p), Mode::See) if !PROBES.contains(&p.as_str()) => {
                    return Err(format!("{at}: {p} is not a probe ({PROBES:?})"))
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

/// The kinds that have criteria.
pub fn kinds() -> impl Iterator<Item = &'static str> {
    SHEETS.iter().map(|(name, _)| *name)
}

/// `kind`'s criteria, if it has any.
pub fn sheet(kind: &str) -> Option<&'static Sheet> {
    sheets().iter().find(|s| s.kind == kind)
}

/// `kind`'s criterion on `target`, if it has one.
pub fn criterion(kind: &str, target: &str) -> Option<&'static Criterion> {
    sheet(kind)?.criteria.iter().find(|c| c.target == target)
}

/// A criterion's name: `urn:iki:ui:criterion:kind:{kind}:target:{target}`.
pub fn iri(kind: &str, target: &str) -> String {
    format!("{PREFIX}kind:{kind}:target:{target}")
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
    let _ = writeln!(out, "  c:kind <{}{kind}> ;", crate::model::KIND_PREFIX);
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
        out.push_str(" .\n");
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
            let _ = writeln!(out, "  [{mode}] {}", check.statement);
        }
    }
    if let Some(note) = &c.note {
        let _ = writeln!(out, "\nKnown gap\n  {note}");
    }
    out.push_str("\nSeeded from (Apache-2.0)\n");
    for s in &c.source {
        let _ = writeln!(out, "  {s}");
    }
    out
}

/// `urn:iki:ui:criterion:kind:{kind}:target:{target}`.
pub(crate) fn endpoint() -> FnEndpoint {
    FnEndpoint::new("ui-criterion", |inv: &Invocation<'_>| {
        let kind = inv.bindings.get("kind").unwrap_or_default();
        let target = inv.bindings.get("target").unwrap_or_default();
        let c = criterion(kind, target).ok_or_else(|| {
            Error::NotFound(format!(
                "{}: no criterion (the kinds with criteria are {}, the targets {})",
                inv.request.target.as_str(),
                kinds().collect::<Vec<_>>().join(", "),
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
        Description::new("ui-criterion")
            .title("A UI node kind's accessibility acceptance criterion")
            .summary(
                "What one node kind's face on one target must do for keyboard and \
                 assistive-technology users: semantics, keys, what a screen reader announces \
                 (name, role, group, state), device-settings checks and SEE and HEAR checks. \
                 Turtle by default, as=text/plain in words.",
            )
            .verb(Verb::Source)
            .verb(Verb::Meta)
            .input(
                ArgSpec::new("kind")
                    .summary("the node kind")
                    .class(crate::XSD_STRING)
                    .binding()
                    .one_of(kinds()),
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
