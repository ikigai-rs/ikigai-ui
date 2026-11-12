//! The meter's numeric domain and its non-color equivalent.
use ikigai_ui::{meter::Reading, model::Node};

fn node(mut properties: serde_json::Value) -> Node {
    properties
        .as_object_mut()
        .unwrap()
        .entry("label")
        .or_insert(serde_json::json!("Capacity"));
    serde_json::from_value(serde_json::json!({
        "kind": ikigai_ui::meter::IRI, "properties": properties
    }))
    .unwrap()
}

#[test]
fn a_bar_clamps_but_the_visible_value_preserves_underflow_and_overflow() {
    for (value, fraction, state) in [(-20, 0., "below minimum"), (120, 1., "above maximum")] {
        let node = node(serde_json::json!({"label":"Capacity", "value":value, "max":100}));
        let reading = Reading::parse(&node).unwrap();
        assert_eq!(reading.fraction(), fraction);
        assert_eq!(reading.token(), "danger");
        let description = reading.description(&node);
        assert!(description.contains(&format!("Capacity: {value} of 100")));
        assert!(description.contains(state));
    }
}

#[test]
fn thresholds_and_optimum_choose_the_same_regions_as_a_native_meter() {
    for (optimum, value, token) in [
        (50, 50, "accent"),
        (50, 10, "warn"),
        (50, 90, "warn"),
        (0, 10, "accent"),
        (0, 50, "warn"),
        (0, 90, "danger"),
        (100, 90, "accent"),
        (100, 50, "warn"),
        (100, 10, "danger"),
        (50, 25, "accent"),
        (50, 75, "accent"),
    ] {
        let node = node(
            serde_json::json!({"label":"Capacity", "value":value, "max":100, "low":25, "high":75, "optimum":optimum}),
        );
        let reading = Reading::parse(&node).unwrap();
        assert_eq!(reading.token(), token, "{reading:?}");
        let words = match token {
            "accent" => "in optimum region",
            "warn" => "outside optimum region",
            _ => "far from optimum",
        };
        assert!(reading.description(&node).contains(words));
    }
}

#[test]
fn finite_decimal_and_nonzero_minimum_produce_a_fraction_of_the_actual_span() {
    let reading = Reading::parse(&node(
        serde_json::json!({"value":"-2.5", "min":-5, "max":5}),
    ))
    .unwrap();
    assert_eq!(reading.fraction(), 0.25);
    assert_eq!(reading.optimum, 0.);
    assert_eq!(reading.low, -5.);
    assert_eq!(reading.high, 5.);
}

#[test]
fn malformed_or_unordered_numeric_contracts_are_refused_before_painting() {
    for properties in [
        serde_json::json!({}),
        serde_json::json!({"value":true}),
        serde_json::json!({"label":"", "value":0}),
        serde_json::json!({"label":true, "value":0}),
        serde_json::json!({"value":"NaN"}),
        serde_json::json!({"value":"inf"}),
        serde_json::json!({"value":"<script>"}),
        serde_json::json!({"value":0, "min":1}),
        serde_json::json!({"value":0, "max":0}),
        serde_json::json!({"value":0, "min":"-1e308", "max":"1e308"}),
        serde_json::json!({"value":0, "low":2}),
        serde_json::json!({"value":0, "low":"0.8", "high":"0.2"}),
        serde_json::json!({"value":0, "high":-1}),
        serde_json::json!({"value":0, "optimum":2}),
    ] {
        assert!(
            Reading::parse(&node(properties.clone())).is_err(),
            "{properties}"
        );
    }
}

#[test]
fn every_meter_pair_is_measured_for_every_palette() {
    use ikigai_ui::palette::{Kind, PAIRS};
    for token in ["accent", "warn", "danger"] {
        assert!(PAIRS
            .iter()
            .any(|(fg, bg, _)| *fg == token && *bg == "surface"));
    }
    for ground in ["bg", "surface"] {
        assert!(PAIRS.contains(&("line", ground, Kind::NonText)));
    }
}
