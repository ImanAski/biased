//! Tauri backend for the bias controller.
//!
//! This app is the **master** on the serial link: a background thread wakes up
//! every [`TICK_MS`] and writes the latest [`DeviceState`] snapshot to the MCU.
//! The frontend never owns the clock; it only pushes state changes through the
//! `set_state` command, and the loop picks the newest snapshot on its next tick.

mod protocol;

use std::io::Write;
use std::sync::{Arc, Mutex, MutexGuard};
use std::thread;
use std::time::{Duration, Instant};

use protocol::DeviceState;
use serde::Serialize;
use serialport::{SerialPort, SerialPortType};
use tauri::{AppHandle, Emitter, Manager, State};

/// Master clock period. The MCU expects a fresh state frame this often.
const TICK_MS: u64 = 300;

/// How long a single write may block before it is treated as a dead link.
const WRITE_TIMEOUT: Duration = Duration::from_millis(200);

/// Shared, mutable state guarded by a single mutex.
///
/// The 300 ms thread and every command take this lock briefly; writes are
/// short, so contention is negligible.
struct Inner {
    port: Option<Box<dyn SerialPort>>,
    state: DeviceState,
    seq: u32,
    txs: u64,
    port_name: Option<String>,
    baud: u32,
    last_error: Option<String>,
}

impl Inner {
    fn new() -> Self {
        Self {
            port: None,
            state: DeviceState::default(),
            seq: 0,
            txs: 0,
            port_name: None,
            baud: 115_200,
            last_error: None,
        }
    }

    fn status(&self) -> Status {
        Status {
            connected: self.port.is_some(),
            port_name: self.port_name.clone(),
            baud: self.baud,
            txs: self.txs,
            last_error: self.last_error.clone(),
            tick_ms: TICK_MS,
        }
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
        self.shared.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
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
                        inner.last_error = Some(format!("write failed: {err}"));
                        let status = inner.status();
                        let _ = app.emit("serial://status", status);
                    }
                }
            }
        });
    }
}

/// Connection snapshot sent to the frontend.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct Status {
    connected: bool,
    port_name: Option<String>,
    baud: u32,
    txs: u64,
    last_error: Option<String>,
    tick_ms: u64,
}

/// Emitted after every successful frame so the UI can show a heartbeat.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct TxEvent {
    seq: u32,
    txs: u64,
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
    let mut inner = manager.lock();

    // Drop any existing link first so a failed reconnect leaves us cleanly
    // disconnected rather than pointing at a stale handle.
    inner.port = None;

    let opened = serialport::new(&port, baud)
        .timeout(WRITE_TIMEOUT)
        .open()
        .map_err(|e| format!("failed to open {port} @ {baud}: {e}"))?;

    inner.port = Some(opened);
    inner.port_name = Some(port);
    inner.baud = baud;
    inner.seq = 0;
    inner.txs = 0;
    inner.last_error = None;

    let status = inner.status();
    let _ = app.emit("serial://status", status.clone());
    Ok(status)
}

#[tauri::command]
fn disconnect(app: AppHandle, manager: State<'_, SerialManager>) -> Result<Status, String> {
    let mut inner = manager.lock();

    if let Some(mut port) = inner.port.take() {
        // Best effort: flush whatever is queued before closing.
        let _ = port.flush();
        inner.port_name = None;
    }

    let status = inner.status();
    let _ = app.emit("serial://status", status.clone());
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
    inner.write_immediate(&frame)?;

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
    inner.write_immediate(&frame)?;

    let status = inner.status();
    let _ = app.emit("serial://status", status.clone());
    Ok(status)
}

/// Write exported text (logs) to a path chosen in the frontend save dialog.
#[tauri::command]
fn write_text_file(path: String, contents: String) -> Result<(), String> {
    std::fs::write(&path, contents).map_err(|e| format!("failed to write {path}: {e}"))
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
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
            write_text_file
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
