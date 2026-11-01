import { computed, reactive, ref, watch } from "vue";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { inTauri, status } from "./device";
import { addLog } from "./logs";

/* -------------------------------------------------------------------------- */
/* Types (mirrors the backend payloads)                                       */
/* -------------------------------------------------------------------------- */

export type PacketEncoding = "text" | "hex";

export interface RxEvent {
  at: number;
  bytes: number[];
  seq: number;
  totalBytes: number;
  synthetic: boolean;
}

export interface RxPacket extends RxEvent {
  id: number;
  /** Milliseconds since the previous chunk, null for the first one. */
  dt: number | null;
}

export interface SenderState {
  running: boolean;
  /** Bumped by the backend on every start/stop; used to drop stale events. */
  generation: number;
  intervalMs: number;
  packetCount: number;
  labels: string[];
  sent: number;
  limit: number;
  nextAt: number | null;
  lastError: string | null;
}

export interface PacketSentEvent {
  at: number;
  label: string;
  index: number;
  bytes: number;
  sent: number;
  txs: number;
  intervalMs: number;
}

export interface TxMark extends PacketSentEvent {
  id: number;
  manual: boolean;
}

export interface ComposerPacket {
  id: number;
  label: string;
  encoding: PacketEncoding;
  payload: string;
}

/* -------------------------------------------------------------------------- */
/* Reactive state                                                            */
/* -------------------------------------------------------------------------- */

export const MAX_RX = 600;
export const MAX_TX_MARKS = 80;

export const rxPackets = reactive<RxPacket[]>([]);
export const capturePaused = ref(false);
export const rxTotals = reactive({ chunks: 0, bytes: 0 });
export const rxRate = ref(0);
export const rxLastAt = ref<number | null>(null);
export const packetError = ref<string | null>(null);

export const txMarks = reactive<TxMark[]>([]);
export const sender = reactive<SenderState>({
  running: false,
  generation: 0,
  intervalMs: 300,
  packetCount: 0,
  labels: [],
  sent: 0,
  limit: 0,
  nextAt: null,
  lastError: null,
});
export const senderBusy = ref(false);

/** Captured packets matching the current (optional) filter is derived in the page. */
export const capturedBytes = computed(() =>
  rxPackets.reduce((total, packet) => total + packet.bytes.length, 0),
);

/* -------------------------------------------------------------------------- */
/* Composer persistence                                                      */
/* -------------------------------------------------------------------------- */

const STORAGE_KEY = "bias-controller.packet-lab";

let nextPacketId = 1;
let nextRxId = 1;
let nextTxId = 1;
let lastRxAt: number | null = null;
let initialized = false;

function newPacket(
  label: string,
  payload: string,
  encoding: PacketEncoding = "text",
): ComposerPacket {
  return { id: nextPacketId++, label, encoding, payload };
}

function defaultPackets(): ComposerPacket[] {
  return [
    newPacket("Ping", "PING\\n"),
    newPacket(
      "TLM sample (inject)",
      "TLM seq=1 theta0=12.3 temp=24.5 vbias=3.3 pin=1.2 pout=0.8\\n",
    ),
  ];
}

interface SavedLab {
  packets?: unknown;
  intervalMs?: unknown;
  limit?: unknown;
}

function loadLab(): {
  packets: ComposerPacket[];
  intervalMs: number;
  limit: number;
} {
  const fallback = {
    packets: defaultPackets(),
    intervalMs: 300,
    limit: 0,
  };

  if (typeof localStorage === "undefined") return fallback;

  try {
    const raw = localStorage.getItem(STORAGE_KEY);
    if (!raw) return fallback;

    const saved = JSON.parse(raw) as SavedLab;
    const packets = Array.isArray(saved.packets)
      ? saved.packets
          .filter((entry): entry is Record<string, unknown> => !!entry && typeof entry === "object")
          .map((entry) =>
            newPacket(
              typeof entry.label === "string" ? entry.label : "",
              typeof entry.payload === "string" ? entry.payload : "",
              entry.encoding === "hex" ? "hex" : "text",
            ),
          )
      : [];

    return {
      packets: packets.length > 0 ? packets : fallback.packets,
      intervalMs:
        typeof saved.intervalMs === "number"
          ? clampInterval(saved.intervalMs)
          : fallback.intervalMs,
      limit:
        typeof saved.limit === "number" && saved.limit >= 0
          ? Math.min(10_000_000, Math.floor(saved.limit))
          : fallback.limit,
    };
  } catch {
    return fallback;
  }
}

const savedLab = loadLab();

export const composerPackets = reactive<ComposerPacket[]>(savedLab.packets);
export const sendIntervalMs = ref(savedLab.intervalMs);
export const sendLimit = ref(savedLab.limit);

function saveLab() {
  if (typeof localStorage === "undefined") return;
  try {
    localStorage.setItem(
      STORAGE_KEY,
      JSON.stringify({
        packets: composerPackets.map(({ label, encoding, payload }) => ({
          label,
          encoding,
          payload,
        })),
        intervalMs: sendIntervalMs.value,
        limit: sendLimit.value,
      }),
    );
  } catch {
    // Storage can be unavailable; nothing breaks.
  }
}

watch(composerPackets, saveLab, { deep: true });
watch([sendIntervalMs, sendLimit], saveLab);

/* -------------------------------------------------------------------------- */
/* Initialisation                                                            */
/* -------------------------------------------------------------------------- */

export async function initPackets() {
  if (initialized) return;
  initialized = true;

  setInterval(() => {
    const cutoff = Date.now() - 2000;
    let count = 0;
    for (let index = rxPackets.length - 1; index >= 0; index -= 1) {
      if (rxPackets[index].at < cutoff) break;
      count += 1;
    }
    rxRate.value = count / 2;
  }, 500);

  // The backend resets its RX counters on connect; mirror that on the wire
  // totals so the header doesn't jump backwards on the first new chunk.
  watch(
    () => status.connected,
    (connected, wasConnected) => {
      if (connected && !wasConnected) {
        rxTotals.chunks = 0;
        rxTotals.bytes = 0;
        rxRate.value = 0;
      }
    },
  );

  if (!inTauri) return;

  await listen<RxEvent>("serial://rx", (event) => pushRx(event.payload));
  await listen<PacketSentEvent>("serial://packet", (event) =>
    pushTxMark(event.payload),
  );
  await listen<SenderState>("serial://sender", (event) =>
    applySender(event.payload),
  );

  try {
    applySender(await invoke<SenderState>("get_packet_sender"));
  } catch {
    // Older backend or preview: keep the local defaults.
  }
}

function pushRx(payload: RxEvent) {
  const at = payload.at || Date.now();
  rxTotals.chunks = payload.seq;
  rxTotals.bytes = payload.totalBytes;

  const dt = lastRxAt === null ? null : Math.max(0, at - lastRxAt);
  lastRxAt = at;
  rxLastAt.value = at;

  if (capturePaused.value) return;

  rxPackets.push({
    ...payload,
    at,
    id: nextRxId++,
    dt,
  });

  if (rxPackets.length > MAX_RX) {
    rxPackets.splice(0, rxPackets.length - MAX_RX);
  }
}

function pushTxMark(payload: PacketSentEvent) {
  status.txs = payload.txs;
  txMarks.push({
    ...payload,
    id: nextTxId++,
    manual: payload.intervalMs === 0,
  });
  if (txMarks.length > MAX_TX_MARKS) {
    txMarks.splice(0, txMarks.length - MAX_TX_MARKS);
  }
}

function applySender(next: SenderState) {
  // A schedule can finish while a stop/start command is in flight. Ignore
  // events from a generation we have already moved past.
  if (next.generation < sender.generation) return;
  Object.assign(sender, next);
}

/* -------------------------------------------------------------------------- */
/* Composer actions                                                          */
/* -------------------------------------------------------------------------- */

export function clampInterval(value: number): number {
  if (!Number.isFinite(value)) return 300;
  return Math.min(60_000, Math.max(10, Math.round(value)));
}

function spec(packet: ComposerPacket) {
  return {
    label: packet.label,
    encoding: packet.encoding,
    payload: packet.payload,
  };
}

export async function sendOnce(packet: ComposerPacket) {
  packetError.value = null;
  if (!inTauri) {
    packetError.value = "Sending is only available in the desktop app.";
    return;
  }
  try {
    const bytes = await invoke<number>("send_packet", { packet: spec(packet) });
    addLog("info", `Sent ${bytes} byte${bytes === 1 ? "" : "s"} · ${packetLabel(packet)}`);
  } catch (err) {
    packetError.value = String(err);
    addLog("error", `Packet send failed: ${String(err)}`);
  }
}

export async function startSending() {
  packetError.value = null;

  if (composerPackets.length === 0) {
    packetError.value = "Add at least one packet first.";
    return;
  }

  senderBusy.value = true;
  try {
    const state = await invoke<SenderState>("start_packet_sender", {
      intervalMs: clampInterval(sendIntervalMs.value),
      packets: composerPackets.map(spec),
      limit: Math.max(0, Math.floor(sendLimit.value)),
    });
    applySender(state);
    addLog(
      "info",
      `Packet schedule started · ${state.intervalMs} ms · ${state.packetCount} packet${state.packetCount === 1 ? "" : "s"}`,
    );
  } catch (err) {
    packetError.value = String(err);
    addLog("error", `Packet schedule failed: ${String(err)}`);
  } finally {
    senderBusy.value = false;
  }
}

export async function stopSending() {
  packetError.value = null;
  senderBusy.value = true;
  try {
    applySender(await invoke<SenderState>("stop_packet_sender"));
    addLog("info", "Packet schedule stopped");
  } catch (err) {
    packetError.value = String(err);
  } finally {
    senderBusy.value = false;
  }
}

export async function injectPacket(packet: ComposerPacket) {
  packetError.value = null;
  if (!inTauri) {
    packetError.value = "Injection is only available in the desktop app.";
    return;
  }
  try {
    capturePaused.value = false;
    const bytes = await invoke<number>("inject_rx", { packet: spec(packet) });
    addLog("info", `Injected ${bytes} byte${bytes === 1 ? "" : "s"} · ${packetLabel(packet)}`);
  } catch (err) {
    packetError.value = String(err);
    addLog("error", `Injection failed: ${String(err)}`);
  }
}

export function addComposerPacket() {
  composerPackets.push(newPacket("", "", "text"));
}

export function removeComposerPacket(id: number) {
  const index = composerPackets.findIndex((packet) => packet.id === id);
  if (index >= 0) composerPackets.splice(index, 1);
}

export function moveComposerPacket(id: number, direction: -1 | 1) {
  const index = composerPackets.findIndex((packet) => packet.id === id);
  const target = index + direction;
  if (index < 0 || target < 0 || target >= composerPackets.length) return;
  const [packet] = composerPackets.splice(index, 1);
  composerPackets.splice(target, 0, packet);
}

export function clearRx() {
  rxPackets.splice(0, rxPackets.length);
  lastRxAt = null;
  rxLastAt.value = null;
}

/* -------------------------------------------------------------------------- */
/* Formatting & decoding                                                     */
/* -------------------------------------------------------------------------- */

export function packetLabel(packet: ComposerPacket, index = 0): string {
  const label = packet.label.trim();
  return label || `packet ${index + 1}`;
}

/** Decode a composed packet with the same rules as the Rust backend. */
export function decodePayload(packet: ComposerPacket): {
  bytes: number[];
  error: string | null;
} {
  return packet.encoding === "hex"
    ? decodeHex(packet.payload)
    : decodeText(packet.payload);
}

function decodeText(payload: string): { bytes: number[]; error: string | null } {
  const encoder = new TextEncoder();
  const bytes: number[] = [];
  const chars = Array.from(payload);

  for (let index = 0; index < chars.length; index += 1) {
    const char = chars[index];

    if (char !== "\\") {
      bytes.push(...encoder.encode(char));
      continue;
    }

    const next = chars[index + 1];
    index += 1;

    switch (next) {
      case "n":
        bytes.push(0x0a);
        break;
      case "r":
        bytes.push(0x0d);
        break;
      case "t":
        bytes.push(0x09);
        break;
      case "0":
        bytes.push(0);
        break;
      case "\\":
        bytes.push(0x5c);
        break;
      case "x":
      case "X": {
        const pair = `${chars[index + 1] ?? ""}${chars[index + 2] ?? ""}`;
        if (!/^[0-9a-fA-F]{2}$/.test(pair)) {
          return { bytes: [], error: `incomplete \\x escape` };
        }
        bytes.push(Number.parseInt(pair, 16));
        index += 2;
        break;
      }
      case undefined:
        bytes.push(0x5c);
        break;
      default:
        bytes.push(0x5c, ...encoder.encode(next));
        break;
    }
  }

  return { bytes, error: null };
}

function decodeHex(payload: string): { bytes: number[]; error: string | null } {
  const digits: string[] = [];

  for (let index = 0; index < payload.length; index += 1) {
    const char = payload[index];

    if (/\s/.test(char) || [",", "_", ":", "-"].includes(char)) continue;

    if (char === "0" && /[xX]/.test(payload[index + 1] ?? "")) {
      index += 1;
      continue;
    }

    if (!/[0-9a-fA-F]/.test(char)) {
      return { bytes: [], error: `unexpected “${char}” in hex payload` };
    }

    digits.push(char);
  }

  if (digits.length % 2 !== 0) {
    return { bytes: [], error: "hex payload needs an even number of digits" };
  }

  const bytes: number[] = [];
  for (let index = 0; index < digits.length; index += 2) {
    bytes.push(Number.parseInt(digits[index] + digits[index + 1], 16));
  }

  return { bytes, error: null };
}

export function toHex(bytes: number[], max = bytes.length): string {
  return bytes
    .slice(0, max)
    .map((byte) => byte.toString(16).padStart(2, "0"))
    .join(" ");
}

export function toAscii(bytes: number[], max = bytes.length): string {
  return bytes
    .slice(0, max)
    .map((byte) => (byte >= 0x20 && byte <= 0x7e ? String.fromCharCode(byte) : "."))
    .join("");
}

export interface HexRow {
  offset: string;
  hex: string;
  ascii: string;
}

/** Classic 16-byte hex dump with a visual gap in the middle. */
export function hexDump(bytes: number[]): HexRow[] {
  const rows: HexRow[] = [];

  for (let offset = 0; offset < bytes.length; offset += 16) {
    const slice = bytes.slice(offset, offset + 16);
    const groups: string[] = [];
    for (let index = 0; index < slice.length; index += 1) {
      groups.push(slice[index].toString(16).padStart(2, "0"));
    }
    const left = groups.slice(0, 8).join(" ");
    const right = groups.slice(8).join(" ");
    rows.push({
      offset: offset.toString(16).padStart(4, "0"),
      hex: right ? `${left.padEnd(23, " ")}  ${right}` : left,
      ascii: toAscii(slice),
    });
  }

  return rows;
}

export function formatBytes(value: number): string {
  if (value < 1024) return `${value} B`;
  if (value < 1024 * 1024) return `${(value / 1024).toFixed(1)} KB`;
  return `${(value / (1024 * 1024)).toFixed(1)} MB`;
}

export function formatAge(ms: number): string {
  if (ms < 1000) return `${Math.round(ms)} ms`;
  if (ms < 60_000) return `${(ms / 1000).toFixed(1)} s`;
  return `${Math.floor(ms / 60_000)} min`;
}

export function formatClock(ms: number): string {
  const date = new Date(ms);
  const time = date.toLocaleTimeString([], { hour12: false });
  const millis = String(date.getMilliseconds()).padStart(3, "0");
  return `${time}.${millis}`;
}
