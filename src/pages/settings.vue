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
  "w-full rounded-lg border border-slate-600 bg-[#12161F] px-3 py-2 text-sm text-white outline-none focus:border-teal-400";

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
      <span class="text-4xl font-semibold text-white">Settings</span>
      <span class="text-slate-400">Link defaults, monitoring ranges and device information</span>
    </div>

    <section class="space-y-3 rounded-2xl border border-slate-400 bg-[#222A3A] p-5">
      <span class="text-xs font-bold uppercase text-slate-400"
        >Serial defaults</span
      >

      <label class="block max-w-xs space-y-1">
        <span class="text-sm text-slate-300">Default baud rate</span>
        <input
          v-model.number="settings.defaultBaud"
          type="number"
          min="300"
          step="100"
          :class="fieldClass"
        />
        <span class="block text-xs text-slate-500">
          Pre-filled when connecting. The live link uses the value chosen in the
          control panel.
        </span>
      </label>
    </section>

    <section class="space-y-3 rounded-2xl border border-slate-400 bg-[#222A3A] p-5">
      <span class="text-xs font-bold uppercase text-slate-400"
        >Monitoring ranges</span
      >
      <p class="text-xs text-slate-500">
        Each card shows a position bar within this range and flags values as out
        of range.
      </p>

      <div class="grid gap-3 md:grid-cols-2">
        <div
          v-for="metric in METRICS"
          :key="metric.key"
          class="space-y-2 rounded-xl border border-slate-700 bg-[#12161F] p-3"
        >
          <div class="flex items-center justify-between">
            <span class="text-sm font-medium text-white">{{ metric.label }}</span>
            <span class="text-xs text-slate-500">{{ metric.unit }}</span>
          </div>

          <div class="grid grid-cols-2 gap-2">
            <label class="space-y-1">
              <span class="text-xs text-slate-400">Min</span>
              <input
                v-model.number="settings.monitors[metric.key].min"
                type="number"
                step="any"
                :class="fieldClass"
              />
            </label>
            <label class="space-y-1">
              <span class="text-xs text-slate-400">Max</span>
              <input
                v-model.number="settings.monitors[metric.key].max"
                type="number"
                step="any"
                :class="fieldClass"
              />
            </label>
          </div>

          <p v-if="invalidRange(metric.key)" class="text-xs text-amber-300">
            Min must be less than max.
          </p>
        </div>
      </div>
    </section>

    <section class="space-y-3 rounded-2xl border border-slate-400 bg-[#222A3A] p-5">
      <span class="text-xs font-bold uppercase text-slate-400"
        >Dither controls</span
      >
      <p class="text-xs text-slate-500">
        Limits and step for the amplitude and frequency controls in the control
        panel.
      </p>

      <div class="grid gap-3 md:grid-cols-2">
        <div
          v-for="field in DITHER_FIELDS"
          :key="field.key"
          class="space-y-2 rounded-xl border border-slate-700 bg-[#12161F] p-3"
        >
          <div class="flex items-center justify-between">
            <span class="text-sm font-medium text-white">{{ field.label }}</span>
            <span class="text-xs text-slate-500">{{ field.unit }}</span>
          </div>

          <div class="grid grid-cols-3 gap-2">
            <label class="space-y-1">
              <span class="text-xs text-slate-400">Min</span>
              <input
                v-model.number="settings.dither[field.key].min"
                type="number"
                step="any"
                :class="fieldClass"
              />
            </label>
            <label class="space-y-1">
              <span class="text-xs text-slate-400">Max</span>
              <input
                v-model.number="settings.dither[field.key].max"
                type="number"
                step="any"
                :class="fieldClass"
              />
            </label>
            <label class="space-y-1">
              <span class="text-xs text-slate-400">Step</span>
              <input
                v-model.number="settings.dither[field.key].step"
                type="number"
                step="any"
                :class="fieldClass"
              />
            </label>
          </div>

          <p v-if="invalidControl(field.key)" class="text-xs text-amber-300">
            {{ invalidControl(field.key) }}
          </p>
        </div>
      </div>
    </section>

    <section class="space-y-2 rounded-2xl border border-slate-400 bg-[#222A3A] p-5">
      <span class="text-xs font-bold uppercase text-slate-400"
        >Master clock</span
      >
      <div class="flex justify-between text-sm">
        <span class="text-slate-400">State frame period</span>
        <span class="text-white">{{ status.tickMs }} ms</span>
      </div>
      <div class="flex justify-between text-sm">
        <span class="text-slate-400">Role</span>
        <span class="text-white">Serial master</span>
      </div>
    </section>

    <section class="space-y-2 rounded-2xl border border-slate-400 bg-[#222A3A] p-5">
      <span class="text-xs font-bold uppercase text-slate-400">Device</span>
      <div class="flex justify-between text-sm">
        <span class="text-slate-400">Controller</span>
        <span class="text-white">MCU-04</span>
      </div>
      <div class="flex justify-between text-sm">
        <span class="text-slate-400">Application</span>
        <span class="text-white">v0.0.1a</span>
      </div>
    </section>

    <button
      class="w-fit rounded-lg border border-slate-600 px-4 py-2 text-sm text-slate-300 transition hover:border-slate-400 hover:text-white"
      @click="resetDefaults"
    >
      Reset to defaults
    </button>
  </div>
</template>
