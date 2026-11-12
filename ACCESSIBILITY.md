# Accessibility

ikigai-ui is the shared shell every ikigai UI builds on, so what it guarantees, every page over
it inherits. The floor is WCAG 2.2 AA (UI conventions D13), and each line below is checked by a
test, not a review.

| what | how it is checked |
|---|---|
| Text contrast 4.5:1, component borders (`--line`) and focus ring 3:1, in light and dark; decorative rules (`--rule`, exempt from SC 1.4.11) at least 1.3:1; 7:1 for every pair in high contrast | `tests/palettes.rs`, through ikigai-a11y, over the palette tables |
| No color outside the palette tables; every `var(--x)` defined | `tests/palettes.rs`, the test kit's lints |
| A palette a host rebinds is parsed as a table and held to the same floor before either face draws with it: a value that is not `#rrggbb`, an unknown token or a pair below its floor is refused, naming it | `tests/palettes.rs` (rebound palettes through `tokens.css`), `gpui/src/theme.rs` |
| Landmarks: a skip link, one `<header>`, one `<main id="main" tabindex="-1">`, one status region | `tests/pages.rs` |
| One live region: an in-place note has no role; its control points at it with `aria-describedby` (removed when the note goes), and `#status` says the sentence, naming the control | `tests/shell.rs`, `axe/shell-smoke.mjs` |
| Focus after a swap: a swap into `#main` focuses its first heading; a swap that took the focused element never leaves focus on `<body>`; on a page nobody has focused, a poll or a lazily loaded region leaves focus on `<body>` and the scroll where it was | `axe/shell-smoke.mjs`, in Chromium |
| Refusals as text, in place, and a 403 disables its control; quiet elements never announce | `axe/shell-smoke.mjs` |
| A live view's refetch (D9) never announces, leaves no note, and keeps focus where it was, inside the view or not | `axe/shell-smoke.mjs` |
| A refused live stream is said once in the status region, and an accepted one says nothing | `axe/shell-smoke.mjs` |
| A theme chooser in every page's header (the shell's, `urn:iki:ui:theme-chooser:{theme}`, for a page composed over the shell without `Pages` too): a visible "Theme" label on a native select, its four choices in words (Auto (system), Light, Dark, High contrast), reached and changed from the keyboard; a change applies at once, and with no script its Apply button reloads the page with `theme=` | `tests/pages.rs` (snapshots in each theme), `tests/chooser.rs`, `axe/shell-smoke.mjs` |
| A viewer's theme is kept in this browser only (`localStorage`, `ikigai-ui:theme`; auto keeps nothing), shown by the chooser, and held across a reload; it reaches the server only as `theme=` when the form is submitted with no script | `axe/shell-smoke.mjs` |
| The chooser passes axe and the contrast floor in all four palettes: it is drawn with `--fg` on `--bg` and a `--line` border, pairs the floor already holds | `tests/palettes.rs`, `axe/check.mjs` over every page in every theme |
| `prefers-color-scheme`, `prefers-contrast: more` and `prefers-reduced-motion` honored | `tests/palettes.rs`, `axe/shell-smoke.mjs` |
| Touch targets of 24 px for navigation links (SC 2.5.8) | axe |
| WCAG 2.0/2.1/2.2 A and AA rules on every demo page, in light, dark and more-contrast media, under the door default CSP | `axe/check.mjs` in CI |
| The native (GPUI) face draws only with the palettes, read as `urn:iki:ui:palette:{name}` through the kernel: light, dark and high contrast, `auto` following the window's light/dark appearance live; no color is written in `gpui/src`, and every pair it draws (`fg`, `muted`, `accent`, `danger` on `bg`; `accent-fg` on `accent`; `line`, `rule`, `focus` on `bg`) is one `PAIRS` holds | `gpui/src/native.rs` window tests (each theme's `bg`/`fg`, a host's palette override, `auto` on an appearance change), `gpui/tests/palette_source.rs`, `tests/palettes.rs` |
| Custom elements (ledger #3358) reach assistive technology only as the core kinds they expand into, never as markup of their own: no face claims `card`, `key-value` or `tag-list` natively. HTML, expanded: a `card` is a `<section>` named by its title (`aria-label`) with a real heading at its `level`, its body a `<p>` and its links a named `<ul>` of `<a>`; a `key-value` is a `<table>` whose `<caption>` names it and whose terms are `<th scope="row">` row headers, so each value is announced with its term; a `tag-list` is a `<ul>` named by its label (`aria-label`), each tag a list item, a link when it has one. HTML, fallback text (no definition readable): one `<p>` of the element's `fallback` text, with no heading, table or list semantics and none of its links or children. GPUI, expanded: the same group, heading, table (drawn as rows of cells) and list, with no assistive-technology semantics (see the native face's limitations below) | `tests/element.rs` (the expanded and fallback HTML held to `tests/snapshots/elements-*.html`, the support matrix, a claiming face keeping each whole), `gpui/tests/worker.rs` (the native face receives only kinds it draws), `axe/check.mjs` over `/elements.html` in every theme |
| An embedded 3D scene (ledger #3514) is never the only form of what it shows: a `scene` without a `replacement` is refused, and the replacement (the gallery's: a table with a row per edge of the graph, row headers on the subject) IS the accessible form. On the html face it is always on the page, after the canvas, and the caption links to it ("The same graph as text"); a face with no 3D, a browser with no WebGPU and a page with no script show it in the scene's place, and the canvas stays `hidden` (out of the accessibility tree) until it has drawn | `tests/scene.rs` (a scene with no replacement refused; snapshots of both paths; the gallery's replacement names every edge of the datum), `axe/shell-smoke.mjs` (WebGPU absent, script off, and drawn when an adapter exists) |
| The drawn canvas is keyboard-operable and named: it is focusable (`tabindex="0"`, `role="application"`, `aria-roledescription="3D scene"`), its `aria-label` names the scene and its keys; arrow keys orbit, `+`/`-` zoom, `N`/`Shift+N` select the next/previous node, `Escape` clears; the mouse does the same (drag, wheel, click). The selection ("Selected topic (PropertyValueSpecification): 3 out, 1 in.") is said in `#status`, the page's one live region | `axe/shell-smoke.mjs` on a browser with a WebGPU adapter (`N` then `#status`) |
| `disclosure` (ledger #3512), the one element both faces claim natively. HTML, native: a `<details class="ui-disclosure">` whose `<summary>` is the summary line, with `open` only when the model's initial `open` is true: the browser gives the summary its button role and expanded/collapsed state, toggles it by Enter and Space and by pointer, with no script and no ARIA written; the body (a `<p>` and its children) is in the tab order and the accessibility tree only while open. HTML, expanded (the descriptor's `elements` without it): a `<section>` named by the summary (`aria-label`) with the summary as a real heading at its `level` and the body always shown, so nothing is hidden from a face that cannot toggle. GPUI, native: the summary is one focusable control in the window's tab order, toggled by Space, Enter or pointer, its state (▸ closed, ▾ open) held by the window, not the model; open, its body's controls follow it in the tab order, closed, they leave it; no assistive-technology semantics (the native face's limitation below), so no `aria-expanded` reaches a screen reader. GPUI, expanded: the same heading, text and children as HTML's, always shown | `tests/element.rs` (native `<details>`/`<summary>`, closed and initially open, escaped; the expansion with native off; `tests/snapshots/disclosure-{native,expanded}.html`; the support matrix saying Native on both), `gpui/src/native.rs` window tests (toggle by Space and Enter, Tab reaching the body's link only when open, the model unchanged; the expansion with native off), `gpui/tests/worker.rs`, `axe/check.mjs` over `/elements.html` in every theme |
| The native face's links: `/page/{name}` loads the screen `{name}` through the host's kernel (the names the HTML face routes), so any app's screens are reached natively; a name that is not a screen name, or one the kernel has no screen for, is refused in words and the screen shown stays. A window holds the keyboard focus when it opens and after each screen loads, so Tab reaches the first link or control without a click (before, nothing had focus and Tab did nothing) | `gpui/src/native.rs` (`a_page_link_loads_any_screen_the_kernel_names_and_refuses_the_rest`), `apps/reader/tests/window.rs` (Tab, Tab, Enter) |
| The reader spike (`apps/reader`, ledger #3524). **Feeds**: a level-1 heading "Public feeds" and a list named "Public feeds" of links, one per public feed, named by the feed's label (HTML: a named `<ul>` of `<a>`). **A feed**: a "Back" nav ("All feeds"), the feed's label as the level-1 heading, then a list named "Entries, newest first", each entry a group named by its title with a level-2 heading, its date as text and a link "Read {title}". **An entry**: a "Back" nav ("Back to the feed", "All feeds"), its title as the level-1 heading, its date, and its Markdown as headings, text, lists and `https:` links only (raw HTML and other schemes dropped). Keyboard path: Tab reaches each feed link, Enter opens it; in a feed, Tab passes "All feeds" to each "Read" link, Enter opens the entry; in an entry, Tab to "Back to the feed" and Enter returns. Colors are the palettes' (the native face's rule above) | `apps/reader/tests/window.rs` (feeds, follow, newest first, Tab Tab Enter to the entry, Tab Enter back to the feed), `apps/reader/tests/reader.rs` (the same screens through the HTML face, Markdown sanitising) |

Known limitation: on a page composed over the shell that FIXES its theme (a no-Rust host's
names file, `theme=auto`), the chooser's no-script Apply asks for the page with `theme=`, and
the host refuses it, so without script a viewer cannot change the theme there (ledger #2885,
part 2, a design question). With script the choice applies at once, and a page built with
`pages()` or `Pages` takes `theme=` and has no such gap.

Known limitations of the native face: GPUI 0.2.2 reports a window's appearance as light or
dark only, not macOS "Increase contrast", the Linux portal's `contrast` or Windows' contrast
themes, so `auto` never picks high contrast there (choose `high-contrast` explicitly). `auto`
follows macOS's Appearance, the Linux XDG portal's `color-scheme` (light when there is no
portal) and Windows' app mode. Its window tests run on GPUI's test platform, which can neither
change a window's appearance (the test calls the appearance observer's handler) nor show the
pixels a real window paints; they check the colors the window draws with, not a screenshot.
Native windows are not checked by axe or any screen-reader test, on any platform: GPUI 0.2.2
exposes no accessibility tree (no AccessKit), so VoiceOver, Orca and Narrator hear nothing of a
window's content. By platform (ledger #3526): the window tests (keyboard paths, themes, focus)
run on GPUI's test platform on macOS, and passed on Linux (lawnboy), but no CI job runs them
(`gpui/README.md`, "Verification"); on Windows only the portable tests are meant to run (once the
`windows` CI input is on), and the windows are untested there.
Real windows were looked at on macOS by hand and on Linux (X11 and Wayland, lawnboy, without a
desktop session) on 2026-11-12, for text and theme only; nobody has driven a real keyboard, IME
or appearance change on Linux or Windows. The reader's window test drives the keyboard on GPUI's test platform and reads the snapshot each screen draws; nobody has looked at its pixels or heard it through VoiceOver.

Known limitations of the 3D scene: the canvas draws pixels only, so a screen reader hears the
scene's name, its keys and what `#status` says, and reads the graph itself in the replacement
(that is the design: the replacement is the accessible form, not a description of the
picture). `prefers-reduced-motion` needs nothing: the scene moves only when the viewer moves it.
CI's headless Chromium may have no WebGPU adapter; the smoke check says which path it took, and
the drawn path is otherwise checked by hand in Safari and Chrome. The GPUI face shows the
replacement (phase 2 is a research spike, see `gpui/README.md`).

What a page puts inside the shell is the host's to keep accessible: name each `<nav>` with
`aria-label` when there is more than one, label every control, and run the test kit and the axe
workflow over your own pages.
