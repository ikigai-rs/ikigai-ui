# Accessibility

ikigai-ui is the shared shell every ikigai UI builds on, so what it guarantees, every page over
it inherits. The floor is WCAG 2.2 AA (UI conventions D13), and each line below is checked by a
test, not a review.

| what | how it is checked |
|---|---|
| Text contrast 4.5:1, borders and focus ring 3:1, in light and dark; 7:1 for every pair in high contrast | `tests/palettes.rs`, through ikigai-a11y, over the palette tables |
| No color outside the palette tables; every `var(--x)` defined | `tests/palettes.rs`, the test kit's lints |
| Landmarks: a skip link, one `<header>`, one `<main id="main" tabindex="-1">`, one status region | `tests/pages.rs` |
| Focus after a swap: a swap into `#main` focuses its first heading; focus is never left on `<body>` | `axe/shell-smoke.mjs`, in Chromium |
| Refusals as text, in place, and a 403 disables its control; quiet elements never announce | `axe/shell-smoke.mjs` |
| A live view's refetch (D9) never announces, leaves no note, and keeps focus where it was, inside the view or not | `axe/shell-smoke.mjs` |
| A theme chooser that works with no script; `prefers-color-scheme`, `prefers-contrast: more` and `prefers-reduced-motion` honored | `tests/palettes.rs`, `axe/shell-smoke.mjs` |
| Touch targets of 24 px for navigation links (SC 2.5.8) | axe |
| WCAG 2.0/2.1/2.2 A and AA rules on every demo page, in light, dark and more-contrast media, under the door default CSP | `axe/check.mjs` in CI |

What a page puts inside the shell is the host's to keep accessible: name each `<nav>` with
`aria-label` when there is more than one, label every control, and run the test kit and the axe
workflow over your own pages.
