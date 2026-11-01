//! Wire protocol between this app (master) and the bias controller MCU (slave).
//!
//! The master sends one complete state snapshot every `TICK_MS` (300 ms). The
//! MCU never has to ask for it; it just applies the newest frame it receives.
//!
//! # Replacing the frame format
//!
//! All bytes that ever leave the serial port are produced by [`encode_state`]
//! and [`encode_calibrate`]. When the real byte layout is known, replace only
//! those functions (and the [`DeviceState`] fields if the MCU expects a
//! different payload) and the rest of the app keeps working unchanged.
//!
//! Current placeholder format is an ASCII line so it can be inspected with any
//! serial monitor:
//!
//! ```text
//! STATE seq=42 bias=1 bp=QUAD+ amp=0.500 freq=1000.0 mode=0\n
//! CAL seq=43\n
//! ```
//!
//! Inbound telemetry is line based. The app understands placeholder `TLM`
//! lines and forwards the parsed values to the monitor cards:
//!
//! ```text
//! TLM seq=12 theta0=12.3 temp=24.5 vbias=3.3 pin=1.2 pout=0.8\n
//! ```

use std::fmt;

use serde::{Deserialize, Serialize};

/// Selectable bias operating point.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum BiasPoint {
    #[serde(rename = "QUAD+")]
    QuadPlus,
    #[serde(rename = "QUAD-")]
    QuadMinus,
    #[serde(rename = "MAX")]
    Max,
    #[default]
    #[serde(rename = "NULL")]
    Null,
}

impl fmt::Display for BiasPoint {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let name = match self {
            BiasPoint::QuadPlus => "QUAD+",
            BiasPoint::QuadMinus => "QUAD-",
            BiasPoint::Max => "MAX",
            BiasPoint::Null => "NULL",
        };
        f.write_str(name)
    }
}

/// Everything the UI can control on the MCU.
///
/// Temperature and bias voltage are monitored only; they are deliberately not
/// part of the writable state.
///
/// This is the single source of truth that the 300 ms loop serialises and
/// writes. The UI updates it through the `set_state` command.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct DeviceState {
    /// Master enable for the bias output.
    pub bias_enabled: bool,
    /// Bias operating point.
    pub bias_point: BiasPoint,
    /// Dither amplitude (unit chosen by the MCU; V by default in the UI).
    pub dither_amp: f32,
    /// Dither frequency (Hz).
    pub dither_freq: f32,
    /// Operating mode passed through to the MCU.
    pub mode: u8,
}

impl Default for DeviceState {
    fn default() -> Self {
        Self {
            bias_enabled: false,
            bias_point: BiasPoint::Null,
            dither_amp: 0.0,
            dither_freq: 1000.0,
            mode: 0,
        }
    }
}

/// Encode one state snapshot as a frame ready to write to the serial port.
///
/// `seq` wraps at `u32::MAX` and lets the MCU detect dropped frames.
pub fn encode_state(seq: u32, state: &DeviceState) -> Vec<u8> {
    format!(
        "STATE seq={seq} bias={} bp={} amp={:.3} freq={:.1} mode={}\n",
        u8::from(state.bias_enabled),
        state.bias_point,
        state.dither_amp,
        state.dither_freq,
        state.mode,
    )
    .into_bytes()
}

/// Encode a one-shot calibration request.
pub fn encode_calibrate(seq: u32) -> Vec<u8> {
    format!("CAL seq={seq}\n").into_bytes()
}

/* -------------------------------------------------------------------------- */
/* Inbound telemetry                                                          */
/* -------------------------------------------------------------------------- */

/// Partial telemetry sample decoded from one `TLM` line.
///
/// Missing fields are left out of the event payload so the UI can merge a
/// sparse update without clearing values that did not change.
#[derive(Clone, Debug, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TelemetryPatch {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub theta0: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub temp: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bias_voltage: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub input_power: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub output_power: Option<f32>,
}

impl TelemetryPatch {
    pub fn is_empty(&self) -> bool {
        self.theta0.is_none()
            && self.temp.is_none()
            && self.bias_voltage.is_none()
            && self.input_power.is_none()
            && self.output_power.is_none()
    }
}

/// Parse one complete inbound line.
///
/// Unknown keys are ignored, malformed tokens are skipped, and anything that
/// is not a `TLM` line (or carries no known field) returns `None`.
pub fn parse_telemetry_line(line: &str) -> Option<TelemetryPatch> {
    let mut tokens = line.trim().split_whitespace();
    if tokens.next()? != "TLM" {
        return None;
    }

    let mut patch = TelemetryPatch::default();

    for token in tokens {
        let Some((key, raw_value)) = token.split_once('=') else {
            continue;
        };
        let Ok(value) = raw_value.parse::<f32>() else {
            continue;
        };
        if !value.is_finite() {
            continue;
        }

        match key.to_ascii_lowercase().as_str() {
            "theta0" | "theta" => patch.theta0 = Some(value),
            "temp" | "temperature" => patch.temp = Some(value),
            "vbias" | "bias" | "biasvoltage" => patch.bias_voltage = Some(value),
            "pin" | "p_in" | "input" | "inputpower" => patch.input_power = Some(value),
            "pout" | "p_out" | "output" | "outputpower" => patch.output_power = Some(value),
            _ => {}
        }
    }

    (!patch.is_empty()).then_some(patch)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encodes_full_state() {
        let state = DeviceState {
            bias_enabled: true,
            bias_point: BiasPoint::QuadPlus,
            dither_amp: 0.5,
            dither_freq: 1000.0,
            mode: 2,
        };

        let frame = encode_state(7, &state);
        assert_eq!(
            String::from_utf8(frame).unwrap(),
            "STATE seq=7 bias=1 bp=QUAD+ amp=0.500 freq=1000.0 mode=2\n"
        );
    }

    #[test]
    fn encodes_calibrate() {
        assert_eq!(
            String::from_utf8(encode_calibrate(43)).unwrap(),
            "CAL seq=43\n"
        );
    }

    #[test]
    fn parses_telemetry_line() {
        let patch =
            parse_telemetry_line("TLM seq=12 theta0=12.3 temp=24.5 vbias=3.3 pin=1.2 pout=0.8\r\n")
                .unwrap();

        assert_eq!(
            patch,
            TelemetryPatch {
                theta0: Some(12.3),
                temp: Some(24.5),
                bias_voltage: Some(3.3),
                input_power: Some(1.2),
                output_power: Some(0.8),
            }
        );
    }

    #[test]
    fn ignores_unknown_lines_and_keys() {
        assert!(parse_telemetry_line("STATE seq=1").is_none());
        assert!(parse_telemetry_line("TLM seq=1 unknown=2").is_none());
    }
}
