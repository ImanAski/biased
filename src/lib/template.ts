import type { DeviceState, Telemetry } from "./device";

/**
 * Text packet templates.
 *
 * This mirrors `src-tauri/src/template.rs` so the editor can preview and
 * validate a packet without asking the backend to expand it first. The backend
 * remains the source of truth; the preview uses the live state and the newest
 * telemetry sample.
 */
export interface TemplateContext {
  /** Packet Lab write counter, exposed as `{seq}`. */
  seq: number;
  /** Sends in the current schedule, exposed as `{sent}`. */
  sent: number;
  /** Zero-based packet position in the script, exposed as `{index}`. */
  index: number;
  /** Packet label, exposed as `{label}`. */
  label: string;
  /** Epoch milliseconds, exposed as `{now}` and `{time}`. */
  now: number;
  state: DeviceState;
  telemetry: Telemetry;
}

export interface TemplatePlaceholder {
  /** Trimmed text between the braces. */
  body: string;
  name: string;
  format: string | null;
  fallback: string | null;
}

export interface TemplateSegment {
  kind: "text" | "placeholder";
  /** Original text for literals, or `{body}` for a placeholder. */
  text: string;
  placeholder?: TemplatePlaceholder;
}

export interface ParsedTemplate {
  segments: TemplateSegment[];
  placeholders: TemplatePlaceholder[];
  error: string | null;
}

export interface TemplateExpansion {
  text: string;
  error: string | null;
}

type TemplateValue =
  | { kind: "int"; value: number }
  | { kind: "float"; value: number }
  | { kind: "bool"; value: boolean }
  | { kind: "text"; value: string }
  | { kind: "missing" };

const DEFAULT_CONTEXT: TemplateContext = {
  seq: 0,
  sent: 0,
  index: 0,
  label: "",
  now: 0,
  state: {
    biasEnabled: false,
    biasPoint: "NULL",
    ditherAmp: 0,
    ditherFreq: 1000,
    mode: 0,
    gain: 1,
    offset: 0,
  },
  telemetry: {
    theta0: null,
    temp: null,
    biasVoltage: null,
    inputPower: null,
    outputPower: null,
    biasCurrent: null,
    supplyVoltage: null,
    phaseError: null,
    lockStatus: null,
    updatedAt: null,
  },
};

/** Split a template into literal text and placeholders. */
export function parseTemplate(template: string): ParsedTemplate {
  const segments: TemplateSegment[] = [];
  const placeholders: TemplatePlaceholder[] = [];
  const chars = Array.from(template);
  let literal = "";
  let index = 0;

  while (index < chars.length) {
    const char = chars[index];

    if (char === "{") {
      if (chars[index + 1] === "{") {
        literal += "{";
        index += 2;
        continue;
      }

      if (literal) {
        segments.push({ kind: "text", text: literal });
        literal = "";
      }

      let body = "";
      index += 1;
      while (index < chars.length && chars[index] !== "}") {
        body += chars[index];
        index += 1;
      }
      if (index >= chars.length) {
        return {
          segments,
          placeholders,
          error: `unterminated template placeholder \`{${body}\``,
        };
      }
      index += 1;

      const placeholder = parsePlaceholder(body);
      placeholders.push(placeholder);
      segments.push({
        kind: "placeholder",
        text: `{${body}}`,
        placeholder,
      });
      continue;
    }

    if (char === "}") {
      if (chars[index + 1] === "}") index += 1;
      literal += "}";
      index += 1;
      continue;
    }

    literal += char;
    index += 1;
  }

  if (literal) segments.push({ kind: "text", text: literal });

  return { segments, placeholders, error: null };
}

function parsePlaceholder(body: string): TemplatePlaceholder {
  const trimmed = body.trim();
  const pipe = trimmed.indexOf("|");
  const expression = pipe === -1 ? trimmed : trimmed.slice(0, pipe).trim();
  const fallback = pipe === -1 ? null : trimmed.slice(pipe + 1);
  const colon = expression.indexOf(":");
  const name = colon === -1 ? expression : expression.slice(0, colon).trim();
  const format = colon === -1 ? null : expression.slice(colon + 1).trim();

  return { body: trimmed, name, format, fallback };
}

/**
 * Validate names and formats without needing live telemetry values.
 *
 * Values that have no sample yet still validate their format so a typo is
 * reported before the first `TLM` line arrives.
 */
export function validateTemplate(
  template: string,
  context: TemplateContext = DEFAULT_CONTEXT,
): string | null {
  const parsed = parseTemplate(template);
  if (parsed.error) return parsed.error;

  for (const placeholder of parsed.placeholders) {
    try {
      if (!placeholder.body) {
        return "empty template placeholder `{}`";
      }
      const value = lookup(placeholder.name, context);
      if (!value) {
        return `unknown template variable \`${placeholder.name}\``;
      }
      if (value.kind === "missing") {
        if (placeholder.format !== null) floatPrecision(placeholder.format);
      } else {
        formatValue(value, placeholder.format);
      }
    } catch (err) {
      return err instanceof Error ? err.message : String(err);
    }
  }

  return null;
}

/** Expand every placeholder in `template`. */
export function expandTemplate(
  template: string,
  context: TemplateContext,
): TemplateExpansion {
  const parsed = parseTemplate(template);
  if (parsed.error) return { text: "", error: parsed.error };

  let text = "";
  try {
    for (const segment of parsed.segments) {
      text +=
        segment.kind === "text"
          ? segment.text
          : renderPlaceholder(segment.placeholder!, context);
    }
  } catch (err) {
    return {
      text: "",
      error: err instanceof Error ? err.message : String(err),
    };
  }

  return { text, error: null };
}

/** Placeholders in a template, for the structure panel. */
export function templatePlaceholders(template: string): TemplatePlaceholder[] {
  return parseTemplate(template).placeholders;
}

function renderPlaceholder(
  placeholder: TemplatePlaceholder,
  context: TemplateContext,
): string {
  if (!placeholder.body) {
    throw new Error("empty template placeholder `{}`");
  }

  const value = lookup(placeholder.name, context);
  if (!value) {
    throw new Error(`unknown template variable \`${placeholder.name}\``);
  }

  if (value.kind === "missing") {
    if (placeholder.format !== null) floatPrecision(placeholder.format);
    return placeholder.fallback ?? "-";
  }

  return formatValue(value, placeholder.format);
}

function lookup(
  name: string,
  context: TemplateContext,
): TemplateValue | undefined {
  const dot = name.indexOf(".");
  if (dot !== -1) {
    const prefix = name.slice(0, dot);
    const key = name.slice(dot + 1);
    if (prefix === "telemetry" || prefix === "tel") {
      return telemetryValue(key, context.telemetry);
    }
    if (prefix === "state") {
      return stateValue(key, context.state);
    }
    return undefined;
  }

  switch (name) {
    case "seq":
      return { kind: "int", value: context.seq };
    case "sent":
      return { kind: "int", value: context.sent };
    case "index":
      return { kind: "int", value: context.index };
    case "now":
    case "time":
      return { kind: "int", value: context.now };
    case "label":
      return { kind: "text", value: context.label };
  }

  return (
    stateValue(name, context.state) ??
    telemetryValue(name, context.telemetry)
  );
}

function stateValue(name: string, state: DeviceState): TemplateValue | undefined {
  switch (name) {
    case "bias":
    case "biasEnabled":
      return { kind: "bool", value: state.biasEnabled };
    case "bp":
    case "biasPoint":
      return { kind: "text", value: state.biasPoint };
    case "amp":
    case "ditherAmp":
      return { kind: "float", value: state.ditherAmp };
    case "freq":
    case "ditherFreq":
      return { kind: "float", value: state.ditherFreq };
    case "mode":
      return { kind: "int", value: state.mode };
    case "gain":
      return { kind: "float", value: state.gain };
    case "offset":
      return { kind: "float", value: state.offset };
    default:
      return undefined;
  }
}

function telemetryValue(
  name: string,
  telemetry: Telemetry,
): TemplateValue | undefined {
  let value: number | null;

  switch (name) {
    case "theta0":
    case "theta":
      value = telemetry.theta0;
      break;
    case "temp":
    case "temperature":
      value = telemetry.temp;
      break;
    case "biasVoltage":
    case "vbias":
      value = telemetry.biasVoltage;
      break;
    case "inputPower":
    case "pin":
      value = telemetry.inputPower;
      break;
    case "outputPower":
    case "pout":
      value = telemetry.outputPower;
      break;
    case "biasCurrent":
    case "ibias":
    case "current":
      value = telemetry.biasCurrent;
      break;
    case "supplyVoltage":
    case "vsupply":
    case "supply":
      value = telemetry.supplyVoltage;
      break;
    case "phaseError":
    case "phase":
    case "perr":
      value = telemetry.phaseError;
      break;
    case "lockStatus":
    case "lock":
      value = telemetry.lockStatus;
      break;
    default:
      return undefined;
  }

  return value === null ? { kind: "missing" } : { kind: "float", value };
}

function formatValue(
  value: TemplateValue,
  format: string | null,
): string {
  const spec = format ?? "";

  switch (value.kind) {
    case "bool":
      return value.value ? "1" : "0";
    case "int":
      if (!spec) return String(value.value);
      return padInteger(value.value, parseWidth(spec));
    case "float":
      if (!spec) return defaultFloat(value.value);
      return value.value.toFixed(floatPrecision(spec));
    case "text":
      return value.value;
    case "missing":
      return "-";
  }
}

function defaultFloat(value: number): string {
  if (Object.is(value, -0)) return "-0";
  return String(value);
}

function parseWidth(spec: string): number {
  const digits = spec.startsWith("0") ? spec.slice(1) : spec;
  if (!/^\d+$/.test(digits)) {
    throw new Error(`unsupported integer format \`${spec}\` (use \`0N\`)`);
  }

  const width = Number(digits);
  if (width > 32) {
    throw new Error("integer width must be 0…32");
  }
  return width;
}

function padInteger(value: number, width: number): string {
  const sign = value < 0 ? "-" : "";
  const digits = String(Math.abs(value));
  return sign + digits.padStart(Math.max(0, width - sign.length), "0");
}

function floatPrecision(spec: string): number {
  if (!spec.startsWith(".")) {
    throw new Error(`unsupported float format \`${spec}\` (use \`.N\`)`);
  }

  const digits = spec.slice(1);
  if (!/^\d+$/.test(digits)) {
    throw new Error(`unsupported float precision \`${spec}\``);
  }

  const precision = Number(digits);
  if (precision > 12) {
    throw new Error("float precision must be 0…12");
  }
  return precision;
}
