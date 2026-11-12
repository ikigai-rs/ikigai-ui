//! This crate turns GPUI's default features off, and GPUI's defaults are what give it a window
//! platform and a text system. Each one dropped fails silently: without `font-kit` on macOS GPUI
//! installs `NoopTextSystem` and every window draws its boxes with no text at all; without
//! `wayland` or `x11` on Linux and FreeBSD it falls back to its headless client and no window
//! opens (and with neither, the text system is `NoopTextSystem` there too); without
//! `windows-manifest` a Windows window is not per-monitor DPI aware and draws blurred when scaled.
//! The window tests cannot see any of that: they run on GPUI's test platform, whose text system
//! is its own. So each target's manifest line is what this pins, here and in the reader app. On
//! Linux and FreeBSD the line is this crate's `wayland` and `x11` features (Cargo.toml says why
//! they are features for now), and the reader's features of the same names forward to them.

/// The `gpui` line of the dependency table headed `[target.'cfg(<cfg>)'.dependencies]`.
fn gpui_line<'a>(manifest: &'a str, cfg: &str) -> &'a str {
    let header = format!("[target.'cfg({cfg})'.dependencies]");
    let table = manifest
        .split(header.as_str())
        .nth(1)
        .unwrap_or_else(|| panic!("a dependency table for {cfg}"));
    let table = table.split("\n[").next().unwrap_or(table);
    table
        .lines()
        .find(|line| line.starts_with("gpui "))
        .unwrap_or_else(|| panic!("gpui in the dependency table for {cfg}"))
}

/// The value of the feature `name = [...]` in the manifest's `[features]` table.
fn feature<'a>(manifest: &'a str, name: &str) -> &'a str {
    let table = manifest
        .split("\n[features]")
        .nth(1)
        .expect("a [features] table");
    let table = table.split("\n[").next().unwrap_or(table);
    table
        .lines()
        .find(|line| line.starts_with(&format!("{name} = ")))
        .unwrap_or_else(|| panic!("the `{name}` feature"))
}

const MACOS: &str = r#"target_os = "macos""#;
const WINDOWS: &str = r#"target_os = "windows""#;

/// Each target and the GPUI feature its window platform needs, with the reason.
const NEEDED: &[(&str, &str, &str)] = &[
    (MACOS, "font-kit", "or windows draw no text"),
    (WINDOWS, "windows-manifest", "or windows are not DPI aware"),
];

/// Linux and FreeBSD: this crate's feature and what it must turn on in GPUI.
const LINUX: &[(&str, &[&str])] = &[
    ("wayland", &["gpui/wayland", "gpui/font-kit"]),
    ("x11", &["gpui/x11", "gpui/font-kit"]),
];

fn check(manifest: &str) {
    for (cfg, feature, reason) in NEEDED {
        let gpui = gpui_line(manifest, cfg);
        assert!(
            gpui.contains(&format!("\"{feature}\"")),
            "the gpui dependency for {cfg} must enable `{feature}`, {reason}: {gpui}"
        );
    }
    for (name, needs) in LINUX {
        let line = feature(manifest, name);
        for need in *needs {
            assert!(
                line.contains(&format!("\"{need}\"")),
                "the `{name}` feature must enable `{need}`, or no window opens on Linux: {line}"
            );
        }
    }
}

#[test]
fn each_targets_gpui_dependency_keeps_its_window_platform_and_text_system() {
    check(include_str!("../Cargo.toml"));
}

#[test]
fn the_readers_manifest_names_the_same_backends() {
    let reader = include_str!("../../apps/reader/Cargo.toml");
    for (cfg, feature, _) in NEEDED {
        assert!(gpui_line(reader, cfg).contains(&format!("\"{feature}\"")));
    }
    for (name, _) in LINUX {
        let line = feature(reader, name);
        assert!(
            line.contains(&format!("\"ikigai-ui-gpui/{name}\"")),
            "{line}"
        );
    }
}

#[test]
fn the_check_fails_when_a_backend_is_dropped() {
    let manifest = include_str!("../Cargo.toml");
    for dropped in [
        manifest.replace(r#""gpui/x11", "#, ""),
        manifest.replace(r#""runtime_shaders", "font-kit""#, r#""runtime_shaders""#),
        manifest.replace(r#", "windows-manifest""#, ""),
    ] {
        assert_ne!(dropped, manifest);
        assert!(std::panic::catch_unwind(|| check(&dropped)).is_err());
    }
}
