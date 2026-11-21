use ikigai_conformance::Suite;

pub fn register(mut suite: Suite) -> Suite {
    for id in [
        // Knowledge lenses and immutable fixtures (ledger #3886).
        "ui-lens-answer",
        "ui-lens-answer-query",
        "ui-lens-passage",
        "ui-lens-passage-query",
        "ui-lens-entity-card",
        "ui-lens-entity-card-query",
        "ui-lens-disagreement-list",
        "ui-lens-disagreement-list-query",
        "ui-lens-entity-neighborhood",
        "ui-lens-entity-neighborhood-query",
        "ui-knowledge-graph",
        "ui-knowledge-lenses-answer",
        "ui-knowledge-lenses-abstain",
        "ui-knowledge-lenses-answer-claim-1",
        "ui-knowledge-lenses-answer-claim-2",
        "ui-knowledge-lenses-passage-a",
        "ui-knowledge-lenses-passage-b",
        "ui-knowledge-lenses-passage-c",
        "ui-knowledge-lenses-entity",
        "ui-knowledge-lenses-disagreements",
        "ui-knowledge-timetable-v2",
        "ui-knowledge-bulletin-v1",
        "ui-knowledge-guide-v3",
    ] {
        suite = suite.pure(id);
    }
    // The published ikigai-knowledge terms the answer fixture carries (ledger #3915), held to
    // their definition; the fixture-only namespace keeps the terms it has none for yet.
    suite
        .vocabulary(include_str!("../fixtures/knowledge-vocabulary.ttl"))
        .namespace("urn:iki:ui:fixture:knowledge:")
}
