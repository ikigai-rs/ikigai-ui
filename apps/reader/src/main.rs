//! The reader's native window, over a LOCAL layer host only (a development spike: the live
//! edge is never joined from here). `ikigai-reader <host config dir> <host state dir> <age key>`.
#[cfg(any(
    target_os = "macos",
    target_os = "linux",
    target_os = "freebsd",
    target_os = "windows"
))]
fn main() {
    use gpui::{px, size, App, AppContext, Application, Bounds, WindowBounds, WindowOptions};
    use ikigai_core::{Fallback, Scope};
    use ikigai_reader::{config_home, Descriptor, Identity, LocalDoor, Reader};
    use ikigai_ui_gpui::{descriptor_space, native::Screen, theme::Theme, worker::Worker};
    use std::sync::Arc;

    ikigai_ui_gpui::native::require_window_platform(
        "cargo run -p ikigai-reader --features wayland,x11 -- <arguments>",
    );
    let args: Vec<String> = std::env::args().skip(1).collect();
    let [config, state, key, descriptor] = &args[..] else {
        eprintln!("usage: ikigai-reader <host config> <host state> <host age key> <layers.json>");
        std::process::exit(2)
    };
    let fail = |e: String| -> ! {
        eprintln!("{e}");
        std::process::exit(1)
    };
    let key = ikigai_layers::at_rest::StateKey::load(std::path::Path::new(key))
        .unwrap_or_else(|e| fail(e.to_string()));
    let host =
        ikigai_layers::Host::open(config, state, &key).unwrap_or_else(|e| fail(e.to_string()));
    let identity =
        Identity::load_or_create(&config_home().unwrap_or_else(|| fail("no HOME".into())))
            .unwrap_or_else(|e| fail(e));
    let descriptor =
        Descriptor::parse(&std::fs::read(descriptor).unwrap_or_else(|e| fail(e.to_string())))
            .unwrap_or_else(|e| fail(e));
    let door = Arc::new(LocalDoor::new(host, "local", &identity.fingerprint));
    let reader = Arc::new(Reader::new(door, identity, descriptor));
    let root = Arc::new(Fallback::new(vec![
        Arc::new(descriptor_space()),
        Arc::new(ikigai_reader::space(reader)),
        Arc::new(ikigai_ui::space()),
        Arc::new(ikigai_ui::screen::screens()),
    ]));
    let kernel = Arc::new(ikigai_core::Kernel::new(root));
    let worker = Worker::new(kernel, ikigai_reader::capability(), Scope::empty());
    Application::new().run(move |cx: &mut App| {
        let bounds = Bounds::centered(None, size(px(820.), px(760.)), cx);
        cx.open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                ..Default::default()
            },
            move |window, cx| cx.new(|cx| Screen::new(worker, "feeds", Theme::Auto, window, cx)),
        )
        .expect("the reader window");
        cx.activate(true);
    });
}

#[cfg(not(any(
    target_os = "macos",
    target_os = "linux",
    target_os = "freebsd",
    target_os = "windows"
)))]
fn main() {
    eprintln!("GPUI has no window platform here.");
    std::process::exit(1);
}
