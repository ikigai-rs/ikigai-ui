//! The native face's theme: which shared palette (`urn:iki:ui:palette:{name}`) it draws with.
//!
//! The palette is data the kernel serves, so a host that overrides `urn:iki:ui:palette:dark`
//! repaints the native windows exactly as it repaints the HTML face. [`Tokens::parse`] reads the
//! palette resource's `text/plain` face (one `--token: #rrggbb;` declaration per line) into the
//! same [`Table`] the token stylesheet is written from, so a palette the HTML face refuses (an
//! unknown or missing token, a value that is not a color, a pair below the contrast floor) the
//! native face refuses too (ledger #3529).
use ikigai_core::Error;
use ikigai_ui::palette::{Palette, Table, TOKENS};

/// What a native window draws with, as the HTML theme chooser offers ([`ikigai_ui::THEMES`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Theme {
    /// Follow the window's system appearance (light or dark), live.
    #[default]
    Auto,
    /// The light palette.
    Light,
    /// The dark palette.
    Dark,
    /// The high-contrast palette (every pair at 7:1).
    HighContrast,
}

impl Theme {
    /// The theme a `theme=` value names: one of [`ikigai_ui::THEMES`].
    pub fn parse(name: &str) -> Option<Self> {
        Some(match name {
            "auto" => Self::Auto,
            "light" => Self::Light,
            "dark" => Self::Dark,
            "high-contrast" => Self::HighContrast,
            _ => return None,
        })
    }

    /// The palette this theme draws with in a window whose system appearance is (or is not) dark.
    ///
    /// GPUI 0.2.2 reports light or dark only, not macOS "Increase contrast", so `auto` never
    /// picks high contrast by itself; choose [`Theme::HighContrast`] explicitly.
    pub fn palette(self, dark: bool) -> &'static str {
        match self {
            Self::Auto if dark => "dark",
            Self::Auto | Self::Light => "light",
            Self::Dark => "dark",
            Self::HighContrast => "high-contrast",
        }
    }
}

/// One palette as the native face reads it: every token's color as `0xRRGGBB`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Tokens {
    /// The palette's name, as in `urn:iki:ui:palette:{name}`.
    pub palette: String,
    table: Table,
}

impl Tokens {
    /// Parse the `text/plain` face of `urn:iki:ui:palette:{name}` with [`Table::parse`].
    pub fn parse(palette: &str, declarations: &str) -> Result<Self, Error> {
        Ok(Self {
            palette: palette.into(),
            table: Table::parse(palette, declarations)?,
        })
    }

    /// The compiled table, which a window draws with only until the resource arrives (or when
    /// the kernel cannot serve it).
    pub fn compiled(palette: &Palette) -> Self {
        Self {
            palette: palette.name.into(),
            table: Table::compiled(palette),
        }
    }

    /// The color of `token` (one of [`TOKENS`]) as `0xRRGGBB`.
    pub fn get(&self, token: &str) -> u32 {
        self.table
            .get(token)
            .unwrap_or_else(|| panic!("--{token} is not one of {TOKENS:?}"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_compiled_palette_parses_to_its_own_values() {
        for p in ikigai_ui::palette::PALETTES {
            let tokens = Tokens::compiled(&p);
            for (token, value) in p.colors {
                assert_eq!(format!("#{:06x}", tokens.get(token)), value, "{}", p.name);
            }
        }
    }

    #[test]
    fn a_palette_missing_a_token_is_refused() {
        let light = ikigai_ui::palette::LIGHT.declarations();
        let partial: String = light.lines().take(2).collect::<Vec<_>>().join("\n");
        assert!(Tokens::parse("partial", &partial).is_err());
        assert!(Tokens::parse("bad", "  --bg: black;").is_err());
    }

    #[test]
    fn a_palette_below_the_floor_is_refused_as_the_html_face_refuses_it() {
        let p = ikigai_ui::palette::DARK;
        let dark = p.declarations();
        let value = |token| p.get(token).unwrap();
        // --fg drawn in the decorative rule's color: far below 4.5:1 on --bg.
        let dim = dark.replace(
            &format!("--fg: {};", value("fg")),
            &format!("--fg: {};", value("rule")),
        );
        let Err(Error::InvalidArgument { detail, .. }) = Tokens::parse("dark", &dim) else {
            panic!("a palette below the floor drew");
        };
        assert!(detail.contains("--fg on --bg"), "{detail}");
        let bg = format!("--bg: {};", value("bg"));
        let injected = dark.replace(&bg, &format!("{bg} }} body {{ color: red"));
        assert!(Tokens::parse("dark", &injected).is_err());
    }

    #[test]
    fn auto_follows_the_appearance_and_explicit_themes_do_not() {
        assert_eq!(Theme::Auto.palette(false), "light");
        assert_eq!(Theme::Auto.palette(true), "dark");
        for dark in [false, true] {
            assert_eq!(Theme::Light.palette(dark), "light");
            assert_eq!(Theme::Dark.palette(dark), "dark");
            assert_eq!(Theme::HighContrast.palette(dark), "high-contrast");
        }
        for name in ikigai_ui::THEMES {
            assert!(Theme::parse(name).is_some(), "{name}");
        }
    }
}
