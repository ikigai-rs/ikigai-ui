//! GPUI's Linux and FreeBSD window platforms (this crate's `wayland` and `x11` features) link
//! libxkbcommon, libxkbcommon-x11 and libxcb by their development names (`libxkbcommon.so`), which
//! only the `-dev` packages install; a desktop has the runtime libraries (`libxkbcommon.so.0`)
//! without them. When a development name is missing, this links the runtime library through a
//! symlink in OUT_DIR, so building needs no `-dev` package; with them installed it does nothing.
//! A runtime library that is absent too (GitHub's stock Ubuntu runner has no libxkbcommon-x11)
//! still fails the link, naming it, and this warns which.

fn main() {
    println!("cargo::rerun-if-changed=build.rs");
    let os = std::env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();
    if os != "linux" && os != "freebsd" {
        return;
    }
    let out = std::path::PathBuf::from(std::env::var_os("OUT_DIR").expect("OUT_DIR"));
    let dirs = [
        "/usr/lib/x86_64-linux-gnu",
        "/usr/lib/aarch64-linux-gnu",
        "/usr/lib64",
        "/usr/lib",
        "/usr/local/lib",
        "/lib/x86_64-linux-gnu",
        "/lib/aarch64-linux-gnu",
    ];
    let mut linked = false;
    for (name, soname) in [
        ("xkbcommon", "libxkbcommon.so.0"),
        ("xkbcommon-x11", "libxkbcommon-x11.so.0"),
        ("xcb", "libxcb.so.1"),
    ] {
        let dev = format!("lib{name}.so");
        if dirs
            .iter()
            .any(|d| std::path::Path::new(d).join(&dev).exists())
        {
            continue;
        }
        let Some(runtime) = dirs
            .iter()
            .map(|d| std::path::Path::new(d).join(soname))
            .find(|p| p.exists())
        else {
            println!("cargo::warning=no {dev} or {soname}: install the {name} library (gpui/README.md, \"Linux\")");
            continue;
        };
        let link = out.join(&dev);
        let _ = std::fs::remove_file(&link);
        #[cfg(unix)]
        std::os::unix::fs::symlink(&runtime, &link).expect("a symlink in OUT_DIR");
        #[cfg(not(unix))]
        let _ = runtime;
        linked = true;
    }
    if linked {
        println!("cargo::rustc-link-search=native={}", out.display());
    }
}
