# ikigai-ui

The shared UI path for [ikigai](https://github.com/ikigai-rs) hosts, as resources. Bind one
space and every page you serve gets the same htmx, the same themes, the same refusal and focus
behavior, and the same tests.

| name | what you get |
|---|---|
| `urn:iki:ui:htmx` | htmx 2.0.4, vendored and SHA-256 pinned; the shell loads it with `integrity=` and configures it once (`allowEval:false`, `allowScriptTags:false`) |
| `urn:iki:ui:tokens.css` | one token set (gonk's names) in light, dark and high contrast; `auto` follows the system, `prefers-contrast: more` picks high contrast, `data-theme` overrides; every pair contrast-tested (WCAG 2.2 AA, AAA for high contrast) |
| `urn:iki:ui:shell.js` | the one first-party script: refusals shown as text where they happened, a 403 disables its control, one status region, focus after a swap, the theme applied before first paint, and live views refetched from the door's push stream |
| `urn:iki:ui:template:shell` | the page shell: landmarks, a skip link, the status region, no inline script or style |
| `pages(family, base)` | your `{base}page:{name}` pages: the shell over your `{base}header`, `{base}title:{name}` and `{base}fragment:{name}`; a name with `:` in it (`item:5`) reaches them whole |
| `Pages::new(family, base)` | the same, with your own stylesheets and scripts in `<head>` and the door's push stream named for live views |
| `artifacts(&[Artifact], dir)` | your own artifacts, with the same two deliveries: compiled in, or a file under your directory prefix |

```rust
let kernel = Kernel::new(Arc::new(Fallback::new(vec![
    Arc::new(ikigai_ui::space()),                      // the artifacts, palettes, tokens.css
    Arc::new(ikigai_ui::pages("app", "urn:app:")?),    // urn:app:page:{name}
    Arc::new(my_app),                                  // urn:app:header, :title:{name}, :fragment:{name}
    // Optional: override any artifact from a directory, golden-threaded.
    Arc::new(EndpointSpace::new().bind(
        UriTemplate::parse("urn:iki:ui:dir:{path}")?,
        ikigai_fs::FileEndpoint::new("ui/").cacheable(),
    )),
])));
// Door routes: /ui/{…} → urn:iki:ui:{…} (one route-table prefix entry, D10) and
// /app/page/{name} → urn:app:page:{name} (the mechanical rule).
```

**Compiled in, or from a directory, under the same names.** Every artifact is compiled in. Bind
a directory at `urn:iki:ui:dir:` and a file there (`shell.html`, `shell.js`, `tokens.template.css`,
`htmx.min.js`) replaces that one artifact, and a write to it recomputes every page over it. A
directory cannot add a name and nothing here serves a header or a CSP: those belong to the door.

**The page splices what the host says, never what the request says.** A page's fragment is
fixed by its own name and family, so a request argument cannot point the shell at another
resource. The page name is bounded before anything is read (RFC 3986's unreserved characters
and `:`; no `/ ? # %`), so it picks among your own `{base}fragment:…` names and nothing else.

**Live views.** Name the push stream on the family (`Pages::new(…).push("/_ikigai/push")`, the
cli door's path), and a view that says what it shows refetches itself on a cut:

```html
<div id="board" hx-get="/app/fragment/board" hx-trigger="ikigai:cut" hx-swap="outerHTML" data-ui-push="">…</div>
```

`data-ui-push` holds the paths the view shows (empty: its own `hx-get`). The shell script opens
one `EventSource` for every such view on the page, sends `ikigai:cut` to the views a cut's
`paths` name (all of them on a `resync`), keeps the refetch quiet and keeps focus. The door
grants `urn:cap:kernel:listen` for the stream to answer.

## The test kit

`ikigai-ui = { version = "0.1", features = ["testkit"] }` as a dev-dependency gives a UI repo:

- `snapshot`: pages held to files in the repo (written once, never re-blessed by a switch);
- `vectors`: FND-H's escaping and link vectors, ikigai-fn's file verbatim, to run any escaper against;
- `contrast`: the floor for every palette, through ikigai-a11y;
- `lint`: no hex color outside the palette tables, every `var(--x)` defined;
- `site`: rendered pages written as a static site, for axe.

And axe as a CI step:

```yaml
  axe:
    uses: ikigai-rs/ikigai-ui/.github/workflows/axe.yml@main
    with:
      prepare: cargo run --example axe_site -- target/axe-site
      site: target/axe-site
```

It serves the site with the door default CSP and fails on any WCAG 2.2 A/AA violation, page
error or console error, in light, dark and more-contrast media.

## Decisions

The UI conventions record (D7, D12, D13, D14, D17, D18) is the design; this crate is its shared
path. Migrating the existing apps onto it comes after the demo app and book.

## License

MIT OR Apache-2.0. htmx is 0BSD (`vendor/htmx-2.0.4.LICENSE`).
