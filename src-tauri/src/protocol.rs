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
}
