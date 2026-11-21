# Design spike: the workspace

Ledger #3941. Status: **proposal, for Brian's decision.** This is a design document. It changes
no code, and it files no items. The names below are proposals; nothing here is bound yet.
It ends with [the decisions](#8-decisions-for-brian).

A **workspace** is a tiled layout of panes, saved as a resource. Each pane shows one of three
things: a **lens** (a view of a resource), a **live stream** (a lens that refetches when its
resource changes) or an **agent session** (a running session's prompts, tool calls and results as
they happen). Selecting something in one pane moves the panes that follow it. Every read in every
pane is made under the viewer's capability, and every action goes through the same Sink, policy
and approval path as any other call.

What this spike checked and what it did not:

- **Read for it in this repository** (main at `5eb7aa0`): the lens model (`src/lens.rs`,
  `src/lens/actions.rs`), regions (`src/region.rs`), settings and screen spaces, the shell's live
  views and write lapse (`assets/shell.js`, sections 5 and 6), the accessibility rules
  (`ACCESSIBILITY.md`) and the GPUI upgrade spike (`docs/design/gpui-upgrade.md`).
- **Read in other repositories through `gh`, not run:** ikigai-gonk (README routes,
  `src/dashboard.rs`, `docs/design/push-at-the-door.md`), ikigai-ledger's README, ikigai-log's
  README and `src/vocabulary.ttl`, ikigai-cli's push stream (`crates/ikigai-web/src/push.rs`),
  ikigai-browse's review routes and gonk-starter's harness hook. Where I could not find something,
  it says **not found**.
- **Not done:** no prototype. No layout was drawn or measured, and no screen reader was used.

## 0. What exists, and the gap

| Piece | What exists | Where it stops |
|---|---|---|
| Views | A lens is a read-only SPARQL query plus a renderer template (`graph`, `table`, `tree`, `timeline`, `thread`, `board`, `construct` and the knowledge lenses). Both faces render it. Its screen name is `lens:{lens}[:facets:{k=v&…}]:resource:{iri}`. As a page it is `/lens/name/{lens}/resource/{iri}`, and as a fragment it is `/ui/html:screen:lens:…`. | One lens per page. Its links go through the home mapping (`urn:iki:ui:home`), so a click leaves the page. |
| Selection | `selected=`, `target=` and `dropped=` are request parameters folded into the screen name (`lens::fold`). The operations view takes a dropped set of at most 32 values. It is "request-local, never shared mutable state" (`src/lens/actions.rs`). | Selection lives in one view and its operations lens. Nothing else hears about it. |
| Live views | `data-ui-push` paths on an element, one `EventSource` per page to the door's push stream, `ikigai:cut` makes htmx refetch. The refetch is quiet and keeps focus (D9). A refused stream is said once. | It is per element and works on any page. Nothing groups views. |
| Confinement | A `region` composes its children in a narrowed space: `allow` lists the names it may read and `scopes` attenuates the capability. | It is a structural node with no layout of its own. |
| Actions | Forms are generated from a Sink's contract (`urn:iki:ui:form:resource:{iri}`). They are offered only when `urn:kernel:actions` (the manifold under the caller's capability) offers the Sink. Submission goes through `urn:iki:ui:lens:operation:{op}`, which rechecks and calls `urn:kernel:validate`. `data-ui-write` controls are disabled when the session's write lapses. | Already the right shape. A pane must not add a path around it. |
| Layout | `urn:iki:ui:hint:group:columns` wraps a group's children as columns. A face without the hint keeps their reading order. | There are no sizes, no splits and no tabs, and nothing is saved. |

The gap is the frame: a stored layout, a contract every pane meets, and one selection that more
than one pane follows. The design below adds those three things and keeps everything else.

## 1. The workspace as a resource

### Names

Singular, type first, keyed partitions, owner before name:

| Name | What it is | Stored or composed |
|---|---|---|
| `urn:iki:ui:workspace:person:{person}:name:{name}` | One person's workspace: its layout and its panes | **stored** (the atom) |
| `…:name:{name}:version:{n}` | One saved version, never rewritten | stored (by the host's store) |
| `…:name:{name}:as-of:{instant}` | The version in force at an RFC 3339 instant | composed: the highest `n` saved at or before `instant` |
| `urn:iki:ui:workspace:person:{person}` | That person's workspaces, as a list | composed |
| `urn:iki:ui:shape:workspace` | The atom's closed SHACL shape (Turtle). `ikigai_ui::workspace::validate` would say the same in Rust, as `scene` and `criterion` do. | artifact |
| `urn:iki:ui:screen:workspace:{workspace-iri}` | The workspace as a screen: the layout composed into ordinary model nodes, with each pane's content composed in | composed |
| `{base}page:workspace:{workspace-iri}` | The same as a page in the shell. A door routes it from `/workspace/{+iri}`, as `Pages::lenses` does for lensed pages. | composed |

`{person}` is whatever IRI the host's sign-in gives a person (for example `urn:person:alice`).
ikigai-ui does not define people. There is no sign-in in the demo yet (#2169), so the demo would use
one fixed person.

### The atom

The stored thing is one small JSON document (JSON-LD by the model's context), and nothing else.
It is closed: unknown fields are refused, naming the field, as target descriptors are.

```json
{
  "label": "Item 3941",
  "layout": {"split": "row", "children": [
    {"share": 300, "pane": "dashboard"},
    {"share": 700, "split": "column", "children": [
      {"share": 600, "pane": "session"},
      {"share": 400, "tabs": ["item", "trail"], "active": "item"}
    ]}
  ]},
  "panes": {
    "dashboard": {"label": "Dashboard", "content": "lens:dashboard:resource:urn:iki:ledger:default", "emits": "item"},
    "session":   {"label": "Session",   "content": "lens:session:resource:{item}", "follows": "item", "live": true},
    "item":      {"label": "Item",      "content": "lens:item:resource:{item}",    "follows": "item", "emits": "item"},
    "trail":     {"label": "Trail",     "content": "lens:trail:resource:{item}",   "follows": "item"}
  },
  "selection": {"item": "urn:iki:ledger:default:item:3941"}
}
```

- **`layout`** is a tree of three node types. A `split` has an axis (`row` or `column`) and children,
  each with a `share` in thousandths (integers, so a version compares byte for byte and no
  float rounds differently on two faces). A `tabs` node lists panes and names the active one. A
  `pane` leaf names a pane by its key. The shares in one split must add up to 1000.
- **`panes`** is a map from a key to a pane (section 2). A key is `[a-z0-9-]`, at most 32 bytes.
- **`selection`** is the saved selection (section 3): a map from a selection role to one IRI.
- **Bounds**, said in words when they are hit: at most 16 panes, a layout at most 4 deep, at most
  8 tabs in a tabs node, at most 8 selection roles.

What the atom does **not** store: any pane's content, any answer, any capability, any width in
pixels and any theme. Content is composed fresh under the viewer's capability every time. Theme,
density and locale stay settings (`urn:iki:ui:setting:name:{name}`).

### What is composed

`urn:iki:ui:screen:workspace:{workspace-iri}` reads the atom, validates it, and composes:

- the whole workspace as a `group` labeled with the workspace's `label`, with a new presentation
  hint `urn:iki:ui:hint:layout:split` (value `row` or `column`, plus the shares) on each split
  group and `urn:iki:ui:hint:layout:tabs` on each tabs group;
- each pane as a **`region`** (the existing structural node) labeled with the pane's `label`, whose
  child is the pane's content screen with the selection put in it.

So a workspace is **an ordinary screen**. The fallback ladder, the descriptors, regions,
references and withheld rungs all apply as they do now. A face whose descriptor does not claim
`urn:iki:ui:hint:layout:` shows the panes in reading order (layout order, depth first), and the
tabs as a list of their panes. That is also the accessible linear form (section 5) and the GPUI
face's form until it claims the hint.

### Saved per person, shareable, versioned, as of a date

- **Saved per person.** The partition is the person. A write to
  `urn:iki:ui:workspace:person:{p}:…` needs `urn:cap:ui:workspace:write:person:{p}`, which a host
  grants a person for their own partition only.
- **Shared** means a read grant, `urn:cap:ui:workspace:read:person:{p}:name:{name}`, given by the
  owner. Someone you share with sees **your layout**, filled with **their own reads**. A pane whose
  content they may not read shows its withheld rung, so sharing a workspace never shares what is
  in it. To change a shared workspace, they **copy** it into their own partition (a Sink,
  `…:name:{name}:copy`, which writes version 1 of a new name). Two people never write the same
  atom. That keeps "two editors, one layout" out of public ikigai-ui.
- **Versioned.** Every save is a new `version:{n}`. The bare name answers the latest version and is
  cut when a version is saved, so a page showing it refetches.
- **As of a date.** `as-of:{instant}` answers the layout that was in force then. That is the
  **layout** as of the date, not the content: the panes still read now. Content as of a date
  needs each pane's resource to answer as of a date, which only some do. A pane that can declares
  it (section 2), and the workspace passes `as-of` to those panes only. Each pane says in words
  whether it shows "now" or "as of {instant}", so nobody mistakes one for the other.

ikigai-ui does not store anything. It defines the shape, validates the atom, and composes. The
versions are kept by whatever store the host binds at the partition. That store must keep every
version and answer by number. If the host's store cannot, the host does not offer `as-of`.

## 2. The pane contract

### What a pane declares

| Field | Required | Meaning |
|---|---|---|
| `label` | yes | The pane's accessible name and its title. Never empty. |
| `content` | yes | The pane's address: a screen name. One of: a lensed screen `lens:{lens}[:facets:…]:resource:{iri}`, a resource view `view:resource:{iri}`, or any `urn:iki:ui:screen:name:{name}`. A saved question (#3892) becomes pane content the same way once it is a resource with a lens over its answer, as gonk's `urn:iki:gonk:question:name:{name}` questions could be. It may contain one `{role}` placeholder for each role in `follows`. |
| `follows` | no | The selection roles this pane follows (section 3). Without it, the pane is pinned. |
| `emits` | no | The one role this pane's selection links set. |
| `accepts` | no | The classes a followed IRI must have for this pane to move. Without it, any IRI moves it. |
| `live` | no | `true`: the pane is a live stream. It refetches when one of its resources is cut. |
| `as-of` | no | `true`: the content answers `as-of=` and the workspace may pass one. |
| `scopes` | no | Attenuate the pane's reads to these capability scopes (the region's `scopes`). A pane can only narrow the viewer's capability, never widen it. |

The three kinds of content are **one contract** with two flags. There is no separate pane type:

- **A lens** is a pane with `content` and nothing else.
- **A live stream** is a lens with `live: true`. Its screen declares the paths it shows with
  `data-ui-push` (the existing shell mechanism). ikigai-ui adds nothing to the wire: the page's one
  `EventSource` carries every live pane's paths, and a `cut` refetches only the panes that named
  that path. The push work is what makes a resource cut when it changes, and a pane is only as live
  as its resource's cuts. What exists: the door's stream (`GET /_ikigai/push`, ikigai-cli, ledger
  #618), which takes `path=`, `thread=` and `prefix=` subscriptions (at most 64), needs
  `urn:cap:kernel:listen`, and sends `ready`, `cut` and `resync`. What does not: gonk's door has
  no push stream yet. Its Dashboard polls every 30 seconds (`src/dashboard.rs`: "Polling, not
  push"), and its push design (`docs/design/push-at-the-door.md`, epic #3189) is mostly unbuilt;
  its build items #3209 and #3210 (the stream itself) are open. I found no ledger items numbered
  #621 or #622, nor #3207 or #3208 (**not found**; those numbers match unrelated ikigai-cli pull
  requests), so this document does not rely on them.
- **An agent session** is a live lens over a session's trail. A `session` lens is a query over the
  session's lines, transrepted to ikigai-log's PROV-O graph, with a renderer in the shape of
  `thread` or `timeline`. It is live because the log file is cut on each append (the kernel cuts
  `urn:file:{path}`; `tail=true` reads it live). Nothing in the pane talks to the agent. It reads
  what the agent has done.

  **The premise needs a correction here.** The item says each tool call already writes a log line.
  What I found is narrower. A call that reaches an ikigai **door** writes a `log:Access` line
  (`door=`, `verb=`, `outcome=`, `principal=`, `grant=`, `dur=`), and a denial writes
  `log:CapabilityDenied`. So an agent calling ikigai through `ikigai mcp` leaves one line per
  call, with no arguments or results. A harness's **own** tools (a file edit, a shell command)
  write nothing to ikigai-log. Harness firings are kept by gonk-starter as command digests, never
  the commands. Prompts are not recorded anywhere I found. ikigai-log has no class for a tool call
  and no way to group lines by session. gonk's session summaries
  (`urn:iki:ledger:{ledger}:item:{id}:sessions`) are content-free by design: harness, model, exit
  status, wall clock, turns, tool-call and token counts, and no transcript. So a session pane can
  be built today only as **door calls plus the summary**. "Prompts, tool calls and results as they
  happen" needs a recorded session trail first (section 7).

Because a session pane is a lens over a recorded trail, the same pane shows a finished session,
a session as of a time, or another person's session if their trail is readable to you. It also
means a session pane shows **only what the log records**, and as written above, that is little
today. A prompt or a tool result is also the most sensitive thing in a workspace. A recorded trail
needs its own read capability, separate from the item's.

### How a pane is addressed

- In the workspace: `{workspace-iri}#pane-{key}`. That is the fragment id of the pane's region on
  the html face, so a link or the pane list moves to it.
- On its own: its `content`, with the selection put in. That is an ordinary screen, so every pane
  can be opened alone at `/lens/name/{lens}/resource/{iri}` (a "Open alone" link in each pane's
  header). A pane must never be reachable only inside a workspace.
- As a fragment, for htmx to swap one pane: the content's existing fragment route
  (`lens::fragment_href`).

### What a pane costs

A workspace page costs the sum of its panes, plus one atom read.

| Cost | Bound | Where it is enforced |
|---|---|---|
| Reads | each pane's own: a lens's answer window (1000 rows) and the store's time and answer budgets | the lens, unchanged |
| Panes | 16 per workspace | the atom's validation |
| First load | every pane composed on the server (no script needed), in parallel | the workspace screen |
| Push subscriptions | every live pane's `data-ui-push` paths, on the page's one stream | the door's subscription bound; past it, the stream is refused and `#status` says so once (existing) |
| Refetches | one per cut per pane that named the path; a cut in flight is not dropped (existing) | `shell.js` |
| Cache | each pane's screen is cached as it is now, keyed on the capability. The workspace screen is cached on the atom's version, the selection and the capability. | the kernel |

Two costs are new and should be measured before the first workspace ships: a selection change
recomposes every following pane at once, and a busy agent session can cut its partition many
times a second. For the second, the session lens should refetch at most once a second (a
`data-ui-push` rate, proposed below). The refetch is of the whole pane. Appending only the new rows
would be cheaper, but it is a second rendering path, and this document does not propose it.

## 3. Coordination across panes

### The rule

Selecting something in a pane sets **one selection role** to **one IRI**. Every pane that follows
that role puts the IRI into its `content` placeholder and recomposes. Panes that do not follow it,
and the pane that emitted it, stay as they are.

A role is a short name in the workspace (`item`, `session`, `review`, …). It is not a class. One IRI
can be followed by many panes that each show something different about it: the item pane shows
the item, the session pane shows the session working the item, and the trail pane shows the
item's trail. The relation from the item to its session or its trail is **in the following pane's
lens query**, a join over the graph, never in the workspace. The workspace only moves one IRI. The
lens decides what that IRI means to it. A host that adds a new relation writes a lens, not a
workspace feature.

`accepts` stops a pane moving to something it cannot show. A pane whose `accepts` the selected
IRI's classes do not meet stays as it was and says in words, in its header: "Shows a ledger item.
The selection is a pull request."

### Where selection lives: in the address

Two choices were considered.

**A. In the address (recommended).** The selection is part of the workspace's request:
`/workspace/{iri}?select.item={item-iri}`. The workspace screen name carries it, as the lens
screen name carries `selected=` today. A selection link in a pane is an ordinary link to the
workspace with that one parameter changed.

- With no script, a click is a GET of the whole workspace with the new selection. Every pane is
  composed on the server. It works, and it is slower.
- With the shell script, the same link is boosted: htmx fetches only the following panes'
  fragments and swaps each into its region (one request per following pane, or one request
  answered with each pane's fragment out of band), and pushes the new URL. Back and forward move
  through selections.
- The selection is **saved with the workspace** when the person saves it (the `selection` in the
  atom). Opening the workspace with no `select.*` uses the saved one.
- It is shareable: the URL **is** the selection. It is readable as of a date along with the
  layout. It is cached like any other request. And it keeps the rule the operations view already
  holds: selection is request-local, never shared mutable state.

**B. In shared state.** A selection resource per workspace (or per person and window), written
by a click and cut so that every pane refetches through the push stream.

- It would let two windows, or two people, follow the same selection.
- It needs a write for every click (so a read-only viewer could not select), a push stream to
  work at all (so nothing with script off), and its own policy (who may move my selection?).

Recommendation: **A** for public ikigai-ui. B, "follow another person's selection", is a host
feature on top: a host can write a selection resource and have a page follow it by setting
`select.*` from it. B never replaces A.

### Selection and the existing selection list

Every lens already lists its result items as links in its **Selection** list through the home
mapping. In a workspace, a pane with `emits` rewrites those links: a result's link becomes a
workspace link that sets the emitted role, instead of leaving the page. The home link stays,
labeled "Open", so nothing loses its way out. A pane without `emits` keeps the home links.

### Saying what happened

When a selection moves panes, focus stays on the link that was activated (the pane the person is
in). `#status` says once, politely, which panes moved: "Item 3941 selected. Session, Item and Trail
now show it." That is one sentence per selection, never one per pane, and never for a live
refetch (D9, D13).

## 4. Actions from a pane

**A workspace adds no authority.** Each pane is a `region`, read under the viewer's capability,
attenuated by the pane's `scopes` if it has any. The workspace itself holds no grant.

- **Reads pass through.** Each pane's reads are the viewer's, exactly as if the pane were opened
  alone. A pane that may not read its content shows the withheld rung, and its value is in no
  byte of the page and no cache entry readable under that grant (the existing reference rule).
- **An action is an effect under the viewer's capability.** Dispatch, approve, merge and write
  are Sinks. A pane offers one only as a form generated from that Sink's contract, and only when
  the viewer's `urn:kernel:actions` offers it. Submitting goes through the existing operation path
  (`urn:iki:ui:lens:operation:{op}`), which checks again and validates before it invokes. Every
  such form carries `data-ui-write`, so it is grayed out when the viewer's write lapses.
- **A selection never writes.** A selection is a GET. It may pre-fill an action's form (as
  `selected=` and `target=` do today), and the form still needs its own submit.
- **No pane can do what its viewer's capability does not admit.** There is no "workspace
  capability" and no pane-level grant. A shared workspace shows each viewer the actions **their**
  capability admits.

**An agent acting through a pane** is just another caller. An agent's tool list is the manifold
under its capability, so it calls the same Sinks a pane's form calls, through the same validation,
policy and approval. Its session pane shows what it did. It does not drive anyone's panes, and a
pane gives it nothing. What exists for policy today is a harness hook: gonk-starter's pre-tool-use
hook refuses an edit without a live claim and
applies the refuse and warn rules in `.gonk/harness/harness.ttl`, and every ikigai call is checked
again against the caller's capability (MCP `tools/list` is the manifold; each `tools/call`
rechecks). **I found no approval queue for tool calls.** The only path where a person approves is
gonk's review Queue (`/queue`, publish or decline a finding). So approval for an agent's action
would be a host Sink that records a pending call and a person approves under their own
capability. The workspace only needs it to be a Sink like any other: the approval pane is a lens
over pending calls, its approve button a generated form, and the session pane shows the call
waiting and then its result. Building that Sink is not public ikigai-ui's work.

Two things are forbidden by this design and should be tested when it is built: a pane's form that
names a Sink the manifold did not offer (refused at submission), and a selection whose IRI is
spliced into an action argument without the form showing it.

## 5. Targets

### The web face (HTMX, and usable with JS off)

| | With the shell script | With no script |
|---|---|---|
| Layout | CSS grid from the split hint's shares; tabs as the ARIA tabs pattern | The same grid (it is CSS, not script); tabs as a list of links to each pane's region, all panes present |
| Selection | boosted links swap the following panes and push the URL | a GET of the whole workspace |
| Live panes | one `EventSource`, quiet refetch | not live. Each live pane says "Shown at {time}. Reload to update." |
| Resize | a splitter that is a keyboard control (below) and a "Layout" form | the "Layout" form |
| Narrow viewport (320 CSS px) | panes stack in reading order; tabs stay tabs | the same |

Every target must support, at the least: the panes in reading order with their labels, the pane
list, selection by a link, and each pane's own fallback. The grid, the tabs and live refetch are
improvements on top. A host's descriptor that does not claim the layout hint gets the linear form.

### The accessible equivalent of a tiled layout (WCAG 2.2 AA)

- **Landmarks.** The workspace is `<main>`. Each pane is a `<section>` named by its label
  (`aria-labelledby` its heading), so it is a region landmark a screen reader lists. A pane's
  header holds its heading, "Open alone" and, if it follows a role, what it follows.
- **A pane list.** First in `<main>`, a `nav` named "Panes" lists every pane with a link to its
  region, and says what each shows ("Session: item 3941, live"). It is the skip link for each
  pane, and on a narrow screen it is the way around. Panes inside inactive tabs are in the list
  too; following the link opens their tab.
- **Keyboard movement between panes.** The pane list always works. The shell script adds a
  shortcut to move to the next and the previous pane. F6 is the convention in desktop
  applications, but browsers use it for the address bar, so the web face should use a modifier
  combination that the browser does not take and that is shown in the pane list ("Next pane:
  Alt+Shift+Arrow Right"). Because it uses a modifier, SC 2.1.4 (character key shortcuts) does
  not apply. Moving to a pane focuses its heading.
- **Resizing without dragging.** SC 2.5.7 (dragging movements) needs a single-pointer way to do
  anything a drag does. Each splitter is a focusable `role="separator"` with `aria-valuenow`
  (the share), moved by the arrow keys, and the "Layout" form sets each share as a number. SC
  2.5.8 (target size) applies to the splitter: at least 24 by 24 CSS pixels.
- **Reflow.** SC 1.4.10: at 320 CSS pixels the panes stack, with no horizontal scrolling of the
  page.
- **Focus.** SC 2.4.11 (focus not obscured): a pane's sticky header must not cover the focused
  element inside it. Focus never moves because another pane refetched (D9) or because a
  selection moved panes (section 3).
- **One live region.** `#status` stays the page's only live region (D13). An agent session pane
  does **not** announce each new tool call. It is a log a person reads when they choose. A host
  may offer "Say new activity in this pane", off by default, which says at most one sentence every
  ten seconds in `#status` (see the decisions).

### GPUI

The GPUI face renders the same workspace screen. It needs:

1. **Split and tab layout, natively.** The descriptor would advertise
   `{"name":"layout","version":"1.0"}` and claim `urn:iki:ui:hint:layout:`. Until it does, it
   gets the linear form, which already works there.
2. **Region focus movement.** Ctrl+F6 or F6 moves between panes (no browser takes the key there),
   and the pane list is the same list.
3. **Live panes.** Its snapshot worker must subscribe to the same push stream. Today the GPUI face
   navigates through a snapshot worker (`gpui/src/worker.rs`), and nothing under `gpui/src`
   subscribes to the push stream (checked by searching the source, not by running it).
4. **An accessibility tree.** On gpui 0.2.2 the native face has none (`gpui-upgrade.md`). A tiled
   native workspace is not accessible until the move to `gpui-pre` and its AccessKit roles land.
   The pane would be a group with a label in AccessKit. **The GPUI workspace should wait for that
   move.**

The GPUI target's `multi-window` capability suggests one window per pane as another option. It
is not recommended for the first version: the panes would have no shared selection without new
inter-window state, and a screen reader user would have to move between windows.

## 6. The public line

| In public ikigai-ui | Added by a host or product |
|---|---|
| `urn:iki:ui:workspace:…` names, the atom's shape and its validation | The store that keeps versions, and where it lives |
| The workspace screen and page: the layout composed into model nodes, the layout hints, the linear fallback | People: sign-in, the `{person}` IRI, who may share with whom |
| The pane contract (`content`, `follows`, `emits`, `accepts`, `live`, `as-of`, `scopes`) | The lenses its panes show (a ledger dashboard, an item lens, a review lens) |
| Linked selection in the address, the rewritten selection links, the status sentence | Following another person's selection (shared state) |
| The pane list, keyboard movement, splitters, the "Layout" form, the narrow form | Default workspaces for its pages, templates, onboarding |
| A generic `session` renderer over PROV-O-shaped trail data (from ikigai-log's vocabulary, which is public) | Which sessions exist, where its trail is, and its push cuts |
| The rule that panes add no authority, and the tests for it | Its approval policy, and its approval Sink |

Public ikigai-ui stays free of any product's vocabulary. Its examples use the demo's fixtures,
as the lenses do, and a fixture trail in ikigai-log's vocabulary for the session renderer.

## 7. The first concrete target

A gonk page that tiles three panes on one item:

1. **The Dashboard**, open and in-flight items, emitting `item`;
2. **a live agent session** working the item, following `item`;
3. **the ledger item** with its children and verdicts, following `item` and emitting `item`
   (choosing a child moves the session pane to the child's session).

What each needs that does not exist yet, as proposed items in plain words (not filed):

What gonk has today, from its README and source: the Dashboard at `/dashboard`
(`urn:iki:gonk:page:dashboard`, with `:ledgers` and `:agents` tabs) built from polled htmx
fragments (`urn:iki:gonk:fragment:dashboard-chart:{ledger}`, `…dashboard-agents:{ledger}`), the
agents tab over `urn:iki:gonk:agent:holder:{holder}` with saved questions in `src/questions/`, the
item page at `/l/{ledger}/item/{id}` (`urn:iki:gonk:page:item:{ledger}:{id}`) and its card
fragment, and actions through `POST /act`. The item is `urn:iki:ledger:{ledger}:item:{id}`, its
children are the items whose `parent` link names it, and its sessions are
`urn:iki:ledger:{ledger}:item:{id}:sessions`. These are XSLT and htmx pages of gonk's own, not
ikigai-ui lenses.

**In ikigai-ui (public):**

1. The workspace atom's shape and its validation, with fixtures, and the names in section 1.
2. The workspace screen: the layout composed into groups with the split and tabs hints and panes
   as regions, and the linear form for a face that does not claim the hints.
3. The html face for the layout hints: the CSS grid, the tabs, the pane list, the keyboard
   splitter and the "Layout" form, with snapshots and axe in light, dark and high contrast.
4. Selection in the address: `select.{role}=` folded into the workspace screen name, a pane's
   selection links rewritten when it `emits`, and the one status sentence.
5. Boosted selection in `shell.js`: swap only the following panes and push the URL, with
   shell-smoke checks that focus stays where it was and that nothing else is announced.
6. Pane movement on the keyboard in `shell.js`, and a person's keyboard and screen-reader pass of
   a fixture workspace, recorded in `ACCESSIBILITY.md`.
7. A per-element refetch rate on `data-ui-push`, so a busy pane refetches at most once a second.
8. A `session` renderer over ikigai-log's vocabulary, with a fixture trail, in the shape of the
   thread and timeline renderers.
9. Tests that a pane's form naming a Sink the manifold did not offer is refused, and that a
   shared workspace shows each viewer only their own reads.

**In gonk:**

10. An item lens: the item, its state, its children (items whose `parent` is it), its comments,
    and its verdict, as a query over the ledger graph. This needs a decision first: "done-when"
    has no field today, and a "verdict" is in three places (the judge's verdict on a finding, the
    dispatcher's close with its evidence, and a `verdict:` line in a close note). The lens should
    show one, so the ledger should say which one counts.
11. A Dashboard lens: the open and in-flight items of a ledger, as a board or a table that emits
    `item`. Today's Dashboard is XSLT over polled fragments, so this is a new lens beside it, not
    a port of it.
12. gonk pages composed over ikigai-ui's `Pages` with lenses on, so a gonk page can serve the
    workspace, and a route for `/workspace/{+iri}` in gonk's door.
13. A store for workspace atoms that keeps every version, and the
    `urn:cap:ui:workspace:{read,write}:person:{person}` grants in gonk's roles.
14. The push stream at gonk's door (the open push build items, #3209 and #3210), and a cut on the
    item's resources when it changes. Without it the item and Dashboard panes poll, and the
    session pane is not live.

**In the session trail (ikigai-log, gonk-starter and the harness):**

15. A recorded session trail: one line per tool call (its name, its arguments' digest or text,
    its outcome and its duration) and per prompt, with the session's IRI
    (`urn:agent:session:id:{id}`) on every line, written where the kernel cuts it on append.
    This is the biggest missing piece. It needs its own decision about what may be recorded
    (prompts and results can hold secrets), who may read it (its own capability), and how long it
    is kept. Today only door calls and content-free summaries are recorded.
16. A link from an item to its live session: which session holds the item's claim now. Session
    summaries are written after a session ends. A pane following `item` needs the running one.
17. A `session` lens in gonk over that trail, joined to the item through the claim.

## 8. Decisions for Brian

Each has a recommendation. The ones marked **blocks** must be decided before any code.

1. **A workspace is an ordinary screen** (blocks). The layout composes into `group` nodes with
   `urn:iki:ui:hint:layout:split` and `:tabs` hints, and each pane into a `region`, so the fallback
   ladder, regions and withheld rungs apply unchanged. The alternative is new core kinds (`split`,
   `tabs`, `pane`), which every descriptor would then have to decide. **Recommend: hints and
   regions, no new kinds.**
2. **The names** (blocks): `urn:iki:ui:workspace:person:{person}:name:{name}`, with `:version:{n}`
   and `:as-of:{instant}`, the shape at `urn:iki:ui:shape:workspace`, the screen at
   `urn:iki:ui:screen:workspace:{iri}` and the page at `/workspace/{+iri}`. **Recommend: as written.**
3. **The atom stores layout, pane addresses and the saved selection, nothing else** (blocks). No
   content, no answers, no pixels, no theme. **Recommend: yes.**
4. **ikigai-ui stores nothing.** It validates and composes. Versions are kept by the store the host
   binds, and a host whose store cannot keep versions does not offer `as-of`. **Recommend: yes.**
5. **Sharing is a read grant, and changing a shared workspace is a copy.** Two people never write
   the same atom. **Recommend: yes**, and leave co-editing out of public ikigai-ui.
6. **"As of a date" means the layout**, and content as of a date only for panes that declare
   `as-of`, each pane saying "now" or "as of {instant}" in words. **Recommend: yes.**
7. **One pane contract with two flags** (`live`, and the content's own address), rather than three
   pane types. An agent session is a live lens over the session's trail in the log. **Recommend:
   yes.**
8. **Selection lives in the address** (`select.{role}={iri}`), saved with the workspace when it is
   saved, boosted by htmx with script and a full GET without it. Shared selection state (following
   another person) is a host feature on top. **Recommend: in the address.**
9. **A selection moves one IRI per role**, and the relation from it to what a pane shows (its
   session, its trail, its review) is in that pane's lens query, never in the workspace.
   **Recommend: yes.**
10. **A workspace adds no authority.** No workspace capability, no pane grant; a pane's `scopes`
    can only narrow; actions only as generated forms from what `urn:kernel:actions` offers,
    through `urn:iki:ui:lens:operation:`, marked `data-ui-write`; a selection never writes. An agent
    is just another caller. **Recommend: yes**, with the two refusal tests in section 4 written
    first.
11. **The web shortcut for moving between panes.** F6 is the desktop convention, but browsers take
    it. **Recommend: Alt+Shift+Arrow Left/Right on the web, shown in the pane list; F6 and Ctrl+F6
    on GPUI**, with the pane list as the way that always works. This needs a person's keyboard and
    screen reader pass before it is called done, not only scripted checks.
12. **A session pane does not announce new activity.** An opt-in "Say new activity in this pane",
    at most one sentence every ten seconds in `#status`, is the only exception. **Recommend: off by
    default; build the opt-in only if someone asks for it.**
13. **A live pane refetches at most once a second.** A busy session would otherwise refetch on
    every line. **Recommend: a per-element rate on `data-ui-push`, default one second for session
    panes**, and whole-pane refetch, not append, for the first version.
14. **The GPUI workspace waits for the gpui move** (`gpui-upgrade.md`, option A) and its AccessKit
    roles. Until then GPUI shows the linear form. One window per pane is not the first version.
    **Recommend: yes.**
15. **The first target is gonk's three-pane item page** (section 7), built in this order:
    the workspace shape and screen with fixtures here (proposals 1 to 9); then the item and
    Dashboard lenses in gonk (10 to 13); then push at gonk's door (14); then the session trail and
    its lens (15 to 17). **Recommend: yes**, and ship the page with a session pane that shows door
    calls and the summary until the trail exists, saying in words what it does not show.
16. **What a session trail records** (blocks proposal 15, not this design). Prompts and tool
    results can hold secrets. **Recommend: record tool names, outcomes, durations and argument
    digests by default; record text only where a host opts in, under its own read capability
    (for example `urn:cap:log:read:session:{id}`), with a retention limit.**
17. **Which verdict an item lens shows** (blocks proposal 10). **Recommend: the dispatcher's close
    and its evidence**, since that is the one that closes the item; the judge's verdicts on findings
    belong to the review pane.
