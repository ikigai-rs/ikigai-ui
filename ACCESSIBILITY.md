# Accessibility

What `ikigai-ui` shows people, the rules its output follows, how each rule is tested, and how to
report a barrier. It is not a conformance claim. ikigai-ui is the shared shell every ikigai UI
builds on, so what it guarantees, every page over it inherits; what a page puts inside the shell
is the host's to keep accessible.

| Field | Value |
|---|---|
| Target | WCAG 2.2 Level AA (UI conventions D13), for what this repository renders |
| Status | Evaluation in progress. No conformance claim. |
| Owner | the maintainers of this repository |
| Report a barrier | [a GitHub issue](https://github.com/ikigai-rs/ikigai-ui/issues/new) (see [Reporting a barrier](#reporting-a-barrier)) |
| Last reviewed | 2026-11-12 |

## What it shows people

- **The HTML face**: the shell (`urn:iki:ui:template:shell`: skip link, header with the theme
  chooser, `#main`, one status region), `tokens.css` and its palettes, `shell.js`, and the HTML
  every node kind and custom element renders to. The demo (`demo/`) and the static site
  `cargo run --example axe_site` writes are pages built from them.
- **The native face** (`gpui/`): GPUI 0.2.2 windows that draw the same screens, and the reader
  spike (`apps/reader`), a feed reader with an HTML and a native face.
- **The embedded 3D scene** (`scene`): a WebGPU canvas on the HTML face, always with a
  replacement in core kinds.
- **The support matrix** (`urn:iki:ui:support`): a page and JSON saying, per target, what each
  face renders and its accessibility tally.
- **Documentation**: this file, the READMEs and the README's screenshots (`docs/screenshots/`).

## The rules its output follows

### The shell

- **Contrast.** Text 4.5:1; component borders (`--line`) and the focus ring 3:1, in light and
  dark; decorative rules (`--rule`, exempt from SC 1.4.11) at least 1.3:1; 7:1 for every pair in
  high contrast. No color outside the palette tables, and every `var(--x)` is defined. A palette
  a host rebinds is parsed as a table and held to the same floor before either face draws with it:
  a value that is not `#rrggbb`, an unknown token or a pair below its floor is refused, naming it.
- **Landmarks.** A skip link, one `<header>`, one `<main id="main" tabindex="-1">`, one status
  region (`#status`, `role="status" aria-live="polite"`).
- **One live region.** An in-place note has no role; its control points at it with
  `aria-describedby` (removed when the note goes), and `#status` says the sentence, naming the
  control. Empty, `#status` is visually hidden but stays in the accessibility tree (never
  `display: none`), so a reader that announces only a region it already knew hears the first
  sentence (ledger #3565).
- **Focus after a swap.** A swap into `#main` focuses its first heading; a swap that took the
  focused element never leaves focus on `<body>`; on a page nobody has focused, a poll or a lazily
  loaded region leaves focus on `<body>` and the scroll where it was.
- **Refusals as text, in place** (PR 68). A 403 marks its control `aria-disabled` alone, never
  `disabled`: it stays in the tab order, announces as unavailable, is described by the refusal
  note, and the shell blocks its activation (a click, Enter, Space, a form submit it would make,
  any request htmx would send for it). Markup in a refusal is text. Quiet elements never announce.
- **Live views** (D9). A refetch never announces, leaves no note, and keeps focus where it was,
  inside the view or not. A refused live stream is said once in the status region; an accepted
  one says nothing.
- **The theme chooser.** In every page's header (the shell's, `urn:iki:ui:theme-chooser:{theme}`,
  for a page composed over the shell without `Pages` too): a visible "Theme" label on a native
  select, its four choices in words (Auto (system), Light, Dark, High contrast), reached and
  changed from the keyboard. A change applies at once; with no script its Apply button reloads
  the page with `theme=`. The choice is kept in this browser only (`localStorage`,
  `ikigai-ui:theme`; auto keeps nothing), shown by the chooser and held across a reload; it
  reaches the server only as `theme=` when the form is submitted with no script. The chooser is
  drawn with `--fg` on `--bg` and a `--line` border, pairs the floor already holds.
- **Media preferences.** `prefers-color-scheme` and `prefers-contrast: more` choose the palette
  when the theme is auto; `prefers-reduced-motion: reduce` turns off animation, transitions and
  smooth scrolling.
- **Target size.** Navigation links are at least 24 px (SC 2.5.8).

### Kinds and custom elements on the HTML face

- **Custom elements** (PR 23) reach assistive technology only as the core kinds they expand
  into, never as markup of their own: no face claims `card`, `key-value` or `tag-list` natively.
  Expanded, a `card` is a `<section>` named by its title (`aria-label`) with a real heading at its
  `level`, its body a `<p>` and its links a named `<ul>` of `<a>`; a `key-value` is a `<table>`
  whose `<caption>` names it and whose terms are `<th scope="row">` row headers, so each value is
  announced with its term; a `tag-list` is a `<ul>` named by its label (`aria-label`), each tag a
  list item, a link when it has one. With no definition readable, the element is one `<p>` of its
  `fallback` text, with no heading, table or list semantics and none of its links or children.
- **`meter`** (PR 59). Native: a `<meter>` with a `<label for>` matching its unique node-path
  `id`; value, min/max, low/high and optimum are numeric attributes (defaults: 0, 1, min, max,
  midpoint), so the browser exposes meter semantics and the range and value. Visible text also
  states the original value, bounds, thresholds, optimum and assessment; a reading beyond the
  range stays explicit in text even though the bar clamps. Expanded (native off in the
  descriptor): an ordinary paragraph, "{label}: {value} of {max}", with no meter role or
  thresholds implied. With the definition unavailable, the ordinary ladder uses a supplied
  core-kind replacement, else the required fallback text, carrying only their core-kind semantics.
- **`disclosure`** (PR 42). Native: a `<details class="ui-disclosure">` whose `<summary>` is the
  summary line, with `open` only when the model's initial `open` is true; the browser gives the
  summary its button role and expanded or collapsed state, toggles it by Enter, Space and
  pointer, with no script and no ARIA written; the body is in the tab order and the accessibility
  tree only while open. Expanded: a `<section>` named by the summary (`aria-label`) with the
  summary as a real heading at its `level` and the body always shown.
- **The embedded 3D scene** (PR 41) is never the only form of what it shows: a `scene` without a
  `replacement` is refused, and the replacement (the gallery's: a table with a row per edge of the
  graph, row headers on the subject) is the accessible form. On the HTML face it is always on the
  page, after the canvas, and the caption links to it ("The same graph as text"); a face with no
  3D, a browser with no WebGPU and a page with no script show it in the scene's place, and the
  canvas stays `hidden` (out of the accessibility tree) until it has drawn. The drawn canvas is
  focusable (`tabindex="0"`, `role="application"`, `aria-roledescription="3D scene"`), its
  `aria-label` names the scene and its keys; arrow keys orbit, `+`/`-` zoom, `N`/`Shift+N` select
  the next or previous node, `Escape` clears; the mouse does the same (drag, wheel, click). The
  selection ("Selected topic (PropertyValueSpecification): 3 out, 1 in.") is said in `#status`.
  The scene moves only when the viewer moves it, so `prefers-reduced-motion` needs nothing more.

### The native (GPUI) face

- It draws only with the palettes, read as `urn:iki:ui:palette:{name}` through the kernel: light,
  dark and high contrast, `auto` following the window's light or dark appearance live. No color is
  written in `gpui/src`, and every pair it draws (`fg`, `muted`, `accent`, `danger` on `bg`;
  `accent-fg` on `accent`; `line`, `rule`, `focus` on `bg`) is one `PAIRS` holds.
- Custom elements expanded draw the same group, heading, table (rows of cells) and list as the
  HTML face. `meter` native is a noninteractive palette bar (accent on surface, warn for a
  suboptimal region, danger for the region farthest from optimum or a value beyond the bounds)
  with a line border and an inset, beside the same visible reading, bounds, thresholds, optimum
  and assessment as HTML: color only reinforces words (SC 1.4.1), and it adds no tab stop.
  `disclosure` native is one focusable control in the window's tab order, toggled by Space, Enter
  or pointer, its state (▸ closed, ▾ open) held by the window, not the model; open, its body's
  controls follow it in the tab order, closed, they leave it.
- Links: `/page/{name}` loads the screen `{name}` through the host's kernel; a name that is not a
  screen name, or one the kernel has no screen for, is refused in words and the screen shown
  stays. A window holds the keyboard focus when it opens and after each screen loads, so Tab
  reaches the first link or control without a click.
- **The reader spike** (PR 43). Feeds: a level-1 heading "Public feeds" and a list named "Public
  feeds" of links, one per public feed, named by the feed's label. A feed: a "Back" nav ("All
  feeds"), the feed's label as the level-1 heading, then a list named "Entries, newest first",
  each entry a group named by its title with a level-2 heading, its date as text and a link "Read
  {title}". An entry: a "Back" nav ("Back to the feed", "All feeds"), its title as the level-1
  heading, its date, and its Markdown as headings, text, lists and `https:` links only (raw HTML
  and other schemes dropped). Keyboard path: Tab reaches each feed link, Enter opens it; in a
  feed, Tab passes "All feeds" to each "Read" link, Enter opens the entry; in an entry, Tab to
  "Back to the feed" and Enter returns.

### Per-kind acceptance criteria

Every node kind (`urn:iki:ui:criterion:kind:{kind}:target:{html,gpui}`, linked from the kind's
descriptor) and the elements `card`, `key-value`, `tag-list`, `disclosure` and `meter`
(`urn:iki:ui:criterion:element:{element}:target:{html,gpui}`) has a criterion per face (PRs 65
to 67): what the face emits, its keys, what a screen reader announces (Name, Role, Group, State)
and its SEE and HEAR checks. `meter`'s and `scene`'s html criteria are written from this file
(the rules above), which neither third-party source covers. The gpui criteria are desktop-native
ones (PR 66), our own, drawn from Apple's accessibility guidelines and AppKit roles, Windows UI
Automation's control types, AT-SPI's roles and states, GNOME's keyboard guidelines and AccessKit's
roles, with what VoiceOver, Narrator and Orca should announce once there is a tree. GPUI 0.2.2
exposes no tree, so every gpui HEAR check is `blocked` by the toolkit, and so are the settings it
does not report (system text size, platform high contrast, a magnifier following focus).

ikigai-ui relies on all of them: it is where they are defined and the face that renders them. The
support matrix's accessibility column is the per-kind summary: for each target and kind or
element, the SEE checks a test verifies of all of them, those blocked by the toolkit, and the HEAR
checks no test verifies. On main today it counts, summed over the 16 kinds and 5 elements:

| Target | SEE checks | with a test | blocked by the toolkit | HEAR checks, untested |
|---|---|---|---|---|
| html | 124 | 103 | 0 | 53 |
| gpui | 161 | 18 | 49 | 31 |

The html SEE checks with a test are held by `tests/criterion.rs` on every node of their kind on
the gallery and elements screens: the element or attribute emitted, native focusability or none,
the 3px `:focus-visible` ring, the accessible name (from its text, `<label for>`,
`aria-labelledby` or `aria-label`), the heading level, the content as text, a table's caption and
scoped headers, a list's items, an image's alt, an option's value, a status that is no live
region, a disclosure's open state, a meter's range, a scene's hidden canvas and linked
replacement, the hint and error in `aria-describedby`, `aria-required` and `aria-invalid`, and no
`px` font sizes. The 21 without a test are each kind's 400% zoom check. The gpui SEE checks with a
test (tab order and toggles) are in a window test CI does not run (below).

## How it is tested

Every named check has a result: Pass, Fail or Blocked, with evidence. A Pass covers only the
stated check, not overall accessibility. Each Pass below was run on main at a60eb0e on
2026-11-12: the cargo tests locally (`cargo test --workspace`, cargo 1.99.0, Linux, all passed)
and in CI's `ci / fmt · clippy · test` job; the browser checks in CI's `axe (WCAG 2.2 AA)` job
([run 43319333090](https://github.com/ikigai-rs/ikigai-ui/actions/runs/43319333090)), Node 22,
Playwright 1.64.0 and axe-core 4.13.0 (`@axe-core/playwright` 4.13.0, pinned in
`axe/package-lock.json`), headless Chrome for Testing 156.0.8078.4. CI repeats both on every pull
request and every push to main. A historical run CI does not repeat is Blocked, with its date.

| Check | Result | Evidence |
|---|---|---|
| Palette contrast floors, light, dark and high contrast | Pass | `tests/palettes.rs`, through ikigai-a11y, over the palette tables. Covers every pair in the tables; not a host's own CSS. |
| No color outside the palettes; every `var(--x)` defined | Pass | `tests/palettes.rs`, the test kit's lints. |
| A rebound palette held to the floor, or refused naming the value | Pass | `tests/palettes.rs` (rebound palettes through `tokens.css`), `gpui/src/theme.rs` unit tests. |
| `prefers-color-scheme` and `prefers-contrast: more` pick the palette | Pass | `tests/palettes.rs` (the media rules and their order in `tokens.css`); axe in light, dark and more-contrast media (below). |
| `prefers-reduced-motion: reduce` rule | Pass | `tests/pages.rs` holds `tokens.css` to its snapshot, rule included. Covers the rule's presence only: no test emulates the setting and observes motion. |
| Landmarks and one status region | Pass | `tests/pages.rs` (`the_shell_has_its_landmarks_and_one_status_region`). |
| In-place notes and the one live region | Pass | `tests/shell.rs`; `axe/shell-smoke.mjs` in Chromium. |
| Focus after a swap, a poll and a lazy load | Pass | `axe/shell-smoke.mjs` in Chromium (five checks). Scripted focus; not a person's keyboard pass. |
| A 403 leaves its control focusable, `aria-disabled` and inert (PR 68) | Pass | `axe/shell-smoke.mjs`, "a 403 leaves its control focusable, aria-disabled, and inert; markup in a refusal is text". |
| Live views refetch quietly, keep focus; a refused stream said once | Pass | `axe/shell-smoke.mjs` (ten live-view checks). |
| Theme chooser: label, choices in words, keyboard, no-script Apply, kept in this browser | Pass | `tests/pages.rs` (snapshots in each theme), `tests/chooser.rs`, `axe/shell-smoke.mjs` (four chooser checks). |
| Automated checker: axe, WCAG 2.0, 2.1 and 2.2 A and AA rules | Pass | `axe/check.mjs`: 31 pages (every demo page in every theme, the gallery, elements and lens pages) x 3 media settings, 0 failures, under the door default CSP. Includes axe's target-size rule (SC 2.5.8). axe finds only what rules can find. |
| Accessibility tree: each kind's role and name (PR 69) | Pass | `axe/aria.spec.mjs` (`toMatchAriaSnapshot`), 13 tests passed, run at the end of `axe/shell-smoke.mjs`. Covers Chromium's tree for buttons, links, fields with hint-then-error descriptions, the radio group, checkboxes, the table, card, key-value, tag-list, meter, image, figure, disclosure's expanded state and `#status`; each expectation derived from its kind's html criterion. Not what a screen reader says. |
| Custom elements expand to core kinds; fallback text | Pass | `tests/element.rs` (expanded and fallback HTML held to `tests/snapshots/elements-*.html`, the support matrix), `gpui/tests/worker.rs` (the native face receives only kinds it draws), axe over `/elements.html`. |
| `meter`, HTML native and expanded | Pass | `tests/element.rs`, `tests/snapshots/meter-{native,defaults,expanded}.html`, `tests/meter.rs`, axe over `/elements.html`. |
| `disclosure`, HTML native and expanded | Pass | `tests/element.rs`, `tests/snapshots/disclosure-{native,expanded}.html`, `axe/aria.spec.mjs` (collapsed, then expanded). |
| A scene needs a replacement; the replacement shows with WebGPU absent or no script | Pass | `tests/scene.rs`; `axe/shell-smoke.mjs` (CI's browser had no WebGPU adapter: "checked the absent path"). |
| The drawn scene canvas: focus, keys and the selection in `#status` | Blocked | CI's headless Chromium has no WebGPU adapter, so `axe/shell-smoke.mjs` takes the absent path. The drawn path has been checked by hand in Safari and Chrome; the date and browser versions were not recorded, and the check is not repeated. |
| Per-kind criteria: shape, sources, gpui HEAR blocked by the toolkit | Pass | `tests/criterion.rs` (9 tests: the sheets and the closed shape agree, no gpui source is MagentaA11y's or atomica11y's, every gpui HEAR check blocked, no html check blocked). |
| html SEE checks with a probe (103 of 124) | Pass | `tests/criterion.rs` (`every_see_check_holds_on_the_gallery_html`, HTML parsed by scraper, CSS by cssparser, PR 70; `a_broken_face_fails_its_probes`). On the gallery and elements screens only. |
| Support matrix accessibility column | Pass | `tests/criterion.rs` (`the_support_matrix_counts_verified_see_and_untested_hear_checks`). |
| Reader spike screens through the HTML face; Markdown sanitizing | Pass | `apps/reader/tests/reader.rs`. Locally it needs a umask that leaves its temporary directory unwritable by others (`umask 022`): under `umask 0002` two tests refuse the state key's directory, as the host is meant to. |
| Native face draws only palette colors | Pass | `gpui/tests/palette_source.rs` (`no_color_is_written_in_the_native_face`), `tests/palettes.rs` (every drawn pair in `PAIRS`). |
| Native windows: themes, keyboard paths, focus on open, `meter` bar, `disclosure` toggle, links | Blocked | `gpui/src/native.rs` window tests and `apps/reader/tests/window.rs` are macOS-only and CI's `macos` input is off (the job is skipped). Recorded as passing locally on macOS (`gpui/README.md`, "Verification"), and on Linux (lawnboy) with GPUI's `test-support` taken on 2026-11-12 (PRs 58 and 67); not run for this review, and CI does not repeat them. |
| gpui SEE checks with a probe (18 of 161) | Blocked | `gpui/src/native.rs` (`every_gpui_see_check_with_a_probe_holds`), a macOS-only window test: passed on Linux with `test-support` taken on 2026-11-12 (PR 67); no CI job runs it. |
| gpui SEE checks with no probe and not blocked (94 of 161): display scale, the explicit high-contrast theme, what is drawn | Blocked | No test verifies them; nobody has checked them on a real window for this review. |
| 400% zoom and reflow (SC 1.4.4, 1.4.10) | Blocked | Each kind's 400% zoom check has no probe, and no browser run at 320 CSS px or 400% zoom is part of CI. |
| Real native windows looked at | Blocked | Looked at on macOS by hand, and on Linux (X11 under Xvfb, Wayland under weston, lawnboy) on 2026-11-12 for text and theme only (`gpui/README.md`, "Verification"). Never on Windows or FreeBSD; nobody has driven a real keyboard, IME or appearance change on Linux or Windows. |
| Manual keyboard | Blocked | The keyboard checks above are scripted. No person has done a keyboard pass of the HTML face for this review. |
| Screen reader | Blocked | No screen reader has been used for this review, and no screen-reader job runs, so every html HEAR check (53) is untested. The native face cannot be heard at all: GPUI 0.2.2 exposes no accessibility tree, so every gpui HEAR check (31) is blocked by the toolkit. |
| Screen magnifier | Blocked | No magnifier has been used on either face. GPUI 0.2.2 does not report focus to one (blocked by the toolkit). |
| Speech input | Blocked | No speech-input software has been used on either face. |
| Switch access | Blocked | No switch-access software has been used on either face. |
| Braille display | Blocked | No braille display has been used on either face. |

## Known limitations

- **A page that fixes its theme.** On a page composed over the shell whose binding writes
  `theme=auto` on its own line, the chooser's no-script Apply asks for the page with `theme=` and
  the host refuses it, so without script a viewer cannot change the theme there. The README's
  recipe (PR 17) declares `theme` as an input with a default instead, which answers Apply
  (`tests/chooser.rs`); with script the choice applies at once either way, and a page built with
  `pages()` or `Pages` takes `theme=`.
- **The native face has no accessibility tree.** GPUI 0.2.2 exposes none (no AccessKit), so
  VoiceOver, Orca and Narrator hear nothing of a window's content, and native windows are not
  checked by axe or any screen reader on any platform. GPUI's main branch has AccessKit
  (zed-industries/zed#56065, merged 2026-05-27), but no release carries it, Zed runs with it off
  by default, and bugs in its focus reporting are open; `gpui/README.md`, "Accessibility tree",
  says what adopting it would take.
- **The native face's settings.** GPUI 0.2.2 reports a window's appearance as light or dark only,
  not macOS "Increase contrast", the Linux portal's `contrast` or Windows' contrast themes, so
  `auto` never picks high contrast there (choose `high-contrast` explicitly). `auto` follows
  macOS's Appearance, the Linux XDG portal's `color-scheme` (light when there is no portal) and
  Windows' app mode. It reports no system text size, so windows' text does not follow Windows'
  Text size or GNOME's text scaling factor; display scaling it follows.
- **The native face's tests.** Its window tests run on GPUI's test platform, which can neither
  change a window's appearance (the test calls the appearance observer's handler) nor show the
  pixels a real window paints: they check the colors the window draws with, not a screenshot. On
  Windows only the portable tests are meant to run, and the windows are untested there.
- **The 3D scene draws pixels only.** A screen reader hears the scene's name, its keys and what
  `#status` says, and reads the graph itself in the replacement: the replacement is the
  accessible form, not a description of the picture. The GPUI face shows the replacement.

## Contributing

Read this file before changing anything listed above, and keep the rules above. A page composed
over the shell is the host's: name each `<nav>` with `aria-label` when there is more than one,
label every control, and run the test kit and the axe workflow (`.github/workflows/axe.yml`) over
your own pages. In a pull request's `## Accessibility` section, give each check a row in a
Check | Result | Evidence table: Pass, Fail or Blocked, with the test or command, tool and version
used, or why it could not run. Keep unrun checks as Blocked, including a screen reader nobody
ran; never call it "not applicable".

## Reporting a barrier

[Open an issue](https://github.com/ikigai-rs/ikigai-ui/issues/new) with "accessibility" in its
title: what you were doing, what happened, and your browser or platform and any assistive
technology, if you are comfortable sharing them. You never need to disclose a disability. This
repository has no private reporting channel yet; for anything security-sensitive, open an issue
that says only that you have something to report privately, with no details.
**We acknowledge a report within 7 days**, and promise nothing further.
