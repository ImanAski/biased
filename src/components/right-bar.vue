<script setup lang="ts">
import { computed, onMounted, ref } from "vue";
import ControlWidget from "./control-widget.vue";
import {
  calibrate,
  connect,
  disconnect,
  ports,
  refreshPorts,
  sendNow,
  status,
  uiError,
} from "../lib/device";
import { settings } from "../lib/settings";
import { rxTotals } from "../lib/packets";
import { widgetConfig } from "../lib/widgets";

const selectedPort = ref("");
const baud = ref(settings.defaultBaud);
const busy = ref(false);

const canConnect = computed(() => selectedPort.value.length > 0);

onMounted(async () => {
  await refreshPorts();
  if (!selectedPort.value && ports.value.length > 0) {
    selectedPort.value = ports.value[0].name;
  }
  baud.value = status.baud || settings.defaultBaud;
});

async function toggleConnection() {
  busy.value = true;
  try {
    if (status.connected) {
      await disconnect();
    } else {
      await connect(selectedPort.value, Number(baud.value));
    }
  } catch {
    // Error is surfaced through `uiError`.
  } finally {
    busy.value = false;
  }
}

async function rescan() {
  await refreshPorts();
  if (!selectedPort.value && ports.value.length > 0) {
    selectedPort.value = ports.value[0].name;
  }
}

const fieldClass =
  "w-full rounded-lg border border-line bg-field px-3 py-2 text-sm text-ink outline-none transition focus:border-accent disabled:opacity-50";
</script>

<template>
  <div class="flex h-full min-w-80 flex-col gap-3 overflow-y-auto px-3 py-3">
    <div class="flex items-baseline justify-between">
      <span class="text-2xl font-semibold text-ink">Control</span>
      <span class="text-[10px] uppercase tracking-wide text-faint">
        {{ widgetConfig.controls.length }} widgets
      </span>
    </div>

    <!-- Serial link -->
    <section class="space-y-2 rounded-2xl border border-line bg-panel p-3">
      <div class="flex items-center justify-between">
        <span class="text-xs font-bold uppercase text-muted">Serial link</span>
        <button
          class="rounded px-1 text-muted transition hover:text-ink disabled:opacity-40"
          :disabled="status.connected"
          title="Rescan ports"
          @click="rescan"
        >
          ↻
        </button>
      </div>

      <select
        v-model="selectedPort"
        :disabled="status.connected"
        :class="fieldClass"
      >
        <option v-if="ports.length === 0" value="">No ports found</option>
        <option v-for="port in ports" :key="port.name" :value="port.name">
          {{ port.name }}{{ port.product ? ` — ${port.product}` : "" }}
        </option>
      </select>

      <div class="flex gap-2">
        <input
          v-model.number="baud"
          type="number"
          :disabled="status.connected"
          :class="fieldClass"
          aria-label="Baud rate"
        />
        <button
          class="shrink-0 rounded-lg px-4 py-2 text-sm font-semibold text-white transition disabled:opacity-40"
          :class="
            status.connected
              ? 'bg-danger/80 hover:bg-danger'
              : 'bg-teal-600 hover:bg-teal-500'
          "
          :disabled="busy || (!status.connected && !canConnect)"
          @click="toggleConnection"
        >
          {{ status.connected ? "Disconnect" : "Connect" }}
        </button>
      </div>

      <p v-if="uiError" class="text-xs text-danger">{{ uiError }}</p>
      <p v-else-if="status.lastError" class="text-xs text-danger">
        {{ status.lastError }}
      </p>

      <div class="flex justify-between font-mono text-[10px] text-faint">
        <span>tx {{ status.txs.toLocaleString() }}</span>
        <span>rx {{ rxTotals.chunks.toLocaleString() }}</span>
      </div>
    </section>

    <!-- Widgets from widgets.toml -->
    <ControlWidget
      v-for="control in widgetConfig.controls"
      :key="control.id"
      :control="control"
    />

    <div
      v-if="widgetConfig.controls.length === 0"
      class="rounded-2xl border border-dashed border-line px-3 py-6 text-center text-xs text-faint"
    >
      No controls configured. Add a <code>[[controls]]</code> entry to
      <code>widgets.toml</code>.
    </div>

    <div class="mt-auto space-y-2 pt-1">
      <button
        class="w-full rounded-lg border border-line py-2 text-sm text-muted transition hover:border-line-strong hover:text-ink disabled:opacity-40"
        :disabled="!status.connected"
        @click="sendNow"
      >
        Send state now
      </button>

      <button
        class="w-full rounded-lg border border-accent/40 bg-accent/15 py-2 text-sm font-semibold text-accent transition hover:bg-accent/25 disabled:opacity-40"
        :disabled="!status.connected"
        @click="calibrate"
      >
        Calibrate
      </button>
    </div>
  </div>
</template>
