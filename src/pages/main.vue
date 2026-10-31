<script setup lang="ts">
import Events from "../components/events.vue";
import Plot from "../components/Plot.vue";
import Power from "../components/power.vue";
import { state, status, telemetry } from "../lib/device";
import { METRICS, type MetricKey } from "../lib/metrics";
import { settings } from "../lib/settings";

const notes: Record<MetricKey, string> = {
  theta0: "Measured angle",
  temp: "Measured temperature",
  inputPower: "Measured at input",
  outputPower: "Measured at output",
};
</script>

<template>
  <div class="mb-4 flex justify-between">
    <div class="mb-4 flex flex-col justify-start">
      <span class="text-4xl font-semibold text-ink">Overview</span>
      <span class="text-md text-muted"
        >Realtime telemetry & bias control for MCU-04</span
      >
    </div>
    <span
      class="h-fit rounded-2xl px-3 py-1 text-sm"
      :class="
        status.connected
          ? 'bg-success/15 text-success'
          : 'bg-faint/20 text-muted'
      "
    >
      {{ status.connected ? "Master active" : "Idle" }}
    </span>
  </div>

  <div class="mb-4">
    <Plot />
  </div>

  <!-- Monitored values reported by the MCU (read-only, range set in Settings). -->
  <div class="mb-4 grid grid-cols-2 gap-4">
    <Power
      v-for="metric in METRICS"
      :key="metric.key"
      :title="metric.label"
      :value="telemetry[metric.key]"
      :unit="metric.unit"
      :digits="metric.digits"
      :min="settings.monitors[metric.key].min"
      :max="settings.monitors[metric.key].max"
      :notes="notes[metric.key]"
    />
  </div>

  <!-- Control state currently being sent to the MCU every tick. -->
  <div class="mb-4 flex items-center gap-3 text-xs text-faint">
    <span class="rounded bg-faint/20 px-2 py-1 text-muted">
      TX every {{ status.tickMs }} ms
    </span>
    <span>
      bias {{ state.biasEnabled ? "on" : "off" }} ·
      {{ state.biasPoint }} · dither {{ state.ditherAmp }} /
      {{ state.ditherFreq }} Hz · mode {{ state.mode }}
    </span>
  </div>

  <Events />
</template>
