# ikigai-ui-gpui

A native GPUI renderer for ikigai-ui's resolved screen model, with windows on macOS, Linux and
FreeBSD (Wayland or X11) and Windows. Run the demo:

```sh
cargo run -p ikigai-ui-gpui --bin ikigai-ui-demo-native -- 127.0.0.1:8080
```

This opens gallery and contact windows and serves the HTML gallery and contact
form at `http://127.0.0.1:8080`. Both targets share **one kernel and one submission
store** in this process. The contact form is generated from the existing demo
service contract. Native submissions call that service's Sink directly, with the
same contact-write capability the HTML door grants. Service refusals appear beside
the named field and in a form summary; values remain editable after refusal.

GPUI's default features are what give it a window platform and a text system, and this crate
turns them off, so `Cargo.toml` names each target's own by name (ledger #3526):

| Target | GPUI features | Window platform | Text system |
|---|---|---|---|
| macOS | `font-kit` (always) | Metal (shaders compiled at run time, so no Xcode Metal compiler) | Core Text via font-kit |
| Linux, FreeBSD | `wayland`, `x11`, `font-kit`, through this crate's `wayland` and `x11` features | Wayland if `WAYLAND_DISPLAY` is set, else X11 on `DISPLAY`; blade on Vulkan | cosmic-text |
| Windows | `windows-manifest` (always) | Direct3D 11 | DirectWrite |

A dropped one fails silently (no text, or no window), so `tests/text_system.rs` pins each line,
here and in the reader app. On Linux and FreeBSD they are features, not yet default, only because
they link libxkbcommon-x11, which GitHub's stock Ubuntu runner lacks and the shared CI cannot yet
install (Verification, below); a build without them says so and exits rather than run headless. No webview, JavaScript runtime or HTML renderer is embedded in the
native windows.

### Linux

What it needs at run time (any desktop has them): libxkbcommon and libxkbcommon-x11, libxcb,
libwayland-client (loaded at run time, Wayland only), fontconfig (loaded at run time), and a
Vulkan driver for blade (Mesa's `mesa-vulkan-drivers`, or the GPU vendor's). Building needs no
`-dev` package: GPUI links libxkbcommon, libxkbcommon-x11 and libxcb by their development names
(`libxkbcommon.so`), and when those are missing `build.rs` links the runtime libraries
(`libxkbcommon.so.0`, `libxkbcommon-x11.so.0`, `libxcb.so.1`) instead. On Debian or Ubuntu:

```sh
sudo apt install libxkbcommon0 libxkbcommon-x11-0 libxcb1 libwayland-client0 libfontconfig1 \
  mesa-vulkan-drivers   # or libxkbcommon-dev libxkbcommon-x11-dev libxcb1-dev to build the usual way
cargo run -p ikigai-ui-gpui --features wayland,x11 --bin ikigai-ui-demo-native -- 127.0.0.1:8080
```

Run it from a terminal in the desktop session: the session sets `WAYLAND_DISPLAY` (Wayland) or
`DISPLAY` (X11). With neither, the demo says so and exits. `WAYLAND_DISPLAY= cargo run ...` forces
X11 (through Xwayland on a Wayland desktop). `RUST_LOG` does nothing: the demo installs no logger.
On a machine with more than one GPU, `VK_DRIVER_FILES=/usr/share/vulkan/icd.d/<driver>_icd.json`
picks blade's Vulkan driver.

Run on lawnboy (Pop!_OS 24.04, Intel UHD and NVIDIA RTX 3060, 2026-11-12), as a user who could
not reach the desktop session's display, so on displays of its own:

- **X11**: Xvfb at `:93`, Mesa's llvmpipe Vulkan (`VK_DRIVER_FILES=/usr/share/vulkan/icd.d/lvp_icd.json`;
  the NVIDIA driver cannot present to Xvfb and drew black windows). The three windows opened and
  drew their text (the gallery, elements and contact screens). GPUI 0.2.2's X11 client draws its
  first frame only after an X event arrives; under a bare X server with no window manager and no
  input, nothing arrives and the windows stay black until something (a window mapped over them,
  a key) wakes it. A desktop session's window manager does this at once.
- **Window tests**: with GPUI's `test-support` taken on Linux too, every window test in
  `src/native.rs` and `src/input.rs` (20) and the reader's `tests/window.rs` passed unchanged
  but for one keystroke (`cmd-a` is `secondary-a` off macOS). They stay macOS-only in the
  manifest until CI can link them (Verification).
- **Wayland**: weston 13 (`--backend=x11 --renderer=pixman --shell=kiosk`) on that Xvfb, same
  Vulkan driver: the contact screen drew in full, text included. Weston's headless backend cannot
  host it: it offers no `wl_seat`, and GPUI 0.2.2 panics without one (`wayland/client.rs:503`).

### Windows

For someone who has not built the repository before (Windows 10 or 11, x64):

1. Install the Rust toolchain: download and run `rustup-init.exe` from <https://rustup.rs>, and
   accept its offer to install the Visual Studio C++ Build Tools ("Desktop development with
   C++", which brings the Windows SDK: the linker, `rc.exe` for the application manifest, and
   `fxc.exe`, which a `--release` build uses to compile GPUI's shaders).
2. Install Git from <https://git-scm.com/download/win>.
3. In a new PowerShell window:

   ```powershell
   git clone https://github.com/ikigai-rs/ikigai-ui.git
   cd ikigai-ui
   cargo run -p ikigai-ui-gpui --bin ikigai-ui-demo-native -- 127.0.0.1:8080
   ```

   The first build takes several minutes. Three windows open (gallery, elements, contact), and
   the same screens are served as HTML at <http://127.0.0.1:8080>. Allow the firewall prompt
   for private networks if one appears (the door listens on 127.0.0.1 only). Add `dark`,
   `light` or `high-contrast` after the address to pick a theme; `auto` follows Windows' app
   mode (Settings, Personalization, Colors).
4. What to check: every window has text (headings, labels, button captions); Tab and Shift+Tab
   move the focus ring; typing, Ctrl+A, Ctrl+C, Ctrl+X and Ctrl+V edit a field; an IME
   (Windows+Period for emoji, or a Japanese keyboard) composes into a field; submitting the
   contact form empty shows the refusals beside the fields; changing Windows' app mode with
   `auto` repaints the windows; the windows are sharp at 150% scaling.

The reader app (`apps/reader`) builds and opens its window the same way: `cargo run -p
ikigai-reader -- <host config> <host state> <host age key> <layers.json>` (a local layer host;
add `--features wayland,x11` on Linux).

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
as it repaints the HTML face. It is parsed into the same `palette::Table` the token stylesheet
is written from, so a palette that is not a full table of `#rrggbb` colors, or that misses the
contrast floor, is refused here as it is there. Until it arrives, or if the kernel refuses it (the refusal is
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

`auto` follows the appearance GPUI reports on each platform: macOS's Appearance setting; on
Linux, the XDG desktop portal's `color-scheme` (GNOME, KDE, COSMIC and others publish it; with no
portal the window reports light); on Windows, the app mode (Settings, Personalization, Colors),
live on all three.

Known limitation: GPUI 0.2.2 reports a window's appearance as light or dark only. It does not
expose macOS "Increase contrast", the portal's `contrast` or Windows' contrast themes, so `auto`
never selects high contrast by itself, unlike the HTML face's `prefers-contrast: more`. Choose
`high-contrast` explicitly.

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
checkboxes. Select-all, copy, cut and paste are Cmd+A/C/X/V on macOS and Ctrl+A/C/X/V on Linux
and Windows (GPUI's `secondary` modifier); Ctrl+Cmd+Space opens the character palette on macOS
only, where it is a system panel (Linux and Windows reach their emoji pickers through the IME:
Windows+Period, or the input method's own key). Composition goes through GPUI's platform input
handler on each: macOS text input, XIM or the `zwp_text_input_v3` protocol on Linux, IMM32 on
Windows. Other field input hints use text editing, leaving authoritative value
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
tests ensure a late older load cannot replace a newer snapshot. These pass locally with the same
test command on macOS, and passed on Linux (lawnboy) with GPUI's `test-support` taken there.

Theme tests draw the same screen in each palette and check it draws with that palette's
`bg` and `fg`, that a host's override of `urn:iki:ui:palette:dark` reaches the window, and that
`auto` switches between light and dark on an appearance change while explicit themes hold. The
test platform cannot change a window's appearance, so the test calls the handler the window's
appearance observer calls; nor can it show pixels, so look at the windows in each theme.

The test platform shapes text with its own text system and opens no real window, so these tests
cannot see whether a real window draws any text, or opens at all. That depends on the GPUI
features each target names (the table above), because this crate turns GPUI's defaults off;
without `font-kit` every macOS window drew boxes and no text. `tests/text_system.rs` pins each
target's line. Look at the demo's windows after any change to the GPUI dependency.

What is tested where:

| | macOS | Linux, FreeBSD | Windows |
|---|---|---|---|
| Portable tests (worker, snapshots, palettes, manifest lines) | locally (CI's `macos` input is off) | CI (`fmt · clippy · test`, `msrv`) | CI once the `windows` input is on (below); clippy cross-compiled locally |
| The window code compiled | locally | CI, without the `wayland`/`x11` backends; with them, locally | cross-compiled locally |
| Window tests on GPUI's test platform | locally | passed on lawnboy; not in CI (below) | not run: `test-support` is not taken there |
| Demo windows opened and looked at | by hand, before ledger #3526 | X11 (Xvfb) and Wayland (weston) on lawnboy, 2026-11-12, above; never on FreeBSD | never: steps above, for real hardware |
| Real keyboard, IME, appearance changes, screen reader | by hand on macOS | not done | not done |

Two CI changes would close the gaps, neither of which this repository can make on its own:

- **Linux backends and window tests.** GitHub's stock Ubuntu runner has libxkbcommon and libxcb
  but not libxkbcommon-x11 (`unable to find library -lxkbcommon-x11`, ledger #3526's first CI
  run), and the shared `rust.yml` has no step that installs packages. With an input there that
  runs `sudo apt-get install -y <packages>` before the cargo steps of every Linux job (here:
  `libxkbcommon-x11-0`; `build.rs` links it without its `-dev` package), the `wayland` and `x11`
  features become the Linux `gpui` line and the `test-support` dev-dependency covers Linux too.
- **Windows.** The shared workflow's `windows: true` input in `.github/workflows/ci.yml`; ledger
  #3526's pull request could not set it (its token may not edit workflows). Until it is set,
  nothing in CI compiles the `cfg(windows)` paths.

`src/input.rs` adapts GPUI 0.2.2's `examples/input.rs`, Copyright Zed Industries,
under Apache-2.0. This crate is Apache-2.0 licensed; see `../LICENSE-APACHE`.
