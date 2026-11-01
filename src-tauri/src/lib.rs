//! Tauri backend for the bias controller.
//!
//! This app is the **master** on the serial link: a background thread wakes up
//! every [`TICK_MS`] and writes the latest [`DeviceState`] snapshot to the MCU.
//! The frontend never owns the clock; it only pushes state changes through the
//! `set_state` command, and the loop picks the newest snapshot on its next tick.
//!
//! A reader thread mirrors inbound bytes to the Packet Lab page and parses
//! telemetry lines for the monitor cards. The Packet Lab can also start a
//! separate packet scheduler that writes arbitrary payloads at a configurable
//! interval, independently of the state tick.

mod config;
mod protocol;

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

/// Master clock period. The MCU expects a fresh state frame this often.
const TICK_MS: u64 = 300;

/// Read/write timeout for the serial handle. Short enough that the reader
/// thread notices a disconnect quickly, long enough for ordinary writes.
const IO_TIMEOUT: Duration = Duration::from_millis(100);

/// Longest inbound line we buffer before dropping a partial frame.
const MAX_LINE: usize = 8192;

/// Shared, mutable state guarded by a single mutex.
///
/// The 300 ms thread, the reader/sender threads and every command take this
/// lock briefly; writes are short, so contention is negligible.
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
            tick_ms: TICK_MS,
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

    /// Spawn the 300 ms master loop. It runs for the lifetime of the process.
    fn spawn_tick(&self, app: AppHandle) {
        let shared = Arc::clone(&self.shared);
        let tick = Duration::from_millis(TICK_MS);

        thread::spawn(move || {
            // Absolute deadline scheduling keeps the average period at exactly
            // TICK_MS even though each write takes a little time. If the thread
            // ever falls behind (e.g. a stalled port), we resync instead of
            // bursting out catch-up frames.
            let mut next = Instant::now() + tick;

            loop {
                let now = Instant::now();
                if next > now {
                    thread::sleep(next - now);
                } else {
                    next = now;
                }
                next += tick;

                let mut inner = shared.lock().unwrap_or_else(|p| p.into_inner());

                // Nothing connected: keep the clock running so timing is stable
                // once the user connects.
                if inner.port.is_none() {
                    continue;
                }

                inner.seq = inner.seq.wrapping_add(1);
                let frame = protocol::encode_state(inner.seq, &inner.state);

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
                        // A failed write means the link is gone. Drop the port and
                        // tell the UI; the loop keeps ticking but sends nothing
                        // until the user reconnects.
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

/// Emitted after every successful state frame so the UI can show a heartbeat.
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

/* -------------------------------------------------------------------------- */
/* Packet payloads                                                            */
/* -------------------------------------------------------------------------- */

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PacketEncoding {
    #[default]
    Text,
    Hex,
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

    fn decode(&self) -> Result<Vec<u8>, String> {
        match self.encoding {
            PacketEncoding::Text => decode_text(&self.payload),
            PacketEncoding::Hex => decode_hex(&self.payload),
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
        let mut line = Vec::<u8>::new();

        loop {
            if stop.load(Ordering::Relaxed) {
                break;
            }

            match port.read(&mut buf) {
                Ok(0) => thread::sleep(Duration::from_millis(10)),
                Ok(n) => {
                    let chunk = &buf[..n];
                    emit_rx(&app, &shared, chunk, false);

                    line.extend_from_slice(chunk);
                    while let Some(position) = line.iter().position(|byte| *byte == b'\n') {
                        let raw: Vec<u8> = line.drain(..=position).collect();
                        parse_and_emit_telemetry(&app, &raw);
                    }
                    if line.len() > MAX_LINE {
                        line.clear();
                    }
                }
                Err(err) => match err.kind() {
                    std::io::ErrorKind::TimedOut | std::io::ErrorKind::WouldBlock => continue,
                    _ => {
                        // The device disappeared. Mark the link down and let the
                        // write side notice on its next frame as well.
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

fn parse_and_emit_telemetry(app: &AppHandle, raw: &[u8]) {
    let text = String::from_utf8_lossy(raw);
    if let Some(patch) = protocol::parse_telemetry_line(&text) {
        let _ = app.emit("telemetry://state", patch);
    }
}

/// Feed a complete payload through the inbound path. Used by the Packet Lab's
/// "inject" button, which lets the UI be exercised without hardware.
fn ingest_bytes(app: &AppHandle, shared: &Arc<Mutex<Inner>>, bytes: &[u8], synthetic: bool) {
    emit_rx(app, shared, bytes, synthetic);

    for raw in bytes.split_inclusive(|byte| *byte == b'\n') {
        if !raw.is_empty() {
            parse_and_emit_telemetry(app, raw);
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
    frames: Vec<(String, Vec<u8>)>,
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

            let (label, payload) = &frames[index];
            let label = label.clone();
            let send_index = index;

            enum Step {
                Sent(SenderState, u64),
                Failed(SenderState, Status),
            }

            let step = {
                let mut inner = shared.lock().unwrap_or_else(|p| p.into_inner());
                if inner.port.is_none() {
                    inner.sender.running = false;
                    inner.sender.next_at = None;
                    inner.sender.last_error = Some("not connected".into());
                    inner.sender_stop = None;
                    inner.last_error = Some("not connected".into());
                    Step::Failed(inner.sender.clone(), inner.status())
                } else {
                    match inner.write_immediate(payload) {
                        Ok(()) => {
                            inner.sender.sent += 1;
                            inner.sender.next_at = Some(epoch_ms().saturating_add(interval_ms));
                            Step::Sent(inner.sender.clone(), inner.txs)
                        }
                        Err(err) => {
                            inner.sender.running = false;
                            inner.sender.next_at = None;
                            inner.sender.last_error = Some(err);
                            inner.sender_stop = None;
                            Step::Failed(inner.sender.clone(), inner.status())
                        }
                    }
                }
            };

            match step {
                Step::Sent(state, txs) => {
                    let _ = app.emit(
                        "serial://packet",
                        PacketSent {
                            at: epoch_ms(),
                            label,
                            index: send_index,
                            bytes: payload.len(),
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
                    index = (index + 1) % frames.len();
                    next += interval;
                    if next < Instant::now() {
                        next = Instant::now() + interval;
                    }
                }
                Step::Failed(state, status) => {
                    let _ = app.emit("serial://sender", state);
                    let _ = app.emit("serial://status", status);
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

/// Replace the snapshot that the 300 ms loop will send on its next tick.
#[tauri::command]
fn set_state(state: DeviceState, manager: State<'_, SerialManager>) -> Result<(), String> {
    manager.lock().state = state;
    Ok(())
}

#[tauri::command]
fn get_status(manager: State<'_, SerialManager>) -> Result<Status, String> {
    Ok(manager.lock().status())
}

/// Write the current snapshot immediately, outside the normal tick.
///
/// Useful for testing the link or for one-shot commands that must not wait up
/// to 300 ms. It does not reset or interfere with the periodic loop.
#[tauri::command]
fn send_now(app: AppHandle, manager: State<'_, SerialManager>) -> Result<Status, String> {
    let mut inner = manager.lock();

    inner.seq = inner.seq.wrapping_add(1);
    let frame = protocol::encode_state(inner.seq, &inner.state);
    if let Err(err) = inner.write_immediate(&frame) {
        let status = inner.status();
        let _ = app.emit("serial://status", status);
        return Err(err);
    }

    let status = inner.status();
    let _ = app.emit("serial://status", status.clone());
    Ok(status)
}

/// Ask the MCU to run its calibration routine.
#[tauri::command]
fn calibrate(app: AppHandle, manager: State<'_, SerialManager>) -> Result<Status, String> {
    let mut inner = manager.lock();

    inner.seq = inner.seq.wrapping_add(1);
    let frame = protocol::encode_calibrate(inner.seq);
    if let Err(err) = inner.write_immediate(&frame) {
        let status = inner.status();
        let _ = app.emit("serial://status", status);
        return Err(err);
    }

    let status = inner.status();
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
#[tauri::command]
fn load_widget_config(
    path: Option<String>,
    app: AppHandle,
) -> Result<WidgetConfigEnvelope, String> {
    let path = match path.map(|value| value.trim().to_string()) {
        Some(value) if !value.is_empty() => PathBuf::from(value),
        _ => default_widget_path(&app)?,
    };

    let source =
        fs::read_to_string(&path).map_err(|e| format!("failed to read {}: {e}", path.display()))?;

    match config::parse(&source) {
        Ok(config) => Ok(WidgetConfigEnvelope {
            path: path.display().to_string(),
            config: Some(config),
            errors: Vec::new(),
        }),
        Err(errors) => Ok(WidgetConfigEnvelope {
            path: path.display().to_string(),
            config: None,
            errors,
        }),
    }
}

/// One-shot write of a composed packet. Useful for testing and for commands
/// that do not need a schedule.
#[tauri::command]
fn send_packet(
    packet: PacketSpec,
    app: AppHandle,
    manager: State<'_, SerialManager>,
) -> Result<u64, String> {
    let bytes = packet.decode()?;
    if bytes.is_empty() {
        return Err("packet is empty".into());
    }
    if bytes.len() > 8192 {
        return Err("packet is larger than 8 KiB".into());
    }

    let outcome = {
        let mut inner = manager.lock();
        match inner.write_immediate(&bytes) {
            Ok(()) => Ok((inner.txs, inner.status())),
            Err(err) => Err((err, inner.status())),
        }
    };

    let (txs, status) = match outcome {
        Ok(result) => result,
        Err((err, status)) => {
            let _ = app.emit("serial://status", status);
            return Err(err);
        }
    };

    let _ = app.emit(
        "serial://packet",
        PacketSent {
            at: epoch_ms(),
            label: packet.label_or(0),
            index: 0,
            bytes: bytes.len(),
            sent: 1,
            txs,
            interval_ms: 0,
        },
    );
    let _ = app.emit("serial://status", status);

    Ok(bytes.len() as u64)
}

#[tauri::command]
fn start_packet_sender(
    interval_ms: u64,
    packets: Vec<PacketSpec>,
    limit: u64,
    app: AppHandle,
    manager: State<'_, SerialManager>,
) -> Result<SenderState, String> {
    if packets.is_empty() {
        return Err("add at least one packet".into());
    }
    if !(10..=60_000).contains(&interval_ms) {
        return Err("interval must be between 10 and 60000 ms".into());
    }
    if limit > 10_000_000 {
        return Err("limit is too large".into());
    }

    let mut frames = Vec::with_capacity(packets.len());
    for (index, packet) in packets.iter().enumerate() {
        let bytes = packet
            .decode()
            .map_err(|err| format!("{}: {err}", packet.label_or(index)))?;
        if bytes.is_empty() {
            return Err(format!("{}: payload is empty", packet.label_or(index)));
        }
        if bytes.len() > 8192 {
            return Err(format!(
                "{}: payload is larger than 8 KiB",
                packet.label_or(index)
            ));
        }
        frames.push((packet.label_or(index), bytes));
    }

    {
        let inner = manager.lock();
        if inner.port.is_none() {
            return Err("connect to a serial port first".into());
        }
    }

    // Replace any running schedule.
    {
        let mut inner = manager.lock();
        inner.stop_sender();
    }

    let stop = Arc::new(AtomicBool::new(false));
    let labels: Vec<String> = frames.iter().map(|(label, _)| label.clone()).collect();
    let state = {
        let mut inner = manager.lock();
        let generation = inner.sender.generation.wrapping_add(1);
        inner.sender = SenderState {
            running: true,
            generation,
            interval_ms,
            packet_count: frames.len(),
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
        frames,
        limit,
    );

    let _ = app.emit("serial://sender", state.clone());
    Ok(state)
}

#[tauri::command]
fn stop_packet_sender(
    app: AppHandle,
    manager: State<'_, SerialManager>,
) -> Result<SenderState, String> {
    let state = {
        let mut inner = manager.lock();
        inner.stop_sender();
        inner.sender.clone()
    };
    let _ = app.emit("serial://sender", state.clone());
    Ok(state)
}

#[tauri::command]
fn get_packet_sender(manager: State<'_, SerialManager>) -> Result<SenderState, String> {
    Ok(manager.lock().sender.clone())
}

/// Inject bytes into the inbound path as if the MCU sent them. Lets the Packet
/// Lab and the monitor cards be tested without a physical link.
#[tauri::command]
fn inject_rx(
    packet: PacketSpec,
    app: AppHandle,
    manager: State<'_, SerialManager>,
) -> Result<u64, String> {
    let bytes = packet.decode()?;
    if bytes.is_empty() {
        return Err("packet is empty".into());
    }
    if bytes.len() > 8192 {
        return Err("packet is larger than 8 KiB".into());
    }
    ingest_bytes(&app, &Arc::clone(&manager.shared), &bytes, true);
    Ok(bytes.len() as u64)
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            if let Err(err) = default_widget_path(app.handle()) {
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
            send_now,
            calibrate,
            write_text_file,
            read_text_file,
            load_widget_config,
            send_packet,
            start_packet_sender,
            stop_packet_sender,
            get_packet_sender,
            inject_rx
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
