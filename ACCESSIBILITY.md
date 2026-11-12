# Accessibility

ikigai-ui is the shared shell every ikigai UI builds on, so what it guarantees, every page over
it inherits. The floor is WCAG 2.2 AA (UI conventions D13), and each line below is checked by a
test, not a review.

| what | how it is checked |
|---|---|
| Text contrast 4.5:1, component borders (`--line`) and focus ring 3:1, in light and dark; decorative rules (`--rule`, exempt from SC 1.4.11) at least 1.3:1; 7:1 for every pair in high contrast | `tests/palettes.rs`, through ikigai-a11y, over the palette tables |
| No color outside the palette tables; every `var(--x)` defined | `tests/palettes.rs`, the test kit's lints |
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

Known limitation: on a page composed over the shell that FIXES its theme (a no-Rust host's
names file, `theme=auto`), the chooser's no-script Apply asks for the page with `theme=`, and
the host refuses it, so without script a viewer cannot change the theme there (ledger #2885,
part 2, a design question). With script the choice applies at once, and a page built with
`pages()` or `Pages` takes `theme=` and has no such gap.

Known limitations of the native face: GPUI 0.2.2 reports a window's appearance as light or
dark only, not macOS "Increase contrast", so `auto` never picks high contrast there (choose
`high-contrast` explicitly). Its window tests run on GPUI's test platform, which can neither
change a window's appearance (the test calls the appearance observer's handler) nor show the
pixels a real window paints; they check the colors the window draws with, not a screenshot.
Native windows are not checked by axe or any screen-reader test.

What a page puts inside the shell is the host's to keep accessible: name each `<nav>` with
`aria-label` when there is more than one, label every control, and run the test kit and the axe
workflow over your own pages.
