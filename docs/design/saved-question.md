# Design: a saved question as a resource

Ledger #3892. Status: **proposed for Brian**. This is a design, not an implementation or an
accepted change to the workspace contract.

## Evidence and scope

The four things have a common intent, but are not already interchangeable implementations.
The [workspace design](workspace.md) is accepted: a pane holds a screen address, and a saved
question can be pane content via a lens over its answer; the workspace stores neither answers
nor capabilities. The UI's [`Lens`](../../src/lens.rs) today combines a read-only SPARQL query,
`graphs`, focus and renderer; its prepared query and results are resources. The
[knowledge README](https://github.com/ikigai-rs/ikigai-knowledge#readme) describes a **planned**
content-addressed question (text and corpus) and answer (#3859), not a shipped general question
endpoint. Its decision trail is shipped, and its model-drafted answer is explicitly uncached.
The [NL README](https://github.com/ikigai-rs/ikigai-nl#readme) describes a shipped SPARQL
drafter which saves a versioned **script draft** with provenance, not a saved question. An
interest submitted to a stream is the fourth intended entry point (#2627); its present contract
was not found in this repository or in an identifiable public stream repository. The mapping
below is a proposal, not a claim that it exists. #2480, #2627, #3517 and #3859 remain related
work; this document does not change their status.

## Decisions

### 1. Identity and definition

A saved question is a durable, named definition, separate from its evaluated answer and from
presentation. Recommend `urn:iki:question:person:{person}:name:{name}` as the stable head,
`…:version:{n}` as an immutable definition, and `…:as-of:{instant}` as the version saved at or
before that instant. The host owns the partition and store; these names are proposed, not live
endpoints. A write requires authority for that partition, uses an expected head version to refuse
lost updates, and creates a new version. Sharing grants access to the definition, never the
owner's data or capability. Copy to a new name to fork it. Retain old versions even if the head
is withdrawn; reads remain subject to policy.

Each version holds a label, exactly one text and language (`sparql` or `english`), and an explicit
set of graph resource IRIs (or a version-pinned corpus/graph-set declaration resolving to that
set), plus optional provenance. Empty graph set means no graphs, not all graphs. Graph-set
resolution is under the reader's capability; no hidden graph names are disclosed. An inaccessible
explicit graph is refused without distinguishing missing from forbidden. A corpus resolver may
instead return only the caller-visible members, as knowledge's corpus does; the answer must say
which policy applies without revealing hidden members. A graph set is a ceiling, not a grant.
Do not persist credentials, capability tokens, rendered output, or the owner's answer in the
definition. Definition read permission and graph read permission are independent.

SPARQL is parsed and restricted to read-only SELECT, ASK or CONSTRUCT; dataset clauses, GRAPH
and SERVICE must not escape the declared graph set or the host's explicit service allowlist.
Bind values as terms, never splice text. Refuse unsupported forms or an unenforceable dataset
boundary. Preserve existing time, size and answer budgets, and report truncation. English is an
intent, not executable authority: the host may answer through knowledge's cited-answer pipeline
or draft a read-only SPARQL query through NL, under the same caller and graph ceiling. A failed
check, denied model, or unavailable pipeline is a visible refusal, not an empty answer. A draft
requires review/publication under its existing policy; saving English does not authorize a model
to write or execute actions. Record which pipeline and derived query were used in answer
provenance. English may yield different answers on repeated evaluations.

### 2. Answer, rendering and citation

`urn:iki:question:answer:question:{version-iri}` is a proposed composed answer, evaluated on
read under the **current caller's** capability, including every graph, model and source read.
The head may alias an answer for the latest definition. Sharing a definition or a pane never
runs it as its author. Cache only where all dependencies and the pipeline permit it; partition by
capability, definition version, graph-set resolution, bindings and relevant execution context.
An uncached model answer stays uncached. Denied, partial, truncated, empty and abstained answers
must remain distinguishable. Never use another caller's cached answer or notification as evidence
of what this caller may see.

Expose a typed answer (SPARQL bindings, boolean or RDF graph; for grounded English, claims and
citations) with metadata identifying definition version, effective visible inputs, evaluation time,
limits and pipeline. A lens is selected separately and renders a compatible answer shape; any
lens can be selected, but an incompatible lens must report that it cannot render the shape rather
than reinterpret it. Existing lenses may need adapters: they currently query resources themselves,
so simply pointing every lens at a results table is not sufficient. The workspace holds the
ordinary lensed screen address and `live` flag, as already decided; it holds no answer or grant.

Cite the immutable definition version to identify **what was asked**. A definition version alone
cannot reproduce **what was answered**: graphs, permissions, model and time can change. When a
host supports citation of an answer, it must persist an immutable, access-controlled answer
snapshot with its own IRI and content digest, definition version, visible input versions or
snapshot identifiers, execution context and provenance. Citations to claims retain knowledge's
passage and quote/position selectors; a citation is resolvable only under the reader's current
capability. A digest verifies retained bytes, but does not make missing historical inputs
reconstructible. If the host cannot retain and authorize a snapshot, offer only the definition
citation and label the answer non-reproducible; `as-of` on a definition does not time-travel its
inputs. Retention and revocation policy belong to the host.

### 3. Four entry points, one definition

| Entry | Conversion and compatibility |
|---|---|
| Knowledge question | Map its planned text and corpus to an English definition and pinned corpus declaration. Preserve its content hash as provenance or an alias to an immutable version only when the exact canonical text and graph-set identity agree. Knowledge's link/context/citation pipeline remains an answer implementation, including citation checking. |
| UI lens query | Extract its query text and `graphs` into a SPARQL definition; keep renderer, focus, facets and bounds in the lens. Capture actual focus/facet bindings separately at evaluation or in a deliberately parameterized definition, never silently bake request state into a shared question. Existing lens names remain adapters until migrated. |
| NL draft (#2480) | Save the user's English and graph set as a question; attach the draft script version, grounding and provenance. A reviewed SPARQL draft may become a new SPARQL question version or a linked question, explicitly chosen by its author. Do not equate an unreviewed script with the English intent or publish it automatically. |
| Stream interest (#2627) | Resolve or create a question version from the submitted text and graph set, then subscribe the caller to its answer. Preserve an existing interest identifier as an alias or subscription provenance. A subscription is per recipient and authority, not a shared elevated execution identity. Exact migration depends on the actual interest contract. |

A version has one language and text: a translation is a new version or linked definition, with
provenance, not an invisible replacement. Existing identifiers continue to resolve through
explicit adapters; there is no promise of automatic migration or equivalence based only on text.

### 4. Subscription: invalidation, then authorized read

A subscriber receives a **change hint**, not answer bytes: question head or pinned version,
opaque cursor and `cut` (or `resync` when continuity is lost). A head subscription follows new
definition versions; a pinned subscription follows only changes to inputs of that version.
The subscriber refetches the answer under its then-current capability, compares its own last
successful answer if it needs a diff, and renders it with its chosen lens. A cut means an input
or definition may have changed, not that the answer definitely differs. Coalesce cuts; ordering
is per subscription, delivery is at least once and may be duplicated. On reconnect, a gap,
overflow, uncertain dependency, or expired cursor sends `resync`, requiring a fresh read.
Do not promise exactly-once delivery, a complete diff, or historical replay. No event contains
rows, graph names, counts, or a digest derived from another capability's answer. Subscription
and each refetch are authorized independently; revocation stops delivery or yields a generic
refusal, and cached snapshots still require current read authorization.

Dependencies include the definition, graph-set declaration and every input actually read,
including prompts and model configuration where tracked. A dynamic graph set must also depend on
its membership source. If a host cannot track all dependencies, it must mark the subscription
unsupported or provide an explicitly bounded polling/resync policy; it must not claim complete
change detection. Nondeterministic English answers may differ without an input cut, so a cut
stream is not a guarantee of detecting every possible answer change. This fits the workspace's
existing push `cut`/`resync` and refetch model; it does not assert that every door implements it.

## Decisions for Brian

1. Accept a named, versioned definition with a graph ceiling, independent of answer and lens?
2. Accept English and read-only SPARQL as alternative texts, with explicit provenance for any
   translation and no automatic publication of NL drafts?
3. Accept caller-scoped evaluation and per-capability caching, with explicit refusal and partial
   result semantics?
4. Accept definition citations by version and answer citations only for retained, authorized
   immutable snapshots, with no promise of historical reconstruction otherwise?
5. Accept cut/resync hints followed by a caller-authorized refetch, including the stated limits
   for dynamic dependencies and nondeterministic answers?
6. Accept the four adapters above, subject to checking the actual stream interest contract before
   implementation and choosing a host and vocabulary for the proposed names?
