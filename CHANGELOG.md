# Changelog

## Unreleased

- Fix (ledger #4483): an html render no longer reads and composes its screen once per node. Each node's views read `urn:iki:ui:html:screen:{screen}:model:{node}`, a cacheable resource composed from its parent's, so a cacheable screen is read once per render. A workspace page reads the workspace's bare name once to learn its version and composes over the saved version (`…:version:{n}`, face name `workspace:…version:{n}:resource:{iri}`), so the kernel's cache answers the re-reads; the bare name stays uncacheable and an edit shows at once. A store that serves no saved versions is read at the bare name, as before. The HTML is byte for byte the same. The demo's saved versions and addable list answer cacheably.
- Workspace 4 (ledger #3953): the `collapse`, `expand`, `close` and `add` edits (`workspace::apply_with`, with the host's `addable` list at `{workspace}:addable`), maximize as `maximize={key}` in the address (never written), and their forms and links in each pane's "Pane" menu and the workspace's "Add pane" form. `apply` keeps its signature and refuses `add`.

## 0.6.1

- Optional inline arguments now refuse invalid values that previously selected a default. The refusal names the argument and explains the problem. No persisted state changes. The minimum ikigai-core version is now 0.4.3.
