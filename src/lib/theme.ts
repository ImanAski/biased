import { ref, watch } from "vue";

export type Theme = "light" | "dark";

const STORAGE_KEY = "bias-controller.theme";

function initialTheme(): Theme {
  if (typeof localStorage !== "undefined") {
    const saved = localStorage.getItem(STORAGE_KEY);
    if (saved === "light" || saved === "dark") return saved;
  }

  if (
    typeof window !== "undefined" &&
    window.matchMedia?.("(prefers-color-scheme: light)").matches
  ) {
    return "light";
  }

  return "dark";
}

export const theme = ref<Theme>(initialTheme());

function apply(value: Theme) {
  document.documentElement.classList.toggle("dark", value === "dark");
}

export function toggleTheme() {
  theme.value = theme.value === "dark" ? "light" : "dark";
}

/** Apply the persisted/system theme and keep <html> in sync. */
export function initTheme() {
  apply(theme.value);

  watch(theme, (value) => {
    apply(value);
    try {
      localStorage.setItem(STORAGE_KEY, value);
    } catch {
      // Storage can be unavailable; the in-memory theme still works.
    }
  });
}
