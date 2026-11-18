import { computed, reactive, ref, watch } from "vue";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import {
  inTauri,
  state as deviceState,
  status,
  telemetry as deviceTelemetry,
} from "./device";
import { addLog } from "./logs";
import {
  expandTemplate,
  parseTemplate,
  validateTemplate,
  type TemplateContext,
} from "./template";

/* -------------------------------------------------------------------------- */
/* Types (mirrors the backend payloads)                                       */
/* -------------------------------------------------------------------------- */

/**
 * How a packet payload is interpreted.
 *
 * - `text` — UTF-8 text with `\n`, `\r`, `\t`, `\0`, `\\` and `\xNN` escapes.
 * - `hex`  — hexadecimal bytes; whitespace, commas, `_`, `:`, `-` and `0x`
 *            prefixes are ignored.
 * - `raw`  — UTF-8 text sent verbatim, with no escape processing.
 */
export type PacketEncoding = "text" | "hex" | "raw";

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
  /** Packet Lab write counter for this packet, exposed to templates as `{seq}`. */
  seq: number;
  bytes: number;
  sent: number;
  txs: number;
  intervalMs: number;
}

export interface TxMark extends PacketSentEvent {
  id: number;
  manual: boolean;
}

/** One parsed packet. The script text is the source of truth. */
export interface ComposerPacket {
  id: number;
  label: string;
  encoding: PacketEncoding;
  payload: string;
}

export interface PacketScriptError {
  /** 1-based line number in the script. */
  line: number;
  message: string;
}

export interface ParsedPacketScript {
  packets: ComposerPacket[];
  errors: PacketScriptError[];
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
/* Packet script                                                             */
/* -------------------------------------------------------------------------- */

/**
 * The packet lab keeps its packets as a small line-oriented script.
 *
 * ```
 * # Comments start with # or //.
 * Ping => PING\n
 * TLM sample => TLM seq=1 temp=24.5\n
 * Cal => [hex] 01 02 A0 FF
 * Note => [raw] C:\logs\raw.txt
 * ```
 *
 * - One packet per non-empty line, sent in script order.
 * - `label =>` is optional; everything after `=>` is the payload.
 * - `[text]`, `[hex]` and `[raw]` (or `text:`, `hex:`, `raw:`) select the
 *   encoding. Text is the default.
 * - Text payloads support `\n`, `\r`, `\t`, `\0`, `\\` and `\xNN`.
 */
const DEFAULT_SCRIPT = [
  "# One packet per line, sent in order.",
  '# Optional label before "=>"; encoding with [text], [hex] or [raw].',
  "# {placeholders} expand at send time: {seq}, {temp:.2}, {biasPoint}.",
  "Ping => PING\\n",
  "TLM sample => TLM seq=1 theta0=12.3 temp=24.5 vbias=3.3 pin=1.2 pout=0.8\\n",
  "Live status => STATE seq={seq} bias={biasEnabled} bp={biasPoint} temp={temp:.2|0.0} pout={outputPower:.3|0.000}\\n",
].join("\n");

const ENCODING_PATTERN = /^\[(text|hex|raw)\]\s*/i;
const NAMED_ENCODING_PATTERN = /^(text|hex|raw)\s*:\s*/i;

/** True when a payload contains a `{placeholder}` expanded by the backend. */
export function hasTemplate(payload: string): boolean {
  return parseTemplate(payload).placeholders.length > 0;
}

/**
 * Static payload validation, without live telemetry values.
 *
 * Template names and formats are checked immediately. Final decoding and size
 * limits are deferred for templated payloads because the substituted values can
 * change the byte sequence; the backend performs the authoritative check at
 * send time.
 */
function staticPayloadProblem(packet: ComposerPacket): string | null {
  const templateProblem = validateTemplate(packet.payload);
  if (templateProblem) return templateProblem;
  if (hasTemplate(packet.payload)) return null;

  const { bytes, error } = decodePayload(packet);
  if (error) return error;
  if (bytes.length === 0) return "payload is empty";
  if (bytes.length > 8192) return "payload is larger than 8 KiB";
  return null;
}

function normalizeEncoding(value: string): PacketEncoding {
  const token = value.toLowerCase();
  if (token === "hex") return "hex";
  if (token === "raw") return "raw";
  return "text";
}

/**
 * Parse a packet script. Invalid lines are reported with their line number and
 * skipped; valid lines are returned in order.
 */
export function parsePacketScript(script: string): ParsedPacketScript {
  const packets: ComposerPacket[] = [];
  const errors: PacketScriptError[] = [];
  const lines = script.split(/\r?\n/);

  for (let index = 0; index < lines.length; index += 1) {
    const lineNumber = index + 1;
    let body = lines[index].trim();

    if (!body || body.startsWith("#") || body.startsWith("//")) continue;

    let label = "";
    const arrow = body.indexOf("=>");
    if (arrow !== -1) {
      label = body.slice(0, arrow).trim();
      body = body.slice(arrow + 2).trim();
    }

    let encoding: PacketEncoding = "text";
    const bracket = body.match(ENCODING_PATTERN);
    const named = bracket ? null : body.match(NAMED_ENCODING_PATTERN);
    if (bracket) {
      encoding = normalizeEncoding(bracket[1]);
      body = body.slice(bracket[0].length);
    } else if (named) {
      encoding = normalizeEncoding(named[1]);
      body = body.slice(named[0].length);
    }

    if (!body) {
      errors.push({ line: lineNumber, message: "payload is empty" });
      continue;
    }

    const packet: ComposerPacket = {
      id: packets.length + 1,
      label,
      encoding,
      payload: body,
    };

    const problem = staticPayloadProblem(packet);
    if (problem) {
      errors.push({ line: lineNumber, message: problem });
      continue;
    }

    packets.push(packet);
  }

  return { packets, errors };
}

/** Render packets back into the script format. Used for legacy migrations. */
export function serializePacketScript(packets: ComposerPacket[]): string {
  return packets
    .map((packet) => {
      const label = packet.label.trim();
      const prefix =
        packet.encoding === "hex"
          ? "[hex] "
          : packet.encoding === "raw"
            ? "[raw] "
            : "";
      return `${label ? `${label} => ` : ""}${prefix}${packet.payload}`;
    })
    .join("\n");
}

/* -------------------------------------------------------------------------- */
/* Composer persistence                                                      */
/* -------------------------------------------------------------------------- */

const STORAGE_KEY = "bias-controller.packet-lab";

/**
 * Shipped demo packets were once persisted with `encoding = "hex"` even though
 * their payload is text. Treat those as text on load instead of failing with
 * "unexpected P in hex payload".
 */
const LEGACY_TEXT_PAYLOADS = new Set([
  "PING\\n",
  "TLM seq=1 theta0=12.3 temp=24.5 vbias=3.3 pin=1.2 pout=0.8\\n",
]);

let nextRxId = 1;
let nextTxId = 1;
let lastRxAt: number | null = null;
/** Backend `{seq}` counter, updated from every successful Packet Lab write. */
const lastPacketSeq = ref(0);
let initialized = false;

interface SavedLab {
  script?: unknown;
  packets?: unknown;
  intervalMs?: unknown;
  limit?: unknown;
}

function legacyPackets(entries: unknown[]): ComposerPacket[] {
  return entries
    .filter(
      (entry): entry is Record<string, unknown> =>
        !!entry && typeof entry === "object",
    )
    .map((entry, index) => {
      const payload = typeof entry.payload === "string" ? entry.payload : "";
      const savedEncoding: PacketEncoding =
        entry.encoding === "hex"
          ? "hex"
          : entry.encoding === "raw"
            ? "raw"
            : "text";
      const encoding =
        savedEncoding === "hex" &&
        LEGACY_TEXT_PAYLOADS.has(payload) &&
        decodeHex(payload).error !== null
          ? "text"
          : savedEncoding;

      return {
        id: index + 1,
        label: typeof entry.label === "string" ? entry.label : "",
        encoding,
        payload,
      };
    });
}

function loadLab(): {
  script: string;
  intervalMs: number;
  limit: number;
} {
  const fallback = {
    script: DEFAULT_SCRIPT,
    intervalMs: 300,
    limit: 0,
  };

  if (typeof localStorage === "undefined") return fallback;

  try {
    const raw = localStorage.getItem(STORAGE_KEY);
    if (!raw) return fallback;

    const saved = JSON.parse(raw) as SavedLab;
    let script = fallback.script;

    if (typeof saved.script === "string") {
      script = saved.script;
    } else if (Array.isArray(saved.packets)) {
      const packets = legacyPackets(saved.packets);
      if (packets.length > 0) script = serializePacketScript(packets);
    }

    return {
      script,
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

export const packetScript = ref(savedLab.script);
export const sendIntervalMs = ref(savedLab.intervalMs);
export const sendLimit = ref(savedLab.limit);

const parsedScript = computed(() => parsePacketScript(packetScript.value));

/** Valid packets from the script, in send order. */
export const composerPackets = computed<ComposerPacket[]>(
  () => parsedScript.value.packets,
);

/** Script problems, reported by line. */
export const scriptErrors = computed<PacketScriptError[]>(
  () => parsedScript.value.errors,
);

function saveLab() {
  if (typeof localStorage === "undefined") return;
  try {
    localStorage.setItem(
      STORAGE_KEY,
      JSON.stringify({
        script: packetScript.value,
        intervalMs: sendIntervalMs.value,
        limit: sendLimit.value,
      }),
    );
  } catch {
    // Storage can be unavailable; nothing breaks.
  }
}

watch([packetScript, sendIntervalMs, sendLimit], saveLab);

// Editing the script clears the previous start/send error so stale messages do
// not hide a new problem.
watch(packetScript, () => {
  packetError.value = null;
});

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
    // Older backend, preview, or a customer build: keep the local defaults.
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
  if (typeof payload.seq === "number") lastPacketSeq.value = payload.seq;
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

/** Validate a packet before it reaches the backend. */
export function packetProblem(packet: ComposerPacket): string | null {
  const problem = staticPayloadProblem(packet);
  return problem ? `${packetLabel(packet)}: ${problem}` : null;
}

export async function sendOnce(packet: ComposerPacket) {
  packetError.value = null;
  if (!inTauri) {
    packetError.value = "Sending is only available in the desktop app.";
    return;
  }

  const problem = packetProblem(packet);
  if (problem) {
    packetError.value = problem;
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

  if (scriptErrors.value.length > 0) {
    packetError.value = `Fix ${scriptErrors.value.length} script problem${
      scriptErrors.value.length === 1 ? "" : "s"
    } first.`;
    return;
  }

  if (composerPackets.value.length === 0) {
    packetError.value = "Add at least one packet to the script first.";
    return;
  }

  for (const packet of composerPackets.value) {
    const problem = packetProblem(packet);
    if (problem) {
      packetError.value = problem;
      return;
    }
  }

  senderBusy.value = true;
  try {
    const state = await invoke<SenderState>("start_packet_sender", {
      intervalMs: clampInterval(sendIntervalMs.value),
      packets: composerPackets.value.map(spec),
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

  const problem = packetProblem(packet);
  if (problem) {
    packetError.value = problem;
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

/** Build the same context the backend uses for the next send. */
function liveTemplateContext(
  packet: ComposerPacket,
  index: number,
): TemplateContext {
  const nextSent = sender.running ? sender.sent + 1 : 1;
  return {
    // A one-shot or scheduled send uses the next counter value; the backend
    // reports the committed value after every successful write.
    seq: lastPacketSeq.value + 1,
    sent: nextSent,
    index,
    label: packetLabel(packet, index),
    now: Date.now(),
    state: deviceState,
    telemetry: deviceTelemetry,
  };
}

/**
 * Decode a composed packet with the same rules as the Rust backend.
 *
 * Templates are expanded with the live state and newest telemetry sample so
 * the editor can show the bytes that would be written right now.
 */
export function decodePayload(
  packet: ComposerPacket,
  index = 0,
): {
  bytes: number[];
  error: string | null;
} {
  const expanded = expandTemplate(
    packet.payload,
    liveTemplateContext(packet, index),
  );
  if (expanded.error) return { bytes: [], error: expanded.error };

  switch (packet.encoding) {
    case "hex":
      return decodeHex(expanded.text);
    case "raw":
      return decodeRaw(expanded.text);
    default:
      return decodeText(expanded.text);
  }
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

function decodeRaw(payload: string): { bytes: number[]; error: string | null } {
  return { bytes: Array.from(new TextEncoder().encode(payload)), error: null };
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

export type PacketKind = "empty" | "text" | "binary" | "mixed";
export type PacketFraming = "LF" | "CRLF" | "CR" | "none";

export interface PacketField {
  key: string;
  value: string;
}

export interface PacketAnalysis {
  length: number;
  /** Share of bytes that are printable ASCII or common whitespace, 0…1. */
  printable: number;
  kind: PacketKind;
  framing: PacketFraming;
  /** Complete lines when the chunk is text, otherwise 0. */
  lines: number;
  /** First whitespace-delimited token, usually a frame tag such as `TLM`. */
  command: string | null;
  /** Whitespace-delimited tokens from the decoded text (limited for the UI). */
  tokens: string[];
  /** `key=value` tokens, also limited for the UI. */
  fields: PacketField[];
  /** UTF-8 decode with replacement characters for binary chunks. */
  text: string;
}

/**
 * Describe the structure of one inbound chunk: framing, text/binary balance and
 * any `key=value` fields. This is intentionally read-only and cheap because the
 * Packet Lab recomputes it for the selected chunk.
 */
export function analyzePacket(bytes: number[]): PacketAnalysis {
  const length = bytes.length;
  const framing: PacketFraming =
    length >= 2 && bytes[length - 2] === 0x0d && bytes[length - 1] === 0x0a
      ? "CRLF"
      : length >= 1 && bytes[length - 1] === 0x0a
        ? "LF"
        : length >= 1 && bytes[length - 1] === 0x0d
          ? "CR"
          : "none";

  let printableBytes = 0;
  for (const byte of bytes) {
    if (byte === 0x09 || byte === 0x0a || byte === 0x0d) {
      printableBytes += 1;
    } else if (byte >= 0x20 && byte <= 0x7e) {
      printableBytes += 1;
    }
  }

  const printable = length === 0 ? 0 : printableBytes / length;
  const kind: PacketKind =
    length === 0
      ? "empty"
      : printable >= 0.85
        ? "text"
        : printable <= 0.3
          ? "binary"
          : "mixed";

  const text = new TextDecoder("utf-8").decode(new Uint8Array(bytes));
  const tokens =
    kind === "binary"
      ? []
      : text
          .trim()
          .split(/\s+/)
          .filter(Boolean)
          .slice(0, 64);
  const fields = tokens
    .map((token) => {
      const separator = token.indexOf("=");
      return separator === -1
        ? null
        : { key: token.slice(0, separator), value: token.slice(separator + 1) };
    })
    .filter((field): field is PacketField => field !== null)
    .slice(0, 24);

  const completeLines = text === "" ? 0 : text.split("\n").length;
  const lines =
    kind === "binary" || completeLines === 0
      ? 0
      : text.endsWith("\n")
        ? completeLines - 1
        : completeLines;

  return {
    length,
    printable,
    kind,
    framing,
    lines,
    command: tokens[0] ?? null,
    tokens,
    fields,
    text,
  };
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
