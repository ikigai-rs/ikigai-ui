# Changelog

## Unreleased

- Workspace 4 (ledger #3953): the `collapse`, `expand`, `close` and `add` edits (`workspace::apply_with`, with the host's `addable` list at `{workspace}:addable`), maximize as `maximize={key}` in the address (never written), and their forms and links in each pane's "Pane" menu and the workspace's "Add pane" form. `apply` keeps its signature and refuses `add`.

## 0.6.1

- Optional inline arguments now refuse invalid values that previously selected a default. The refusal names the argument and explains the problem. No persisted state changes. The minimum ikigai-core version is now 0.4.3.
