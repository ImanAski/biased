import { reactive, ref, watch } from "vue";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import type { BiasPoint } from "./metrics";
import { addLog } from "./logs";

/** One state snapshot, mirrored 1:1 with `DeviceState` in `src-tauri/src/protocol.rs`. */
export interface DeviceState {
  biasEnabled: boolean;
  /** Selected bias operating point. */
  biasPoint: BiasPoint;
  /** Dither amplitude. */
  ditherAmp: number;
  /** Dither frequency, Hz. */
  ditherFreq: number;
  mode: number;
}

export interface PortInfo {
  name: string;
  kind: string;
  manufacturer?: string | null;
  product?: string | null;
}

export interface SerialStatus {
  connected: boolean;
  portName: string | null;
  baud: number;
  /** Frames written since the last connect. */
  txs: number;
  lastError: string | null;
  /** Master clock period, reported by the backend. */
  tickMs: number;
}

export interface TxEvent {
  seq: number;
  txs: number;
}

/**
 * Values reported *by* the MCU. These are monitored only; the app never writes
 * them. `null` means "no sample yet", which is the state while RX support is
 * disabled (the app is currently transmit-only).
 */
export interface Telemetry {
  /** Θ₀, degrees. */
  theta0: number | null;
  /** Temperature, degrees Celsius. */
  temp: number | null;
  /** Measured bias voltage (plotted, not settable). */
  biasVoltage: number | null;
  /** Input power. Unit is whatever the MCU reports (see `metrics.ts`). */
  inputPower: number | null;
  /** Output power. Unit is whatever the MCU reports (see `metrics.ts`). */
  outputPower: number | null;
  /** Epoch ms of the most recent sample, or null before the first one. */
  updatedAt: number | null;
}

/** Payload shape for the (future) `telemetry://state` event from Rust. */
export type TelemetrySample = Partial<
  Pick<Telemetry, "theta0" | "temp" | "biasVoltage" | "inputPower" | "outputPower">
>;

/** True when running inside a Tauri webview (not a plain browser tab). */
export const inTauri =
  typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;

/** Editable state. Mutate fields directly; changes are pushed to the backend. */
export const state = reactive<DeviceState>({
  biasEnabled: false,
  biasPoint: "NULL",
  ditherAmp: 0,
  ditherFreq: 1000,
  mode: 0,
});

/** Monitored values received from the MCU (read-only in the UI). */
export const telemetry = reactive<Telemetry>({
  theta0: null,
  temp: null,
  biasVoltage: null,
  inputPower: null,
  outputPower: null,
  updatedAt: null,
});

/** Merge a telemetry sample and stamp it with the arrival time. */
export function applyTelemetry(patch: Partial<Telemetry>) {
  const firstSample = telemetry.updatedAt === null;
  Object.assign(telemetry, patch, { updatedAt: Date.now() });
  if (firstSample) addLog("info", "Telemetry stream started");
}

export function isStale(seconds = 2): boolean {
  return (
    telemetry.updatedAt === null ||
    Date.now() - telemetry.updatedAt > seconds * 1000
  );
}

/** Live link status, updated from backend events. */
export const status = reactive<SerialStatus>({
  connected: false,
  portName: null,
  baud: 115200,
  txs: 0,
  lastError: null,
  tickMs: 300,
});

export const ports = ref<PortInfo[]>([]);
export const uiError = ref<string | null>(null);
/** Sequence number of the most recent frame the MCU received. */
export const lastSeq = ref(0);

let initialized = false;
let pushTimer: ReturnType<typeof setTimeout> | null = null;
let lastPushError: string | null = null;

async function pushState() {
  if (!inTauri) return;
  try {
    await invoke("set_state", { state: snapshot() });
    uiError.value = null;
    lastPushError = null;
  } catch (err) {
    uiError.value = String(err);
    if (lastPushError !== uiError.value) {
      lastPushError = uiError.value;
      addLog("error", `State push failed: ${uiError.value}`);
    }
  }
}

/** Coalesce rapid edits (slider drags) into a single backend update. */
function schedulePush() {
  if (pushTimer !== null) return;
  pushTimer = setTimeout(() => {
    pushTimer = null;
    void pushState();
  }, 30);
}

function snapshot(): DeviceState {
  return {
    biasEnabled: state.biasEnabled,
    biasPoint: state.biasPoint,
    ditherAmp: state.ditherAmp,
    ditherFreq: state.ditherFreq,
    mode: state.mode,
  };
}

export async function initDevice() {
  if (initialized) return;
  initialized = true;

  addLog("info", "Application ready");
  watch(state, schedulePush, { deep: true });

  if (!inTauri) return;

  await listen<SerialStatus>("serial://status", (event) => {
    const wasConnected = status.connected;
    const previousError = status.lastError;
    Object.assign(status, event.payload);

    if (wasConnected !== status.connected) {
      if (status.connected) {
        addLog(
          "info",
          `Connected to ${status.portName} @ ${status.baud} baud`,
        );
      } else {
        addLog("warn", "Serial link closed");
      }
    }

    if (status.lastError && status.lastError !== previousError) {
      addLog("error", status.lastError);
    }
  });

  await listen<TxEvent>("serial://tx", (event) => {
    status.txs = event.payload.txs;
    lastSeq.value = event.payload.seq;
  });

  // Reserved for the RX path. The backend does not emit this yet, so the
  // monitoring cards stay empty until MCU replies are parsed.
  await listen<TelemetrySample>("telemetry://state", (event) => {
    applyTelemetry(event.payload);
  });

  await Promise.all([refreshStatus(), refreshPorts()]);
}

export async function refreshStatus() {
  if (!inTauri) return;
  try {
    Object.assign(status, await invoke<SerialStatus>("get_status"));
  } catch (err) {
    uiError.value = String(err);
  }
}

export async function refreshPorts() {
  if (!inTauri) return;
  try {
    ports.value = await invoke<PortInfo[]>("list_ports");
  } catch (err) {
    uiError.value = String(err);
    addLog("error", `Port scan failed: ${uiError.value}`);
  }
}

export async function connect(port: string, baud: number) {
  uiError.value = null;
  try {
    Object.assign(status, await invoke<SerialStatus>("connect", { port, baud }));
  } catch (err) {
    uiError.value = String(err);
    addLog("error", `Connect failed: ${uiError.value}`);
    throw err;
  }
}

export async function disconnect() {
  uiError.value = null;
  try {
    Object.assign(status, await invoke<SerialStatus>("disconnect"));
  } catch (err) {
    uiError.value = String(err);
    addLog("error", `Disconnect failed: ${uiError.value}`);
  }
}

/** Write the current snapshot immediately, bypassing the 300 ms tick. */
export async function sendNow() {
  if (!inTauri) return;
  try {
    Object.assign(status, await invoke<SerialStatus>("send_now"));
    addLog("info", "State sent immediately");
  } catch (err) {
    uiError.value = String(err);
    addLog("error", `Immediate send failed: ${uiError.value}`);
  }
}

/** Trigger the MCU calibration routine (one-shot frame). */
export async function calibrate() {
  if (!inTauri) return;
  try {
    Object.assign(status, await invoke<SerialStatus>("calibrate"));
    addLog("info", "Calibration requested");
  } catch (err) {
    uiError.value = String(err);
    addLog("error", `Calibration failed: ${uiError.value}`);
  }
}
