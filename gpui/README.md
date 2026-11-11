# ikigai-ui-gpui

A native macOS GPUI renderer for ikigai-ui's resolved screen model. Run the demo:

```sh
cargo run -p ikigai-ui-gpui --bin ikigai-ui-demo-native -- 127.0.0.1:8080
```

This opens gallery and contact windows and serves the HTML gallery and contact
form at `http://127.0.0.1:8080`. Both targets share **one kernel and one submission
store** in this process. The contact form is generated from the existing demo
service contract. Native submissions call that service's Sink directly, with the
same contact-write capability the HTML door grants. Service refusals appear beside
the named field and in a form summary; values remain editable after refusal.

The window backend currently requires macOS. GPUI 0.2.2 uses runtime Metal shader
compilation, so Xcode's separate Metal compiler is unnecessary. Linux and Windows
can build and test the portable snapshot/worker API, but this crate does not yet
supply windows on those platforms. No webview, JavaScript runtime or HTML renderer
is embedded in the native windows.

## Host integration

Place `ikigai_ui_gpui::descriptor_space()` **before** the base UI space in the
host's fallback space. It upgrades the base library's prospective GPUI descriptor
without changing the shared library or its HTML descriptor. It renders heading,
text, button, link, list, status, group, field, choice, option, form, nav and table.
Image and visual take the model's ordinary fallback-to-text ladder. Unknown hints
are ignored; hint values are never code. The host must not install a descriptor
that advertises other native kinds or elements without implementing them.

Construct `worker::Worker` with the host kernel, a capability and its resolution
scope. `Worker::load("contact")` issues
`urn:iki:ui:face:target:gpui:screen:contact` on a dedicated OS thread. The face
resolves references, composes the screen, expands elements and applies the target
contract there. The result is validated before publication as an immutable
`Snapshot`. `native::Screen` polls for that result without waiting, retaining the
previous screen while it loads. Rendering only walks the snapshot and local form
state; it has no resolution calls. `Worker::submit` also executes off-thread and
preserves the same authority and scope.

Fields support native text services (IME/marked text, UTF-16 ranges), grapheme
movement/deletion, selection, clipboard, multiline text areas, and boolean
checkboxes. Other field input hints use text editing, leaving authoritative value
validation to the Sink. Choices use selectable options; Tab/Shift-Tab traverse
controls, and Enter/Space activate options and submit buttons. Native forms
require a `resource` and `verb: sink` (Sink is the default). HTML-only `action`
URLs and other form verbs are refused visibly. Successful Sink responses show a
native acknowledgment; response HTML is never rendered. HTTP(S) and mail links
open in the system handler. Demo `/page/gallery` and `/page/contact` links load
native screens; other relative routes report that no native route exists.

## Verification

```sh
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
RUSTDOCFLAGS='-D warnings' cargo doc --workspace --no-deps
```

The portable integration tests exercise real target faces and the existing demo
Sink, field refusals, capability denial, and vocabulary rejection. A synchronous
composition that sleeps for five seconds runs on a different OS thread while
nonblocking polling continues.

GPUI tests create test windows on macOS and Linux's headless backend, enter native text while
that five-second composition is executing, and exercise contact form editing,
multiline Unicode input, keyboard choice selection/submission, field refusals,
retained values and focus traversal. The repository's Linux CI runs both the
worker tests and these GPUI window tests without an X11/Wayland server or system
window-library development packages. The same tests also run on macOS.

`src/input.rs` adapts GPUI 0.2.2's `examples/input.rs`, Copyright Zed Industries,
under Apache-2.0. This crate is Apache-2.0 licensed; see `../LICENSE-APACHE`.
