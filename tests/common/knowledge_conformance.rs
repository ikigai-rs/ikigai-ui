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
    suite.namespace("urn:iki:ui:fixture:knowledge:")
}
