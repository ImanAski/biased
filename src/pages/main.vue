<script setup lang="ts">
import Events from "../components/events.vue";
import MonitorCard from "../components/monitor-card.vue";
import Plot from "../components/Plot.vue";
import { state, status } from "../lib/device";
import { configErrors, monitorValue, widgetConfig } from "../lib/widgets";
</script>

<template>
  <div class="mb-4 flex justify-between">
    <div class="mb-4 flex flex-col justify-start">
      <span class="text-4xl font-semibold text-ink">Overview</span>
      <span class="text-md text-muted">
        Realtime telemetry &amp; bias control for {{ widgetConfig.title }}
      </span>
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

  <div
    v-if="configErrors.length > 0"
    class="mb-4 rounded-2xl border border-warning/40 bg-warning/10 px-4 py-3 text-sm text-warning"
  >
    Widget config has {{ configErrors.length }}
    {{ configErrors.length === 1 ? "problem" : "problems" }}. Showing the last
    good layout — fix it in Settings.
  </div>

  <!-- Monitors are defined in widgets.toml and fed by inbound TLM lines. -->
  <div v-if="widgetConfig.monitors.length > 0" class="mb-4 grid grid-cols-2 gap-4">
    <MonitorCard
      v-for="monitor in widgetConfig.monitors"
      :key="monitor.id"
      :title="monitor.label"
      :value="monitorValue(monitor)"
      :unit="monitor.unit"
      :digits="monitor.digits"
      :min="monitor.range?.min ?? null"
      :max="monitor.range?.max ?? null"
      :low="monitor.threshold?.low ?? null"
      :high="monitor.threshold?.high ?? null"
      :notes="monitor.description"
    />
  </div>

  <div
    v-else
    class="mb-4 rounded-2xl border border-dashed border-line px-4 py-8 text-center text-sm text-faint"
  >
    No monitors configured. Add a <code>[[monitors]]</code> entry to
    <code>widgets.toml</code>.
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
