//! The native face's theme: which shared palette (`urn:iki:ui:palette:{name}`) it draws with.
//!
//! The palette is data the kernel serves, so a host that overrides `urn:iki:ui:palette:dark`
//! repaints the native windows exactly as it repaints the HTML face. [`Tokens::parse`] reads the
//! palette resource's `text/plain` face (one `--token: #rrggbb;` declaration per line) and refuses
//! a palette that leaves out any of [`ikigai_ui::palette::TOKENS`].
use std::collections::BTreeMap;

use ikigai_core::Error;
use ikigai_ui::palette::{Palette, TOKENS};

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
    colors: BTreeMap<&'static str, u32>,
}

impl Tokens {
    /// Parse the `text/plain` face of `urn:iki:ui:palette:{name}`.
    pub fn parse(palette: &str, declarations: &str) -> Result<Self, Error> {
        let refuse = |detail: String| Error::InvalidArgument {
            name: "palette".into(),
            detail: format!("urn:iki:ui:palette:{palette}: {detail}"),
        };
        let mut found = BTreeMap::new();
        for line in declarations
            .lines()
            .map(str::trim)
            .filter(|l| !l.is_empty())
        {
            let declaration = line.strip_suffix(';').unwrap_or(line);
            let (name, value) = declaration
                .strip_prefix("--")
                .and_then(|d| d.split_once(':'))
                .ok_or_else(|| refuse(format!("not a token declaration: {line}")))?;
            let hex = value.trim().strip_prefix('#').filter(|h| h.len() == 6);
            let color = hex
                .and_then(|h| u32::from_str_radix(h, 16).ok())
                .ok_or_else(|| refuse(format!("--{}: expected #rrggbb", name.trim())))?;
            found.insert(name.trim().to_string(), color);
        }
        let mut colors = BTreeMap::new();
        for token in TOKENS {
            let color = found
                .get(token)
                .ok_or_else(|| refuse(format!("no --{token}")))?;
            colors.insert(token, *color);
        }
        Ok(Self {
            palette: palette.into(),
            colors,
        })
    }

    /// The compiled table, which a window draws with only until the resource arrives (or when
    /// the kernel cannot serve it).
    pub fn compiled(palette: &Palette) -> Self {
        Self::parse(palette.name, &palette.declarations()).expect("the compiled palettes parse")
    }

    /// The color of `token` (one of [`TOKENS`]) as `0xRRGGBB`.
    pub fn get(&self, token: &str) -> u32 {
        self.colors[token]
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
