//! TOML-driven widget layout.
//!
//! The file only describes *presentation* and *limits*; it never changes the
//! wire protocol. Monitors map onto telemetry fields and controls map onto
//! fields of [`crate::protocol::DeviceState`].

use std::collections::HashSet;

use serde::{Deserialize, Serialize};

/// Seeded into the app config directory on first launch.
pub const DEFAULT_TOML: &str = include_str!("../widgets.toml");

/// Telemetry fields a monitor may read.
pub const MONITOR_SOURCES: [&str; 5] =
    ["theta0", "temp", "biasVoltage", "inputPower", "outputPower"];

/// Device state fields a control may write.
pub const CONTROL_TARGETS: [&str; 5] = [
    "biasEnabled",
    "biasPoint",
    "ditherAmp",
    "ditherFreq",
    "mode",
];

fn default_digits() -> u8 {
    1
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WidgetConfig {
    pub title: String,
    pub monitors: Vec<MonitorWidget>,
    pub controls: Vec<ControlWidget>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MonitorWidget {
    pub id: String,
    pub label: String,
    pub source: String,
    pub unit: String,
    pub digits: u8,
    pub description: String,
    pub range: Option<NumberRange>,
    pub threshold: Option<Threshold>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ControlWidget {
    pub id: String,
    pub label: String,
    pub kind: ControlKind,
    pub target: String,
    pub unit: String,
    pub description: String,
    pub options: Vec<String>,
    pub range: Option<NumberRange>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ControlKind {
    Toggle,
    Select,
    Range,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NumberRange {
    pub min: f64,
    pub max: f64,
    pub step: Option<f64>,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Threshold {
    pub low: Option<f64>,
    pub high: Option<f64>,
}

/* -------------------------------------------------------------------------- */
/* Raw serde mirror                                                           */
/* -------------------------------------------------------------------------- */

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct RawConfig {
    #[serde(default)]
    title: Option<String>,
    #[serde(default)]
    monitors: Vec<RawMonitor>,
    #[serde(default)]
    controls: Vec<RawControl>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct RawMonitor {
    id: String,
    #[serde(default)]
    label: Option<String>,
    #[serde(default)]
    source: Option<String>,
    #[serde(default)]
    unit: Option<String>,
    #[serde(default = "default_digits")]
    digits: u8,
    #[serde(default)]
    description: Option<String>,
    #[serde(default)]
    range: Option<RawRange>,
    #[serde(default)]
    threshold: Option<RawThreshold>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct RawControl {
    id: String,
    #[serde(default)]
    label: Option<String>,
    kind: ControlKind,
    #[serde(default)]
    target: Option<String>,
    #[serde(default)]
    unit: Option<String>,
    #[serde(default)]
    description: Option<String>,
    #[serde(default)]
    options: Vec<String>,
    #[serde(default)]
    range: Option<RawRange>,
}

#[derive(Clone, Copy, Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct RawRange {
    min: f64,
    max: f64,
    #[serde(default)]
    step: Option<f64>,
}

#[derive(Clone, Copy, Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct RawThreshold {
    #[serde(default)]
    low: Option<f64>,
    #[serde(default)]
    high: Option<f64>,
}

/* -------------------------------------------------------------------------- */
/* Parse + validate                                                           */
/* -------------------------------------------------------------------------- */

/// Parse and validate a widget file.
///
/// Returns every problem at once so the UI can show a useful list instead of
/// only the first typo.
pub fn parse(input: &str) -> Result<WidgetConfig, Vec<String>> {
    let raw: RawConfig = toml::from_str(input).map_err(|err| vec![err.to_string()])?;

    let mut errors = Vec::new();
    let mut ids = HashSet::new();

    let mut monitors = Vec::with_capacity(raw.monitors.len());
    for raw in raw.monitors {
        let id = raw.id.trim().to_string();
        if id.is_empty() {
            errors.push("monitor: `id` must not be empty".into());
            continue;
        }
        if !ids.insert(id.clone()) {
            errors.push(format!("`{id}`: duplicate widget id"));
        }

        let source = raw
            .source
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .unwrap_or(&id)
            .to_string();

        if !MONITOR_SOURCES.contains(&source.as_str()) {
            errors.push(format!(
                "`{id}`: unknown monitor source `{source}` (expected one of {})",
                MONITOR_SOURCES.join(", ")
            ));
        }

        let range = raw.range.map(|range| {
            if !range.min.is_finite() || !range.max.is_finite() || range.min >= range.max {
                errors.push(format!(
                    "`{id}`: monitor range needs finite min < max (got {}..={})",
                    range.min, range.max
                ));
            }
            NumberRange {
                min: range.min,
                max: range.max,
                step: None,
            }
        });

        let threshold = raw.threshold.map(|threshold| {
            if threshold.low.is_some_and(|low| !low.is_finite())
                || threshold.high.is_some_and(|high| !high.is_finite())
            {
                errors.push(format!("`{id}`: threshold values must be finite"));
            }
            if let (Some(low), Some(high)) = (threshold.low, threshold.high) {
                if low >= high {
                    errors.push(format!(
                        "`{id}`: threshold needs low < high (got {low}..={high})"
                    ));
                }
            }
            Threshold {
                low: threshold.low,
                high: threshold.high,
            }
        });

        if raw.digits > 8 {
            errors.push(format!("`{id}`: digits must be between 0 and 8"));
        }

        monitors.push(MonitorWidget {
            label: raw
                .label
                .map(|label| label.trim().to_string())
                .filter(|label| !label.is_empty())
                .unwrap_or_else(|| id.clone()),
            id,
            source,
            unit: raw.unit.unwrap_or_default(),
            digits: raw.digits.min(8),
            description: raw.description.unwrap_or_default(),
            range,
            threshold,
        });
    }

    let mut controls = Vec::with_capacity(raw.controls.len());
    let mut targets = HashSet::new();
    for raw in raw.controls {
        let id = raw.id.trim().to_string();
        if id.is_empty() {
            errors.push("control: `id` must not be empty".into());
            continue;
        }
        if !ids.insert(id.clone()) {
            errors.push(format!("`{id}`: duplicate widget id"));
        }

        let target = raw.target.unwrap_or_default().trim().to_string();
        if target.is_empty() {
            errors.push(format!("`{id}`: control needs a `target`"));
        } else if !CONTROL_TARGETS.contains(&target.as_str()) {
            errors.push(format!(
                "`{id}`: unknown control target `{target}` (expected one of {})",
                CONTROL_TARGETS.join(", ")
            ));
        } else if !targets.insert(target.clone()) {
            errors.push(format!("`{id}`: target `{target}` is already controlled"));
        }

        let numeric_target = matches!(target.as_str(), "ditherAmp" | "ditherFreq" | "mode");

        match raw.kind {
            ControlKind::Toggle if target != "biasEnabled" => {
                errors.push(format!(
                    "`{id}`: toggle controls can only target `biasEnabled`"
                ));
            }
            ControlKind::Select if target != "biasPoint" && target != "mode" => {
                errors.push(format!(
                    "`{id}`: select controls can only target `biasPoint` or `mode`"
                ));
            }
            ControlKind::Range if !numeric_target => {
                errors.push(format!(
                    "`{id}`: range controls can only target `ditherAmp`, `ditherFreq` or `mode`"
                ));
            }
            _ => {}
        }

        if raw.kind == ControlKind::Select {
            if raw.options.is_empty() {
                errors.push(format!("`{id}`: select controls need at least one option"));
            }
            if target == "biasPoint"
                && raw
                    .options
                    .iter()
                    .any(|option| !["QUAD+", "QUAD-", "MAX", "NULL"].contains(&option.as_str()))
            {
                errors.push(format!(
                    "`{id}`: biasPoint options must be QUAD+, QUAD-, MAX or NULL"
                ));
            }
            if target == "mode"
                && raw
                    .options
                    .iter()
                    .any(|option| option.parse::<u8>().is_err())
            {
                errors.push(format!(
                    "`{id}`: mode options must be integers from 0 to 255"
                ));
            }
        }

        let range = raw.range.map(|range| {
            if !range.min.is_finite() || !range.max.is_finite() || range.min >= range.max {
                errors.push(format!(
                    "`{id}`: control range needs finite min < max (got {}..={})",
                    range.min, range.max
                ));
            }
            if let Some(step) = range.step {
                if !step.is_finite() || step <= 0.0 {
                    errors.push(format!("`{id}`: control step must be greater than 0"));
                }
            }
            NumberRange {
                min: range.min,
                max: range.max,
                step: Some(range.step.unwrap_or(1.0)),
            }
        });

        if raw.kind == ControlKind::Range && range.is_none() {
            errors.push(format!("`{id}`: range controls need a `range` table"));
        }

        controls.push(ControlWidget {
            label: raw
                .label
                .map(|label| label.trim().to_string())
                .filter(|label| !label.is_empty())
                .unwrap_or_else(|| id.clone()),
            id,
            kind: raw.kind,
            target,
            unit: raw.unit.unwrap_or_default(),
            description: raw.description.unwrap_or_default(),
            options: raw.options,
            range,
        });
    }

    if !errors.is_empty() {
        return Err(errors);
    }

    Ok(WidgetConfig {
        title: raw
            .title
            .map(|title| title.trim().to_string())
            .filter(|title| !title.is_empty())
            .unwrap_or_else(|| "Bias controller".into()),
        monitors,
        controls,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_the_shipped_default() {
        let config = parse(DEFAULT_TOML).expect("default widget file must be valid");
        assert_eq!(config.monitors.len(), 4);
        assert_eq!(config.controls.len(), 5);
        assert_eq!(config.monitors[0].source, "theta0");
    }

    #[test]
    fn rejects_unknown_source() {
        let errors = parse(
            r#"
            [[monitors]]
            id = "nope"
            source = "made_up"
            "#,
        )
        .unwrap_err();
        assert!(
            errors
                .iter()
                .any(|error| error.contains("unknown monitor source"))
        );
    }

    #[test]
    fn rejects_bad_threshold() {
        let errors = parse(
            r#"
            [[monitors]]
            id = "temp"
            source = "temp"
            threshold = { low = 80, high = 10 }
            "#,
        )
        .unwrap_err();
        assert!(errors.iter().any(|error| error.contains("threshold")));
    }

    #[test]
    fn rejects_conflicting_control_targets() {
        let errors = parse(
            r#"
            [[controls]]
            id = "amp_a"
            kind = "range"
            target = "ditherAmp"
            range = { min = 0, max = 1, step = 0.1 }

            [[controls]]
            id = "amp_b"
            kind = "range"
            target = "ditherAmp"
            range = { min = 0, max = 1, step = 0.1 }
            "#,
        )
        .unwrap_err();
        assert!(
            errors
                .iter()
                .any(|error| error.contains("already controlled"))
        );
    }
}
