# ikigai-ui

The shared UI path for [ikigai](https://github.com/ikigai-rs) hosts, as resources. Bind one
space and every page you serve gets the same htmx, the same themes, the same refusal and focus
behavior, and the same tests.

| name | what you get |
|---|---|
| `urn:iki:ui:htmx` | htmx 2.0.4, vendored and SHA-256 pinned; the shell loads it with `integrity=` and configures it once (`allowEval:false`, `allowScriptTags:false`) |
| `urn:iki:ui:tokens.css` | one token set (gonk's names, with `--line` for component borders and `--rule` for decorative lines) in light, dark and high contrast; `auto` follows the system, `prefers-contrast: more` picks high contrast, `data-theme` overrides; every pair contrast-tested (WCAG 2.2 AA, AAA for high contrast) |
| `urn:iki:ui:shell.js` | the one first-party script: refusals shown as text where they happened, a 403 disables its control, one status region, focus after a swap, the theme applied before first paint, and live views refetched from the door's push stream |
| `urn:iki:ui:template:shell` | the page shell: landmarks, a skip link, the status region, the theme chooser in the header, no inline script or style |
| `urn:iki:ui:theme-chooser:{theme}` | the theme chooser, `{theme}` selected: a GET form with a "Theme" label, a native select of Auto (system), Light, Dark and High contrast, and an Apply button for a page with no script |
| `urn:iki:ui:chooser:{family}:{theme}` | what the shell puts in a family's header, for ANY family: the family's `urn:iki:ui:chooser-override:{family}:{theme}` if one is bound, else the theme chooser |
| `urn:iki:ui:form:resource:{iri}` | a `form` node generated from a MODEL, not written by hand: `{iri}`'s own contract (its Meta), one field per input its Sink reads, required or optional, typed, a choice for a closed set, its default as the value. The contract is the model; OpenAPI is one projection of it, and a SHACL shape or a JSON Schema would serve the same way |
| `urn:iki:ui:form:openapi:{doc}:operation:{op}` | the same form from an OpenAPI 3.1 document's operation, for a service that is not an ikigai resource; the same model gives the same form |
| `pages(family, base)` | your `{base}page:{name}` pages: the shell over your `{base}header` (then the theme chooser), `{base}title:{name}` and `{base}fragment:{name}`; a name with `:` in it (`item:5`) reaches them whole |
| `Pages::new(family, base)` | the same, with your own stylesheets and scripts in `<head>`, the door's push stream named for live views, and the chooser turned off with `.theme_chooser(false)` |
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

**A theme chooser on every page.** The shell puts the chooser in every page's `<header>`, after
your `{base}header`, with the page's theme selected, so a family gets it without writing it (the
same chooser the tutorial books have: auto, light, dark, high contrast, in words). With the shell
script, a change applies at once and the Apply button is hidden; with no script, Apply reloads
the page with `theme=`, which the page renders. To put it somewhere else, turn the shell's off
(`Pages::new(…).theme_chooser(false)`) and splice `urn:iki:ui:theme-chooser:{theme}` where you
want it; to have none, just turn it off. One chooser per page (its select is `#ui-theme`).

**A page composed over the shell directly** (`urn:iki:fn:compose src=<urn:iki:ui:template:shell>
family=… name=…`, say from a names file) gets the chooser too, with nothing to bind: the shell
reads `urn:iki:ui:chooser:{family}:{theme}`, and this crate's space answers it for every family.
Declare `theme` as an input of the name, with a default, so the chooser's no-script Apply works:

```text
urn:quakes:page = source urn:iki:fn:compose src=<urn:iki:ui:template:shell> family=quakes name=home
  input theme default=auto one-of="auto" one-of="light" one-of="dark" one-of="high-contrast"
```

A bare request renders `auto`, Apply's `?theme=dark` renders dark, and a value that is not a theme
is refused. Do not write `theme=auto` on the binding's own line: that fixes the argument, and a
caller who gives a fixed argument is refused, so Apply is too (with script, the choice still
applies at once, since it never reaches the server).

To turn the chooser off, bind the family's override to nothing, for every theme:

```text
urn:iki:ui:chooser-override:quakes:{theme} = source urn:file:empty.html
```

Bind the override to your own markup to replace the chooser there instead. A page that fixes its
theme only needs that theme's override, but with a `theme` input, an override for `auto` alone
brings the chooser back at `?theme=dark`. Bind the override, not
`urn:iki:ui:chooser:{family}:{theme}` itself: a host resolves a name in the first space that binds
it, and this crate's space usually comes first (a names layer comes last), so a binding of the
shell's own name after it is never reached. `Pages::theme_chooser(false)` binds the same override,
so it holds wherever the host puts the family.

A viewer's choice is kept in this browser only: `localStorage` under `ikigai-ui:theme` (choosing
auto clears it), applied before first paint over the server's `auto`. It is never sent to the
server, except as `theme=` in the URL when the form is submitted with no script. A GET form's
`theme=` replaces the page's own query string, so a page whose query matters wants the script,
or its own chooser.

**Live views.** Name the push stream on the family (`Pages::new(…).push("/_ikigai/push")`, the
cli door's path), and a view that says what it shows refetches itself on a cut:

```html
<div id="board" hx-get="/app/fragment/board" hx-trigger="ikigai:cut" hx-swap="outerHTML" data-ui-push="">…</div>
```

`data-ui-push` holds the paths the view shows (empty: its own `hx-get`). The shell script opens
one `EventSource` for every such view on the page, sends `ikigai:cut` to the views a cut's
`paths` name (all of them on a `resync`), keeps the refetch quiet and keeps focus. A cut that
lands while a view's own request is in flight is sent again once it settles (tracked on the
view, so an `hx-indicator` elsewhere does not hide it); a view that swaps `outerHTML` wants an
`id`, which is how its replacement is found. When a swap changes the declared paths the stream
is reopened, and every view is refetched once the new stream is ready, since a cut in the gap
was heard by nobody. A stream the door refuses (a `400` past its subscription bound) is said
once in `#status`. The door grants `urn:cap:kernel:listen` for the stream to answer.

## The test kit

`ikigai-ui = { version = "0.5", features = ["testkit"] }` as a dev-dependency gives a UI repo:

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

Two of those decisions stand as amended on 2026-11-01 (ledger #2133, carried out in #2832):

- **D13, one status region.** `#status` is the page's only live region. A refusal note beside its
  control is plain text with no role; the control points at it with `aria-describedby` (removed
  when the note goes), and `#status` says the same sentence politely, naming the control
  ("Load details: not found"). `role="alert"` is kept for a failure with no control to sit
  beside. Quiet elements and live views say nothing.
- **D12, the 3:1 floor on component borders only.** `--line` is the border that identifies a
  control (an input, a button) and is held to SC 1.4.11's 3:1. `--rule` is a decorative line (a
  table rule, a section divider), which SC 1.4.11 exempts; it is held to a stated 1.3:1 so no
  palette can make it vanish, and light keeps gonk's `#c9d1d9`. High contrast holds both to 7:1.

## License

MIT OR Apache-2.0. htmx is 0BSD (`vendor/htmx-2.0.4.LICENSE`).
