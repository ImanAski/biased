//! Wire protocol between this app (master) and the bias controller MCU (slave).
//!
//! Protocol behavior is driven by the `[protocol]` section of the widget/config
//! TOML (`crate::config::ProtocolConfig`), which is editable in editor builds and
//! embedded/fixed in configuration-locked customer builds.
//!
//! Supported drivers:
//! - `legacy`: periodic QUERY poll and on-demand STATE/CAL frames, with selectable
//!   text/binary placeholder encoding (`WireFormat`) for development.
//! - `rs`: RSProtocol-style fixed binary packets (configurable sync/type/CRC/
//!   endianness and basic field scaling). Outbound frames map the app's command
//!   state to a slave/command packet; inbound frames map master/telemetry packets
//!   into `TelemetryPatch`.

use std::fmt;

use serde::{Deserialize, Serialize};

use crate::config::{CrcKind, Endianness, ProtocolConfig, ProtocolDriver};

/// Legacy outbound wire format for master-generated query/command frames.
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
    pub fn as_u8(self) -> u8 {
        match self {
            BiasPoint::QuadPlus => 0,
            BiasPoint::QuadMinus => 1,
            BiasPoint::Max => 2,
            BiasPoint::Null => 3,
        }
    }

    pub fn from_u8(value: u8) -> Option<Self> {
        match value {
            0 => Some(BiasPoint::QuadPlus),
            1 => Some(BiasPoint::QuadMinus),
            2 => Some(BiasPoint::Max),
            3 => Some(BiasPoint::Null),
            _ => None,
        }
    }
}

/// Everything the UI can control on the MCU.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct DeviceState {
    /// Master enable for the bias output.
    pub bias_enabled: bool,
    /// Bias operating point.
    pub bias_point: BiasPoint,
    /// Dither amplitude (UI units).
    pub dither_amp: f32,
    /// Dither frequency (Hz).
    pub dither_freq: f32,
    /// Operating mode passed through to the MCU.
    pub mode: u8,
    /// Loop gain.
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

/* -------------------------------------------------------------------------- */
/* Legacy (QUERY/STATE/CAL)                                                   */
/* -------------------------------------------------------------------------- */

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
/* RSProtocol-style binary packets (configurable)                             */
/* -------------------------------------------------------------------------- */

fn clamp_u16(value: f64) -> u16 {
    if !value.is_finite() {
        return 0;
    }
    if value <= 0.0 {
        0
    } else if value >= u16::MAX as f64 {
        u16::MAX
    } else {
        value.round() as u16
    }
}

fn put_u16(out: &mut Vec<u8>, value: u16, endian: Endianness) {
    match endian {
        Endianness::Little => out.extend_from_slice(&value.to_le_bytes()),
        Endianness::Big => out.extend_from_slice(&value.to_be_bytes()),
    }
}

fn get_u16(bytes: &[u8], endian: Endianness) -> Option<u16> {
    let arr: [u8; 2] = bytes.try_into().ok()?;
    Some(match endian {
        Endianness::Little => u16::from_le_bytes(arr),
        Endianness::Big => u16::from_be_bytes(arr),
    })
}

fn crc16(kind: CrcKind, data: &[u8]) -> u16 {
    match kind {
        CrcKind::None => 0,
        CrcKind::Crc16Modbus => {
            let mut crc: u16 = 0xFFFF;
            for &b in data {
                crc ^= b as u16;
                for _ in 0..8 {
                    if (crc & 1) != 0 {
                        crc = (crc >> 1) ^ 0xA001;
                    } else {
                        crc >>= 1;
                    }
                }
            }
            crc
        }
        CrcKind::Crc16Ibm => {
            let mut crc: u16 = 0x0000;
            for &b in data {
                crc ^= b as u16;
                for _ in 0..8 {
                    if (crc & 1) != 0 {
                        crc = (crc >> 1) ^ 0xA001;
                    } else {
                        crc >>= 1;
                    }
                }
            }
            crc
        }
        CrcKind::Crc16CcittFalse => {
            let mut crc: u16 = 0xFFFF;
            for &b in data {
                crc ^= (b as u16) << 8;
                for _ in 0..8 {
                    if (crc & 0x8000) != 0 {
                        crc = (crc << 1) ^ 0x1021;
                    } else {
                        crc <<= 1;
                    }
                }
            }
            crc
        }
    }
}

/// Encode an RSProtocol-style slave/command packet from the current state.
///
/// Matches the provided RSProtocol.cpp `send(SlavePacket)` layout:
/// `[txHeader][ditherAmp u16][ditherFreq u16][biasPoint u16][ditherOn u16]
///  [calibrationRequest u16][rst0 u16][rst1 u16][rst2 u16][txFooter]`
/// (18 bytes, uint16 fields in configured endianness; default big-endian).
/// Extra UI state is mapped into reserved slots for flexibility:
/// `rst0=mode`, `rst1=gain*telemetryScale`, `rst2=offset encoded`.
pub fn encode_rs_slave(
    state: &DeviceState,
    calibration_request: bool,
    cfg: &ProtocolConfig,
) -> Vec<u8> {
    let mut frame = Vec::with_capacity(20);
    frame.push(cfg.tx_header);

    let amp = clamp_u16(state.dither_amp as f64 * cfg.dither_amp_scale);
    let freq = clamp_u16(state.dither_freq as f64 * cfg.dither_freq_scale);
    let bp = state.bias_point.as_u8() as u16;
    let on = u16::from(state.bias_enabled);
    let cal = u16::from(calibration_request);
    let rst0 = state.mode as u16;
    let rst1 = clamp_u16(state.gain as f64 * cfg.telemetry_scale);
    let rst2 = clamp_u16((state.offset as f64 + 32768.0).max(0.0));

    for value in [amp, freq, bp, on, cal, rst0, rst1, rst2] {
        put_u16(&mut frame, value, cfg.endianness);
    }

    frame.push(cfg.tx_footer);

    // cpp computes CRC but does not transmit it; append only if explicitly enabled.
    if cfg.crc != CrcKind::None {
        let c = crc16(cfg.crc, &frame[1..]);
        put_u16(&mut frame, c, cfg.endianness);
    }

    frame
}

/// Try to decode one RSProtocol-style master/telemetry packet from `buf`.
///
/// Matches the provided RSProtocol.cpp receive path:
/// fixed 20-byte frame starting with `rxSync` (default 0xFC):
/// `[sync][biasVoltage u16][inputPower u16][outputPower u16][temperature u16]
///  [calibrationDone u16][rsv0 u16][rsv1 u16][rsv2 u16][rsv3 u16][footer]`
/// (uint16 fields in configured endianness; default big-endian). No CRC check
/// by default (as in cpp). `rsv0..rsv3` are mapped to additional telemetry
/// fields for flexibility.
///
/// Returns `(bytes_consumed, patch)`; consuming 1 byte on bad sync/type allows
/// resynchronization.
pub fn decode_rs_master(buf: &[u8], cfg: &ProtocolConfig) -> Option<(usize, TelemetryPatch)> {
    if buf.is_empty() {
        return None;
    }
    if buf[0] != cfg.rx_sync {
        return Some((1, TelemetryPatch::default()));
    }

    let base_len = 20usize; // header + 9*u16 + footer
    let crc_len = if cfg.crc == CrcKind::None { 0 } else { 2 };
    let frame_len = base_len + crc_len;
    if buf.len() < frame_len {
        return None;
    }

    if cfg.crc != CrcKind::None {
        let expected = crc16(cfg.crc, &buf[1..base_len]);
        let got = get_u16(&buf[base_len..base_len + 2], cfg.endianness)?;
        if expected != got {
            return Some((1, TelemetryPatch::default()));
        }
    }

    let mut off = 1usize;
    let mut read = || -> Option<u16> {
        let v = get_u16(&buf[off..off + 2], cfg.endianness)?;
        off += 2;
        Some(v)
    };

    let bias_voltage = read()? as f64 / cfg.telemetry_scale;
    let input_power = read()? as f64 / cfg.telemetry_scale;
    let output_power = read()? as f64 / cfg.telemetry_scale;
    let temp = read()? as f64 / cfg.telemetry_scale;
    let cal_done = read()?;
    let rsv0 = read()? as f64 / cfg.telemetry_scale;
    let rsv1 = read()? as f64 / cfg.telemetry_scale;
    let rsv2 = read()? as f64 / cfg.telemetry_scale;
    let rsv3 = read()? as f64 / cfg.telemetry_scale;
    let _footer = buf.get(off).copied();

    let patch = TelemetryPatch {
        theta0: Some(rsv3 as f32),
        temp: Some(temp as f32),
        bias_voltage: Some(bias_voltage as f32),
        input_power: Some(input_power as f32),
        output_power: Some(output_power as f32),
        bias_current: Some(rsv0 as f32),
        supply_voltage: Some(rsv1 as f32),
        phase_error: Some(rsv2 as f32),
        lock_status: Some(if cal_done != 0 { 1.0 } else { 0.0 }),
    };

    Some((frame_len, patch))
}

/// Encode an outbound legacy/RS frame for the periodic master tick.
pub fn encode_periodic(
    seq: u32,
    state: &DeviceState,
    calibration_request: bool,
    cfg: &ProtocolConfig,
    legacy_format: WireFormat,
) -> Vec<u8> {
    match cfg.driver {
        ProtocolDriver::Legacy => encode_query(seq, legacy_format),
        ProtocolDriver::Rs => encode_rs_slave(state, calibration_request, cfg),
    }
}

/// Encode an on-demand command (Send command). In RS mode this sends the slave packet.
pub fn encode_command(
    seq: u32,
    state: &DeviceState,
    calibration_request: bool,
    cfg: &ProtocolConfig,
    legacy_format: WireFormat,
) -> Vec<u8> {
    match cfg.driver {
        ProtocolDriver::Legacy => encode_state(seq, state, legacy_format),
        ProtocolDriver::Rs => encode_rs_slave(state, calibration_request, cfg),
    }
}

/// Encode an on-demand calibration request.
pub fn encode_calibration(
    seq: u32,
    state: &DeviceState,
    cfg: &ProtocolConfig,
    legacy_format: WireFormat,
) -> Vec<u8> {
    match cfg.driver {
        ProtocolDriver::Legacy => encode_calibrate(seq, legacy_format),
        ProtocolDriver::Rs => encode_rs_slave(state, true, cfg),
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
