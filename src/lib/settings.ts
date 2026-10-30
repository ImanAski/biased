import { reactive, watch } from "vue";
import type { DitherKey, MetricKey } from "./metrics";

export interface MonitorRange {
  min: number;
  max: number;
}

export interface ControlRange {
  min: number;
  max: number;
  step: number;
}

export interface AppSettings {
  /** Baud rate pre-filled in the connect panel. */
  defaultBaud: number;
  /** Allowed display range for each monitoring card. */
  monitors: Record<MetricKey, MonitorRange>;
  /** Allowed range and step for each dither control. */
  dither: Record<DitherKey, ControlRange>;
}

const STORAGE_KEY = "bias-controller.settings";

const DEFAULTS: AppSettings = {
  defaultBaud: 115200,
  monitors: {
    theta0: { min: -180, max: 180 },
    temp: { min: 0, max: 100 },
    inputPower: { min: 0, max: 100 },
    outputPower: { min: 0, max: 100 },
  },
  dither: {
    amp: { min: 0, max: 10, step: 0.01 },
    freq: { min: 1, max: 5000, step: 1 },
  },
};

function defaults(): AppSettings {
  return {
    defaultBaud: DEFAULTS.defaultBaud,
    monitors: {
      theta0: { ...DEFAULTS.monitors.theta0 },
      temp: { ...DEFAULTS.monitors.temp },
      inputPower: { ...DEFAULTS.monitors.inputPower },
      outputPower: { ...DEFAULTS.monitors.outputPower },
    },
    dither: {
      amp: { ...DEFAULTS.dither.amp },
      freq: { ...DEFAULTS.dither.freq },
    },
  };
}

function load(): AppSettings {
  const base = defaults();
  if (typeof localStorage === "undefined") return base;

  try {
    const raw = localStorage.getItem(STORAGE_KEY);
    if (!raw) return base;

    const parsed = JSON.parse(raw) as {
      defaultBaud?: unknown;
      monitors?: Partial<Record<MetricKey, Partial<MonitorRange>>>;
      dither?: Partial<Record<DitherKey, Partial<ControlRange>>>;
    };

    if (typeof parsed.defaultBaud === "number") {
      base.defaultBaud = parsed.defaultBaud;
    }

    for (const key of Object.keys(base.monitors) as MetricKey[]) {
      const saved = parsed.monitors?.[key];
      if (saved && typeof saved.min === "number") base.monitors[key].min = saved.min;
      if (saved && typeof saved.max === "number") base.monitors[key].max = saved.max;
    }

    for (const key of Object.keys(base.dither) as DitherKey[]) {
      const saved = parsed.dither?.[key];
      if (saved && typeof saved.min === "number") base.dither[key].min = saved.min;
      if (saved && typeof saved.max === "number") base.dither[key].max = saved.max;
      if (saved && typeof saved.step === "number") base.dither[key].step = saved.step;
    }

    return base;
  } catch {
    return base;
  }
}

/** Persisted user preferences. */
export const settings = reactive<AppSettings>(load());

export function resetSettings() {
  const fresh = defaults();
  settings.defaultBaud = fresh.defaultBaud;
  settings.monitors = fresh.monitors;
  settings.dither = fresh.dither;
}

watch(
  settings,
  (value) => {
    try {
      localStorage.setItem(STORAGE_KEY, JSON.stringify(value));
    } catch {
      // Storage can be unavailable (private mode, quota); ignore.
    }
  },
  { deep: true },
);
