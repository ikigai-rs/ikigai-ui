//! The contrast floor (UI conventions D12), measured with ikigai-a11y's own math.
//!
//! [`measure`] walks every pair in [`PAIRS`](crate::palette::PAIRS) for every palette it is
//! given and returns one row per pair, failing or not, so a test can both assert the floor and
//! print the table. A repo whose own palette tables have not moved here yet builds
//! [`Palette`](crate::palette::Palette) values for them and runs the same check.

use ikigai_a11y::{ratio, Rgba};

use crate::palette::{Kind, Palette, PAIRS};

/// One measured pair.
#[derive(Debug, Clone, PartialEq)]
pub struct Row {
    /// The palette's name.
    pub palette: &'static str,
    /// The foreground token.
    pub fg: &'static str,
    /// The ground token.
    pub bg: &'static str,
    /// Text or non-text.
    pub kind: Kind,
    /// The measured contrast ratio.
    pub ratio: f64,
    /// The floor this pair is held to in this palette.
    pub floor: f64,
}

impl Row {
    /// Whether the pair clears its floor.
    pub fn passes(&self) -> bool {
        self.ratio >= self.floor
    }
}

impl std::fmt::Display for Row {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{}: --{} on --{} is {:.2}:1 (floor {}:1){}",
            self.palette,
            self.fg,
            self.bg,
            self.ratio,
            self.floor,
            if self.passes() { "" } else { " FAILS" }
        )
    }
}

/// Every pair of every palette in `palettes`, measured. A token a palette does not define, or
/// a value that is not a color, is a row with ratio 0, so it fails rather than vanishing.
pub fn measure(palettes: &[Palette]) -> Vec<Row> {
    let color = |p: &Palette, token: &str| p.get(token).and_then(|v| Rgba::parse(v).ok());
    let mut rows = Vec::new();
    for p in palettes {
        for (fg, bg, kind) in PAIRS {
            let measured = match (color(p, fg), color(p, bg)) {
                (Some(f), Some(b)) => ratio(f, b),
                _ => 0.0,
            };
            rows.push(Row {
                palette: p.name,
                fg,
                bg,
                kind,
                ratio: measured,
                floor: p.floor(kind),
            });
        }
    }
    rows
}

/// The rows of [`measure`] that fail their floor.
///
/// ```
/// let failing = ikigai_ui::testkit::contrast::failures(&ikigai_ui::palette::PALETTES);
/// assert!(failing.is_empty(), "{failing:?}");
/// ```
pub fn failures(palettes: &[Palette]) -> Vec<Row> {
    measure(palettes)
        .into_iter()
        .filter(|r| !r.passes())
        .collect()
}
