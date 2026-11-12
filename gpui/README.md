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

## Themes

The windows draw only with the shared palettes (`src/palette.rs`), never a color of their own.
`native::Screen::new` takes a `theme::Theme`: `Auto` (the default) follows the window's system
appearance and switches live when it changes; `Light`, `Dark` and `HighContrast` are chosen
explicitly, as the HTML chooser offers. The demo takes the theme as its second argument:

```sh
cargo run -p ikigai-ui-gpui --bin ikigai-ui-demo-native -- 127.0.0.1:8080 dark
```

The palette is a resource: `Worker::palette` reads `urn:iki:ui:palette:{name}` through the
host's kernel on a worker thread, so a host that overrides a palette repaints the native windows
as it repaints the HTML face. Until it arrives, or if the kernel refuses it (the refusal is
shown), the compiled table of the same name stands in. Colors go by role, as the HTML face
uses them: text `fg` on `bg`, placeholders `muted`, links `accent`, errors `danger`, buttons,
selected options and selected text `accent-fg` on `accent`, control borders `line`, table
rules `rule`, the focus ring and the text cursor `focus` (drawn outside the control, on `bg`).
Meters use `surface` for the track, `accent` for the fill, `warn` for a suboptimal region
and `danger` for the farthest region or a value beyond the range; `line` outlines the
track. The visible sentence beside the bar states the original value, bounds, thresholds,
optimum and assessment, so color only reinforces the text.
Each is a pair `palette::PAIRS` holds to the contrast floor. `tests/palette_source.rs` fails if
a color is written anywhere in `src`.

Known limitation: GPUI 0.2.2 reports a window's appearance as light or dark only. It does not
expose macOS "Increase contrast", so `auto` never selects high contrast by itself, unlike the
HTML face's `prefers-contrast: more`. Choose `high-contrast` explicitly.

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
validation to the Sink. Choices use selectable options, and a choice with the `urn:iki:ui:hint:choice:radio` hint is one radio group: one tab stop, the arrow keys move the selection (wrapping), and the status line announces it ("Lens: graph, checked, 4 of 4"); Tab/Shift-Tab traverse
controls, and Enter/Space activate options and submit buttons. Native forms
require a `resource` and `verb: sink` (Sink is the default). HTML-only `action`
URLs and other form verbs are refused visibly. Successful Sink responses show a
native acknowledgment; response HTML is never rendered. HTTP(S) and mail links
open in the system handler. Demo `/page/gallery` and `/page/contact` links load
native screens; other relative routes report that no native route exists.

## The 3D scene: phase 2 (ledger #3516, spike behind `scene`)

Phase 1 of ledger #3514 ships the `scene` kind on the html face only, and this face's
descriptor still falls `scene` back to text (the replacement). The `scene` cargo feature (off
by default) adds the offscreen route, `ikigai_ui_gpui::scene`: the datum is rendered with
`wgpu` into a BGRA texture, read back, and handed over as pixels for a GPUI image.

- **One scene, two faces.** The shaders are `urn:iki:ui:scene-renderer.js`'s own:
  `ikigai_ui::scene::wgsl` reads `NODE_WGSL` and `EDGE_WGSL` out of the script, so there is
  no second copy to drift. The camera (yaw, pitch, distance), the keys (arrows orbit, `+`/`-`
  zoom, N / Shift+N / Escape select) and the status sentence are the script's.
- **Colors by role, from the shared palettes**, through the shader's uniform on both faces:
  the ground `--bg`, nodes `--accent`, edges `--line`, the selection `--focus` (and 1.8 times
  larger, so never color alone). Each is a pair `PAIRS` already holds (`accent`/`line`/`focus`
  on `bg`), so no new pair.
- **Measured** (`cargo test -p ikigai-ui-gpui --features scene --release --test scene measure
  -- --nocapture`, Apple Silicon Mac): device and pipelines 12-32 ms once; a frame plus
  readback 9.3 ms at 640x360 and 9.8 ms at 1280x720, the same at the gallery's 8 nodes and at
  `MAX_NODES` (512 nodes, 1536 edges), worst of 60 under 10.2 ms. The cost is the readback's
  round trip, not the graph. Under a 60 Hz frame (16.7 ms), so orbiting can be smooth.

**Drawn natively with the feature on** (step 2): the descriptor (`assets/gpui-scene.json`)
claims `scene`, and `Snapshot::from_face` keeps a scene's replacement (validated as the rest
of the tree) instead of refusing the node. The worker reads the node's datum (a
`urn:iki:ui:scene:graph:` IRI only) under the host's capability; `scene::Painter` renders each
frame (640x360) on its own OS thread, never the UI thread, and the window paints it with
`img(Arc<RenderImage>)`. Keys asked while a frame is painting coalesce into one more frame. The
scene takes GPUI focus (Tab reaches it; a click focuses it) and handles the html renderer's keys
through `View::key`; N / Shift+N / Escape put `announce`'s sentence in the window's status line,
as the html face says it in `#status`. The replacement (the graph as text) is always drawn
beside the image. With the feature off, nothing changes: `scene` is its replacement.

## Verification

```sh
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
RUSTDOCFLAGS='-D warnings' cargo doc --workspace --no-deps
```

The portable integration tests exercise real target faces and the existing demo
Sink, exact named Unicode values, field refusals, capability denial, vocabulary
rejection, and model size/depth bounds. A synchronous composition that sleeps for
five seconds runs on a different OS thread while
nonblocking polling continues.

On macOS, GPUI tests create test windows, draw the entire gallery, exercise
boolean controls, and enter native text while the five-second composition is
executing. They also exercise contact form editing, multiline Unicode input,
keyboard choice selection/submission, field refusals, retained values and focus
traversal. Input regressions cover stale layout byte offsets after Unicode edits,
IME selection direction, grapheme deletion and large multiline pastes. Navigation
tests ensure a late older load cannot replace a newer snapshot. These pass locally
with the same test command.

Theme tests draw the same screen in each palette and check it draws with that palette's
`bg` and `fg`, that a host's override of `urn:iki:ui:palette:dark` reaches the window, and that
`auto` switches between light and dark on an appearance change while explicit themes hold. The
test platform cannot change a window's appearance, so the test calls the handler the window's
appearance observer calls; nor can it show pixels, so look at the windows in each theme.

The test platform shapes text with its own text system, so these tests cannot see
whether a real window draws any. That depends on GPUI's `font-kit` feature, which
this crate enables on macOS by name because it turns GPUI's defaults off; without
it every window drew boxes and no text. `tests/text_system.rs` pins that line.
Look at the demo's windows after any change to the GPUI dependency.

The repository's stock Linux CI compiles the native walker, runs the portable
worker tests and existing repository tests, and checks MSRV, wasm, docs and HTML
accessibility. Window tests stay on macOS: GPUI's `test-support` feature forces
X11/Wayland link dependencies even when using its headless test platform, and
those development libraries are absent from the stock runner. No shared CI
workflow changes are needed.

`src/input.rs` adapts GPUI 0.2.2's `examples/input.rs`, Copyright Zed Industries,
under Apache-2.0. This crate is Apache-2.0 licensed; see `../LICENSE-APACHE`.
