<script setup lang="ts">
import { status } from "../lib/device";
import { addLog } from "../lib/logs";
import {
  DITHER_FIELDS,
  METRICS,
  type DitherKey,
  type MetricKey,
} from "../lib/metrics";
import { resetSettings, settings } from "../lib/settings";

const fieldClass =
  "w-full rounded-lg border border-line bg-field px-3 py-2 text-sm text-ink outline-none transition focus:border-accent";

function invalidRange(key: MetricKey): boolean {
  const range = settings.monitors[key];
  return range.min >= range.max;
}

function invalidControl(key: DitherKey): string | null {
  const range = settings.dither[key];
  if (range.min >= range.max) return "Min must be less than max.";
  if (range.step <= 0) return "Step must be greater than 0.";
  return null;
}

function resetDefaults() {
  resetSettings();
  addLog("info", "Settings reset to defaults");
}
</script>

<template>
  <div class="flex flex-col gap-4">
    <div class="flex flex-col">
      <span class="text-4xl font-semibold text-ink">Settings</span>
      <span class="text-muted"
        >Link defaults, monitoring ranges and device information</span
      >
    </div>

    <section class="space-y-3 rounded-2xl border border-line bg-panel p-5">
      <span class="text-xs font-bold uppercase text-muted"
        >Serial defaults</span
      >

      <label class="block max-w-xs space-y-1">
        <span class="text-sm text-muted">Default baud rate</span>
        <input
          v-model.number="settings.defaultBaud"
          type="number"
          min="300"
          step="100"
          :class="fieldClass"
        />
        <span class="block text-xs text-faint">
          Pre-filled when connecting. The live link uses the value chosen in the
          control panel.
        </span>
      </label>
    </section>

    <section class="space-y-3 rounded-2xl border border-line bg-panel p-5">
      <span class="text-xs font-bold uppercase text-muted"
        >Monitoring ranges</span
      >
      <p class="text-xs text-faint">
        Each card shows a position bar within this range and flags values as out
        of range.
      </p>

      <div class="grid gap-3 md:grid-cols-2">
        <div
          v-for="metric in METRICS"
          :key="metric.key"
          class="space-y-2 rounded-xl border border-line bg-field p-3"
        >
          <div class="flex items-center justify-between">
            <span class="text-sm font-medium text-ink">{{ metric.label }}</span>
            <span class="text-xs text-faint">{{ metric.unit }}</span>
          </div>

          <div class="grid grid-cols-2 gap-2">
            <label class="space-y-1">
              <span class="text-xs text-muted">Min</span>
              <input
                v-model.number="settings.monitors[metric.key].min"
                type="number"
                step="any"
                :class="fieldClass"
              />
            </label>
            <label class="space-y-1">
              <span class="text-xs text-muted">Max</span>
              <input
                v-model.number="settings.monitors[metric.key].max"
                type="number"
                step="any"
                :class="fieldClass"
              />
            </label>
          </div>

          <p v-if="invalidRange(metric.key)" class="text-xs text-warning">
            Min must be less than max.
          </p>
        </div>
      </div>
    </section>

    <section class="space-y-3 rounded-2xl border border-line bg-panel p-5">
      <span class="text-xs font-bold uppercase text-muted"
        >Dither controls</span
      >
      <p class="text-xs text-faint">
        Limits and step for the amplitude and frequency controls in the control
        panel.
      </p>

      <div class="grid gap-3 md:grid-cols-2">
        <div
          v-for="field in DITHER_FIELDS"
          :key="field.key"
          class="space-y-2 rounded-xl border border-line bg-field p-3"
        >
          <div class="flex items-center justify-between">
            <span class="text-sm font-medium text-ink">{{ field.label }}</span>
            <span class="text-xs text-faint">{{ field.unit }}</span>
          </div>

          <div class="grid grid-cols-3 gap-2">
            <label class="space-y-1">
              <span class="text-xs text-muted">Min</span>
              <input
                v-model.number="settings.dither[field.key].min"
                type="number"
                step="any"
                :class="fieldClass"
              />
            </label>
            <label class="space-y-1">
              <span class="text-xs text-muted">Max</span>
              <input
                v-model.number="settings.dither[field.key].max"
                type="number"
                step="any"
                :class="fieldClass"
              />
            </label>
            <label class="space-y-1">
              <span class="text-xs text-muted">Step</span>
              <input
                v-model.number="settings.dither[field.key].step"
                type="number"
                step="any"
                :class="fieldClass"
              />
            </label>
          </div>

          <p v-if="invalidControl(field.key)" class="text-xs text-warning">
            {{ invalidControl(field.key) }}
          </p>
        </div>
      </div>
    </section>

    <section class="space-y-2 rounded-2xl border border-line bg-panel p-5">
      <span class="text-xs font-bold uppercase text-muted">Master clock</span>
      <div class="flex justify-between text-sm">
        <span class="text-muted">State frame period</span>
        <span class="text-ink">{{ status.tickMs }} ms</span>
      </div>
      <div class="flex justify-between text-sm">
        <span class="text-muted">Role</span>
        <span class="text-ink">Serial master</span>
      </div>
    </section>

    <section class="space-y-2 rounded-2xl border border-line bg-panel p-5">
      <span class="text-xs font-bold uppercase text-muted">Device</span>
      <div class="flex justify-between text-sm">
        <span class="text-muted">Controller</span>
        <span class="text-ink">MCU-04</span>
      </div>
      <div class="flex justify-between text-sm">
        <span class="text-muted">Application</span>
        <span class="text-ink">v0.0.1a</span>
      </div>
    </section>

    <button
      class="w-fit rounded-lg border border-line px-4 py-2 text-sm text-muted transition hover:border-line-strong hover:text-ink"
      @click="resetDefaults"
    >
      Reset to defaults
    </button>
  </div>
</template>
