# Design spike: moving ikigai-ui-gpui off gpui 0.2.2

Ledger #3779. Status: **proposal, for Brian's decision.** Nothing here changes `gpui/Cargo.toml`,
`Cargo.lock` or any source on main. This spike was time-boxed to 45 minutes, so the probes are
partial; each claim says whether it was verified on this machine (macOS, Apple silicon, rustc 1.99.1)
or taken from somewhere else (**unverified**).

## Summary

- **Option A, `gpui-pre` 0.3.13 (zed@125ca86), builds and passes.** On the probe branch
  [`probe/3779-gpui-pre`](https://github.com/ikigai-rs/ikigai-ui/tree/probe/3779-gpui-pre)
  (commit `352f9920e436fdb7eeab5411ae1f526cc12449b3`), `ikigai-ui-gpui` builds with all targets
  and the `scene` feature, and its lib's 30 GPUI window tests pass on macOS. Getting there was
  **+51 / −50 lines** in five files. One manifest test (`tests/text_system.rs`) fails because it
  checks the old manifest's wording, as you would expect.
- **Option B, a Zed git pin at the same revision, builds after two manifest and lock fixes.** On
  [`probe/3779-zed-git`](https://github.com/ikigai-rs/ikigai-ui/tree/probe/3779-zed-git) (commit `9dee16d9bddcc0a39430b19084378364554bc6b2`, A's
  source fixes plus a git manifest), Cargo first failed to resolve: the Zed repository holds a
  second package named `gpui`, and one of Zed's exact pins clashed with our lock. With
  `version = "0.2.2"` added and `cargo update --precise`, `cargo build --all-targets` succeeds with
  the same source as A. I did not run its window tests in the time box. Details in
  [B](#b-zed-git-revision-125ca86-verified-to-a-build).
- **Recommendation: A, `gpui-pre`, pinned exactly, moved by hand.** Port first with no
  accessibility work (a small change), then do the AccessKit roles as a separate change. Switch
  to `gpui` 0.3 as soon as Zed publishes it. See [Recommendation](#7-recommendation).

## The options

| | A: `gpui-pre` | B: Zed git rev | C: stay on 0.2.2 | D: forks (`gpui-ce`, `gpui-fast`) |
|---|---|---|---|---|
| Accessibility (AccessKit) | yes | yes | **no** | gpui-ce 0.3.0 predates it (unverified); gpui-fast unknown |
| crates.io-resolvable | yes | no (git) | yes | yes |
| Who publishes | huacnlee, alone | Zed (source only) | Zed | community / Longbridge |
| Pin moves as | ~20 exact `=0.3.N` pins, one version | one `rev` | nothing | one family |
| Probe result | builds, 30/30 lib window tests pass | builds after 2 resolution fixes; tests not run | n/a | not probed |

The families do not mix: a dependency graph should hold exactly one of `gpui`, `gpui-pre`,
git `gpui`, `gpui-ce` and `gpui-fast`. `apps/reader` must move in the same commit as `gpui/`,
which the probes do.

### Published crates that depend on ikigai-ui-gpui

None. `ikigai-ui-gpui`, `ikigai-ui-demo` and `ikigai-reader` are all `publish = false` (root
`Cargo.toml`: "every other member is `publish = false`"). The only published crate here is
`ikigai-ui`, and it does not depend on `gpui/`. So neither a git dependency (B) nor `gpui-pre` (A)
blocks any crates.io release from this repository. I did not search other ikigai repositories
for path or git dependencies on `ikigai-ui-gpui` (**unverified**). It is not on crates.io, so
nothing can depend on it from there.

## 1. What breaks

### A: `gpui-pre` 0.3.13 (verified)

Manifest: `gpui = { package = "gpui-pre", version = "=0.3.13", default-features = false }` (the
lib is still called `gpui`, so `use gpui::…` is unchanged), plus a new
`gpui_platform = { package = "gpui-pre-platform", version = "=0.3.13", … }`. Platform features
move from `gpui` to `gpui_platform`. Build logs for each round are on the probe branch under
`docs/design/probe-3779/` (`a-build.log` … `a-build4.log`, `a-test*.log`).

| API area | What broke (rustc) | Fix on the probe | Size |
|---|---|---|---|
| Features / platform setup | `font-kit`, `runtime_shaders`, `wayland`, `x11`, `test-support` now belong to `gpui_platform` (`gpui-pre`'s own `font-kit = []`, `wayland = []` and `x11 = []` do nothing) | moved them; `windows-manifest` stays on `gpui` | manifest only |
| Platform setup | `Application::new()` is gone (E0599, `main.rs:89`) | `gpui_platform::application().run(…)` | 1 line × 2 bins |
| App | `on_window_closed` callback takes `(cx, window_id)` (E0593) | `\|cx, _\|` | 1 line × 2 bins |
| Elements / focus | `Window::focus(&handle)` is now `focus(&handle, cx)` (E0061 ×10, plus borrow errors in `on_click` closures that used to ignore `cx`) | added `cx`; closures name `cx` | ~24 call sites, mechanical |
| Elements | `impl IntoElement for TextElement` conflicts with a new blanket `impl<T: Element> IntoElement for T` (E0119, `input.rs:493`) | deleted our impl | −7 lines |
| Text system | `ShapedLine::paint` takes `align: TextAlign, align_width: Option<Pixels>` (E0061, `input.rs:676`) | `TextAlign::Left, None` | 1 line |
| Keymap (runtime, not compile) | `KeyBinding::new("secondary-a", …)` now panics with `InvalidKeystrokeError`: `secondary` needs an OS to resolve, and `KeyBinding::new` passes none. This failed 23 of 31 window tests. | `cmd-…` on macOS, `ctrl-…` elsewhere, by `cfg!` | 3 bindings |
| Test support | no changes needed: `#[gpui::test]`, `TestAppContext`, `VisualTestContext`, `simulate_keystrokes` compile as they are | — | 0 |
| `RenderImage` / `ImageSource::Render` | no changes needed (built with `--features scene`) | — | 0 |
| Manifest test | `tests/text_system.rs` (3 tests) reads the manifest for `gpui`'s features | rewrite it to check `gpui_platform`'s | ~30 lines (not done) |

Window tests, macOS, `cargo test -p ikigai-ui-gpui` (`a-test3.log`): lib **30 passed, 0 failed**;
`palette_source` 2/2, `region` 1/1, `screenshot_script` 1/1; `text_system` 0/3 (the manifest
test above). With `--features scene`, `fixture_labels_are_visible_once` fails only because the
gallery's text fixture is the one recorded without `scene`. main does the same with the feature
on (not re-checked on main in this spike).

Not done in the time box: `apps/reader` has the same two `main.rs` changes (its manifest is
ported; its source is not), the `tests/text_system.rs` rewrite, and a build on Linux and Windows.
**Estimated size of the whole port: about 1 day**, most of it the Linux and Windows checks in CI,
not the code.

### B: Zed git revision 125ca86 (verified to a build)

Manifest: `gpui = { git = "https://github.com/zed-industries/zed", rev = "125ca86…" }` and the
same for `gpui_platform`. Logs: `b-build1.log`, `b-build2.log`, `b-build3.log` on the branch.

1. **Two packages named `gpui` in Zed's repository.** Besides `crates/gpui`, there is
   `tooling/lints/test_fixture/gpui/Cargo.toml`. Cargo picked the wrong one: "package
   `ikigai-ui-gpui` depends on `gpui` with feature `test-support` but `gpui` does not have that
   feature". Adding `version = "0.2.2"` gets past this. Zed main still calls its gpui `0.2.2`
   ([Cargo.toml at 125ca86](https://github.com/zed-industries/zed/blob/125ca8672e928b19a62c2e4b761fb29fe2a34ae3/crates/gpui/Cargo.toml)),
   so the version qualifier says nothing about the API.
2. **Zed's exact pins clash with our lock.** `gpui_web` pins `unicode-properties = "=0.1.3"`, while
   `usvg 0.46` → `rustybuzz 0.20.1` had locked 0.1.5. Our CI commits no lockfile, so CI would
   resolve from scratch, but the same problem can come back at any later `cargo update`. 
   `cargo update -p unicode-properties@0.1.5 --precise 0.1.3` fixed it locally (`b-build3.log`,
   exit 0). A bare CI resolve is not verified.
3. Cost: the first fetch cloned **416 MB** of Zed history into `~/.cargo/git/db`, plus Zed's git
   dependencies (`async-tar`, `font-kit`, `oo7`, `scap`, `xim-rs`, and **wgpu from a gfx-rs git
   revision**). That is git traffic on every cold CI cache.

Once it resolves, B builds with exactly A's source changes (`b-build3.log`, exit 0, no errors).
The API port is therefore the same for A and B. The window tests were not run on B.

### C: stay on 0.2.2

Nothing breaks, and nothing improves: native windows keep exposing nothing to a screen reader.
That is an open WCAG 2.2 AA gap (4.1.2 Name, Role, Value; 1.3.1 Info and Relationships) on the
native face, which ACCESSIBILITY.md lists as a face this repository ships. 0.2.2 is from
2025-10-22, and Zed has not said when, or whether, it will publish 0.3 (**unverified**; I found no
statement).

## 2. Accessibility

How the layer works (from `gpui-pre` 0.3.13's `src/_accessibility.rs`, i.e.
[zed@125ca86 `crates/gpui/src/_accessibility.rs`](https://github.com/zed-industries/zed/blob/125ca8672e928b19a62c2e4b761fb29fe2a34ae3/crates/gpui/src/_accessibility.rs)):
an element reaches assistive technology only if it has **both** a global id (`.id(…)`, composed
from its ancestors' ids) **and** a role (`.role(Role::…)`). Ids must be unique in each frame and
stable across frames, or the screen reader hears a removal followed by an addition. Properties
are `aria_label`, `aria_description`, `aria_level`, `aria_expanded`, `aria_toggled`,
`aria_selected`, `aria_numeric_value`/`min`/`max`, `aria_position_in_set`/`size_of_set`,
`aria_row_index`/`column_index`/`row_count`/`column_count`, `aria_placeholder`, `aria_value` and
`aria_orientation` (`elements/div.rs:1357–1540`). Actions use `on_a11y_action(AccessibleAction::…)`.
`Role` is `accesskit::Role` (accesskit 0.24).

**Where it goes in our walker.** `Screen::walk` (`gpui/src/native.rs:750`) already gives every
node a `node_body(path)`, and interactive nodes already have path-derived ids
(`format!("label:{path}")`, `"{path}:canvas"`). The model path is stable and unique, so it is the
right id. The work is one `.role()` call and its `aria_*` calls per arm of the `match node.kind`.

| UI model kind | AccessKit role | Properties |
|---|---|---|
| `heading` | `Heading` | `aria_level(level)` |
| `text` | `Label` (or no role: text is read from the parent) | — |
| `status` | `Status` (live region) | — |
| `field` (text) | `TextInput` | `aria_label`, `aria_placeholder`, `aria_value`; required and description: not in the API (gap) |
| `field` checkbox | `CheckBox` | `aria_toggled` |
| `choice` radio | `RadioGroup`, options `RadioButton` | `aria_selected`, `aria_position_in_set`/`size_of_set` |
| `choice` (other) | `ComboBox` or `ListBox` | `aria_expanded` |
| `button` | `Button` | `aria_label`; `on_a11y_action(Click)` |
| `link` | `Link` | `on_a11y_action(Click)` |
| `form` | `Form` | `aria_label(label)` |
| `group` / regions | `Group`, or a landmark role (`Navigation`, `Main`) | `aria_label(name)`; region names stay out of visible text (#76) |
| `list` / item | `List` / `ListItem` | `aria_position_in_set` |
| `table` / row / cell | `Table` / `Row` / `Cell` | `aria_row_index`, `aria_column_index`, counts; caption as `aria_label` |
| `meter` | `Meter` | `aria_numeric_value`, min, max |
| disclosure | `Button` + `aria_expanded` | — |
| `image` / media | `Image` | `aria_label(alt)` |
| `scene` | `Canvas` or `Group` | `aria_label`; selection announced through the `Status` node |

**What a screen reader would then say, for `contact`** (expected VoiceOver output from the
mapping above, **not heard**: the probe has no roles yet): "Contact, heading level 1" · "Contact,
form" · "name, required, edit text" · "email, required, edit text" · … · "Submit, button". Today,
on 0.2.2, VoiceOver finds only the window title.

**Upstream issues that would affect us:**
- [zed#66690](https://github.com/zed-industries/zed/issues/66690) (open): views drawn from the
  cache drop their nodes, and focus falls back to the window. This affects us directly, because
  `Screen` re-renders from snapshots. We must not cache views, or must test that nodes survive a
  cached frame.
- [zed#66691](https://github.com/zed-industries/zed/issues/66691) (open, macOS): the window is
  reported as the focused element. This breaks "focus follows the keyboard" for VoiceOver users,
  on our one platform with window tests in CI.
- [zed#66730](https://github.com/zed-industries/zed/issues/66730) (open): clipped nodes are
  reported as visible. This affects long scrolled screens (lenses, tables).
- [zed#65186](https://github.com/zed-industries/zed/pull/65186), the Touch Bar crash fix: the
  brief calls it fixed, but **it is an open PR, not merged**, as of 2026-11-16. Until it lands,
  Touch Bar Macs can crash when a window closes.

The tests should check the AccessKit tree, not the pixels. `gpui-pre` ships an `a11y` example
and test-support. Whether the test platform exposes the tree to `#[gpui::test]` is
**unverified**; checking it is step 2 of the work.

## 3. Supply chain and cadence

| | A `gpui-pre` | B git rev | C 0.2.2 |
|---|---|---|---|
| Publisher | one crates.io owner, `huacnlee` (Longbridge / GPUI Kit), not Zed | Zed's repository | Zed |
| Cadence | 14 versions, 0.3.0 on 2026-09-03 to 0.3.13 on 2026-11-16, weekly (one week skipped, 10-12 to 10-26) | continuous | none since 2025-10-22 |
| A pin is | `=0.3.N` on `gpui-pre` and `gpui-pre-platform`; the other ~18 `gpui-pre-*` crates come along, because each pins its siblings `=0.3.N` | one `rev` per line (2–4 lines) | `=0.2.2` |
| `cargo update` | nothing moves (exact pins) | nothing moves (rev) | nothing |
| Drift scans | see a new `gpui-pre` every week: noise, unless the scan is set to monthly for this crate | can't see anything: a git rev is invisible to crates.io-based scanners, and Zed still says `0.2.2` | "0.2.2, latest" |
| Provenance | the snapshot names its source in `[package.metadata.gpui-pre]` (`zed-rev = "125ca86…"`) and in its description | direct | direct |
| If the publisher stops | we keep building on the last snapshot, and can move to B at the same `zed-rev` | n/a | n/a |

Two differences from the Zed source make `gpui-pre` not quite the same as "Zed at rev X":
`gpui-pre-wgpu` depends on **crates.io `wgpu ^30.0.1`**, where Zed uses a gfx-rs git revision;
and it is edition 2024 (rustc ≥ 1.85, below our MSRV of 1.89). Zed builds with a toolchain pinned
to 1.98.1, and `gpui-pre` declares no `rust-version`, so our MSRV job is the first place an MSRV
break would show up (**unverified** that 1.89 builds it; I built with 1.99.1).

## 4. Workarounds

| Workaround (0.2.2) | Under A / B |
|---|---|
| `libc = "=0.2.189"` (zed-async-tar → xattr 0.2 needs ENOATTR) | **Drop it.** Probe A's `Cargo.lock` has no `xattr` and no `zed-async-tar` on macOS. Zed now takes `async-tar` from its own git fork. Whether the Linux graph still pulls xattr 0.2 is **unverified**. |
| `font-kit` feature on macOS | **Changes**: it moves to `gpui_platform/font-kit` (→ `gpui_macos/font-kit`). The `font-kit` on `gpui` itself is an empty feature. If you leave it off, the NoopTextSystem trap is probably still there (not tested). `tests/text_system.rs` should keep guarding it, on the new crate. |
| `wayland` / `x11` features | **Changes**: they become `gpui_platform/wayland` / `x11` (→ `gpui_linux`). Linux GPUI test-support still turns on `wayland` and `x11`, so the libxkbcommon-x11 CI limit stays. |
| `windows-manifest` | **Stays** on `gpui` (`gpui-pre`'s feature, `dep:embed-resource`). |
| `runtime_shaders` | moves to `gpui_platform` (→ `gpui_macos`, `gpui_ios`). |

## 5. The scene feature

`scene` runs its own `wgpu` 25 device beside GPUI's. On the new code:
- **macOS** is still Metal ([zed#46758](https://github.com/zed-industries/zed/pull/46758) only
  changed Linux/FreeBSD), and **Windows** is still DirectX 11. There is no GPUI wgpu device to
  share there, so the second device stays.
- **Linux/FreeBSD**: GPUI renders with wgpu (`gpui-pre-wgpu`, wgpu 30). Sharing would mean (1)
  moving `scene` from wgpu 25 to 30 (the API changes between 25 and 30 are **unverified**), and
  (2) a GPUI API that hands out its `Device`/`Queue` or accepts a wgpu texture as a paint source.
  I did not find a public one in `gpui-pre` 0.3.13's `gpui` crate in the time box (**unverified**
  for `gpui-pre-wgpu`).

**Don't do it in the same move.** It would help one platform family, it depends on an API that
may not exist, and the `RenderImage` hand-off already works on all three platforms (the probe
compiled it unchanged). Moving `scene` to wgpu 30 is worth doing on its own, so the graph does
not hold two wgpu majors on Linux.

## 6. Web

GPUI's wasm32 backend ([zed#50228](https://github.com/zed-industries/zed/pull/50228)) paints into
a canvas: no DOM, no menus, and multithreading only on nightly. ikigai-ui's web face is HTML: real
elements, htmx, the token stylesheet, and the axe-checked accessibility that ACCESSIBILITY.md
commits to. A canvas face would reach a screen reader only through whatever AccessKit's web
adapter provides (**unverified** whether gpui_web exposes one at all), and it would lose text
selection, find-in-page, reader modes and user stylesheets. It does not replace the HTML face,
and nothing in this move should depend on it. Its only use for us would be a "same pixels as the
native face" preview for designers, and that is not a goal anyone has set.

## 7. Recommendation

**Option A: `gpui-pre`, exact pins, moved on purpose.**

Order of work:
1. **Port** (≈1 day, no new behaviour): the probe's diff, plus `apps/reader/src/main.rs`, plus
   `tests/text_system.rs` rewritten for `gpui_platform`, plus dropping `libc` if the Linux graph
   confirms it. Done when CI is green on macOS (window tests) and Linux/Windows build. Update
   ACCESSIBILITY.md's "GPUI 0.2.2".
2. **Roles** (one PR per group of kinds): `.role()` and `aria_*` in `Screen::walk` as in the table
   in section 2. First, a test that reads the AccessKit tree for `contact` and `gallery`. Then a
   manual VoiceOver pass recorded in ACCESSIBILITY.md as Pass / Fail / Blocked. Track #66690,
   #66691 and #66730 there as Blocked items.
3. **Cadence**: move the pin monthly, or when an upstream a11y fix lands. Each move is one PR
   that bumps `=0.3.N` and records the `zed-rev`. Set the drift scan to ignore `gpui-pre`
   between moves.
4. **Later, separately**: `scene` on wgpu 30. Device sharing on Linux only if GPUI exposes a
   hook.

What would change this recommendation:
- **Zed publishes `gpui` 0.3** (or later) on crates.io → switch to it immediately. Same source,
  first-party publisher. The port from `gpui-pre` is a rename in the manifest.
- **`gpui-pre` stops publishing**, or a snapshot diverges from its `zed-rev` in ways that matter
  → B at the last good rev, with the two resolution fixes found here.
- **A crash or a11y regression in a snapshot we need to fix fast** → B at a rev with the fix,
  temporarily.
- **The whole move fails CI on Linux or Windows** in ways that take more than a few days → C
  for now, and the native face stays documented as screen-reader-inaccessible.

## Probe branches

| Branch | What | Delete? |
|---|---|---|
| [`probe/3779-gpui-pre`](https://github.com/ikigai-rs/ikigai-ui/tree/probe/3779-gpui-pre) | Option A: builds, 30/30 lib window tests pass on macOS; logs in `docs/design/probe-3779/` | keep until step 1 of the work starts, as its starting diff; then delete |
| [`probe/3779-zed-git`](https://github.com/ikigai-rs/ikigai-ui/tree/probe/3779-zed-git) | Option B: the resolution fixes, then a clean build; logs in `docs/design/probe-3779/b-*.log` | delete once this document is accepted |

Neither branch has a pull request, and neither is to be merged.
