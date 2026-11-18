//! Wire protocol between this app (master) and the bias controller MCU (slave).
//!
//! The master owns the serial clock and polls the MCU for telemetry by sending a
//! query frame every `TICK_MS` (300 ms). Command frames (state snapshots and
//! calibration requests) are sent on demand when the user explicitly triggers
//! them.
//!
//! # Replacing the frame format
//!
//! All bytes that ever leave the serial port are produced by [`encode_query`],
//! [`encode_state`] and [`encode_calibrate`]. When the real byte layout is
//! known, replace only those functions (and the [`DeviceState`] fields if the MCU
//! expects a different payload) and the rest of the app keeps working unchanged.
//!
//! The app supports selectable outbound formats for master frames. The current
//! placeholder **text** format is an ASCII line so it can be inspected with any
//! serial monitor:
//!
//! ```text
//! QUERY seq=41\n
//! STATE seq=42 bias=1 bp=QUAD+ amp=0.500 freq=1000.0 mode=0 gain=1.000 offset=0.000\n
//! CAL seq=43\n
//! ```
//!
//! The **binary** format is a compact placeholder encoding (tag + sequence +
//! fields, little-endian) intended for development; replace it with the real MCU
//! binary layout when known.
//!
//! Inbound telemetry is line based. The app understands placeholder `TLM`
//! lines and forwards the parsed values to the monitor cards:
//!
//! ```text
//! TLM seq=12 theta0=12.3 temp=24.5 vbias=3.3 pin=1.2 pout=0.8 ibias=0.4 vsupply=12.1 phase=1.5 lock=1\n
//! ```

use std::fmt;

use serde::{Deserialize, Serialize};

/// Outbound wire format for master-generated frames (query/state/calibrate).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WireFormat {
    #[default]
    Text,
    Binary,
}

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

impl BiasPoint {
    fn as_u8(self) -> u8 {
        match self {
            BiasPoint::QuadPlus => 0,
            BiasPoint::QuadMinus => 1,
            BiasPoint::Max => 2,
            BiasPoint::Null => 3,
        }
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
    /// Loop gain (dimensionless unless the MCU defines otherwise).
    pub gain: f32,
    /// Loop offset / trim.
    pub offset: f32,
}

impl Default for DeviceState {
    fn default() -> Self {
        Self {
            bias_enabled: false,
            bias_point: BiasPoint::Null,
            dither_amp: 0.0,
            dither_freq: 1000.0,
            mode: 0,
            gain: 1.0,
            offset: 0.0,
        }
    }
}

/// Encode a periodic query frame that requests the latest telemetry from the MCU.
///
/// `seq` wraps at `u32::MAX` and lets the MCU detect dropped frames.
pub fn encode_query(seq: u32, format: WireFormat) -> Vec<u8> {
    match format {
        WireFormat::Text => format!("QUERY seq={seq}\n").into_bytes(),
        WireFormat::Binary => {
            let mut out = Vec::with_capacity(6);
            out.extend_from_slice(b"Q");
            out.extend_from_slice(&seq.to_le_bytes());
            out.push(b'\n');
            out
        }
    }
}

/// Encode one state snapshot as a command frame ready to write to the serial port.
///
/// `seq` wraps at `u32::MAX` and lets the MCU detect dropped frames.
pub fn encode_state(seq: u32, state: &DeviceState, format: WireFormat) -> Vec<u8> {
    match format {
        WireFormat::Text => format!(
            "STATE seq={seq} bias={} bp={} amp={:.3} freq={:.1} mode={} gain={:.3} offset={:.3}\n",
            u8::from(state.bias_enabled),
            state.bias_point,
            state.dither_amp,
            state.dither_freq,
            state.mode,
            state.gain,
            state.offset,
        )
        .into_bytes(),
        WireFormat::Binary => {
            let mut out = Vec::with_capacity(24);
            out.extend_from_slice(b"S");
            out.extend_from_slice(&seq.to_le_bytes());
            out.push(u8::from(state.bias_enabled));
            out.push(state.bias_point.as_u8());
            out.extend_from_slice(&state.dither_amp.to_le_bytes());
            out.extend_from_slice(&state.dither_freq.to_le_bytes());
            out.push(state.mode);
            out.extend_from_slice(&state.gain.to_le_bytes());
            out.extend_from_slice(&state.offset.to_le_bytes());
            out.push(b'\n');
            out
        }
    }
}

/// Encode a one-shot calibration command.
pub fn encode_calibrate(seq: u32, format: WireFormat) -> Vec<u8> {
    match format {
        WireFormat::Text => format!("CAL seq={seq}\n").into_bytes(),
        WireFormat::Binary => {
            let mut out = Vec::with_capacity(6);
            out.extend_from_slice(b"C");
            out.extend_from_slice(&seq.to_le_bytes());
            out.push(b'\n');
            out
        }
    }
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
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bias_current: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub supply_voltage: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub phase_error: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub lock_status: Option<f32>,
}

impl TelemetryPatch {
    pub fn is_empty(&self) -> bool {
        self.theta0.is_none()
            && self.temp.is_none()
            && self.bias_voltage.is_none()
            && self.input_power.is_none()
            && self.output_power.is_none()
            && self.bias_current.is_none()
            && self.supply_voltage.is_none()
            && self.phase_error.is_none()
            && self.lock_status.is_none()
    }

    /// Merge a sparse sample into a running snapshot.
    ///
    /// Inbound `TLM` lines may omit fields; keeping the previous value for the
    /// omitted fields is what lets packet templates interpolate a complete
    /// picture instead of a different subset on every line.
    pub fn merge(&mut self, patch: &TelemetryPatch) {
        macro_rules! merge {
            ($($field:ident),* $(,)?) => {
                $(
                    if patch.$field.is_some() {
                        self.$field = patch.$field;
                    }
                )*
            };
        }

        merge!(
            theta0,
            temp,
            bias_voltage,
            input_power,
            output_power,
            bias_current,
            supply_voltage,
            phase_error,
            lock_status,
        );
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
            "ibias" | "i_bias" | "biascurrent" | "current" => patch.bias_current = Some(value),
            "vsupply" | "v_supply" | "supply" | "supplyvoltage" => {
                patch.supply_voltage = Some(value)
            }
            "phase" | "phaseerror" | "perr" => patch.phase_error = Some(value),
            "lock" | "lockstatus" | "locked" => patch.lock_status = Some(value),
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
            gain: 1.5,
            offset: -0.25,
        };

        let frame = encode_state(7, &state, WireFormat::Text);
        assert_eq!(
            String::from_utf8(frame).unwrap(),
            "STATE seq=7 bias=1 bp=QUAD+ amp=0.500 freq=1000.0 mode=2 gain=1.500 offset=-0.250\n"
        );
    }

    #[test]
    fn encodes_query() {
        assert_eq!(
            String::from_utf8(encode_query(41, WireFormat::Text)).unwrap(),
            "QUERY seq=41\n"
        );
    }

    #[test]
    fn encodes_calibrate() {
        assert_eq!(
            String::from_utf8(encode_calibrate(43, WireFormat::Text)).unwrap(),
            "CAL seq=43\n"
        );
    }

    #[test]
    fn encodes_binary_frames() {
        let state = DeviceState {
            bias_enabled: true,
            bias_point: BiasPoint::QuadPlus,
            dither_amp: 0.5,
            dither_freq: 1000.0,
            mode: 2,
            gain: 1.5,
            offset: -0.25,
        };

        let query = encode_query(41, WireFormat::Binary);
        assert_eq!(&query[0..1], b"Q");
        assert_eq!(u32::from_le_bytes(query[1..5].try_into().unwrap()), 41);
        assert_eq!(*query.last().unwrap(), b'\n');

        let frame = encode_state(7, &state, WireFormat::Binary);
        assert_eq!(&frame[0..1], b"S");
        assert_eq!(u32::from_le_bytes(frame[1..5].try_into().unwrap()), 7);
        assert_eq!(frame[5], 1);
        assert_eq!(frame[6], 0); // QUAD+
        assert_eq!(*frame.last().unwrap(), b'\n');

        let cal = encode_calibrate(43, WireFormat::Binary);
        assert_eq!(&cal[0..1], b"C");
        assert_eq!(u32::from_le_bytes(cal[1..5].try_into().unwrap()), 43);
    }

    #[test]
    fn parses_telemetry_line() {
        let patch = parse_telemetry_line(
            "TLM seq=12 theta0=12.3 temp=24.5 vbias=3.3 pin=1.2 pout=0.8 \
             ibias=0.4 vsupply=12.1 phase=1.5 lock=1\r\n",
        )
        .unwrap();

        assert_eq!(
            patch,
            TelemetryPatch {
                theta0: Some(12.3),
                temp: Some(24.5),
                bias_voltage: Some(3.3),
                input_power: Some(1.2),
                output_power: Some(0.8),
                bias_current: Some(0.4),
                supply_voltage: Some(12.1),
                phase_error: Some(1.5),
                lock_status: Some(1.0),
            }
        );
    }

    #[test]
    fn ignores_unknown_lines_and_keys() {
        assert!(parse_telemetry_line("STATE seq=1").is_none());
        assert!(parse_telemetry_line("TLM seq=1 unknown=2").is_none());
    }

    #[test]
    fn merges_sparse_telemetry() {
        let mut running = TelemetryPatch {
            temp: Some(24.5),
            lock_status: Some(1.0),
            ..TelemetryPatch::default()
        };

        running.merge(&TelemetryPatch {
            temp: Some(25.0),
            ..TelemetryPatch::default()
        });

        assert_eq!(running.temp, Some(25.0));
        assert_eq!(running.lock_status, Some(1.0));
    }
}
