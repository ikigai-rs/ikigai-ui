//! The palette tables (UI conventions D12): the only place a color is written.
//!
//! Every UI that binds this crate gets its colors from these tables, through the token
//! stylesheet `urn:iki:ui:tokens.css`. Token names are gonk.css's (`--bg` through `--line`) plus
//! the four the other palettes need (`--ok-fg`, `--focus`, `--warn`, `--warn-fg`). The light and
//! dark values start from gonk.css and the high-contrast values from ikigai-name; where a
//! starting value missed the floor, the table says so beside it.
//!
//! [`PAIRS`] is the contrast floor as data: every foreground/ground pair the shared CSS draws,
//! with the minimum each must clear. The test kit measures it with ikigai-a11y, for every
//! palette, so a value that drifts below the floor fails a test, not a review.
//!
//! ```
//! use ikigai_ui::palette::{palette, LIGHT};
//! assert_eq!(LIGHT.get("bg"), Some("#ffffff"));
//! assert_eq!(palette("high-contrast").unwrap().get("bg"), Some("#000000"));
//! assert!(palette("chartreuse").is_none());
//! ```

/// The color tokens, in the order every palette states them. A new token is a change to the
/// conventions record (D12) and to every palette.
pub const TOKENS: [&str; 14] = [
    "bg",
    "surface",
    "fg",
    "muted",
    "accent",
    "accent-fg",
    "danger",
    "danger-fg",
    "ok",
    "ok-fg",
    "warn",
    "warn-fg",
    "line",
    "focus",
];

/// The tokens that are not colors, as the token stylesheet defines them once for every palette.
pub const LAYOUT: [(&str, &str); 2] = [("radius", "0.5rem"), ("gap", "0.75rem")];

/// WCAG 2.2 SC 1.4.3 (AA): text against its ground.
pub const FLOOR_TEXT: f64 = 4.5;
/// WCAG 2.2 SC 1.4.11 (AA): a border or a focus ring against its ground.
pub const FLOOR_NON_TEXT: f64 = 3.0;
/// WCAG 2.2 SC 1.4.6 (AAA): what the high-contrast palette holds EVERY pair to.
pub const FLOOR_ENHANCED: f64 = 7.0;

/// What a pair is, which fixes its floor.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    /// Text: [`FLOOR_TEXT`].
    Text,
    /// A border or focus ring: [`FLOOR_NON_TEXT`].
    NonText,
}

/// Every pair the shared CSS draws: (foreground token, ground token, kind).
///
/// D12's list (`--fg`, `--muted` and each `-fg` on its own color; `--line` and `--focus` on
/// their grounds), plus `--accent` and `--danger` as text on both grounds, because the token
/// stylesheet colors links with `--accent` and refusal notes with `--danger`. The focus ring is
/// drawn outside its control (`outline-offset`), so its ground is the page, never the control.
pub const PAIRS: [(&str, &str, Kind); 16] = [
    ("fg", "bg", Kind::Text),
    ("fg", "surface", Kind::Text),
    ("muted", "bg", Kind::Text),
    ("muted", "surface", Kind::Text),
    ("accent", "bg", Kind::Text),
    ("accent", "surface", Kind::Text),
    ("danger", "bg", Kind::Text),
    ("danger", "surface", Kind::Text),
    ("accent-fg", "accent", Kind::Text),
    ("danger-fg", "danger", Kind::Text),
    ("ok-fg", "ok", Kind::Text),
    ("warn-fg", "warn", Kind::Text),
    ("line", "bg", Kind::NonText),
    ("line", "surface", Kind::NonText),
    ("focus", "bg", Kind::NonText),
    ("focus", "surface", Kind::NonText),
];

/// One palette: a name `theme=` accepts and a value for every token in [`TOKENS`].
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Palette {
    /// The name `theme=` and `urn:iki:ui:palette:{name}` use.
    pub name: &'static str,
    /// The label a chooser shows.
    pub label: &'static str,
    /// The CSS `color-scheme` this palette paints in.
    pub scheme: &'static str,
    /// Hold every pair to [`FLOOR_ENHANCED`] (the high-contrast palette).
    pub enhanced: bool,
    /// `(token, #rrggbb)`, in [`TOKENS`] order.
    pub colors: [(&'static str, &'static str); 14],
}

impl Palette {
    /// The value of `token` (without its `--`), if this palette defines it.
    pub fn get(&self, token: &str) -> Option<&'static str> {
        self.colors
            .iter()
            .find(|(t, _)| *t == token)
            .map(|(_, v)| *v)
    }

    /// The floor `kind` is held to in this palette.
    pub fn floor(&self, kind: Kind) -> f64 {
        match (self.enhanced, kind) {
            (true, _) => FLOOR_ENHANCED,
            (false, Kind::Text) => FLOOR_TEXT,
            (false, Kind::NonText) => FLOOR_NON_TEXT,
        }
    }

    /// The palette as CSS custom-property declarations, one per line, indented for the
    /// token stylesheet, with no final newline (the template's line supplies it): the `text/plain`
    /// face of `urn:iki:ui:palette:{name}`.
    ///
    /// ```
    /// let css = ikigai_ui::palette::LIGHT.declarations();
    /// assert!(css.starts_with("  --bg: #ffffff;\n  --surface: #f4f6f8;\n"));
    /// assert_eq!(css.lines().count(), 14);
    /// ```
    pub fn declarations(&self) -> String {
        self.colors
            .iter()
            .map(|(t, v)| format!("  --{t}: {v};"))
            .collect::<Vec<_>>()
            .join("\n")
    }

    /// The palette as Turtle, one triple per token: the subject is the palette's IRI and the
    /// predicate the token's (`urn:iki:ui:token:{name}`). No blank nodes.
    ///
    /// ```
    /// let ttl = ikigai_ui::palette::DARK.turtle();
    /// assert!(ttl.starts_with("<urn:iki:ui:palette:dark> <urn:iki:ui:token:bg> \"#0d1117\" .\n"));
    /// assert_eq!(ttl.lines().count(), 14);
    /// ```
    pub fn turtle(&self) -> String {
        self.colors
            .iter()
            .map(|(t, v)| {
                format!(
                    "<urn:iki:ui:palette:{}> <urn:iki:ui:token:{t}> \"{v}\" .\n",
                    self.name
                )
            })
            .collect()
    }
}

/// The light palette: gonk.css's light values.
pub const LIGHT: Palette = Palette {
    name: "light",
    label: "Light",
    scheme: "light",
    enhanced: false,
    colors: [
        ("bg", "#ffffff"),
        ("surface", "#f4f6f8"),
        ("fg", "#1b1f24"),
        ("muted", "#545d68"),
        ("accent", "#0b5cad"),
        ("accent-fg", "#ffffff"),
        ("danger", "#a8200d"),
        ("danger-fg", "#ffffff"),
        ("ok", "#19703a"),
        ("ok-fg", "#ffffff"),
        ("warn", "#8a4b00"),
        ("warn-fg", "#ffffff"),
        // gonk.css has #c9d1d9, 1.54:1 on --bg: below the 3:1 non-text floor.
        ("line", "#848d97"),
        ("focus", "#0b5cad"),
    ],
};

/// The dark palette: gonk.css's dark values.
pub const DARK: Palette = Palette {
    name: "dark",
    label: "Dark",
    scheme: "dark",
    enhanced: false,
    colors: [
        ("bg", "#0d1117"),
        ("surface", "#161b22"),
        ("fg", "#e6edf3"),
        ("muted", "#9ea8b3"),
        ("accent", "#6cb6ff"),
        ("accent-fg", "#0d1117"),
        ("danger", "#ff9b8f"),
        ("danger-fg", "#0d1117"),
        ("ok", "#5bd07a"),
        ("ok-fg", "#0d1117"),
        ("warn", "#e3b341"),
        ("warn-fg", "#0d1117"),
        // gonk.css has #30363d, 1.55:1 on --bg: below the 3:1 non-text floor.
        ("line", "#656d76"),
        ("focus", "#6cb6ff"),
    ],
};

/// The high-contrast palette: ikigai-name's, held to 7:1 for every pair.
pub const HIGH_CONTRAST: Palette = Palette {
    name: "high-contrast",
    label: "High contrast",
    scheme: "dark",
    enhanced: true,
    colors: [
        ("bg", "#000000"),
        ("surface", "#1a1a1a"),
        ("fg", "#ffffff"),
        ("muted", "#e0e0e0"),
        ("accent", "#ffe066"),
        ("accent-fg", "#000000"),
        ("danger", "#ff9e9e"),
        ("danger-fg", "#000000"),
        ("ok", "#7ee787"),
        ("ok-fg", "#000000"),
        ("warn", "#ffd33d"),
        ("warn-fg", "#000000"),
        ("line", "#ffffff"),
        ("focus", "#ffe066"),
    ],
};

/// Every palette, in chooser order.
pub const PALETTES: [Palette; 3] = [LIGHT, DARK, HIGH_CONTRAST];

/// The palette named `name`, or `None`.
pub fn palette(name: &str) -> Option<&'static Palette> {
    PALETTES.iter().find(|p| p.name == name)
}
