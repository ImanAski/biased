//! TOML-driven widget layout and protocol configuration.
//!
//! The file describes presentation, limits, and (optionally) wire/protocol
//! behavior. Monitors map onto telemetry fields and controls map onto fields of
//! [`crate::protocol::DeviceState`]. Protocol settings are editable in editor
//! builds and become fixed when embedded in a configuration-locked customer
//! build.

use std::collections::HashSet;

use serde::{Deserialize, Serialize};

/// Seeded into the app config directory on first launch.
pub const DEFAULT_TOML: &str = include_str!("../widgets.toml");

/// Telemetry fields a monitor may read.
pub const MONITOR_SOURCES: [&str; 9] = [
    "theta0",
    "temp",
    "biasVoltage",
    "inputPower",
    "outputPower",
    "biasCurrent",
    "supplyVoltage",
    "phaseError",
    "lockStatus",
];

/// Device state fields a control may write.
pub const CONTROL_TARGETS: [&str; 7] = [
    "biasEnabled",
    "biasPoint",
    "ditherAmp",
    "ditherFreq",
    "mode",
    "gain",
    "offset",
];

/// Numeric targets accepted by `range` and `stepper` controls.
const NUMERIC_TARGETS: [&str; 5] = ["ditherAmp", "ditherFreq", "mode", "gain", "offset"];

fn default_digits() -> u8 {
    1
}

fn default_history() -> u16 {
    60
}

fn default_segments() -> u8 {
    20
}

fn default_tick_ms() -> u64 {
    300
}

fn default_true() -> bool {
    true
}

/// Header of the status read request and of received telemetry frames.
fn default_rx_sync() -> u8 {
    0x77
}

/// Header of write frames and of the slave's write acknowledge.
fn default_tx_header() -> u8 {
    0x71
}

/// Footer of status reads, telemetry responses and write acknowledges.
fn default_rx_footer() -> u8 {
    0x23
}

fn default_tx_footer() -> u8 {
    0x00
}

fn default_scale() -> f64 {
    1.0
}

fn kind_label(kind: ControlKind) -> &'static str {
    match kind {
        ControlKind::Toggle => "toggle",
        ControlKind::Select => "select",
        ControlKind::Range => "range",
        ControlKind::Stepper => "stepper",
        ControlKind::Knob => "knob",
        ControlKind::Segmented => "segmented",
    }
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WidgetConfig {
    pub title: String,
    pub monitors: Vec<MonitorWidget>,
    pub controls: Vec<ControlWidget>,
    pub protocol: ProtocolConfig,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProtocolDriver {
    /// Legacy app protocol: periodic QUERY poll and on-demand STATE/CAL frames.
    Legacy,
    /// RSProtocol-style fixed binary packets (configurable framing/CRC/mapping).
    #[default]
    Rs,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Endianness {
    #[default]
    Little,
    Big,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CrcKind {
    None,
    #[default]
    Crc16Modbus,
    Crc16Ibm,
    Crc16CcittFalse,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct ProtocolConfig {
    pub driver: ProtocolDriver,
    /// Master clock period for periodic outbound activity.
    pub tick_ms: u64,
    /// When true, the master clock periodically sends the outbound packet/frame.
    pub periodic_send: bool,
    /// RS: sync byte for status read requests and received telemetry frames.
    pub rx_sync: u8,
    /// RS: header byte for transmitted write/command frames and their ACK.
    pub tx_header: u8,
    /// RS: footer byte closing status reads, telemetry frames and ACKs.
    pub rx_footer: u8,
    /// RS: footer byte written in transmitted write/command frames.
    pub tx_footer: u8,
    pub endianness: Endianness,
    pub crc: CrcKind,
    /// Scale applied when encoding dither amplitude to uint16 wire units.
    pub dither_amp_scale: f64,
    /// Scale applied when encoding dither frequency to uint16 wire units.
    pub dither_freq_scale: f64,
    /// Scale applied when encoding/decoding telemetry-scaled uint16 values.
    pub telemetry_scale: f64,
}

impl Default for ProtocolConfig {
    fn default() -> Self {
        Self {
            driver: ProtocolDriver::Rs,
            tick_ms: default_tick_ms(),
            periodic_send: true,
            rx_sync: default_rx_sync(),
            tx_header: default_tx_header(),
            rx_footer: default_rx_footer(),
            tx_footer: default_tx_footer(),
            endianness: Endianness::Big,
            crc: CrcKind::None,
            dither_amp_scale: 1000.0,
            dither_freq_scale: 1.0,
            telemetry_scale: 100.0,
        }
    }
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MonitorWidget {
    pub id: String,
    pub label: String,
    pub source: String,
    pub unit: String,
    pub digits: u8,
    /// Multiplier applied to inbound telemetry before display.
    pub scale: f64,
    pub description: String,
    pub variant: MonitorVariant,
    /// Number of samples kept by the `sparkline` variant.
    pub history: u16,
    /// Number of cells drawn by the `meter` variant.
    pub segments: u8,
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
    pub presets: Vec<f64>,
    pub range: Option<NumberRange>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MonitorVariant {
    #[default]
    Bar,
    Gauge,
    Sparkline,
    Led,
    Tile,
    Meter,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ControlKind {
    Toggle,
    Select,
    Range,
    Stepper,
    Knob,
    Segmented,
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
#[serde(rename_all = "camelCase", deny_unknown_fields, default)]
struct RawConfig {
    title: Option<String>,
    monitors: Vec<RawMonitor>,
    controls: Vec<RawControl>,
    protocol: RawProtocolConfig,
}

impl Default for RawConfig {
    fn default() -> Self {
        Self {
            title: None,
            monitors: Vec::new(),
            controls: Vec::new(),
            protocol: RawProtocolConfig::default(),
        }
    }
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields, default)]
struct RawProtocolConfig {
    driver: ProtocolDriver,
    tick_ms: u64,
    periodic_send: bool,
    rx_sync: u8,
    tx_header: u8,
    rx_footer: u8,
    tx_footer: u8,
    endianness: Endianness,
    crc: CrcKind,
    dither_amp_scale: f64,
    dither_freq_scale: f64,
    telemetry_scale: f64,
}

impl Default for RawProtocolConfig {
    fn default() -> Self {
        let d = ProtocolConfig::default();
        Self {
            driver: d.driver,
            tick_ms: d.tick_ms,
            periodic_send: d.periodic_send,
            rx_sync: d.rx_sync,
            tx_header: d.tx_header,
            rx_footer: d.rx_footer,
            tx_footer: d.tx_footer,
            endianness: d.endianness,
            crc: d.crc,
            dither_amp_scale: d.dither_amp_scale,
            dither_freq_scale: d.dither_freq_scale,
            telemetry_scale: d.telemetry_scale,
        }
    }
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
    #[serde(default = "default_scale")]
    scale: f64,
    #[serde(default)]
    description: Option<String>,
    #[serde(default)]
    variant: MonitorVariant,
    #[serde(default = "default_history")]
    history: u16,
    #[serde(default = "default_segments")]
    segments: u8,
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
    presets: Vec<f64>,
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

        if !raw.scale.is_finite() || raw.scale == 0.0 {
            errors.push(format!(
                "`{id}`: scale must be a finite non-zero number"
            ));
        }

        if !(8..=600).contains(&raw.history) {
            errors.push(format!(
                "`{id}`: history must be between 8 and 600 samples (got {})",
                raw.history
            ));
        }

        if !(5..=40).contains(&raw.segments) {
            errors.push(format!(
                "`{id}`: segments must be between 5 and 40 (got {})",
                raw.segments
            ));
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
            scale: raw.scale,
            description: raw.description.unwrap_or_default(),
            variant: raw.variant,
            history: raw.history.clamp(8, 600),
            segments: raw.segments.clamp(5, 40),
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

        let numeric_target = NUMERIC_TARGETS.contains(&target.as_str());

        match raw.kind {
            ControlKind::Toggle | ControlKind::Segmented if target != "biasEnabled" => {
                errors.push(format!(
                    "`{id}`: {} controls can only target `biasEnabled`",
                    kind_label(raw.kind)
                ));
            }
            ControlKind::Select if target != "biasPoint" && target != "mode" => {
                errors.push(format!(
                    "`{id}`: select controls can only target `biasPoint` or `mode`"
                ));
            }
            ControlKind::Range | ControlKind::Stepper | ControlKind::Knob if !numeric_target => {
                errors.push(format!(
                    "`{id}`: {} controls can only target {}",
                    kind_label(raw.kind),
                    NUMERIC_TARGETS.join(", ")
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

        if matches!(
            raw.kind,
            ControlKind::Range | ControlKind::Stepper | ControlKind::Knob
        ) && range.is_none()
        {
            errors.push(format!(
                "`{id}`: {} controls need a `range` table",
                kind_label(raw.kind)
            ));
        }

        if !raw.presets.is_empty() {
            if !matches!(
                raw.kind,
                ControlKind::Range | ControlKind::Stepper | ControlKind::Knob
            ) {
                errors.push(format!(
                    "`{id}`: presets are only available on range, stepper and knob controls"
                ));
            }
            if raw.presets.len() > 8 {
                errors.push(format!("`{id}`: at most 8 presets are allowed"));
            }
            for preset in &raw.presets {
                if !preset.is_finite() {
                    errors.push(format!("`{id}`: presets must be finite"));
                    continue;
                }
                if target == "mode" && (preset.fract() != 0.0 || !(0.0..=255.0).contains(preset)) {
                    errors.push(format!(
                        "`{id}`: mode presets must be integers from 0 to 255"
                    ));
                }
                if let Some(range) = range {
                    let min = range.min.min(range.max);
                    let max = range.min.max(range.max);
                    if *preset < min || *preset > max {
                        errors.push(format!(
                            "`{id}`: preset {preset} is outside the control range {min}…{max}"
                        ));
                    }
                }
            }
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
            presets: raw.presets,
            range,
        });
    }

    // Protocol validation (kept permissive so editors can adjust for target devices).
    let p = raw.protocol;
    if !(10..=60_000).contains(&p.tick_ms) {
        errors.push(format!(
            "protocol.tickMs must be between 10 and 60000 (got {})",
            p.tick_ms
        ));
    }
    if !p.dither_amp_scale.is_finite() || p.dither_amp_scale == 0.0 {
        errors.push("protocol.ditherAmpScale must be a finite non-zero number".into());
    }
    if !p.dither_freq_scale.is_finite() || p.dither_freq_scale == 0.0 {
        errors.push("protocol.ditherFreqScale must be a finite non-zero number".into());
    }
    if !p.telemetry_scale.is_finite() || p.telemetry_scale == 0.0 {
        errors.push("protocol.telemetryScale must be a finite non-zero number".into());
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
        protocol: ProtocolConfig {
            driver: p.driver,
            tick_ms: p.tick_ms,
            periodic_send: p.periodic_send,
            rx_sync: p.rx_sync,
            tx_header: p.tx_header,
            rx_footer: p.rx_footer,
            tx_footer: p.tx_footer,
            endianness: p.endianness,
            crc: p.crc,
            dither_amp_scale: p.dither_amp_scale,
            dither_freq_scale: p.dither_freq_scale,
            telemetry_scale: p.telemetry_scale,
        },
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_the_shipped_default() {
        let config = parse(DEFAULT_TOML).expect("default widget file must be valid");
        assert_eq!(config.monitors.len(), 8);
        assert_eq!(config.controls.len(), 7);
        assert_eq!(config.monitors[0].source, "theta0");
        assert_eq!(config.monitors[0].variant, MonitorVariant::Bar);
        assert!(
            config
                .monitors
                .iter()
                .any(|m| m.variant == MonitorVariant::Gauge)
        );
        assert!(
            config
                .monitors
                .iter()
                .any(|m| m.variant == MonitorVariant::Led)
        );
        assert!(
            config
                .monitors
                .iter()
                .any(|m| m.variant == MonitorVariant::Meter)
        );
        assert!(
            config
                .monitors
                .iter()
                .any(|m| m.variant == MonitorVariant::Tile)
        );
        assert!(
            config
                .controls
                .iter()
                .any(|c| c.kind == ControlKind::Stepper)
        );
        assert!(config.controls.iter().any(|c| c.kind == ControlKind::Knob));
        assert_eq!(config.protocol.rx_sync, 0x77);
        assert_eq!(config.protocol.tx_header, 0x71);
        assert_eq!(config.protocol.rx_footer, 0x23);
        assert_eq!(config.protocol.tick_ms, 300);
    }

    #[test]
    fn parses_framing_overrides() {
        let config = parse(
            r#"
            [protocol]
            rxSync = 0x11
            txHeader = 0x22
            rxFooter = 0x33
            txFooter = 0x44
            "#,
        )
        .expect("layout must be valid");

        assert_eq!(config.protocol.rx_sync, 0x11);
        assert_eq!(config.protocol.tx_header, 0x22);
        assert_eq!(config.protocol.rx_footer, 0x33);
        assert_eq!(config.protocol.tx_footer, 0x44);
    }

    #[test]
    fn parses_variant_history_and_presets() {
        let config = parse(
            r#"
            [[monitors]]
            id = "psu"
            source = "supplyVoltage"
            variant = "sparkline"
            history = 120

            [[controls]]
            id = "offset"
            kind = "stepper"
            target = "offset"
            range = { min = -5, max = 5, step = 0.1 }
            presets = [-1, 0, 1]
            "#,
        )
        .expect("layout must be valid");

        assert_eq!(config.monitors[0].variant, MonitorVariant::Sparkline);
        assert_eq!(config.monitors[0].history, 120);
        assert_eq!(config.controls[0].presets, vec![-1.0, 0.0, 1.0]);
    }

    #[test]
    fn parses_meter_knob_and_segmented() {
        let config = parse(
            r#"
            [[monitors]]
            id = "level"
            source = "inputPower"
            variant = "meter"
            segments = 24

            [[controls]]
            id = "gain"
            kind = "knob"
            target = "gain"
            range = { min = 0, max = 4, step = 0.05 }
            presets = [1, 2]

            [[controls]]
            id = "output"
            kind = "segmented"
            target = "biasEnabled"
            "#,
        )
        .expect("layout must be valid");

        assert_eq!(config.monitors[0].variant, MonitorVariant::Meter);
        assert_eq!(config.monitors[0].segments, 24);
        assert_eq!(config.controls[0].kind, ControlKind::Knob);
        assert_eq!(config.controls[1].kind, ControlKind::Segmented);
    }

    #[test]
    fn rejects_bad_segments_and_segmented_target() {
        let errors = parse(
            r#"
            [[monitors]]
            id = "level"
            source = "inputPower"
            segments = 2

            [[controls]]
            id = "amp"
            kind = "segmented"
            target = "ditherAmp"
            "#,
        )
        .unwrap_err();

        assert!(errors.iter().any(|error| error.contains("segments")));
        assert!(
            errors
                .iter()
                .any(|error| error.contains("segmented") && error.contains("biasEnabled"))
        );
    }

    #[test]
    fn rejects_presets_outside_range() {
        let errors = parse(
            r#"
            [[controls]]
            id = "gain"
            kind = "range"
            target = "gain"
            range = { min = 0, max = 10, step = 0.5 }
            presets = [2, 20]
            "#,
        )
        .unwrap_err();
        assert!(
            errors
                .iter()
                .any(|error| error.contains("outside the control range"))
        );
    }

    #[test]
    fn rejects_bad_history_and_presets_on_toggle() {
        let errors = parse(
            r#"
            [[monitors]]
            id = "temp"
            source = "temp"
            history = 2

            [[controls]]
            id = "bias"
            kind = "toggle"
            target = "biasEnabled"
            presets = [1]
            "#,
        )
        .unwrap_err();
        assert!(errors.iter().any(|error| error.contains("history")));
        assert!(errors.iter().any(|error| error.contains("presets")));
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
    fn parses_monitor_scale_with_default() {
        let config = parse(
            r#"
            [[monitors]]
            id = "temp"
            source = "temp"

            [[monitors]]
            id = "bias_mv"
            source = "biasVoltage"
            scale = 1000.0
            "#,
        )
        .expect("layout must be valid");

        assert_eq!(config.monitors[0].scale, 1.0);
        assert_eq!(config.monitors[1].scale, 1000.0);
    }

    #[test]
    fn rejects_invalid_monitor_scale() {
        for scale in ["0", "nan", "inf"] {
            let input = format!(
                r#"
                [[monitors]]
                id = "temp"
                source = "temp"
                scale = {scale}
                "#
            );
            let errors = parse(&input).unwrap_err();
            assert!(
                errors.iter().any(|error| error.contains("scale")),
                "scale = {scale} should be rejected: {errors:?}"
            );
        }
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

    fn documentation_toml_blocks(document: &str) -> Vec<(usize, String)> {
        let mut blocks = Vec::new();
        let mut current: Option<(usize, String)> = None;

        for (index, line) in document.lines().enumerate() {
            if let Some(info) = line.strip_prefix("```") {
                if let Some(block) = current.take() {
                    blocks.push(block);
                } else if info.trim() == "toml" {
                    current = Some((index + 1, String::new()));
                }
                continue;
            }

            if let Some((_, body)) = current.as_mut() {
                body.push_str(line);
                body.push('\n');
            }
        }

        assert!(current.is_none(), "unclosed TOML fence");
        blocks
    }

    #[test]
    fn parses_documentation_examples() {
        for (name, document) in [
            ("README.md", include_str!("../../README.md")),
            ("docs/widgets.md", include_str!("../../docs/widgets.md")),
        ] {
            let blocks = documentation_toml_blocks(document);
            assert!(!blocks.is_empty(), "{name} should contain TOML examples");

            for (line, block) in blocks {
                if let Err(errors) = parse(&block) {
                    panic!(
                        "{name} TOML block at line {line} is invalid:\n{}",
                        errors.join("\n")
                    );
                }
            }
        }
    }
}
