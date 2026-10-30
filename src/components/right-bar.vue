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
  const snapped = Number((min + Math.round((clamped - min) / step) * step).toFixed(6));

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
  "w-full rounded-lg border border-slate-600 bg-[#12161F] px-3 py-2 text-sm text-white outline-none focus:border-teal-400 disabled:opacity-50";
</script>

<template>
  <div class="flex h-full min-w-80 flex-col gap-4 px-3 py-3">
    <span class="text-2xl font-semibold text-white">Control</span>

    <!-- Serial link -->
    <section class="space-y-2 rounded-2xl border border-slate-600 bg-[#222A3A] p-3">
      <div class="flex items-center justify-between">
        <span class="text-xs font-bold uppercase text-slate-400">Serial link</span>
        <button
          class="rounded px-1 text-slate-400 hover:text-white disabled:opacity-40"
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
              ? 'bg-red-500/80 hover:bg-red-500'
              : 'bg-teal-600 hover:bg-teal-500'
          "
          :disabled="busy || (!status.connected && !canConnect)"
          @click="toggleConnection"
        >
          {{ status.connected ? "Disconnect" : "Connect" }}
        </button>
      </div>

      <p v-if="uiError" class="text-xs text-red-400">{{ uiError }}</p>
      <p v-else-if="status.lastError" class="text-xs text-red-400">
        {{ status.lastError }}
      </p>
    </section>

    <!-- Bias point -->
    <section class="space-y-3 rounded-2xl border border-slate-600 bg-[#222A3A] p-3">
      <div class="flex items-center justify-between">
        <span class="text-xs font-bold uppercase text-slate-400">Bias</span>
        <button
          class="relative h-6 w-11 rounded-full transition"
          :class="state.biasEnabled ? 'bg-teal-500' : 'bg-slate-600'"
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
              ? 'border-teal-400/60 bg-teal-500/20 text-teal-200'
              : 'border-slate-600 bg-[#12161F] text-slate-400 hover:border-slate-500'
          "
          @click="state.biasPoint = point"
        >
          {{ point }}
        </button>
      </div>
    </section>

    <!-- Dither -->
    <section class="space-y-3 rounded-2xl border border-slate-600 bg-[#222A3A] p-3">
      <span class="text-xs font-bold uppercase text-slate-400">Dither</span>

      <div
        v-for="field in DITHER_FIELDS"
        :key="field.key"
        class="space-y-1"
      >
        <div class="flex justify-between text-xs text-slate-400">
          <span>{{ field.label }}</span>
          <span class="text-white">
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
      class="rounded-lg border border-teal-400/60 bg-teal-500/15 py-2 text-sm font-semibold text-teal-200 transition hover:bg-teal-500/25 disabled:opacity-40"
      :disabled="!status.connected"
      @click="calibrate"
    >
      Calibrate
    </button>

    <button
      class="rounded-lg border border-slate-600 py-2 text-sm text-slate-300 hover:border-slate-400 hover:text-white disabled:opacity-40"
      :disabled="!status.connected"
      @click="sendNow"
    >
      Send now
    </button>
  </div>
</template>
