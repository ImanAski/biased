//! Wire protocol between this app (master) and the bias controller MCU (slave).
//!
//! Protocol behavior is driven by the `[protocol]` section of the widget/config
//! TOML (`crate::config::ProtocolConfig`), which is editable in editor builds and
//! embedded/fixed in configuration-locked customer builds.
//!
//! Supported drivers:
//! - `legacy`: periodic QUERY poll and on-demand STATE/CAL frames, with selectable
//!   text/binary placeholder encoding (`WireFormat`) for development.
//! - `rs`: master-driven RSProtocol-style binary transactions (configurable
//!   framing, endianness and basic field scaling). The master tick sends a
//!   two-byte status read; on-demand writes carry the command state and are
//!   acknowledged by the slave. Inbound telemetry frames map into
//!   `TelemetryPatch`.
//!
//! RS wire framing:
//! ```text
//! master -> slave  read:   [rx_sync=0x77][rx_footer=0x23]
//! master -> slave  write:  [tx_header=0x71][8 x uint16][tx_footer]
//! slave -> master  ack:    [tx_header=0x71][rx_footer=0x23]
//! slave -> master  data:   [rx_sync=0x77][9 x uint16][rx_footer=0x23]
//! ```

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

/// Encode an RSProtocol-style write/command packet from the current state.
///
/// This is only sent on demand (Send command / Calibrate). The slave answers
/// with a two-byte acknowledge; the periodic tick sends a status read instead.
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

/// Build the two-byte status read request written on every RS master tick.
///
/// The slave answers with a telemetry frame of the same framing: sync byte,
/// the data bytes, and the terminating footer.
pub fn encode_rs_status_read(cfg: &ProtocolConfig) -> Vec<u8> {
    vec![cfg.rx_sync, cfg.rx_footer]
}

/// Try to decode one RSProtocol-style response from `buf`.
///
/// Two response shapes are recognised:
/// - `[tx_header][rx_footer]`: acknowledge of a write; consumed with an empty
///   patch so it never reaches the telemetry pipeline.
/// - `[rx_sync][biasVoltage u16][inputPower u16][outputPower u16][temperature u16]
///   [calibrationDone u16][rsv0 u16][rsv1 u16][rsv2 u16][rsv3 u16][rx_footer]`:
///   telemetry data (20 bytes, uint16 fields in configured endianness, default
///   big-endian). `rsv0..rsv3` are mapped to additional telemetry fields by
///   `[protocol.telemetrySlots]` (theta0 defaults to `rsv0`).
///
/// With a CRC configured, two CRC bytes follow the frame, as in the cpp. No CRC
/// check is performed by default (as in cpp).
///
/// Returns `(bytes_consumed, patch)`. An incomplete telemetry frame returns
/// `None` so the reader can wait for the rest; bad framing consumes a single
/// byte to allow resynchronization.
pub fn decode_rs_master(buf: &[u8], cfg: &ProtocolConfig) -> Option<(usize, TelemetryPatch)> {
    if buf.is_empty() {
        return None;
    }

    let base_len = 20usize; // header + 9*u16 + footer
    let crc_len = if cfg.crc == CrcKind::None { 0 } else { 2 };
    let frame_len = base_len + crc_len;

    if buf[0] == cfg.rx_sync {
        if buf.len() < frame_len {
            return None;
        }
        // Reject a mis-framed frame before publishing any of its fields.
        if buf[base_len - 1] != cfg.rx_footer {
            return Some((1, TelemetryPatch::default()));
        }

        if cfg.crc != CrcKind::None {
            let expected = crc16(cfg.crc, &buf[1..base_len]);
            let got = get_u16(&buf[base_len..base_len + 2], cfg.endianness)?;
            if expected != got {
                return Some((1, TelemetryPatch::default()));
            }
        }
    } else {
        // Write acknowledge: [tx_header][rx_footer]. Wait for the footer when
        // the two bytes arrive in separate reads; otherwise consume the ACK
        // silently and drop unexpected bytes one at a time to resynchronize.
        let consumed = if buf[0] == cfg.tx_header {
            match buf.get(1) {
                Some(byte) if *byte == cfg.rx_footer => 2,
                Some(_) => 1,
                None => return None,
            }
        } else {
            1
        };
        return Some((consumed, TelemetryPatch::default()));
    }

    let mut off = 1usize;
    let mut read = || -> Option<u16> {
        let v = get_u16(&buf[off..off + 2], cfg.endianness)?;
        off += 2;
        Some(v)
    };

    let mut fields = [0u16; 9];
    for field in &mut fields {
        *field = read()?;
    }
    let _footer = buf.get(off).copied();

    let scaled = |index: usize| fields[index] as f64 / cfg.telemetry_scale;
    // Fixed fields: bias, input, output, temperature, calibration flag.
    // The last four are reserved slots whose meaning is device-specific;
    // `[protocol.telemetrySlots]` decides which source reads each one.
    let rsv = |slot: crate::config::TelemetrySlot| scaled(5 + slot.index()) as f32;
    let slots = cfg.telemetry_slots;

    let patch = TelemetryPatch {
        theta0: Some(rsv(slots.theta0)),
        temp: Some(scaled(3) as f32),
        bias_voltage: Some(scaled(0) as f32),
        input_power: Some(scaled(1) as f32),
        output_power: Some(scaled(2) as f32),
        bias_current: Some(rsv(slots.bias_current)),
        supply_voltage: Some(rsv(slots.supply_voltage)),
        phase_error: Some(rsv(slots.phase_error)),
        lock_status: Some(if fields[4] != 0 { 1.0 } else { 0.0 }),
    };

    Some((frame_len, patch))
}

/// Encode the frame for the periodic master tick.
///
/// In RS mode the tick is a status read only. Writes (including calibration)
/// never happen on the clock: they are sent by `encode_command` /
/// `encode_calibration` when the user acts.
pub fn encode_periodic(
    seq: u32,
    _state: &DeviceState,
    cfg: &ProtocolConfig,
    legacy_format: WireFormat,
) -> Vec<u8> {
    match cfg.driver {
        ProtocolDriver::Legacy => encode_query(seq, legacy_format),
        ProtocolDriver::Rs => encode_rs_status_read(cfg),
    }
}

/// Encode the on-demand **Send command** write. In RS mode the
/// `calibrationRequest` field is always 0; use `encode_calibration` for 1.
pub fn encode_command(
    seq: u32,
    state: &DeviceState,
    cfg: &ProtocolConfig,
    legacy_format: WireFormat,
) -> Vec<u8> {
    match cfg.driver {
        ProtocolDriver::Legacy => encode_state(seq, state, legacy_format),
        ProtocolDriver::Rs => encode_rs_slave(state, false, cfg),
    }
}

/// Encode the on-demand **Calibrate** write. In RS mode this is the same
/// packet as `encode_command` with the `calibrationRequest` field set to 1.
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

    fn telemetry_frame(cfg: &ProtocolConfig) -> Vec<u8> {
        let mut frame = vec![cfg.rx_sync];
        for value in [330u16, 120, 80, 2450, 1, 123, 40, 1210, 150] {
            put_u16(&mut frame, value, cfg.endianness);
        }
        frame.push(cfg.rx_footer);
        frame
    }

    #[test]
    fn rs_tick_sends_only_a_status_read() {
        let cfg = ProtocolConfig::default();
        let frame = encode_periodic(9, &DeviceState::default(), &cfg, WireFormat::Text);

        assert_eq!(frame, vec![0x77, 0x23]);
        assert_eq!(encode_rs_status_read(&cfg), frame);
    }

    #[test]
    fn rs_send_and_calibrate_are_writes_with_distinct_flags() {
        let cfg = ProtocolConfig::default();
        let state = DeviceState::default();

        // Send command: write frame, calibrationRequest = 0.
        let send = encode_command(1, &state, &cfg, WireFormat::Text);
        assert_eq!(send[0], 0x71);
        assert_eq!(send.len(), 18); // header + 8*u16 + footer
        assert_eq!(send[17], cfg.tx_footer);
        assert_eq!(&send[9..11], &[0x00, 0x00]);

        // Calibrate: the same write frame, calibrationRequest = 1.
        let cal = encode_calibration(2, &state, &cfg, WireFormat::Text);
        assert_eq!(cal[0], 0x71);
        assert_eq!(cal.len(), send.len());
        assert_eq!(&cal[9..11], &[0x00, 0x01]);

        // Everything except the calibration flag is identical.
        let mut expected = send.clone();
        expected[10] = 0x01;
        assert_eq!(cal, expected);
    }

    #[test]
    fn decodes_new_telemetry_framing() {
        let cfg = ProtocolConfig::default();
        let frame = telemetry_frame(&cfg);
        let (consumed, patch) = decode_rs_master(&frame, &cfg).unwrap();

        assert_eq!(consumed, frame.len());
        assert_eq!(patch.bias_voltage, Some(3.3));
        assert_eq!(patch.input_power, Some(1.2));
        assert_eq!(patch.output_power, Some(0.8));
        assert_eq!(patch.temp, Some(24.5));
        assert_eq!(patch.lock_status, Some(1.0));
        assert_eq!(patch.bias_current, Some(0.4));
        assert_eq!(patch.supply_voltage, Some(12.1));
        assert_eq!(patch.phase_error, Some(1.5));
        assert_eq!(patch.theta0, Some(1.23));
    }

    #[test]
    fn decodes_theta_from_rsv0_by_default() {
        // Real frame from the bench: 22 a8 in rsv0 is theta (88.72 degrees).
        let frame = [
            0x77, 0x02, 0xa2, 0x00, 0x00, 0x01, 0x7f, 0x0c, 0x80, 0x00, 0x00, 0x22, 0xa8,
            0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x23,
        ];
        let cfg = ProtocolConfig::default();
        let (consumed, patch) = decode_rs_master(&frame, &cfg).unwrap();

        assert_eq!(consumed, frame.len());
        assert_eq!(patch.theta0, Some(88.72));
        assert_eq!(patch.bias_voltage, Some(6.74));
        assert_eq!(patch.output_power, Some(3.83));
        assert_eq!(patch.temp, Some(32.0));
    }

    #[test]
    fn honors_configured_telemetry_slots() {
        let cfg = ProtocolConfig {
            telemetry_slots: crate::config::TelemetrySlots {
                theta0: crate::config::TelemetrySlot::Rsv3,
                bias_current: crate::config::TelemetrySlot::Rsv0,
                supply_voltage: crate::config::TelemetrySlot::Rsv1,
                phase_error: crate::config::TelemetrySlot::Rsv2,
            },
            ..ProtocolConfig::default()
        };
        let frame = telemetry_frame(&cfg);
        let (_, patch) = decode_rs_master(&frame, &cfg).unwrap();

        assert_eq!(patch.theta0, Some(1.5));
        assert_eq!(patch.bias_current, Some(1.23));
        assert_eq!(patch.supply_voltage, Some(0.4));
        assert_eq!(patch.phase_error, Some(12.1));
    }

    #[test]
    fn consumes_write_ack_without_telemetry() {
        let cfg = ProtocolConfig::default();
        let (consumed, patch) = decode_rs_master(&[0x71, 0x23, 0x77], &cfg).unwrap();

        assert_eq!(consumed, 2);
        assert!(patch.is_empty());
    }

    #[test]
    fn waits_for_split_write_ack() {
        let cfg = ProtocolConfig::default();

        assert!(decode_rs_master(&[0x71], &cfg).is_none());

        let (consumed, patch) = decode_rs_master(&[0x71, 0x23], &cfg).unwrap();
        assert_eq!(consumed, 2);
        assert!(patch.is_empty());
    }

    #[test]
    fn waits_for_partial_and_rejects_bad_footer() {
        let cfg = ProtocolConfig::default();
        let frame = telemetry_frame(&cfg);

        assert!(decode_rs_master(&frame[..5], &cfg).is_none());

        let mut bad = frame;
        bad[19] = 0x00;
        let (consumed, patch) = decode_rs_master(&bad, &cfg).unwrap();
        assert_eq!(consumed, 1);
        assert!(patch.is_empty());
    }

    #[test]
    fn resynchronizes_after_noise() {
        let cfg = ProtocolConfig::default();
        let mut buf = vec![0x00];
        buf.extend_from_slice(&telemetry_frame(&cfg));

        let (consumed, patch) = decode_rs_master(&buf, &cfg).unwrap();
        assert_eq!(consumed, 1);
        assert!(patch.is_empty());
    }
}
