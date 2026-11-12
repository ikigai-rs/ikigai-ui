//! D12: the palettes clear the contrast floor, the token stylesheet carries every palette
//! behind the right selector, and no color is written anywhere but the palette tables. A palette
//! is read as data (ledger #3529): a rebound one is parsed and held to the floor before the
//! stylesheet is written from it, so it can carry no CSS.

mod common;

use std::sync::Arc;

use common::*;
use ikigai_core::{
    EndpointSpace, Error, Exact, Fallback, FnEndpoint, Kernel, ReprType, Representation, Space,
};
use ikigai_ui::palette::{
    palette, Kind, Reason, Table, FLOOR_DECORATIVE, FLOOR_ENHANCED, FLOOR_NON_TEXT, PALETTES,
    TOKENS,
};
use ikigai_ui::testkit::{contrast, lint};

#[test]
fn every_palette_clears_the_contrast_floor() {
    let rows = contrast::measure(&PALETTES);
    for row in &rows {
        println!("{row}");
    }
    let failing: Vec<String> = rows
        .iter()
        .filter(|r| !r.passes())
        .map(|r| r.to_string())
        .collect();
    assert!(failing.is_empty(), "{}", failing.join("\n"));
    assert_eq!(rows.len(), PALETTES.len() * ikigai_ui::palette::PAIRS.len());
}

#[test]
fn every_palette_defines_every_token_in_order() {
    for p in PALETTES {
        let names: Vec<&str> = p.colors.iter().map(|(t, _)| *t).collect();
        assert_eq!(names, TOKENS, "{}", p.name);
        for (t, v) in p.colors {
            assert!(
                ikigai_a11y::Rgba::parse(v).is_ok() && v.len() == 7,
                "{}: --{t} is {v}, not #rrggbb",
                p.name
            );
        }
    }
}

#[test]
fn the_stylesheet_carries_each_palette_behind_its_selector() {
    let css = text(&host(None), ikigai_ui::TOKENS_CSS, &[]);
    for (selector, name) in [
        (":root {\n  color-scheme: light;", "light"),
        ("@media (prefers-color-scheme: dark) {\n  :root:not([data-theme]), :root[data-theme=\"auto\"] {\n    color-scheme: dark;", "dark"),
        ("@media (prefers-contrast: more) {\n  :root:not([data-theme]), :root[data-theme=\"auto\"] {\n    color-scheme: dark;", "high-contrast"),
        (":root[data-theme=\"light\"] {\n  color-scheme: light;", "light"),
        (":root[data-theme=\"dark\"] {\n  color-scheme: dark;", "dark"),
        (":root[data-theme=\"high-contrast\"] {\n  color-scheme: dark;", "high-contrast"),
    ] {
        let block = css
            .split_once(selector)
            .unwrap_or_else(|| panic!("`{selector}` in the stylesheet"))
            .1;
        let declarations = palette(name).unwrap().declarations();
        assert!(
            block.contains(&declarations),
            "`{selector}` is followed by the {name} palette"
        );
    }
    // The contrast setting comes after the dark scheme, so on a dark system that asks for more
    // contrast, high contrast wins.
    assert!(
        css.find("@media (prefers-contrast: more)").unwrap()
            > css.find("@media (prefers-color-scheme: dark)").unwrap()
    );
    assert!(!css.contains("light-dark("), "no light-dark() (D12)");
}

#[test]
fn no_artifact_writes_a_color_and_every_var_is_defined() {
    let kernel = host(None);
    let css = text(&kernel, ikigai_ui::TOKENS_CSS, &[]);
    for a in &ikigai_ui::ARTIFACTS {
        if a.iri == "urn:iki:ui:htmx" {
            continue; // vendored as it is; it states no color (includeIndicatorStyles is off)
        }
        let source = std::str::from_utf8(a.bytes).unwrap();
        assert!(
            lint::hex_colors(source).is_empty(),
            "{}: {:?}",
            a.iri,
            lint::hex_colors(source)
        );
    }
    let undefined = lint::undefined_vars(&[&css]);
    assert!(
        undefined.is_empty(),
        "used and never defined: {undefined:?}"
    );
    // Every color in the composed stylesheet is a palette value.
    let values: Vec<String> = PALETTES
        .iter()
        .flat_map(|p| p.colors.iter().map(|(_, v)| v.to_string()))
        .collect();
    for hex in lint::hex_colors(&css) {
        assert!(
            values.contains(&hex),
            "{hex} in tokens.css is not a palette value"
        );
    }
}

#[test]
fn the_turtle_face_is_one_triple_per_token() {
    let kernel = host(None);
    for p in PALETTES {
        let iri = format!("urn:iki:ui:palette:{}", p.name);
        let rep = issue(
            &kernel,
            ikigai_core::Verb::Source,
            &iri,
            &[("as", b"text/turtle")],
        )
        .unwrap();
        assert_eq!(rep.repr_type.media_type, "text/turtle");
        let ttl = String::from_utf8(rep.bytes).unwrap();
        assert_eq!(ttl.lines().count(), TOKENS.len());
        assert!(!ttl.contains("_:"), "no blank nodes");
        for (t, v) in p.colors {
            assert!(ttl.contains(&format!("<{iri}> <urn:iki:ui:token:{t}> \"{v}\" .\n")));
        }
    }
    assert!(matches!(
        source(&kernel, "urn:iki:ui:palette:chartreuse"),
        Err(ikigai_core::Error::NotFound(_))
    ));
}

#[test]
fn every_palette_splits_line_from_rule() {
    use ikigai_ui::palette::PAIRS;
    let rows = contrast::measure(&PALETTES);
    for p in PALETTES {
        assert!(p.get("line").is_some(), "{}: --line", p.name);
        assert!(p.get("rule").is_some(), "{}: --rule", p.name);
    }
    // --line is a component border (SC 1.4.11): 3:1 or better on both grounds, everywhere.
    for ground in ["bg", "surface"] {
        assert!(
            PAIRS.contains(&("line", ground, Kind::NonText)),
            "--line on --{ground}"
        );
        assert!(
            PAIRS.iter().any(|(f, b, _)| *f == "rule" && *b == ground),
            "--rule on --{ground}"
        );
    }
    for r in rows.iter().filter(|r| r.fg == "line") {
        assert!(r.ratio >= FLOOR_NON_TEXT, "{r}");
    }
    // --rule is decorative and exempt from 3:1, but it has a stated minimum, and high contrast
    // holds it to its AAA floor like every other pair.
    for r in rows.iter().filter(|r| r.fg == "rule") {
        assert!(r.ratio >= FLOOR_DECORATIVE, "{r}");
        if r.palette == "high-contrast" {
            assert!(r.ratio >= FLOOR_ENHANCED, "{r}");
        }
    }
    // gonk's own light divider, which the 3:1 floor refused for --line.
    assert_eq!(palette("light").unwrap().get("rule"), Some("#c9d1d9"));
}

/// A host that rebinds `urn:iki:ui:palette:{name}` to `body`, ahead of the UI space, as a host
/// with its own (or a user's) palette does.
fn rebound(name: &str, body: String) -> Kernel {
    let mine = EndpointSpace::new().bind(
        Exact::new(format!("urn:iki:ui:palette:{name}")),
        FnEndpoint::new("host-palette", move |_| {
            Ok(
                Representation::new(ReprType::new("text/plain"), body.clone().into_bytes())
                    .cacheable(),
            )
        }),
    );
    let spaces: Vec<Arc<dyn Space>> = vec![Arc::new(mine), Arc::new(ikigai_ui::space())];
    kernel(Arc::new(Fallback::new(spaces)))
}

/// `name`'s compiled declarations with `token`'s value replaced by `value`.
fn with(name: &str, token: &str, value: &str) -> String {
    let p = palette(name).unwrap();
    let old = format!("--{token}: {};", p.get(token).unwrap());
    let declarations = p.declarations();
    assert!(declarations.contains(&old));
    declarations.replace(&old, &format!("--{token}: {value};"))
}

/// What `urn:iki:ui:tokens.css` answers over a host that rebinds `name` to `body`.
fn tokens_over(name: &str, body: String) -> Result<String, Error> {
    source(&rebound(name, body), ikigai_ui::TOKENS_CSS).map(|r| String::from_utf8(r.bytes).unwrap())
}

fn refusal(result: Result<String, Error>) -> String {
    match result {
        Ok(css) => panic!("served:\n{css}"),
        Err(Error::InvalidArgument { name, detail }) if name == "palette" => detail,
        Err(other) => panic!("refused, but not as a palette: {other}"),
    }
}

#[test]
fn a_rebound_palette_carrying_css_is_refused_not_spliced() {
    // The audit's case (#3525): before #3529 this reached tokens.css as written.
    for injected in [
        "#0d1117; } body { background: url(https://evil.example/x)",
        "#0d1117; color: red",
        "#0d1117 } :root { --bg: red",
        "red",
        "#0d11",
        "#0d1117ff",
        "var(--fg)",
        "url(https://evil.example/x)",
    ] {
        let detail = refusal(tokens_over("dark", with("dark", "bg", injected)));
        assert!(
            detail.starts_with("urn:iki:ui:palette:dark: --bg:"),
            "{injected}: {detail}"
        );
        assert!(detail.contains("is not #rrggbb"), "{injected}: {detail}");
    }
    // A line that is not a declaration, and a token that is not one.
    let mut extra = palette("dark").unwrap().declarations();
    extra.push_str("\n} body { color: red; }");
    assert!(refusal(tokens_over("dark", extra)).contains("not a `--token: #rrggbb;` declaration"));
    let mut unknown = palette("dark").unwrap().declarations();
    unknown.push_str("\n  --x}body{color: #ff0000;");
    assert!(refusal(tokens_over("dark", unknown)).contains("is not a token"));
}

#[test]
fn a_rebound_palette_below_the_floor_is_refused_naming_the_pair() {
    // --muted in the decorative rule's color: below 4.5:1 on --bg.
    let rule = palette("light").unwrap().get("rule").unwrap();
    let detail = refusal(tokens_over("light", with("light", "muted", rule)));
    assert!(
        detail.starts_with("urn:iki:ui:palette:light: --muted on --bg is "),
        "{detail}"
    );
    assert!(detail.ends_with("below its 4.5:1 floor"), "{detail}");
    // High contrast holds every pair to 7:1, an override included: 6.1:1 on --surface is
    // refused there though it clears 4.5:1.
    let detail = refusal(tokens_over(
        "high-contrast",
        with("high-contrast", "muted", "#999999"),
    ));
    assert!(detail.contains("--muted on --surface is "), "{detail}");
    assert!(detail.ends_with("below its 7:1 floor"), "{detail}");
}

#[test]
fn a_valid_rebound_palette_is_served_as_its_parsed_table() {
    // A darker accent clears every floor. The stylesheet carries it wherever the light palette
    // goes (`:root` and `data-theme="light"`), written from the parsed table: uppercase hex
    // comes out lowercase, as every compiled value is written.
    let body = with("light", "accent", "#0A4F96");
    let css = tokens_over("light", body.clone()).unwrap();
    let table = Table::parse("light", &body).unwrap();
    assert_eq!(table.get("accent"), Some(0x0a4f96));
    assert_eq!(css.matches(&table.declarations()).count(), 2, "{css}");
    assert!(!css.contains(&palette("light").unwrap().declarations()));
    assert!(!css.contains("#0A4F96"));
}

#[test]
fn every_compiled_palette_parses_to_its_own_table() {
    for p in PALETTES {
        let table = Table::compiled(&p);
        assert_eq!(table.name(), p.name);
        assert_eq!(table.declarations(), p.declarations(), "{}", p.name);
        for (fg, bg, kind) in ikigai_ui::palette::PAIRS {
            assert!(
                table.ratio(fg, bg) >= p.floor(kind),
                "{}: --{fg} on --{bg}",
                p.name
            );
        }
    }
}

#[test]
fn the_parse_refuses_every_shape_but_a_full_table() {
    let light = palette("light").unwrap().declarations();
    let reason = |name: &str, text: &str| Table::parse(name, text).unwrap_err().reason;
    assert_eq!(reason("Light", &light), Reason::Name);
    assert_eq!(reason("", &light), Reason::Name);
    let partial: String = light.lines().take(14).collect::<Vec<_>>().join("\n");
    assert_eq!(reason("light", &partial), Reason::Missing("focus"));
    let twice = format!("{light}\n  --bg: #ffffff;");
    assert_eq!(reason("light", &twice), Reason::Twice("bg"));
    assert_eq!(
        reason("light", &format!("{light}\n  --radius: #000000;")),
        Reason::UnknownToken("radius".into())
    );
    assert!(matches!(
        reason("light", &format!("{light}\n  color: red;")),
        Reason::NotADeclaration(_)
    ));
    assert!(matches!(
        reason("light", &with("light", "bg", "#ffffff;")),
        Reason::NotAColor { token: "bg", .. }
    ));
    // Blank lines, no final `;` and spaces around the colon are a table still.
    let loose = light.replace(": ", " :  ").replace(";\n", "\n\n");
    assert_eq!(
        Table::parse("light", &loose).unwrap(),
        Table::compiled(palette("light").unwrap())
    );
}

#[test]
fn a_template_marker_other_than_a_palette_slot_is_refused() {
    let dir = scratch("tokens-template-marker");
    let template = std::str::from_utf8(
        ikigai_ui::artifact(ikigai_ui::TOKENS_TEMPLATE)
            .unwrap()
            .bytes,
    )
    .unwrap();
    std::fs::write(
        dir.join("tokens.template.css"),
        format!("{template}\n/* $r{{urn:iki:ui:palette:dark}} */\n"),
    )
    .unwrap();
    let err = source(&host(Some(&dir)), ikigai_ui::TOKENS_CSS).unwrap_err();
    assert!(err.to_string().contains("fills palette slots"), "{err}");
    // `$$` is a literal `$`, as in a compose template.
    std::fs::write(
        dir.join("tokens.template.css"),
        format!("{template}/* $$ */\n"),
    )
    .unwrap();
    let css = text(&host(Some(&dir)), ikigai_ui::TOKENS_CSS, &[]);
    assert!(css.ends_with("/* $ */\n"));
}
