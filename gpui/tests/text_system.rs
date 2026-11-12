//! This crate turns GPUI's default features off, and GPUI's defaults are what give it a window
//! platform and a text system. Each one dropped fails silently: without `font-kit` on macOS GPUI
//! installs `NoopTextSystem` and every window draws its boxes with no text at all; without
//! `wayland` or `x11` on Linux and FreeBSD it falls back to its headless client and no window
//! opens (and with neither, the text system is `NoopTextSystem` there too); without
//! `windows-manifest` a Windows window is not per-monitor DPI aware and draws blurred when scaled.
//! The window tests cannot see any of that: they run on GPUI's test platform, whose text system
//! is its own. So each target's manifest line is what this pins, here and in the reader app.

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

const MACOS: &str = r#"target_os = "macos""#;
const LINUX: &str = r#"any(target_os = "linux", target_os = "freebsd")"#;
const WINDOWS: &str = r#"target_os = "windows""#;

/// Each target and the features its window platform needs, with the reason.
const NEEDED: &[(&str, &str, &str)] = &[
    (MACOS, "font-kit", "or windows draw no text"),
    (LINUX, "wayland", "or no window opens on Wayland"),
    (LINUX, "x11", "or no window opens on X11"),
    (LINUX, "font-kit", "or cosmic-text finds no system fonts"),
    (WINDOWS, "windows-manifest", "or windows are not DPI aware"),
];

fn check(manifest: &str, crate_name: &str) {
    for (cfg, feature, reason) in NEEDED {
        let gpui = gpui_line(manifest, cfg);
        assert!(
            gpui.contains(&format!("\"{feature}\"")),
            "{crate_name}'s gpui dependency for {cfg} must enable `{feature}`, {reason}: {gpui}"
        );
    }
}

#[test]
fn each_targets_gpui_dependency_keeps_its_window_platform_and_text_system() {
    check(include_str!("../Cargo.toml"), "ikigai-ui-gpui");
}

#[test]
fn the_readers_gpui_dependency_names_the_same_backends() {
    check(
        include_str!("../../apps/reader/Cargo.toml"),
        "ikigai-reader",
    );
}

#[test]
fn the_check_fails_when_a_backend_is_dropped() {
    let manifest = include_str!("../Cargo.toml");
    let dropped = manifest.replace(r#""wayland", "x11", "#, "");
    assert!(std::panic::catch_unwind(|| check(&dropped, "dropped")).is_err());
}
