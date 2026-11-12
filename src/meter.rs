//! Numeric interpretation shared by the native meter faces. The expansion stays plain data.
use crate::model::{Node, Value};
use ikigai_core::Error;

/// The meter element's name.
pub const IRI: &str = "urn:iki:ui:element:name:meter";

/// A finite reading and its ordered bounds, following HTML's default thresholds.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Reading {
    /// The original reading; the bar alone clamps it to the range.
    pub value: f64,
    /// The lower bound (default 0).
    pub min: f64,
    /// The upper bound (default 1).
    pub max: f64,
    /// The low threshold (default min).
    pub low: f64,
    /// The high threshold (default max).
    pub high: f64,
    /// The ideal reading (default midpoint).
    pub optimum: f64,
}

impl Reading {
    /// Interpret numeric properties, refusing non-finite numbers and inconsistent bounds.
    pub fn parse(node: &Node) -> Result<Self, Error> {
        let refuse = |detail: &str| Error::InvalidArgument {
            name: "meter".into(),
            detail: detail.into(),
        };
        if !matches!(node.properties.get("label"), Some(Value::Text(label)) if !label.trim().is_empty())
        {
            return Err(refuse("label must be non-empty text"));
        }
        let number = |name: &str, default: Option<f64>| -> Result<f64, Error> {
            let value = match node.properties.get(name) {
                Some(Value::Number(n)) => Some(*n as f64),
                Some(Value::Text(s)) => s.parse::<f64>().ok(),
                None => default,
                _ => None,
            };
            value
                .filter(|v| v.is_finite())
                .ok_or_else(|| refuse(&format!("{name} must be a finite number")))
        };
        let min = number("min", Some(0.))?;
        let max = number("max", Some(1.))?;
        if min >= max || !(max - min).is_finite() {
            return Err(refuse("min must be less than max, with a finite span"));
        }
        let reading = Self {
            value: number("value", None)?,
            min,
            max,
            low: number("low", Some(min))?,
            high: number("high", Some(max))?,
            optimum: number("optimum", Some(min + (max - min) / 2.))?,
        };
        if reading.low < min
            || reading.high > max
            || reading.low > reading.high
            || reading.optimum < min
            || reading.optimum > max
        {
            return Err(refuse("thresholds must satisfy min <= low <= high <= max, and optimum must be within the range"));
        }
        Ok(reading)
    }

    /// A normalized numeric HTML attribute, including defaults for optional thresholds.
    pub fn property(&self, name: &str) -> f64 {
        match name {
            "min" => self.min,
            "max" => self.max,
            "low" => self.low,
            "high" => self.high,
            "optimum" => self.optimum,
            _ => self.value,
        }
    }

    /// The fraction filled, clamped only for painting; text retains the real reading.
    pub fn fraction(&self) -> f32 {
        ((self.value.clamp(self.min, self.max) - self.min) / (self.max - self.min)) as f32
    }

    /// The fill token. Optimum determines which of the three HTML meter regions is best.
    /// Text states the same assessment, so color is never its only carrier.
    pub fn token(&self) -> &'static str {
        if self.value < self.min || self.value > self.max {
            return "danger";
        }
        if self.optimum < self.low {
            if self.value > self.high {
                "danger"
            } else if self.value > self.low {
                "warn"
            } else {
                "accent"
            }
        } else if self.optimum > self.high {
            if self.value < self.low {
                "danger"
            } else if self.value < self.high {
                "warn"
            } else {
                "accent"
            }
        } else if self.value < self.low || self.value > self.high {
            "warn"
        } else {
            "accent"
        }
    }

    /// Visible equivalent of the bar, bounds, thresholds and its color assessment.
    pub fn description(&self, node: &Node) -> String {
        let label = match node.properties.get("label") {
            Some(Value::Text(s)) => s.as_str(),
            _ => "Meter",
        };
        let state = if self.value < self.min {
            "below minimum"
        } else if self.value > self.max {
            "above maximum"
        } else {
            match self.token() {
                "danger" => "far from optimum",
                "warn" => "outside optimum region",
                _ => "in optimum region",
            }
        };
        // Numeric geometry uses f64, but the readout keeps the author's numeral: large
        // integers must not round, nor should decimal text lose its supplied precision.
        let numeral = |name: &str, default: f64| match node.properties.get(name) {
            Some(Value::Text(text)) => text.clone(),
            Some(Value::Number(number)) => number.to_string(),
            _ => default.to_string(),
        };
        format!(
            "{label}: {} of {} (minimum {}; low {}; high {}; optimum {}; {state})",
            numeral("value", self.value),
            numeral("max", self.max),
            self.min,
            self.low,
            self.high,
            self.optimum
        )
    }
}
