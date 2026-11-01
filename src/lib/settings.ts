import { reactive, watch } from "vue";

/**
 * Persisted app preferences.
 *
 * Widget ranges and thresholds live in `widgets.toml` (see `lib/widgets.ts`),
 * so only link-level defaults remain here.
 */
export interface AppSettings {
  /** Baud rate pre-filled in the connect panel. */
  defaultBaud: number;
}

const STORAGE_KEY = "bias-controller.settings";

const DEFAULTS: AppSettings = {
  defaultBaud: 115200,
};

function defaults(): AppSettings {
  return { ...DEFAULTS };
}

function load(): AppSettings {
  const base = defaults();
  if (typeof localStorage === "undefined") return base;

  try {
    const raw = localStorage.getItem(STORAGE_KEY);
    if (!raw) return base;

    const parsed = JSON.parse(raw) as { defaultBaud?: unknown };
    if (typeof parsed.defaultBaud === "number") {
      base.defaultBaud = parsed.defaultBaud;
    }

    return base;
  } catch {
    return base;
  }
}

/** Persisted user preferences. */
export const settings = reactive<AppSettings>(load());

export function resetSettings() {
  Object.assign(settings, defaults());
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
