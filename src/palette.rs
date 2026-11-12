//! The palette tables (UI conventions D12): the only place a color is written.
//!
//! Every UI that binds this crate gets its colors from these tables, through the token
//! stylesheet `urn:iki:ui:tokens.css`. Token names are gonk.css's (`--bg` through `--line`) plus
//! the four the other palettes need (`--ok-fg`, `--focus`, `--warn`, `--warn-fg`) and `--rule`.
//! The light and dark values start from gonk.css and the high-contrast values from ikigai-name;
//! where a starting value missed the floor, the table says so beside it.
//!
//! Lines come in two kinds (D12 as amended, ledger #2133). `--line` is a component border, the
//! edge that identifies a control (an input, a button), held to the 3:1 non-text floor of
//! WCAG 2.2 SC 1.4.11. `--rule` is a decorative line (a table rule, a section divider), which
//! SC 1.4.11 exempts; it is held only to [`FLOOR_DECORATIVE`], so no palette makes it vanish.
//!
//! [`PAIRS`] is the contrast floor as data: every foreground/ground pair the shared CSS draws,
//! with the minimum each must clear. The test kit measures it with ikigai-a11y, for every
//! palette, so a value that drifts below the floor fails a test, not a review.
//!
//! A palette served as a resource is data a host may rebind, so it is read back as a [`Table`]:
//! parsed, every value a color, and measured against [`PAIRS`] before either face draws with
//! it (ledger #3529).
//!
//! ```
//! use ikigai_ui::palette::{palette, LIGHT};
//! assert_eq!(LIGHT.get("bg"), Some("#ffffff"));
//! assert_eq!(palette("high-contrast").unwrap().get("bg"), Some("#000000"));
//! assert!(palette("chartreuse").is_none());
//! ```

/// The color tokens, in the order every palette states them. A new token is a change to the
/// conventions record (D12) and to every palette.
pub const TOKENS: [&str; 15] = [
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
    "rule",
    "focus",
];

/// The tokens that are not colors, as the token stylesheet defines them once for every palette.
pub const LAYOUT: [(&str, &str); 2] = [("radius", "0.5rem"), ("gap", "0.75rem")];

/// WCAG 2.2 SC 1.4.3 (AA): text against its ground.
pub const FLOOR_TEXT: f64 = 4.5;
/// WCAG 2.2 SC 1.4.11 (AA): a border or a focus ring against its ground.
pub const FLOOR_NON_TEXT: f64 = 3.0;
/// A decorative line (`--rule`) against its ground. SC 1.4.11 exempts it; this is ikigai-ui's own
/// minimum, below gonk's lightest divider (1.42:1 on `--surface`), so a palette cannot make a
/// rule vanish.
pub const FLOOR_DECORATIVE: f64 = 1.3;
/// WCAG 2.2 SC 1.4.6 (AAA): what the high-contrast palette holds EVERY pair to.
pub const FLOOR_ENHANCED: f64 = 7.0;

/// What a pair is, which fixes its floor.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    /// Text: [`FLOOR_TEXT`].
    Text,
    /// A component border or focus ring: [`FLOOR_NON_TEXT`].
    NonText,
    /// A decorative line: [`FLOOR_DECORATIVE`].
    Decorative,
}

/// Every pair the shared CSS draws: (foreground token, ground token, kind).
///
/// D12's list (`--fg`, `--muted` and each `-fg` on its own color; `--line`, `--rule` and
/// `--focus` on their grounds), plus `--accent` and `--danger` as text on both grounds, because the token
/// stylesheet colors links with `--accent` and refusal notes with `--danger`. The focus ring is
/// drawn outside its control (`outline-offset`), so its ground is the page, never the control.
pub const PAIRS: [(&str, &str, Kind); 18] = [
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
    ("rule", "bg", Kind::Decorative),
    ("rule", "surface", Kind::Decorative),
    ("focus", "bg", Kind::NonText),
    ("focus", "surface", Kind::NonText),
];

/// The floor `kind` is held to, in a palette held (or not) to [`FLOOR_ENHANCED`].
fn floor(enhanced: bool, kind: Kind) -> f64 {
    match (enhanced, kind) {
        (true, _) => FLOOR_ENHANCED,
        (false, Kind::Text) => FLOOR_TEXT,
        (false, Kind::NonText) => FLOOR_NON_TEXT,
        (false, Kind::Decorative) => FLOOR_DECORATIVE,
    }
}

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
    pub colors: [(&'static str, &'static str); 15],
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
        floor(self.enhanced, kind)
    }

    /// The palette as CSS custom-property declarations, one per line, indented for the
    /// token stylesheet, with no final newline (the template's line supplies it): the `text/plain`
    /// face of `urn:iki:ui:palette:{name}`.
    ///
    /// ```
    /// let css = ikigai_ui::palette::LIGHT.declarations();
    /// assert!(css.starts_with("  --bg: #ffffff;\n  --surface: #f4f6f8;\n"));
    /// assert_eq!(css.lines().count(), 15);
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
    /// assert_eq!(ttl.lines().count(), 15);
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
        // gonk.css has #c9d1d9, 1.54:1 on --bg: below the 3:1 non-text floor, so it is --rule.
        ("line", "#848d97"),
        // gonk.css's --line: 1.54:1 on --bg, 1.42:1 on --surface.
        ("rule", "#c9d1d9"),
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
        // gonk.css has #30363d, 1.55:1 on --bg: below the 3:1 non-text floor, so it is --rule.
        ("line", "#656d76"),
        // gonk.css's --line: 1.55:1 on --bg, 1.42:1 on --surface.
        ("rule", "#30363d"),
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
        // Held to 7:1 like every pair here: 17.4:1 on --surface.
        ("rule", "#ffffff"),
        ("focus", "#ffe066"),
    ],
};

/// Every palette, in chooser order.
pub const PALETTES: [Palette; 3] = [LIGHT, DARK, HIGH_CONTRAST];

/// The palette named `name`, or `None`.
pub fn palette(name: &str) -> Option<&'static Palette> {
    PALETTES.iter().find(|p| p.name == name)
}

/// A palette's name: one or more of `a-z 0-9 -`, as `urn:iki:ui:palette:{name}` takes it.
pub fn name_ok(name: &str) -> bool {
    !name.is_empty()
        && name
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
}

/// A palette read as data (ledger #3529): every token of [`TOKENS`] bound to a color, checked
/// against the contrast floor ([`PAIRS`]) before anything draws with it.
///
/// A palette resource (`urn:iki:ui:palette:{name}`) is data a host may rebind, so both faces
/// read it through [`Table::parse`], never as text: the token stylesheet writes its
/// declarations from the parsed colors ([`Table::declarations`]), and the native face draws
/// with them ([`Table::get`]). What the parse refuses never reaches either face.
///
/// ```
/// use ikigai_ui::palette::{Table, LIGHT};
/// let table = Table::parse("light", &LIGHT.declarations()).unwrap();
/// assert_eq!(table.get("bg"), Some(0xffffff));
/// assert_eq!(table.declarations(), LIGHT.declarations());
/// // Not a color, or not a token: refused, so `}` and `;` never reach tokens.css.
/// assert!(Table::parse("light", "  --bg: #fff; } body { color: red").is_err());
/// ```
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Table {
    name: String,
    enhanced: bool,
    /// `0xRRGGBB`, in [`TOKENS`] order.
    colors: [u32; 15],
}

/// Why a palette was refused: [`PaletteError::reason`].
#[derive(Debug, Clone, PartialEq)]
pub enum Reason {
    /// The palette's name is not one or more of `a-z 0-9 -`.
    Name,
    /// A line that is not one `--token: #rrggbb;` declaration.
    NotADeclaration(String),
    /// A token [`TOKENS`] does not hold.
    UnknownToken(String),
    /// A token declared twice.
    Twice(&'static str),
    /// A value that is not `#rrggbb`.
    NotAColor {
        /// The token.
        token: &'static str,
        /// What it was given.
        value: String,
    },
    /// A token the palette leaves out.
    Missing(&'static str),
    /// A pair of [`PAIRS`] below its floor.
    BelowFloor {
        /// The foreground token.
        fg: &'static str,
        /// The ground token.
        bg: &'static str,
        /// The measured ratio.
        ratio: f64,
        /// The floor the pair is held to in this palette.
        floor: f64,
    },
}

/// A palette [`Table::parse`] refused, and why.
#[derive(Debug, Clone, PartialEq)]
pub struct PaletteError {
    /// The palette's name, as in `urn:iki:ui:palette:{name}`.
    pub palette: String,
    /// What was wrong with it.
    pub reason: Reason,
}

impl std::fmt::Display for PaletteError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "urn:iki:ui:palette:{}: ", self.palette)?;
        match &self.reason {
            Reason::Name => f.write_str("not a palette name (one or more of a-z, 0-9 and -)"),
            Reason::NotADeclaration(line) => {
                write!(f, "not a `--token: #rrggbb;` declaration: {line:?}")
            }
            Reason::UnknownToken(token) => write!(
                f,
                "--{token:?} is not a token (the tokens are: {})",
                TOKENS.join(", ")
            ),
            Reason::Twice(token) => write!(f, "--{token} is declared twice"),
            Reason::NotAColor { token, value } => {
                write!(f, "--{token}: {value:?} is not #rrggbb")
            }
            Reason::Missing(token) => write!(f, "no --{token}"),
            Reason::BelowFloor {
                fg,
                bg,
                ratio,
                floor,
            } => write!(
                f,
                "--{fg} on --{bg} is {ratio:.2}:1, below its {floor}:1 floor"
            ),
        }
    }
}

impl std::error::Error for PaletteError {}

impl From<PaletteError> for ikigai_core::Error {
    fn from(e: PaletteError) -> Self {
        ikigai_core::Error::InvalidArgument {
            name: "palette".into(),
            detail: e.to_string(),
        }
    }
}

impl Table {
    /// Parse and check the `text/plain` face of `urn:iki:ui:palette:{name}`: one
    /// `--token: #rrggbb;` declaration per line (blank lines aside), every token of [`TOKENS`]
    /// once and no other, and every pair of [`PAIRS`] at its floor. A palette named as a
    /// compiled one is held to that one's floors ([`FLOOR_ENHANCED`] for `high-contrast`).
    pub fn parse(name: &str, declarations: &str) -> Result<Self, PaletteError> {
        let refuse = |reason: Reason| PaletteError {
            palette: name.to_string(),
            reason,
        };
        if !name_ok(name) {
            return Err(refuse(Reason::Name));
        }
        let mut colors: [Option<u32>; 15] = [None; 15];
        for line in declarations
            .lines()
            .map(str::trim)
            .filter(|l| !l.is_empty())
        {
            let (token, value) = line
                .strip_prefix("--")
                .and_then(|d| d.strip_suffix(';').unwrap_or(d).split_once(':'))
                .ok_or_else(|| refuse(Reason::NotADeclaration(line.to_string())))?;
            let token = token.trim();
            let at = TOKENS
                .iter()
                .position(|t| *t == token)
                .ok_or_else(|| refuse(Reason::UnknownToken(token.to_string())))?;
            let token = TOKENS[at];
            let value = value.trim();
            let color = value
                .strip_prefix('#')
                .filter(|h| h.len() == 6 && h.bytes().all(|b| b.is_ascii_hexdigit()))
                .and_then(|h| u32::from_str_radix(h, 16).ok())
                .ok_or_else(|| {
                    refuse(Reason::NotAColor {
                        token,
                        value: value.to_string(),
                    })
                })?;
            if colors[at].replace(color).is_some() {
                return Err(refuse(Reason::Twice(token)));
            }
        }
        let mut table = Self {
            name: name.to_string(),
            enhanced: palette(name).is_some_and(|p| p.enhanced),
            colors: [0; 15],
        };
        for (at, color) in colors.into_iter().enumerate() {
            table.colors[at] = color.ok_or_else(|| refuse(Reason::Missing(TOKENS[at])))?;
        }
        for (fg, bg, kind) in PAIRS {
            let ratio = table.ratio(fg, bg);
            let floor = table.floor(kind);
            if ratio < floor {
                return Err(refuse(Reason::BelowFloor {
                    fg,
                    bg,
                    ratio,
                    floor,
                }));
            }
        }
        Ok(table)
    }

    /// The compiled palette `p`, as a table.
    pub fn compiled(p: &Palette) -> Self {
        Self::parse(p.name, &p.declarations()).expect("the compiled palettes parse")
    }

    /// The palette's name, as in `urn:iki:ui:palette:{name}`.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// The color of `token` (one of [`TOKENS`], without its `--`) as `0xRRGGBB`.
    pub fn get(&self, token: &str) -> Option<u32> {
        TOKENS
            .iter()
            .position(|t| *t == token)
            .map(|at| self.colors[at])
    }

    /// The floor `kind` is held to in this palette, as [`Palette::floor`].
    pub fn floor(&self, kind: Kind) -> f64 {
        floor(self.enhanced, kind)
    }

    /// The WCAG contrast ratio of `fg` on `bg`, with ikigai-a11y's math (0 for a token that is
    /// not one of [`TOKENS`]).
    pub fn ratio(&self, fg: &str, bg: &str) -> f64 {
        let rgba = |c: u32| ikigai_a11y::Rgba::rgb((c >> 16) as u8, (c >> 8) as u8, c as u8);
        match (self.get(fg), self.get(bg)) {
            (Some(f), Some(b)) => ikigai_a11y::ratio(rgba(f), rgba(b)),
            _ => 0.0,
        }
    }

    /// The table as CSS custom-property declarations, written from the parsed colors in
    /// [`TOKENS`] order, as [`Palette::declarations`] writes a compiled one: what the token
    /// stylesheet fills a palette slot with.
    pub fn declarations(&self) -> String {
        TOKENS
            .iter()
            .zip(self.colors)
            .map(|(t, c)| format!("  --{t}: #{c:06x};"))
            .collect::<Vec<_>>()
            .join("\n")
    }
}
