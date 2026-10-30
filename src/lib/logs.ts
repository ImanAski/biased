import { reactive } from "vue";

export type LogLevel = "info" | "warn" | "error";

export interface LogEntry {
  id: number;
  time: number;
  level: LogLevel;
  message: string;
}

/** Keep memory bounded for long sessions. */
const MAX_ENTRIES = 1000;

let nextId = 1;

/** In-memory session log, newest last. */
export const logs = reactive<LogEntry[]>([]);

export function addLog(level: LogLevel, message: string) {
  logs.push({ id: nextId++, time: Date.now(), level, message });
  if (logs.length > MAX_ENTRIES) {
    logs.splice(0, logs.length - MAX_ENTRIES);
  }
}

export function clearLogs() {
  logs.splice(0, logs.length);
}

/** Plain-text export, one event per line with an ISO timestamp. */
export function formatLogs(): string {
  const header = `# Bias Controller log — exported ${new Date().toISOString()}\n`;
  const body = logs
    .map(
      (entry) =>
        `${new Date(entry.time).toISOString()}\t${entry.level.toUpperCase()}\t${entry.message}`,
    )
    .join("\n");
  return `${header}${body}${body ? "\n" : ""}`;
}
