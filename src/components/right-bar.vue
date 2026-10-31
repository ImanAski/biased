<script setup lang="ts">
import { computed, onMounted, ref, watch } from "vue";
import {
  calibrate,
  connect,
  disconnect,
  ports,
  refreshPorts,
  sendNow,
  state,
  status,
  uiError,
} from "../lib/device";
import { BIAS_POINTS, DITHER_FIELDS, type DitherKey } from "../lib/metrics";
import { settings } from "../lib/settings";

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

/* ------------------------------ Dither ------------------------------ */

function ditherValue(key: DitherKey): number {
  return key === "amp" ? state.ditherAmp : state.ditherFreq;
}

/** Clamp to the configured range and snap to the configured step. */
function applyDither(key: DitherKey, raw: number) {
  const range = settings.dither[key];
  const min = Math.min(range.min, range.max);
  const max = Math.max(range.min, range.max);
  const step = range.step > 0 ? range.step : 1;

  if (!Number.isFinite(raw)) return;

  const clamped = Math.min(max, Math.max(min, raw));
  const snapped = Number(
    (min + Math.round((clamped - min) / step) * step).toFixed(6),
  );

  if (key === "amp") state.ditherAmp = snapped;
  else state.ditherFreq = snapped;
}

function onDitherInput(key: DitherKey, event: Event) {
  const target = event.target as HTMLInputElement;
  applyDither(key, Number(target.value));
}

/** Re-clamp when the user edits min/max/step in Settings. */
watch(
  () => settings.dither,
  () => {
    applyDither("amp", state.ditherAmp);
    applyDither("freq", state.ditherFreq);
  },
  { deep: true },
);

function displayValue(value: number): string {
  return Number.isInteger(value) ? String(value) : value.toFixed(2);
}

const fieldClass =
  "w-full rounded-lg border border-line bg-field px-3 py-2 text-sm text-ink outline-none transition focus:border-accent disabled:opacity-50";
</script>

<template>
  <div class="flex h-full min-w-80 flex-col gap-4 px-3 py-3">
    <span class="text-2xl font-semibold text-ink">Control</span>

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
    </section>

    <!-- Bias point -->
    <section class="space-y-3 rounded-2xl border border-line bg-panel p-3">
      <div class="flex items-center justify-between">
        <span class="text-xs font-bold uppercase text-muted">Bias</span>
        <button
          class="relative h-6 w-11 rounded-full transition"
          :class="state.biasEnabled ? 'bg-teal-500' : 'bg-line'"
          role="switch"
          :aria-checked="state.biasEnabled"
          @click="state.biasEnabled = !state.biasEnabled"
        >
          <span
            class="absolute left-0.5 top-0.5 h-5 w-5 rounded-full bg-white transition-transform"
            :class="state.biasEnabled ? 'translate-x-5' : 'translate-x-0'"
          />
        </button>
      </div>

      <div class="grid grid-cols-2 gap-2">
        <button
          v-for="point in BIAS_POINTS"
          :key="point"
          class="rounded-lg border px-2 py-2 text-sm font-medium transition"
          :class="
            state.biasPoint === point
              ? 'border-accent/50 bg-accent/15 text-accent'
              : 'border-line bg-field text-muted hover:border-line-strong hover:text-ink'
          "
          @click="state.biasPoint = point"
        >
          {{ point }}
        </button>
      </div>
    </section>

    <!-- Dither -->
    <section class="space-y-3 rounded-2xl border border-line bg-panel p-3">
      <span class="text-xs font-bold uppercase text-muted">Dither</span>

      <div v-for="field in DITHER_FIELDS" :key="field.key" class="space-y-1">
        <div class="flex justify-between text-xs text-muted">
          <span>{{ field.label }}</span>
          <span class="text-ink">
            {{ displayValue(ditherValue(field.key)) }} {{ field.unit }}
          </span>
        </div>
        <input
          type="range"
          class="w-full accent-teal-500"
          :min="Math.min(settings.dither[field.key].min, settings.dither[field.key].max)"
          :max="Math.max(settings.dither[field.key].min, settings.dither[field.key].max)"
          :step="settings.dither[field.key].step > 0 ? settings.dither[field.key].step : 1"
          :value="ditherValue(field.key)"
          @input="onDitherInput(field.key, $event)"
        />
        <input
          type="number"
          :class="fieldClass"
          :min="Math.min(settings.dither[field.key].min, settings.dither[field.key].max)"
          :max="Math.max(settings.dither[field.key].min, settings.dither[field.key].max)"
          :step="settings.dither[field.key].step > 0 ? settings.dither[field.key].step : 1"
          :value="ditherValue(field.key)"
          @change="onDitherInput(field.key, $event)"
        />
      </div>
    </section>

    <button
      class="rounded-lg border border-line py-2 text-sm text-muted transition hover:border-line-strong hover:text-ink disabled:opacity-40"
      :disabled="!status.connected"
      @click="sendNow"
    >
      Send now
    </button>

    <button
      class="rounded-lg border border-accent/40 bg-accent/15 py-2 text-sm font-semibold text-accent transition hover:bg-accent/25 disabled:opacity-40"
      :disabled="!status.connected"
      @click="calibrate"
    >
      Calibrate
    </button>
  </div>
</template>
