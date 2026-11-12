//! The demo host's native companion: one shared Store, a web door and GPUI windows, on macOS,
//! Linux and FreeBSD (Wayland or X11) and Windows.
#[cfg(any(
    target_os = "macos",
    target_os = "linux",
    target_os = "freebsd",
    target_os = "windows"
))]
fn main() {
    use gpui::{px, size, App, AppContext, Application, Bounds, WindowBounds, WindowOptions};
    use ikigai_core::{Capability, Fallback, Scope};
    use ikigai_ui_gpui::{descriptor_space, native::Screen, theme::Theme, worker::Worker};
    use std::sync::Arc;

    let store = ikigai_ui_demo::Store::default();
    let root = Arc::new(Fallback::new(vec![
        Arc::new(descriptor_space()),
        ikigai_ui_demo::root(store),
    ]));
    let kernel = Arc::new(ikigai_ui_demo::kernel(root));
    let web_kernel = kernel.clone();
    let addr = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "127.0.0.1:8080".into());
    // The second argument is a theme, as the HTML chooser names them; `auto` by default.
    let theme = match std::env::args().nth(2) {
        None => Theme::Auto,
        Some(name) => Theme::parse(&name).unwrap_or_else(|| {
            eprintln!(
                "unknown theme {name}: expected one of {:?}",
                ikigai_ui::THEMES
            );
            std::process::exit(2)
        }),
    };
    std::thread::spawn(move || {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("web runtime");
        runtime.block_on(async move {
            match tokio::net::TcpListener::bind(&addr).await {
                Ok(listener) => {
                    eprintln!("HTML demo: http://{}", listener.local_addr().unwrap());
                    if let Err(error) = ikigai_web::serve_with_listener(
                        web_kernel,
                        ikigai_ui_demo::cap(),
                        listener,
                        ikigai_ui_demo::door(),
                    )
                    .await
                    {
                        eprintln!("HTML door: {error}");
                    }
                }
                Err(error) => eprintln!("HTML door: {error}"),
            }
        });
    });
    // With neither WAYLAND_DISPLAY nor DISPLAY set, GPUI runs headless and no window opens.
    #[cfg(any(target_os = "linux", target_os = "freebsd"))]
    if ["WAYLAND_DISPLAY", "DISPLAY"]
        .iter()
        .all(|var| std::env::var_os(var).is_none_or(|v| v.is_empty()))
    {
        eprintln!(
            "No display: set WAYLAND_DISPLAY (Wayland) or DISPLAY (X11) to open the windows."
        );
        std::process::exit(1);
    }
    let capability = Capability::root().attenuate([ikigai_ui_demo::CONTACT_WRITE]);
    let worker = Worker::new(kernel, capability, Scope::empty());
    Application::new().run(move |cx: &mut App| {
        for screen in ["gallery", "elements", "contact"] {
            let worker = worker.clone();
            let bounds = Bounds::centered(None, size(px(820.), px(760.)), cx);
            cx.open_window(
                WindowOptions {
                    window_bounds: Some(WindowBounds::Windowed(bounds)),
                    ..Default::default()
                },
                move |window, cx| cx.new(|cx| Screen::new(worker, screen, theme, window, cx)),
            )
            .expect("native demo window");
        }
        cx.activate(true);
        cx.on_window_closed(|cx| {
            if cx.windows().is_empty() {
                cx.quit();
            }
        })
        .detach();
    });
}

#[cfg(not(any(
    target_os = "macos",
    target_os = "linux",
    target_os = "freebsd",
    target_os = "windows"
)))]
fn main() {
    eprintln!("GPUI has no window platform here. The snapshot worker is portable.");
    std::process::exit(1);
}
