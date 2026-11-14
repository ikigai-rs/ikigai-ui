//! Exercise the capture driver's orchestration without requiring a display or taking images.
#![cfg(unix)]

use std::{fs, os::unix::fs::PermissionsExt, path::Path, process::Command};

fn executable(dir: &Path, name: &str, body: &str) {
    let path = dir.join(name);
    fs::write(&path, format!("#!/bin/sh\n{body}\n")).unwrap();
    fs::set_permissions(path, fs::Permissions::from_mode(0o755)).unwrap();
}

#[test]
fn capture_driver_requires_a_backend_screen_access_and_images() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
    let dir = root.join(format!("target/capture-script-test-{}", std::process::id()));
    fs::create_dir_all(dir.join("scripts")).unwrap();
    fs::create_dir_all(dir.join("bin")).unwrap();
    fs::create_dir_all(dir.join("target/debug")).unwrap();
    fs::copy(
        root.join("scripts/gpui-screenshots.sh"),
        dir.join("scripts/gpui-screenshots.sh"),
    )
    .unwrap();
    let bin = dir.join("bin");
    executable(&bin, "uname", "echo Linux");
    executable(&bin, "cargo", "echo \"$*\" > build-args");
    executable(&bin, "xdotool", "printf '11\\n12\\n13\\n'");
    executable(&bin, "sleep", ":");
    executable(&bin, "oxipng", ":");
    executable(
        &dir.join("target/debug"),
        "ikigai-ui-demo-native",
        "exec /bin/sleep 60",
    );
    // Some screenshot tools return success without producing an image (observed on macOS).
    executable(&bin, "import", ":");
    let run = || {
        Command::new("bash")
            .arg(dir.join("scripts/gpui-screenshots.sh"))
            .env(
                "PATH",
                format!("{}:{}", bin.display(), std::env::var("PATH").unwrap()),
            )
            .env("DISPLAY", ":test")
            .output()
            .unwrap()
    };
    fs::create_dir_all(dir.join("docs/screenshots")).unwrap();
    let old = dir.join("docs/screenshots/gpui-gallery-light.png");
    fs::write(&old, "previous image").unwrap();
    let result = run();
    let args = fs::read_to_string(dir.join("build-args")).unwrap();
    assert!(
        args.contains("--features x11"),
        "Linux capture needs a window backend: {args}"
    );
    assert!(
        !result.status.success(),
        "a capture that wrote no image must fail"
    );
    assert_eq!(fs::read_to_string(&old).unwrap(), "previous image");

    // With working capture tooling, both themes and all three fixture windows are published.
    executable(&bin, "import", "printf 'test image' > \"$3\"");
    let result = run();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    for theme in ["light", "dark"] {
        for screen in ["gallery", "elements", "contact"] {
            assert!(dir
                .join(format!("docs/screenshots/gpui-{screen}-{theme}.png"))
                .is_file());
        }
    }
    // macOS should diagnose permission denial before compiling or launching the demo.
    fs::remove_file(dir.join("build-args")).unwrap();
    executable(&bin, "uname", "echo Darwin");
    executable(&bin, "swift", "exit 1");
    executable(&bin, "screencapture", ":");
    let result = run();
    assert!(!result.status.success());
    assert!(String::from_utf8_lossy(&result.stderr).contains("Screen Recording access required"));
    assert!(!dir.join("build-args").exists());
    fs::remove_dir_all(dir).unwrap();
}
