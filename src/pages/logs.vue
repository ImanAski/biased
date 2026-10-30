<script setup lang="ts">
import { computed, ref } from "vue";
import { invoke } from "@tauri-apps/api/core";
import { save } from "@tauri-apps/plugin-dialog";
import { addLog, clearLogs, formatLogs, logs } from "../lib/logs";
import { inTauri } from "../lib/device";

const exporting = ref(false);
const exportError = ref<string | null>(null);

const hasLogs = computed(() => logs.length > 0);

const levelClass: Record<string, string> = {
  info: "text-slate-300",
  warn: "text-amber-300",
  error: "text-red-400",
};

function formatTime(ms: number): string {
  return new Date(ms).toLocaleTimeString([], { hour12: false });
}

async function exportLogs() {
  exportError.value = null;

  if (!inTauri) {
    exportError.value = "Export is only available in the desktop app.";
    return;
  }

  exporting.value = true;
  try {
    const stamp = new Date().toISOString().replace(/[:.]/g, "-");
    const path = await save({
      title: "Export logs",
      defaultPath: `bias-controller-${stamp}.log`,
      filters: [{ name: "Log file", extensions: ["log", "txt"] }],
    });

    if (!path) return; // user cancelled

    await invoke("write_text_file", { path, contents: formatLogs() });
    addLog("info", `Logs exported to ${path}`);
  } catch (err) {
    exportError.value = String(err);
  } finally {
    exporting.value = false;
  }
}
</script>

<template>
  <div class="flex flex-col gap-4">
    <div class="flex items-end justify-between">
      <div class="flex flex-col">
        <span class="text-4xl font-semibold text-white">Logs</span>
        <span class="text-slate-400">Session events and export</span>
      </div>

      <div class="flex items-center gap-2">
        <button
          class="rounded-lg border border-slate-600 px-3 py-2 text-sm text-slate-300 transition hover:border-slate-400 hover:text-white disabled:opacity-40"
          :disabled="!hasLogs"
          @click="clearLogs"
        >
          Clear
        </button>
        <button
          class="rounded-lg bg-teal-600 px-4 py-2 text-sm font-semibold text-white transition hover:bg-teal-500 disabled:opacity-40"
          :disabled="!hasLogs || exporting"
          @click="exportLogs"
        >
          {{ exporting ? "Exporting…" : "Export" }}
        </button>
      </div>
    </div>

    <p v-if="exportError" class="text-sm text-red-400">{{ exportError }}</p>

    <div
      class="overflow-hidden rounded-2xl border border-slate-400 bg-[#222A3A]"
    >
      <div
        class="flex items-center justify-between border-b border-slate-600 px-4 py-2 text-xs text-slate-400"
      >
        <span class="font-bold uppercase">Session log</span>
        <span>{{ logs.length }} entries</span>
      </div>

      <div
        v-if="hasLogs"
        class="max-h-[60vh] overflow-auto px-4 py-3 font-mono text-xs"
      >
        <div
          v-for="entry in logs"
          :key="entry.id"
          class="flex gap-3 py-0.5"
        >
          <span class="shrink-0 text-slate-500">{{ formatTime(entry.time) }}</span>
          <span
            class="w-12 shrink-0 uppercase"
            :class="levelClass[entry.level]"
            >{{ entry.level }}</span
          >
          <span class="whitespace-pre-wrap break-words text-slate-200">{{
            entry.message
          }}</span>
        </div>
      </div>

      <div v-else class="px-4 py-10 text-center text-sm text-slate-500">
        No events yet. Connect to the MCU to populate the log.
      </div>
    </div>
  </div>
</template>
