<script setup lang="ts">
import Events from "../components/events.vue";
import MonitorView from "../components/monitor-view.vue";
import Plot from "../components/Plot.vue";
import { state, status } from "../lib/device";
import { configErrors, locked, widgetConfig } from "../lib/widgets";
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

  <div v-for="plot in widgetConfig.plots" :key="plot.id" class="mb-4">
    <Plot :plot="plot" />
  </div>

  <div
    v-if="configErrors.length > 0"
    class="mb-4 rounded-2xl border border-warning/40 bg-warning/10 px-4 py-3 text-sm text-warning"
  >
    Widget config has {{ configErrors.length }}
    {{ configErrors.length === 1 ? "problem" : "problems" }}.
    <template v-if="locked">
      The layout embedded in this build is invalid — contact the supplier.
    </template>
    <template v-else>
      Showing the last good layout — fix it in Settings.
    </template>
  </div>

  <!-- Monitors are defined in widgets.toml and fed by inbound TLM lines. -->
  <div v-if="widgetConfig.monitors.length > 0" class="mb-4 grid grid-cols-2 gap-4">
    <MonitorView
      v-for="monitor in widgetConfig.monitors"
      :key="monitor.id"
      :monitor="monitor"
    />
  </div>

  <div
    v-else
    class="mb-4 rounded-2xl border border-dashed border-line px-4 py-8 text-center text-sm text-faint"
  >
    No monitors configured. Add a <code>[[monitors]]</code> entry to
    <code>widgets.toml</code>.
  </div>

  <!-- Pending command state, sent on demand and polled every tick. -->
  <div class="mb-4 flex items-center gap-3 text-xs text-faint">
    <span class="rounded bg-faint/20 px-2 py-1 text-muted">
      Query every {{ status.tickMs }} ms
    </span>
    <span>
      bias {{ state.biasEnabled ? "on" : "off" }} ·
      {{ state.biasPoint }} · dither {{ state.ditherAmp }} /
      {{ state.ditherFreq }} Hz · mode {{ state.mode }} · gain
      {{ state.gain }} · offset {{ state.offset }}
    </span>
  </div>

  <Events />
</template>
