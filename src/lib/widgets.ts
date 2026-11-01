import { reactive, ref } from "vue";
import { invoke } from "@tauri-apps/api/core";
import { inTauri, state, telemetry } from "./device";
import { addLog } from "./logs";

/* -------------------------------------------------------------------------- */
/* Types (mirrors src-tauri/src/config.rs)                                    */
/* -------------------------------------------------------------------------- */

export type MonitorSource =
  | "theta0"
  | "temp"
  | "biasVoltage"
  | "inputPower"
  | "outputPower";

export type ControlKind = "toggle" | "select" | "range";

export type ControlTarget =
  | "biasEnabled"
  | "biasPoint"
  | "ditherAmp"
  | "ditherFreq"
  | "mode";

export interface NumberRange {
  min: number;
  max: number;
  step: number | null;
}

export interface Threshold {
  low: number | null;
  high: number | null;
}

export interface MonitorWidget {
  id: string;
  label: string;
  source: MonitorSource;
  unit: string;
  digits: number;
  description: string;
  range: NumberRange | null;
  threshold: Threshold | null;
}

export interface ControlWidget {
  id: string;
  label: string;
  kind: ControlKind;
  target: ControlTarget;
  unit: string;
  description: string;
  options: string[];
  range: NumberRange | null;
}

export interface WidgetConfig {
  title: string;
  monitors: MonitorWidget[];
  controls: ControlWidget[];
}

export interface WidgetConfigEnvelope {
  path: string;
  config: WidgetConfig | null;
  errors: string[];
}

/* -------------------------------------------------------------------------- */
/* Built-in fallback (used in browser preview and before the file loads)      */
/* -------------------------------------------------------------------------- */

function fallbackConfig(): WidgetConfig {
  return {
    title: "MCU-04 · Bias stage",
    monitors: [
      {
        id: "theta0",
        label: "Θ₀",
        source: "theta0",
        unit: "°",
        digits: 1,
        description: "Measured angle",
        range: { min: -180, max: 180, step: null },
        threshold: { low: -90, high: 90 },
      },
      {
        id: "temp",
        label: "Temperature",
        source: "temp",
        unit: "°C",
        digits: 1,
        description: "Board temperature",
        range: { min: 0, max: 100, step: null },
        threshold: { low: 5, high: 75 },
      },
      {
        id: "input_power",
        label: "Input power",
        source: "inputPower",
        unit: "W",
        digits: 2,
        description: "Measured at input",
        range: { min: 0, max: 100, step: null },
        threshold: { low: null, high: 90 },
      },
      {
        id: "output_power",
        label: "Output power",
        source: "outputPower",
        unit: "W",
        digits: 2,
        description: "Measured at output",
        range: { min: 0, max: 100, step: null },
        threshold: { low: 5, high: 80 },
      },
    ],
    controls: [
      {
        id: "bias_enabled",
        label: "Bias output",
        kind: "toggle",
        target: "biasEnabled",
        unit: "",
        description: "Master enable for the bias stage",
        options: [],
        range: null,
      },
      {
        id: "bias_point",
        label: "Bias point",
        kind: "select",
        target: "biasPoint",
        unit: "",
        description: "Operating point of the bias loop",
        options: ["QUAD+", "QUAD-", "MAX", "NULL"],
        range: null,
      },
      {
        id: "dither_amp",
        label: "Dither amplitude",
        kind: "range",
        target: "ditherAmp",
        unit: "V",
        description: "Peak-to-peak dither amplitude",
        options: [],
        range: { min: 0, max: 10, step: 0.01 },
      },
      {
        id: "dither_freq",
        label: "Dither frequency",
        kind: "range",
        target: "ditherFreq",
        unit: "Hz",
        description: "Dither tone frequency",
        options: [],
        range: { min: 1, max: 5000, step: 1 },
      },
      {
        id: "mode",
        label: "Operating mode",
        kind: "select",
        target: "mode",
        unit: "",
        description: "Mode code passed through to the MCU",
        options: ["0", "1", "2", "3"],
        range: null,
      },
    ],
  };
}

/* -------------------------------------------------------------------------- */
/* Reactive state                                                            */
/* -------------------------------------------------------------------------- */

export const widgetConfig = reactive<WidgetConfig>(fallbackConfig());
export const configPath = ref<string>("built-in preview");
export const configErrors = ref<string[]>([]);
export const configLoading = ref(false);

const STORAGE_KEY = "bias-controller.widgets-path";
let initialized = false;

function storedPath(): string | null {
  if (typeof localStorage === "undefined") return null;
  try {
    return localStorage.getItem(STORAGE_KEY);
  } catch {
    return null;
  }
}

function rememberPath(path: string | null) {
  if (typeof localStorage === "undefined") return;
  try {
    if (path) localStorage.setItem(STORAGE_KEY, path);
    else localStorage.removeItem(STORAGE_KEY);
  } catch {
    // Storage can be unavailable; the path still works for this session.
  }
}

/* -------------------------------------------------------------------------- */
/* Loading                                                                   */
/* -------------------------------------------------------------------------- */

export async function loadWidgetConfig(
  path?: string | null,
  options: { quiet?: boolean } = {},
): Promise<boolean> {
  if (!inTauri) {
    configPath.value = "built-in preview";
    configErrors.value = [];
    return true;
  }

  configLoading.value = true;
  try {
    const envelope = await invoke<WidgetConfigEnvelope>("load_widget_config", {
      path: path ?? null,
    });

    configPath.value = envelope.path;
    configErrors.value = envelope.errors;

    if (!envelope.config) {
      if (!options.quiet) {
        addLog(
          "error",
          `Widget config rejected: ${envelope.errors[0] ?? "unknown error"}`,
        );
      }
      return false;
    }

    widgetConfig.title = envelope.config.title;
    widgetConfig.monitors = envelope.config.monitors;
    widgetConfig.controls = envelope.config.controls;

    if (path) rememberPath(envelope.path);

    if (!options.quiet) {
      addLog(
        "info",
        `Widget layout loaded: ${envelope.config.monitors.length} monitors, ${envelope.config.controls.length} controls`,
      );
    }
    return true;
  } catch (err) {
    configErrors.value = [String(err)];
    if (!options.quiet) {
      addLog("error", `Widget config failed to load: ${String(err)}`);
    }
    return false;
  } finally {
    configLoading.value = false;
  }
}

/** Load the remembered file, falling back to the seeded default. */
export async function initWidgets() {
  if (initialized) return;
  initialized = true;

  const saved = storedPath();
  if (!saved) {
    await loadWidgetConfig(null, { quiet: true });
    return;
  }

  const ok = await loadWidgetConfig(saved, { quiet: true });
  if (!ok) {
    rememberPath(null);
    await loadWidgetConfig(null, { quiet: true });
  }
}

/** Load the app's own widgets.toml and forget any user-selected path. */
export async function useDefaultConfig() {
  rememberPath(null);
  return loadWidgetConfig(null);
}

/* -------------------------------------------------------------------------- */
/* Monitor helpers                                                           */
/* -------------------------------------------------------------------------- */

export function monitorValue(monitor: MonitorWidget): number | null {
  const value = telemetry[monitor.source];
  return typeof value === "number" ? value : null;
}

/* -------------------------------------------------------------------------- */
/* Control helpers                                                           */
/* -------------------------------------------------------------------------- */

export function controlValue(
  control: ControlWidget,
): boolean | string | number {
  switch (control.target) {
    case "biasEnabled":
      return state.biasEnabled;
    case "biasPoint":
      return state.biasPoint;
    case "ditherAmp":
      return state.ditherAmp;
    case "ditherFreq":
      return state.ditherFreq;
    case "mode":
      return state.mode;
  }
}

/** Clamp and snap a raw numeric value to a control's configured range. */
export function clampToControl(
  control: ControlWidget,
  raw: number,
): number {
  if (!Number.isFinite(raw)) return 0;

  const range = control.range;
  if (!range) return raw;

  const min = Math.min(range.min, range.max);
  const max = Math.max(range.min, range.max);
  const step = range.step && range.step > 0 ? range.step : 1;

  const clamped = Math.min(max, Math.max(min, raw));
  return Number(
    (min + Math.round((clamped - min) / step) * step).toFixed(6),
  );
}

export function setControlValue(
  control: ControlWidget,
  value: boolean | string | number,
) {
  switch (control.target) {
    case "biasEnabled":
      state.biasEnabled = Boolean(value);
      break;
    case "biasPoint": {
      const point = String(value);
      if (point === "QUAD+" || point === "QUAD-" || point === "MAX" || point === "NULL") {
        state.biasPoint = point;
      }
      break;
    }
    case "ditherAmp":
      state.ditherAmp = clampToControl(control, Number(value));
      break;
    case "ditherFreq":
      state.ditherFreq = clampToControl(control, Number(value));
      break;
    case "mode":
      state.mode = Math.max(
        0,
        Math.min(255, Math.round(clampToControl(control, Number(value)))),
      );
      break;
  }
}

/** Display a numeric control value without trailing zeros. */
export function formatControlValue(value: number): string {
  return Number.isInteger(value) ? String(value) : value.toFixed(2);
}
