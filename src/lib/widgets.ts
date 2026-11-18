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
  | "outputPower"
  | "biasCurrent"
  | "supplyVoltage"
  | "phaseError"
  | "lockStatus";

export type MonitorVariant =
  | "bar"
  | "gauge"
  | "sparkline"
  | "led"
  | "tile"
  | "meter";

export type ControlKind =
  | "toggle"
  | "select"
  | "range"
  | "stepper"
  | "knob"
  | "segmented";

export type ControlTarget =
  | "biasEnabled"
  | "biasPoint"
  | "ditherAmp"
  | "ditherFreq"
  | "mode"
  | "gain"
  | "offset";

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
  variant: MonitorVariant;
  /** Samples retained by the sparkline variant. */
  history: number;
  /** Cells drawn by the meter variant. */
  segments: number;
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
  /** Quick-set values shown under range and stepper controls. */
  presets: number[];
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

export interface BuildInfo {
  locked: boolean;
  /** Developer tools (Packet Lab) are available only in editable builds. */
  packetLab: boolean;
  debug: boolean;
  configSource: string | null;
  exportName: string;
}

export interface LockedExport {
  path: string;
  bytes: number;
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
        variant: "bar",
        history: 60,
        segments: 20,
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
        variant: "gauge",
        history: 60,
        segments: 20,
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
        variant: "meter",
        history: 60,
        segments: 24,
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
        variant: "sparkline",
        history: 60,
        segments: 20,
        range: { min: 0, max: 100, step: null },
        threshold: { low: 5, high: 80 },
      },
      {
        id: "bias_current",
        label: "Bias current",
        source: "biasCurrent",
        unit: "A",
        digits: 3,
        description: "Current through the bias stage",
        variant: "tile",
        history: 60,
        segments: 20,
        range: { min: 0, max: 1, step: null },
        threshold: { low: null, high: 0.8 },
      },
      {
        id: "supply_voltage",
        label: "Supply voltage",
        source: "supplyVoltage",
        unit: "V",
        digits: 2,
        description: "Rail feeding the output stage",
        variant: "sparkline",
        history: 60,
        segments: 20,
        range: { min: 0, max: 15, step: null },
        threshold: { low: 11, high: 13 },
      },
      {
        id: "phase_error",
        label: "Phase error",
        source: "phaseError",
        unit: "°",
        digits: 2,
        description: "Residual loop phase error",
        variant: "gauge",
        history: 60,
        segments: 20,
        range: { min: -10, max: 10, step: null },
        threshold: { low: -2, high: 2 },
      },
      {
        id: "lock_status",
        label: "Loop lock",
        source: "lockStatus",
        unit: "",
        digits: 0,
        description: "MCU reports a locked loop",
        variant: "led",
        history: 60,
        segments: 20,
        range: null,
        threshold: null,
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
        presets: [],
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
        presets: [],
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
        presets: [0, 0.5, 1, 5],
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
        presets: [10, 100, 1000, 2500],
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
        presets: [],
        range: null,
      },
      {
        id: "gain",
        label: "Loop gain",
        kind: "knob",
        target: "gain",
        unit: "×",
        description: "Error amplifier gain",
        options: [],
        presets: [0.5, 1, 2],
        range: { min: 0, max: 4, step: 0.05 },
      },
      {
        id: "offset",
        label: "Loop offset",
        kind: "stepper",
        target: "offset",
        unit: "V",
        description: "Fine trim added to the bias setpoint",
        options: [],
        presets: [-1, 0, 1],
        range: { min: -5, max: 5, step: 0.1 },
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

/** True when this executable carries an uneditable, embedded layout. */
export const locked = ref(false);
/** True when developer tools such as the Packet Lab are available. */
export const packetLab = ref(!inTauri);
/** True once `get_build_info` has resolved, so the UI can hide editor tools. */
export const buildInfoReady = ref(false);
export const debugBuild = ref(false);
export const configSource = ref<string | null>(null);
export const exportName = ref("tauri-biased-customer");

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

    if (path && !locked.value) rememberPath(envelope.path);

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

  if (inTauri) {
    try {
      const info = await invoke<BuildInfo>("get_build_info");
      locked.value = info.locked;
      // Older backends do not report the flag; a browser preview stays editable.
      packetLab.value = info.packetLab ?? true;
      debugBuild.value = info.debug;
      configSource.value = info.configSource;
      exportName.value = info.exportName;
    } catch (err) {
      addLog("warn", `Build information unavailable: ${String(err)}`);
    }
  }
  buildInfoReady.value = true;

  // A customer build always uses its embedded layout; any remembered path on
  // the machine is irrelevant.
  if (locked.value) {
    await loadWidgetConfig(null, { quiet: true });
    return;
  }

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
  if (locked.value) return loadWidgetConfig(null);
  rememberPath(null);
  return loadWidgetConfig(null);
}

/**
 * Copy this executable to `target` with the active layout embedded.
 *
 * The backend refuses the operation in a locked build, in a debug build and
 * when the layout fails validation.
 */
export function exportLockedBuild(target: string): Promise<LockedExport> {
  const source = configPath.value;
  const configPathArg =
    source === "built-in preview" || source === "embedded configuration"
      ? null
      : source;

  return invoke<LockedExport>("export_locked_build", {
    target,
    configPath: configPathArg,
  });
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
    case "gain":
      return state.gain;
    case "offset":
      return state.offset;
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
    case "gain":
      state.gain = clampToControl(control, Number(value));
      break;
    case "offset":
      state.offset = clampToControl(control, Number(value));
      break;
  }
}

/** Display a numeric control value without trailing zeros. */
export function formatControlValue(value: number): string {
  return Number.isInteger(value) ? String(value) : value.toFixed(2);
}
