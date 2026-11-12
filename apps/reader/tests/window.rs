//! The reader's native window on GPUI's test platform (macOS; CI's Linux jobs compile this to
//! nothing): the feeds screen, follow a feed, its entries newest first, follow an entry from
//! the keyboard. The
//! layer host is a LOCAL one in this process, never the live edge.
#![cfg(target_os = "macos")]

use std::sync::Arc;
use std::time::{Duration, Instant};

use gpui::{Entity, TestAppContext, VisualTestContext};
use ikigai_core::{Fallback, Kernel, Scope};
use ikigai_ui::model::Node;
use ikigai_ui_gpui::{descriptor_space, native::Screen, theme::Theme, worker::Worker};

mod common;
use common::{host, publish, reader, Scratch};

fn settle(screen: &Entity<Screen>, cx: &mut VisualTestContext) {
    let deadline = Instant::now() + Duration::from_secs(10);
    while !screen.update(cx, |s, cx| {
        s.poll(cx);
        s.settled()
    }) {
        assert!(Instant::now() < deadline, "the screen never settled");
        std::thread::sleep(Duration::from_millis(5));
    }
    cx.run_until_parked();
}

use ikigai_ui_gpui::text as prop;

fn every<'a>(node: &'a Node, kind: &str, out: &mut Vec<&'a Node>) {
    if node.kind == kind {
        out.push(node);
    }
    node.children.iter().for_each(|c| every(c, kind, out));
}

fn of_kind(screen: &Entity<Screen>, cx: &mut VisualTestContext, kind: &str) -> Vec<Node> {
    screen.read_with(cx, |s, _| {
        assert!(s.message().is_empty(), "{}", s.message());
        let mut out = Vec::new();
        every(s.snapshot().expect("a screen").root(), kind, &mut out);
        out.into_iter().cloned().collect()
    })
}

fn href(links: &[Node], label: &str) -> String {
    let link = links
        .iter()
        .find(|l| prop(l, "label") == Some(label))
        .unwrap_or_else(|| panic!("no link {label}"));
    prop(link, "href").unwrap().to_string()
}

#[gpui::test]
fn feeds_then_a_feed_newest_first_then_an_entry(cx: &mut TestAppContext) {
    let local = host();
    publish(&local, "First", "Hello");
    publish(
        &local,
        "Second",
        "# Big\n\nA *para* <script>alert(1)</script>",
    );
    let home = Scratch::new("home-window");
    let reader = reader(&local, &home.0.join("reader"));
    let root = Arc::new(Fallback::new(vec![
        Arc::new(descriptor_space()),
        Arc::new(ikigai_reader::space(reader)),
        Arc::new(ikigai_ui::space()),
        Arc::new(ikigai_ui::screen::screens()),
    ]));
    let worker = Worker::new(
        Arc::new(Kernel::new(root)),
        ikigai_reader::capability(),
        Scope::empty(),
    );
    let (screen, cx) =
        cx.add_window_view(|window, cx| Screen::new(worker, "feeds", Theme::Light, window, cx));
    settle(&screen, cx);

    // The feeds screen lists the descriptor's public feed as a link.
    let feed = href(&of_kind(&screen, cx, "link"), "News");
    assert_eq!(feed, "/page/feed-news");

    // Follow it natively: the entries appear, newest first.
    screen.update(cx, |s, cx| s.follow(&feed, cx));
    settle(&screen, cx);
    let headings: Vec<String> = of_kind(&screen, cx, "heading")
        .iter()
        .filter_map(|h| prop(h, "content").map(str::to_string))
        .collect();
    assert_eq!(headings, ["News", "Second", "First"]);

    // Follow the newest entry from the keyboard: Tab passes "All feeds" to "Read Second", and
    // Enter follows it.
    let links = of_kind(&screen, cx, "link");
    let labels: Vec<_> = links.iter().filter_map(|l| prop(l, "label")).collect();
    assert_eq!(labels, ["All feeds", "Read Second", "Read First"]);
    cx.simulate_keystrokes("tab tab enter");
    settle(&screen, cx);
    let headings: Vec<String> = of_kind(&screen, cx, "heading")
        .iter()
        .filter_map(|h| prop(h, "content").map(str::to_string))
        .collect();
    assert_eq!(headings, ["Second", "Big"]);
    let texts = of_kind(&screen, cx, "text");
    assert!(texts
        .iter()
        .all(|t| !prop(t, "content").unwrap_or("").contains('<')));
    let back = href(&of_kind(&screen, cx, "link"), "Back to the feed");
    assert_eq!(back, "/page/feed-news");
    // Back from the keyboard: the first Tab reaches "Back to the feed", Enter follows it.
    cx.simulate_keystrokes("tab enter");
    settle(&screen, cx);
    let headings: Vec<String> = of_kind(&screen, cx, "heading")
        .iter()
        .filter_map(|h| prop(h, "content").map(str::to_string))
        .collect();
    assert_eq!(headings, ["News", "Second", "First"]);
}
