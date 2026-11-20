//! Tauri backend for the bias controller.
//!
//! This app is the **master** on the serial link and starts every transaction:
//! a background thread wakes up every configured tick (300 ms by default) and writes a two-byte status
//! read in RS mode (a legacy `QUERY` frame otherwise). Command writes (state
//! snapshots and calibration requests) are sent on demand when the user
//! explicitly triggers them; the frontend pushes state changes through the
//! `set_state` command so the latest snapshot is ready when a command is sent.
//!
//! A reader thread mirrors inbound bytes to the Packet Lab page and parses
//! telemetry frames/lines for the monitor cards, silently consuming write
//! acknowledges. The Packet Lab can also start a separate packet scheduler that
//! writes arbitrary payloads at a configurable interval, independently of the
//! master tick.

mod config;
mod locking;
mod protocol;
mod template;

use std::fs;
use std::io::{Read, Write};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use protocol::DeviceState;
use serde::{Deserialize, Serialize};
use serialport::{SerialPort, SerialPortType};
use tauri::{AppHandle, Emitter, Manager, State};

/// Read/write timeout for the serial handle. Short enough that the reader
/// thread notices a disconnect quickly, long enough for ordinary writes.
const IO_TIMEOUT: Duration = Duration::from_millis(100);

/// Longest inbound buffer/line we keep before dropping data to resync.
const MAX_RX_BUFFER: usize = 8192;

/// Shared, mutable state guarded by a single mutex.
struct Inner {
    port: Option<Box<dyn SerialPort>>,
    state: DeviceState,
    seq: u32,
    txs: u64,
    rxs: u64,
    rx_bytes: u64,
    port_name: Option<String>,
    baud: u32,
    last_error: Option<String>,
    /// Newest telemetry sample, merged from inbound frames/lines.
    telemetry: protocol::TelemetryPatch,
    /// Counter for raw Packet Lab writes, exposed to templates as `{seq}`.
    packet_seq: u64,
    /// Legacy outbound wire format for query/command frames (legacy driver).
    wire_format: protocol::WireFormat,
    /// Active protocol configuration (from widget/config TOML; locked in customer builds).
    protocol_cfg: config::ProtocolConfig,
    reader_stop: Option<Arc<AtomicBool>>,
    reader_handle: Option<JoinHandle<()>>,
    sender_stop: Option<Arc<AtomicBool>>,
    sender: SenderState,
}

impl Inner {
    fn new() -> Self {
        Self {
            port: None,
            state: DeviceState::default(),
            seq: 0,
            txs: 0,
            rxs: 0,
            rx_bytes: 0,
            port_name: None,
            baud: 115_200,
            last_error: None,
            telemetry: protocol::TelemetryPatch::default(),
            packet_seq: 0,
            wire_format: protocol::WireFormat::Text,
            protocol_cfg: config::ProtocolConfig::default(),
            reader_stop: None,
            reader_handle: None,
            sender_stop: None,
            sender: SenderState::stopped(),
        }
    }

    fn status(&self) -> Status {
        Status {
            connected: self.port.is_some(),
            port_name: self.port_name.clone(),
            baud: self.baud,
            txs: self.txs,
            rxs: self.rxs,
            rx_bytes: self.rx_bytes,
            last_error: self.last_error.clone(),
            tick_ms: self.protocol_cfg.tick_ms,
        }
    }

    /// Stop the reader thread; the caller joins the handle outside the lock.
    fn stop_reader(&mut self) -> Option<JoinHandle<()>> {
        if let Some(stop) = self.reader_stop.take() {
            stop.store(true, Ordering::SeqCst);
        }
        self.reader_handle.take()
    }

    fn stop_sender(&mut self) {
        if let Some(stop) = self.sender_stop.take() {
            stop.store(true, Ordering::SeqCst);
        }
        self.sender.generation = self.sender.generation.wrapping_add(1);
        self.sender.running = false;
        self.sender.next_at = None;
    }

    /// Write a frame immediately, outside the periodic tick. A failed write
    /// drops the link so the UI can surface it.
    fn write_immediate(&mut self, frame: &[u8]) -> Result<(), String> {
        if self.port.is_none() {
            return Err("not connected".into());
        }

        let result = match self.port.as_mut() {
            Some(port) => port.write_all(frame).and_then(|()| port.flush()),
            None => return Err("not connected".into()),
        };

        match result {
            Ok(()) => {
                self.txs += 1;
                self.last_error = None;
                Ok(())
            }
            Err(err) => {
                self.port = None;
                if let Some(stop) = &self.reader_stop {
                    stop.store(true, Ordering::SeqCst);
                }
                let message = format!("write failed: {err}");
                self.last_error = Some(message.clone());
                Err(message)
            }
        }
    }
}

/// Owns the shared state and starts/stops the master clock.
pub struct SerialManager {
    shared: Arc<Mutex<Inner>>,
}

impl SerialManager {
    fn new() -> Self {
        Self {
            shared: Arc::new(Mutex::new(Inner::new())),
        }
    }

    fn lock(&self) -> MutexGuard<'_, Inner> {
        // Recover the guard if a previous holder panicked; the state is still
        // structurally valid because every mutation is a simple field write.
        self.shared
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    /// Spawn the master clock loop. It runs for the lifetime of the process.
    fn spawn_tick(&self, app: AppHandle) {
        let shared = Arc::clone(&self.shared);

        thread::spawn(move || {
            let mut next = Instant::now();
            let mut primed = false;

            loop {
                let now = Instant::now();
                if primed {
                    if next > now {
                        thread::sleep(next - now);
                    } else {
                        next = now;
                    }
                }

                let mut inner = shared.lock().unwrap_or_else(|p| p.into_inner());
                // The interval is read every tick, so a config reload takes
                // effect without restarting the clock. The first tick is
                // scheduled from the loaded config instead of a fixed 300 ms.
                let tick = Duration::from_millis(inner.protocol_cfg.tick_ms.max(10));
                next = if primed {
                    next + tick
                } else {
                    Instant::now() + tick
                };
                primed = true;

                if inner.port.is_none() || !inner.protocol_cfg.periodic_send {
                    continue;
                }

                inner.seq = inner.seq.wrapping_add(1);
                // A tick is a status read in RS mode; no write state is involved.
                let frame = protocol::encode_periodic(
                    inner.seq,
                    &inner.state,
                    &inner.protocol_cfg,
                    inner.wire_format,
                );

                let result = match inner.port.as_mut() {
                    Some(port) => port.write_all(&frame).and_then(|()| port.flush()),
                    None => continue,
                };

                match result {
                    Ok(()) => {
                        inner.txs += 1;
                        inner.last_error = None;
                        let _ = app.emit(
                            "serial://tx",
                            TxEvent {
                                seq: inner.seq,
                                txs: inner.txs,
                            },
                        );
                    }
                    Err(err) => {
                        inner.port = None;
                        if let Some(stop) = &inner.reader_stop {
                            stop.store(true, Ordering::SeqCst);
                        }
                        inner.last_error = Some(format!("write failed: {err}"));
                        let status = inner.status();
                        let _ = app.emit("serial://status", status);
                    }
                }
            }
        });
    }
}

/* -------------------------------------------------------------------------- */
/* Events & payloads                                                          */
/* -------------------------------------------------------------------------- */

/// Connection snapshot sent to the frontend.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct Status {
    connected: bool,
    port_name: Option<String>,
    baud: u32,
    txs: u64,
    rxs: u64,
    rx_bytes: u64,
    last_error: Option<String>,
    tick_ms: u64,
}

/// Emitted after every successful periodic frame so the UI can show a heartbeat.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct TxEvent {
    seq: u32,
    txs: u64,
}

/// One chunk read from the serial port (or injected by the Packet Lab).
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct RxEvent {
    at: u64,
    bytes: Vec<u8>,
    /// Chunks received since the last connect.
    seq: u64,
    /// Bytes received since the last connect.
    total_bytes: u64,
    synthetic: bool,
}

/// Public scheduler snapshot for the UI.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SenderState {
    running: bool,
    /// Bumped on every start/stop so late events from an old schedule can be
    /// ignored by the UI.
    generation: u64,
    interval_ms: u64,
    packet_count: usize,
    labels: Vec<String>,
    sent: u64,
    /// 0 means "run until stopped".
    limit: u64,
    /// Epoch milliseconds of the next scheduled write.
    next_at: Option<u64>,
    last_error: Option<String>,
}

impl SenderState {
    fn stopped() -> Self {
        Self {
            running: false,
            generation: 0,
            interval_ms: 300,
            packet_count: 0,
            labels: Vec::new(),
            sent: 0,
            limit: 0,
            next_at: None,
            last_error: None,
        }
    }
}

/// Emitted after every raw packet written by the scheduler or the manual send.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct PacketSent {
    at: u64,
    label: String,
    index: usize,
    /// Counter value carried by this write, also exposed to templates as
    /// `{seq}`. The editor uses it to preview the next packet accurately.
    seq: u64,
    bytes: usize,
    sent: u64,
    txs: u64,
    interval_ms: u64,
}

/// A serial port the user can pick in the UI.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct PortInfo {
    name: String,
    kind: String,
    manufacturer: Option<String>,
    product: Option<String>,
}

/// Result of loading a widget file. Parse/validation problems are reported in
/// `errors` while `config` stays `null`, so the UI can keep the last good
/// layout visible.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct WidgetConfigEnvelope {
    path: String,
    config: Option<config::WidgetConfig>,
    errors: Vec<String>,
}

/// Build metadata for the settings page.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct BuildInfo {
    /// The executable carries its own layout and refuses to load another one.
    locked: bool,
    /// Developer tools (Packet Lab) are available only in editable builds.
    packet_lab: bool,
    /// Customer exports are only meaningful from a production build.
    debug: bool,
    /// Where the embedded layout came from, when this is a locked build.
    config_source: Option<String>,
    /// Suggested file name for a new export.
    export_name: String,
}

/// Result of writing a configuration-locked copy of the app.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct ExportResult {
    path: String,
    bytes: u64,
}

/// Immutable lock status, resolved once at startup.
struct BuildLock {
    locked: bool,
    source: String,
    config: Option<String>,
    error: Option<String>,
}

impl BuildLock {
    fn load() -> Self {
        let Some(path) = locking::executable_path() else {
            return Self::unlocked();
        };
        let source = path.display().to_string();

        match locking::read_from(&path) {
            Some(Ok(config)) => Self {
                locked: true,
                source,
                config: Some(config),
                error: None,
            },
            // A malformed trailer still means "customer build": do not fall
            // back to an editable file the customer could change.
            Some(Err(error)) => Self {
                locked: true,
                source,
                config: None,
                error: Some(error),
            },
            None => Self::unlocked(),
        }
    }

    fn unlocked() -> Self {
        Self {
            locked: false,
            source: "embedded configuration".into(),
            config: None,
            error: None,
        }
    }
}

/// The Packet Lab is an editor tool and is deliberately unavailable in a
/// configuration-locked customer build.
fn ensure_packet_lab(lock: &BuildLock) -> Result<(), String> {
    if lock.locked {
        Err("The Packet Lab is not available in a customer build.".into())
    } else {
        Ok(())
    }
}

/// File name shown in the save dialog for a new customer build.
fn export_name() -> String {
    let source = locking::executable_path();
    let base = source
        .as_deref()
        .and_then(|path| {
            path.file_stem()
                .map(|name| name.to_string_lossy().into_owned())
        })
        .unwrap_or_else(|| "tauri-biased".into());
    let extension = source
        .as_deref()
        .and_then(|path| {
            path.extension()
                .map(|extension| format!(".{}", extension.to_string_lossy()))
        })
        .unwrap_or_else(|| std::env::consts::EXE_SUFFIX.into());

    format!("{base}-customer{extension}")
}

/* -------------------------------------------------------------------------- */
/* Packet payloads                                                            */
/* -------------------------------------------------------------------------- */

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PacketEncoding {
    #[default]
    Text,
    Hex,
    /// UTF-8 text sent verbatim, with no escape processing.
    Raw,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PacketSpec {
    #[serde(default)]
    pub label: String,
    #[serde(default)]
    pub encoding: PacketEncoding,
    pub payload: String,
}

impl PacketSpec {
    fn label_or(&self, index: usize) -> String {
        let label = self.label.trim();
        if label.is_empty() {
            format!("packet {}", index + 1)
        } else {
            label.to_string()
        }
    }
}

/// Decode a text payload, supporting the escapes that are useful on a serial
/// line: `\n`, `\r`, `\t`, `\0`, `\\` and `\xNN`.
fn decode_text(payload: &str) -> Result<Vec<u8>, String> {
    let mut out = Vec::with_capacity(payload.len());
    let mut chars = payload.chars();

    while let Some(ch) = chars.next() {
        if ch != '\\' {
            let mut buf = [0u8; 4];
            out.extend_from_slice(ch.encode_utf8(&mut buf).as_bytes());
            continue;
        }

        match chars.next() {
            Some('n') => out.push(b'\n'),
            Some('r') => out.push(b'\r'),
            Some('t') => out.push(b'\t'),
            Some('0') => out.push(0),
            Some('\\') => out.push(b'\\'),
            Some('x') | Some('X') => {
                let hi = chars.next().ok_or("incomplete \\x escape")?;
                let lo = chars.next().ok_or("incomplete \\x escape")?;
                let pair = [hi, lo].iter().collect::<String>();
                let byte = u8::from_str_radix(&pair, 16)
                    .map_err(|_| format!("invalid hex escape `\\x{pair}`"))?;
                out.push(byte);
            }
            Some(other) => {
                out.push(b'\\');
                let mut buf = [0u8; 4];
                out.extend_from_slice(other.encode_utf8(&mut buf).as_bytes());
            }
            None => out.push(b'\\'),
        }
    }

    Ok(out)
}

/// Decode hexadecimal, tolerating spaces, commas, underscores, dashes and
/// `0x` prefixes so pasted dumps work without editing.
fn decode_hex(payload: &str) -> Result<Vec<u8>, String> {
    let mut digits = Vec::new();
    let mut chars = payload.chars().peekable();

    while let Some(ch) = chars.next() {
        if ch.is_whitespace() || matches!(ch, ',' | '_' | ':' | '-') {
            continue;
        }
        if ch == '0' {
            if let Some(next) = chars.peek() {
                if matches!(next, 'x' | 'X') {
                    chars.next();
                    continue;
                }
            }
        }
        if ch.is_ascii_hexdigit() {
            digits.push(ch);
        } else {
            return Err(format!("unexpected `{ch}` in hex payload"));
        }
    }

    if digits.len() % 2 != 0 {
        return Err("hex payload needs an even number of digits".into());
    }

    let mut out = Vec::with_capacity(digits.len() / 2);
    for pair in digits.chunks(2) {
        let text: String = pair.iter().collect();
        let byte =
            u8::from_str_radix(&text, 16).map_err(|_| format!("invalid hex byte `{text}`"))?;
        out.push(byte);
    }

    Ok(out)
}

/// Expand a packet template against the live state and decode it.
///
/// Expansion happens at send time so a scheduled packet can follow telemetry
/// and device state instead of freezing the values when the schedule starts.
fn render_packet(
    packet: &PacketSpec,
    inner: &Inner,
    seq: u64,
    sent: u64,
    index: usize,
) -> Result<Vec<u8>, String> {
    let label = packet.label_or(index);
    let vars = template::TemplateVars {
        seq,
        sent,
        index,
        label: &label,
        now_ms: epoch_ms(),
        state: &inner.state,
        telemetry: &inner.telemetry,
    };
    let payload =
        template::expand(&packet.payload, &vars).map_err(|err| format!("{label}: {err}"))?;

    let bytes = match packet.encoding {
        PacketEncoding::Text => decode_text(&payload),
        PacketEncoding::Hex => decode_hex(&payload),
        PacketEncoding::Raw => Ok(payload.into_bytes()),
    }
    .map_err(|err| format!("{label}: {err}"))?;

    if bytes.is_empty() {
        return Err(format!("{label}: payload is empty"));
    }
    if bytes.len() > 8192 {
        return Err(format!("{label}: payload is larger than 8 KiB"));
    }

    Ok(bytes)
}

/* -------------------------------------------------------------------------- */
/* Inbound path                                                               */
/* -------------------------------------------------------------------------- */

fn epoch_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|elapsed| elapsed.as_millis() as u64)
        .unwrap_or(0)
}

fn spawn_reader(
    app: AppHandle,
    shared: Arc<Mutex<Inner>>,
    mut port: Box<dyn SerialPort>,
    stop: Arc<AtomicBool>,
) -> JoinHandle<()> {
    thread::spawn(move || {
        let mut buf = [0u8; 2048];
        let mut rxbuf = Vec::<u8>::new();

        loop {
            if stop.load(Ordering::Relaxed) {
                break;
            }

            match port.read(&mut buf) {
                Ok(0) => thread::sleep(Duration::from_millis(10)),
                Ok(n) => {
                    let chunk = &buf[..n];
                    emit_rx(&app, &shared, chunk, false);

                    let driver = {
                        let inner = shared.lock().unwrap_or_else(|p| p.into_inner());
                        inner.protocol_cfg.driver
                    };

                    rxbuf.extend_from_slice(chunk);
                    match driver {
                        config::ProtocolDriver::Legacy => {
                            while let Some(position) = rxbuf.iter().position(|byte| *byte == b'\n') {
                                let raw: Vec<u8> = rxbuf.drain(..=position).collect();
                                parse_and_emit_telemetry(&app, &shared, &raw);
                            }
                            if rxbuf.len() > MAX_RX_BUFFER {
                                rxbuf.clear();
                            }
                        }
                        config::ProtocolDriver::Rs => {
                            loop {
                                let cfg = {
                                    let inner = shared.lock().unwrap_or_else(|p| p.into_inner());
                                    inner.protocol_cfg.clone()
                                };
                                match protocol::decode_rs_master(&rxbuf, &cfg) {
                                    None => break,
                                    Some((consumed, patch)) => {
                                        if consumed == 0 {
                                            break;
                                        }
                                        rxbuf.drain(..consumed.min(rxbuf.len()));
                                        if !patch.is_empty() {
                                            {
                                                let mut inner =
                                                    shared.lock().unwrap_or_else(|p| p.into_inner());
                                                inner.telemetry.merge(&patch);
                                            }
                                            let _ = app.emit("telemetry://state", patch);
                                        }
                                    }
                                }
                                if rxbuf.is_empty() {
                                    break;
                                }
                            }
                            if rxbuf.len() > MAX_RX_BUFFER {
                                rxbuf.clear();
                            }
                        }
                    }
                }
                Err(err) => match err.kind() {
                    std::io::ErrorKind::TimedOut | std::io::ErrorKind::WouldBlock => continue,
                    _ => {
                        let status = {
                            let mut inner = shared.lock().unwrap_or_else(|p| p.into_inner());
                            inner.port = None;
                            inner.last_error = Some(format!("read failed: {err}"));
                            inner.status()
                        };
                        let _ = app.emit("serial://status", status);
                        stop.store(true, Ordering::SeqCst);
                        break;
                    }
                },
            }
        }
    })
}

fn emit_rx(app: &AppHandle, shared: &Arc<Mutex<Inner>>, bytes: &[u8], synthetic: bool) {
    let (seq, total_bytes) = {
        let mut inner = shared.lock().unwrap_or_else(|p| p.into_inner());
        inner.rxs += 1;
        inner.rx_bytes = inner.rx_bytes.saturating_add(bytes.len() as u64);
        (inner.rxs, inner.rx_bytes)
    };

    let _ = app.emit(
        "serial://rx",
        RxEvent {
            at: epoch_ms(),
            bytes: bytes.to_vec(),
            seq,
            total_bytes,
            synthetic,
        },
    );
}

fn parse_and_emit_telemetry(app: &AppHandle, shared: &Arc<Mutex<Inner>>, raw: &[u8]) {
    let text = String::from_utf8_lossy(raw);
    if let Some(patch) = protocol::parse_telemetry_line(&text) {
        {
            let mut inner = shared.lock().unwrap_or_else(|p| p.into_inner());
            inner.telemetry.merge(&patch);
        }
        let _ = app.emit("telemetry://state", patch);
    }
}

/// Feed a complete payload through the inbound path. Used by the Packet Lab's
/// "inject" button, which lets the UI be exercised without hardware.
fn ingest_bytes(app: &AppHandle, shared: &Arc<Mutex<Inner>>, bytes: &[u8], synthetic: bool) {
    emit_rx(app, shared, bytes, synthetic);

    let driver = {
        let inner = shared.lock().unwrap_or_else(|p| p.into_inner());
        inner.protocol_cfg.driver
    };

    match driver {
        config::ProtocolDriver::Legacy => {
            for raw in bytes.split_inclusive(|byte| *byte == b'\n') {
                if !raw.is_empty() {
                    parse_and_emit_telemetry(app, shared, raw);
                }
            }
        }
        config::ProtocolDriver::Rs => {
            let mut buf = bytes.to_vec();
            loop {
                let cfg = {
                    let inner = shared.lock().unwrap_or_else(|p| p.into_inner());
                    inner.protocol_cfg.clone()
                };
                match protocol::decode_rs_master(&buf, &cfg) {
                    None => break,
                    Some((consumed, patch)) => {
                        if consumed == 0 {
                            break;
                        }
                        buf.drain(..consumed.min(buf.len()));
                        if !patch.is_empty() {
                            {
                                let mut inner = shared.lock().unwrap_or_else(|p| p.into_inner());
                                inner.telemetry.merge(&patch);
                            }
                            let _ = app.emit("telemetry://state", patch);
                        }
                    }
                }
                if buf.is_empty() {
                    break;
                }
            }
        }
    }
}

/* -------------------------------------------------------------------------- */
/* Packet scheduler                                                           */
/* -------------------------------------------------------------------------- */

fn sleep_until(deadline: Instant, stop: &AtomicBool) {
    loop {
        if stop.load(Ordering::Relaxed) {
            return;
        }
        let now = Instant::now();
        if now >= deadline {
            return;
        }
        thread::sleep((deadline - now).min(Duration::from_millis(20)));
    }
}

fn spawn_sender(
    app: AppHandle,
    shared: Arc<Mutex<Inner>>,
    stop: Arc<AtomicBool>,
    interval_ms: u64,
    packets: Vec<PacketSpec>,
    limit: u64,
) {
    thread::spawn(move || {
        let interval = Duration::from_millis(interval_ms);
        let mut next = Instant::now() + interval;
        let mut index = 0usize;

        loop {
            sleep_until(next, &stop);
            if stop.load(Ordering::Relaxed) {
                return;
            }

            let send_index = index;
            let label = packets[index].label_or(index);

            enum Step {
                Sent(SenderState, u64, usize, u64),
                Failed(SenderState, Option<Status>),
            }

            let step = {
                let mut inner = shared.lock().unwrap_or_else(|p| p.into_inner());
                if inner.port.is_none() {
                    inner.sender.running = false;
                    inner.sender.next_at = None;
                    inner.sender.last_error = Some("not connected".into());
                    inner.sender_stop = None;
                    inner.last_error = Some("not connected".into());
                    Step::Failed(inner.sender.clone(), Some(inner.status()))
                } else {
                    // Render under the lock so the template sees one consistent
                    // snapshot and `{seq}` stays unique across writers.
                    let seq = inner.packet_seq.wrapping_add(1);
                    let sent = inner.sender.sent.saturating_add(1);
                    match render_packet(&packets[index], &inner, seq, sent, index) {
                        Err(err) => {
                            inner.sender.running = false;
                            inner.sender.next_at = None;
                            inner.sender.last_error = Some(err);
                            inner.sender_stop = None;
                            Step::Failed(inner.sender.clone(), None)
                        }
                        Ok(payload) => match inner.write_immediate(&payload) {
                            Ok(()) => {
                                inner.packet_seq = seq;
                                inner.sender.sent += 1;
                                inner.sender.next_at = Some(epoch_ms().saturating_add(interval_ms));
                                Step::Sent(inner.sender.clone(), inner.txs, payload.len(), seq)
                            }
                            Err(err) => {
                                inner.sender.running = false;
                                inner.sender.next_at = None;
                                inner.sender.last_error = Some(err);
                                inner.sender_stop = None;
                                Step::Failed(inner.sender.clone(), Some(inner.status()))
                            }
                        },
                    }
                }
            };

            match step {
                Step::Sent(state, txs, bytes, seq) => {
                    let _ = app.emit(
                        "serial://packet",
                        PacketSent {
                            at: epoch_ms(),
                            label,
                            index: send_index,
                            seq,
                            bytes,
                            sent: state.sent,
                            txs,
                            interval_ms,
                        },
                    );

                    if limit > 0 && state.sent >= limit {
                        let final_state = {
                            let mut inner = shared.lock().unwrap_or_else(|p| p.into_inner());
                            inner.sender.running = false;
                            inner.sender.next_at = None;
                            inner.sender_stop = None;
                            inner.sender.clone()
                        };
                        let _ = app.emit("serial://sender", final_state);
                        return;
                    }

                    let _ = app.emit("serial://sender", state);
                    index = (index + 1) % packets.len();
                    next += interval;
                    if next < Instant::now() {
                        next = Instant::now() + interval;
                    }
                }
                Step::Failed(state, status) => {
                    let _ = app.emit("serial://sender", state);
                    if let Some(status) = status {
                        let _ = app.emit("serial://status", status);
                    }
                    return;
                }
            }
        }
    });
}

/* -------------------------------------------------------------------------- */
/* Commands                                                                   */
/* -------------------------------------------------------------------------- */

#[tauri::command]
fn list_ports() -> Result<Vec<PortInfo>, String> {
    let ports = serialport::available_ports().map_err(|e| e.to_string())?;

    Ok(ports
        .into_iter()
        .map(|port| {
            let (kind, manufacturer, product) = match port.port_type {
                SerialPortType::UsbPort(usb) => (
                    format!("usb {:04x}:{:04x}", usb.vid, usb.pid),
                    usb.manufacturer,
                    usb.product,
                ),
                SerialPortType::BluetoothPort => ("bluetooth".into(), None, None),
                SerialPortType::PciPort => ("pci".into(), None, None),
                SerialPortType::Unknown => ("unknown".into(), None, None),
            };

            PortInfo {
                name: port.port_name,
                kind,
                manufacturer,
                product,
            }
        })
        .collect())
}

#[tauri::command]
fn connect(
    port: String,
    baud: u32,
    app: AppHandle,
    manager: State<'_, SerialManager>,
) -> Result<Status, String> {
    // Stop the old reader before opening the new handle. On Windows a cloned
    // handle keeps the COM port busy until its thread exits.
    let old_reader = {
        let mut inner = manager.lock();
        let handle = inner.stop_reader();
        inner.stop_sender();
        inner.port = None;
        handle
    };
    if let Some(handle) = old_reader {
        let _ = handle.join();
    }

    let opened = match serialport::new(&port, baud).timeout(IO_TIMEOUT).open() {
        Ok(port) => port,
        Err(err) => {
            let message = format!("failed to open {port} @ {baud}: {err}");
            let status = {
                let mut inner = manager.lock();
                inner.last_error = Some(message.clone());
                inner.status()
            };
            let _ = app.emit("serial://status", status);
            return Err(message);
        }
    };

    let reader_port = match opened.try_clone() {
        Ok(port) => port,
        Err(err) => {
            return Err(format!("failed to clone {port}: {err}"));
        }
    };

    let stop = Arc::new(AtomicBool::new(false));

    {
        let mut inner = manager.lock();
        inner.port = Some(opened);
        inner.port_name = Some(port);
        inner.baud = baud;
        inner.seq = 0;
        inner.txs = 0;
        inner.rxs = 0;
        inner.rx_bytes = 0;
        inner.last_error = None;
        inner.reader_stop = Some(Arc::clone(&stop));
    }

    let handle = spawn_reader(app.clone(), Arc::clone(&manager.shared), reader_port, stop);

    let status = {
        let mut inner = manager.lock();
        inner.reader_handle = Some(handle);
        let status = inner.status();
        let _ = app.emit("serial://status", status.clone());
        let _ = app.emit("serial://sender", inner.sender.clone());
        status
    };

    Ok(status)
}

#[tauri::command]
fn disconnect(app: AppHandle, manager: State<'_, SerialManager>) -> Result<Status, String> {
    let (status, sender_state, reader) = {
        let mut inner = manager.lock();
        let reader = inner.stop_reader();
        inner.stop_sender();

        if let Some(mut port) = inner.port.take() {
            // Best effort: flush whatever is queued before closing.
            let _ = port.flush();
            inner.port_name = None;
        }

        (inner.status(), inner.sender.clone(), reader)
    };

    if let Some(handle) = reader {
        let _ = handle.join();
    }

    let _ = app.emit("serial://status", status.clone());
    let _ = app.emit("serial://sender", sender_state);
    Ok(status)
}

/// Replace the command snapshot that will be sent when the user explicitly
/// triggers a command transmission.
#[tauri::command]
fn set_state(state: DeviceState, manager: State<'_, SerialManager>) -> Result<(), String> {
    manager.lock().state = state;
    Ok(())
}

#[tauri::command]
fn get_status(manager: State<'_, SerialManager>) -> Result<Status, String> {
    Ok(manager.lock().status())
}

#[tauri::command]
fn get_wire_format(manager: State<'_, SerialManager>) -> protocol::WireFormat {
    manager.lock().wire_format
}

#[tauri::command]
fn set_wire_format(
    format: protocol::WireFormat,
    manager: State<'_, SerialManager>,
    lock: State<'_, BuildLock>,
) -> Result<(), String> {
    if lock.locked {
        return Err("Wire format cannot be changed in a customer build.".into());
    }
    manager.lock().wire_format = format;
    Ok(())
}

/// Send the current command snapshot immediately, outside the periodic tick.
#[tauri::command]
fn send_now(app: AppHandle, manager: State<'_, SerialManager>) -> Result<Status, String> {
    let mut inner = manager.lock();

    inner.seq = inner.seq.wrapping_add(1);
    // Send command is a write with calibrationRequest = 0 in RS mode.
    let frame = protocol::encode_command(
        inner.seq,
        &inner.state,
        &inner.protocol_cfg,
        inner.wire_format,
    );
    if let Err(err) = inner.write_immediate(&frame) {
        let status = inner.status();
        let _ = app.emit("serial://status", status);
        return Err(err);
    }

    let seq = inner.seq;
    let txs = inner.txs;
    let status = inner.status();
    drop(inner);

    let _ = app.emit("serial://tx", TxEvent { seq, txs });
    let _ = app.emit("serial://status", status.clone());
    Ok(status)
}

/// Ask the MCU to run its calibration routine by sending a calibration command/request.
#[tauri::command]
fn calibrate(app: AppHandle, manager: State<'_, SerialManager>) -> Result<Status, String> {
    let mut inner = manager.lock();

    inner.seq = inner.seq.wrapping_add(1);
    // Calibrate is the same write, but with calibrationRequest = 1 in RS mode.
    let frame = protocol::encode_calibration(
        inner.seq,
        &inner.state,
        &inner.protocol_cfg,
        inner.wire_format,
    );
    if let Err(err) = inner.write_immediate(&frame) {
        let status = inner.status();
        let _ = app.emit("serial://status", status);
        return Err(err);
    }

    let seq = inner.seq;
    let txs = inner.txs;
    let status = inner.status();
    drop(inner);

    let _ = app.emit("serial://tx", TxEvent { seq, txs });
    let _ = app.emit("serial://status", status.clone());
    Ok(status)
}

/// Write exported text (logs, config copies) to a path chosen in a dialog.
#[tauri::command]
fn write_text_file(path: String, contents: String) -> Result<(), String> {
    fs::write(&path, contents).map_err(|e| format!("failed to write {path}: {e}"))
}

#[tauri::command]
fn read_text_file(path: String) -> Result<String, String> {
    fs::read_to_string(&path).map_err(|e| format!("failed to read {path}: {e}"))
}

fn default_widget_path(app: &AppHandle) -> Result<PathBuf, String> {
    let dir = app
        .path()
        .app_config_dir()
        .map_err(|e| format!("cannot resolve the app config directory: {e}"))?;
    fs::create_dir_all(&dir).map_err(|e| format!("cannot create {}: {e}", dir.display()))?;

    let path = dir.join("widgets.toml");
    if !path.exists() {
        fs::write(&path, config::DEFAULT_TOML)
            .map_err(|e| format!("cannot seed {}: {e}", path.display()))?;
    }

    Ok(path)
}

/// Load a widget layout from `path`, or from the app config directory when no
/// path is supplied. The default file is created on first use.
///
/// A locked customer build ignores `path` entirely and always reports the
/// layout embedded in its own executable.
#[tauri::command]
fn load_widget_config(
    path: Option<String>,
    app: AppHandle,
    manager: State<'_, SerialManager>,
    lock: State<'_, BuildLock>,
) -> Result<WidgetConfigEnvelope, String> {
    if lock.locked {
        let source = lock.source.clone();
        return Ok(match lock.config.as_deref() {
            Some(text) => match config::parse(text) {
                Ok(config) => {
                    manager.lock().protocol_cfg = config.protocol.clone();
                    WidgetConfigEnvelope {
                        path: source,
                        config: Some(config),
                        errors: Vec::new(),
                    }
                }
                Err(errors) => WidgetConfigEnvelope {
                    path: source,
                    config: None,
                    errors,
                },
            },
            None => WidgetConfigEnvelope {
                path: source,
                config: None,
                errors: vec![
                    lock.error
                        .clone()
                        .unwrap_or_else(|| "embedded configuration is unavailable".into()),
                ],
            },
        });
    }

    let path = match path.map(|value| value.trim().to_string()) {
        Some(value) if !value.is_empty() => PathBuf::from(value),
        _ => default_widget_path(&app)?,
    };

    let source =
        fs::read_to_string(&path).map_err(|e| format!("failed to read {}: {e}", path.display()))?;

    match config::parse(&source) {
        Ok(config) => {
            manager.lock().protocol_cfg = config.protocol.clone();
            Ok(WidgetConfigEnvelope {
                path: path.display().to_string(),
                config: Some(config),
                errors: Vec::new(),
            })
        }
        Err(errors) => Ok(WidgetConfigEnvelope {
            path: path.display().to_string(),
            config: None,
            errors,
        }),
    }
}

#[tauri::command]
fn get_build_info(lock: State<'_, BuildLock>) -> BuildInfo {
    BuildInfo {
        locked: lock.locked,
        packet_lab: !lock.locked,
        debug: cfg!(debug_assertions),
        config_source: lock.locked.then(|| lock.source.clone()),
        export_name: export_name(),
    }
}

/// Write a standalone copy of this executable with `config_path`'s layout
/// embedded as an uneditable configuration.
#[tauri::command]
fn export_locked_build(
    target: String,
    config_path: Option<String>,
    app: AppHandle,
    lock: State<'_, BuildLock>,
) -> Result<ExportResult, String> {
    if lock.locked {
        return Err("This executable is already a configuration-locked customer build.".into());
    }
    if cfg!(debug_assertions) {
        return Err(
            "Customer builds can only be exported from a production build. Run \
             `bun run tauri build` and launch the built app instead."
                .into(),
        );
    }

    let target = target.trim();
    if target.is_empty() {
        return Err("Choose a destination for the customer build.".into());
    }
    let target = PathBuf::from(target);

    let source_path = match config_path.map(|value| value.trim().to_string()) {
        Some(value) if !value.is_empty() && value != "built-in preview" => PathBuf::from(value),
        _ => default_widget_path(&app)?,
    };
    let text = fs::read_to_string(&source_path)
        .map_err(|error| format!("Failed to read {}: {error}", source_path.display()))?;

    // Never distribute a build whose layout the app itself would reject.
    config::parse(&text).map_err(|errors| {
        format!(
            "The active widget layout is invalid; fix it before exporting:\n{}",
            errors.join("\n")
        )
    })?;

    let executable = locking::executable_path()
        .ok_or_else(|| "Cannot locate the running executable.".to_string())?;
    let bytes = locking::write_copy(&executable, &target, &text)?;

    Ok(ExportResult {
        path: target.display().to_string(),
        bytes,
    })
}

/// One-shot write of a composed packet. Useful for testing and for commands
/// that do not need a schedule.
#[tauri::command]
fn send_packet(
    packet: PacketSpec,
    app: AppHandle,
    manager: State<'_, SerialManager>,
    lock: State<'_, BuildLock>,
) -> Result<u64, String> {
    ensure_packet_lab(lock.inner())?;

    let outcome: Result<(usize, u64, u64, Status), (String, Option<Status>)> = {
        let mut inner = manager.lock();
        // `{seq}` counts raw packets, so this one is the next value. Only commit
        // it after a successful write; a failed attempt reuses the number.
        let seq = inner.packet_seq.wrapping_add(1);
        match render_packet(&packet, &inner, seq, 1, 0) {
            Err(err) => Err((err, None)),
            Ok(bytes) => match inner.write_immediate(&bytes) {
                Ok(()) => {
                    inner.packet_seq = seq;
                    Ok((bytes.len(), inner.txs, seq, inner.status()))
                }
                Err(err) => Err((err, Some(inner.status()))),
            },
        }
    };

    let (bytes, txs, seq, status) = match outcome {
        Ok(result) => result,
        Err((err, status)) => {
            if let Some(status) = status {
                let _ = app.emit("serial://status", status);
            }
            return Err(err);
        }
    };

    let _ = app.emit(
        "serial://packet",
        PacketSent {
            at: epoch_ms(),
            label: packet.label_or(0),
            index: 0,
            seq,
            bytes,
            sent: 1,
            txs,
            interval_ms: 0,
        },
    );
    let _ = app.emit("serial://status", status);

    Ok(bytes as u64)
}

#[tauri::command]
fn start_packet_sender(
    interval_ms: u64,
    packets: Vec<PacketSpec>,
    limit: u64,
    app: AppHandle,
    manager: State<'_, SerialManager>,
    lock: State<'_, BuildLock>,
) -> Result<SenderState, String> {
    ensure_packet_lab(lock.inner())?;

    if packets.is_empty() {
        return Err("add at least one packet".into());
    }
    if !(10..=60_000).contains(&interval_ms) {
        return Err("interval must be between 10 and 60000 ms".into());
    }
    if limit > 10_000_000 {
        return Err("limit is too large".into());
    }

    {
        let inner = manager.lock();
        if inner.port.is_none() {
            return Err("connect to a serial port first".into());
        }

        // Catch template typos before the schedule starts. Payload decoding is
        // deferred to send time because a placeholder may be missing until the
        // first telemetry sample arrives.
        for (index, packet) in packets.iter().enumerate() {
            let label = packet.label_or(index);
            let vars = template::TemplateVars {
                seq: inner.packet_seq,
                sent: 0,
                index,
                label: &label,
                now_ms: epoch_ms(),
                state: &inner.state,
                telemetry: &inner.telemetry,
            };
            let expanded = template::expand(&packet.payload, &vars)
                .map_err(|err| format!("{label}: {err}"))?;
            if expanded.is_empty() {
                return Err(format!("{label}: payload is empty"));
            }
        }
    }

    // Replace any running schedule.
    {
        let mut inner = manager.lock();
        inner.stop_sender();
    }

    let stop = Arc::new(AtomicBool::new(false));
    let labels: Vec<String> = packets
        .iter()
        .enumerate()
        .map(|(index, packet)| packet.label_or(index))
        .collect();
    let state = {
        let mut inner = manager.lock();
        let generation = inner.sender.generation.wrapping_add(1);
        inner.sender = SenderState {
            running: true,
            generation,
            interval_ms,
            packet_count: packets.len(),
            labels,
            sent: 0,
            limit,
            next_at: Some(epoch_ms().saturating_add(interval_ms)),
            last_error: None,
        };
        inner.sender_stop = Some(Arc::clone(&stop));
        inner.sender.clone()
    };

    spawn_sender(
        app.clone(),
        Arc::clone(&manager.shared),
        stop,
        interval_ms,
        packets,
        limit,
    );

    let _ = app.emit("serial://sender", state.clone());
    Ok(state)
}

#[tauri::command]
fn stop_packet_sender(
    app: AppHandle,
    manager: State<'_, SerialManager>,
    lock: State<'_, BuildLock>,
) -> Result<SenderState, String> {
    ensure_packet_lab(lock.inner())?;

    let state = {
        let mut inner = manager.lock();
        inner.stop_sender();
        inner.sender.clone()
    };
    let _ = app.emit("serial://sender", state.clone());
    Ok(state)
}

#[tauri::command]
fn get_packet_sender(
    manager: State<'_, SerialManager>,
    lock: State<'_, BuildLock>,
) -> Result<SenderState, String> {
    ensure_packet_lab(lock.inner())?;
    Ok(manager.lock().sender.clone())
}

/// Inject bytes into the inbound path as if the MCU sent them. Lets the Packet
/// Lab and the monitor cards be tested without a physical link.
#[tauri::command]
fn inject_rx(
    packet: PacketSpec,
    app: AppHandle,
    manager: State<'_, SerialManager>,
    lock: State<'_, BuildLock>,
) -> Result<u64, String> {
    ensure_packet_lab(lock.inner())?;

    let bytes = {
        let inner = manager.lock();
        // Injection does not write to the wire, so it neither advances `{seq}`
        // nor counts as a sent packet.
        render_packet(&packet, &inner, inner.packet_seq, 0, 0)?
    };

    ingest_bytes(&app, &Arc::clone(&manager.shared), &bytes, true);
    Ok(bytes.len() as u64)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn spec(encoding: PacketEncoding, payload: &str) -> PacketSpec {
        PacketSpec {
            label: "test".into(),
            encoding,
            payload: payload.into(),
        }
    }

    #[test]
    fn renders_text_templates_and_escapes() {
        let mut inner = Inner::new();
        inner.state.bias_enabled = true;
        inner.state.bias_point = protocol::BiasPoint::QuadPlus;
        inner.telemetry.temp = Some(24.5);

        let packet = spec(
            PacketEncoding::Text,
            "STATE seq={seq} sent={sent} bias={bias} temp={temp:.1}\\n",
        );
        let bytes = render_packet(&packet, &inner, 7, 2, 3).unwrap();

        assert_eq!(
            String::from_utf8(bytes).unwrap(),
            "STATE seq=7 sent=2 bias=1 temp=24.5\n"
        );
    }

    #[test]
    fn raw_keeps_escapes_but_expands_templates() {
        let inner = Inner::new();
        let packet = spec(PacketEncoding::Raw, "C:\\logs\\{index}\\r");
        let bytes = render_packet(&packet, &inner, 1, 1, 4).unwrap();

        assert_eq!(String::from_utf8(bytes).unwrap(), "C:\\logs\\4\\r");
    }

    #[test]
    fn hex_is_decoded_after_expansion() {
        let mut inner = Inner::new();
        inner.telemetry.temp = Some(16.0);

        let packet = spec(PacketEncoding::Hex, "A5 {temp:.0}");
        let bytes = render_packet(&packet, &inner, 1, 1, 0).unwrap();

        assert_eq!(bytes, vec![0xA5, 0x16]);
    }

    #[test]
    fn rejects_empty_and_unknown_templates() {
        let inner = Inner::new();

        let empty = render_packet(&spec(PacketEncoding::Raw, ""), &inner, 1, 0, 0);
        assert!(empty.unwrap_err().contains("payload is empty"));

        let unknown = render_packet(&spec(PacketEncoding::Text, "{nope}"), &inner, 1, 0, 0);
        assert!(unknown.unwrap_err().contains("unknown template variable"));
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            let lock = BuildLock::load();
            let locked = lock.locked;
            app.manage(lock);

            // Locked builds never need (or create) an editable layout file.
            if !locked && let Err(err) = default_widget_path(app.handle()) {
                eprintln!("widget config: {err}");
            }

            let manager = SerialManager::new();
            manager.spawn_tick(app.handle().clone());
            app.manage(manager);
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            list_ports,
            connect,
            disconnect,
            set_state,
            get_status,
            get_wire_format,
            set_wire_format,
            send_now,
            calibrate,
            write_text_file,
            read_text_file,
            load_widget_config,
            get_build_info,
            export_locked_build,
            send_packet,
            start_packet_sender,
            stop_packet_sender,
            get_packet_sender,
            inject_rx
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
