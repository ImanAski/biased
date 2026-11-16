<script setup lang="ts">
import { computed } from "vue";
import ControlKnob from "./control-knob.vue";
import {
  clampToControl,
  controlValue,
  formatControlValue,
  setControlValue,
  type ControlWidget,
} from "../lib/widgets";

const props = defineProps<{ control: ControlWidget }>();

const value = computed(() => controlValue(props.control));
const isOn = computed(() => value.value === true);
const selected = computed(() => String(value.value));
const numeric = computed(() => Number(value.value));

const bounds = computed(() => {
  const range = props.control.range;
  if (!range) return { min: 0, max: 1, step: 1 };
  return {
    min: Math.min(range.min, range.max),
    max: Math.max(range.min, range.max),
    step: range.step && range.step > 0 ? range.step : 1,
  };
});

const presets = computed(() => props.control.presets ?? []);
const numericKind = computed(
  () =>
    props.control.kind === "range" ||
    props.control.kind === "stepper" ||
    props.control.kind === "knob",
);

function presetActive(preset: number) {
  return Math.abs(preset - numeric.value) < bounds.value.step / 2;
}

function onNumericInput(event: Event) {
  const target = event.target as HTMLInputElement;
  setControlValue(props.control, clampToControl(props.control, Number(target.value)));
}

function nudge(direction: number) {
  setControlValue(
    props.control,
    clampToControl(props.control, numeric.value + direction * bounds.value.step),
  );
}
</script>

<template>
  <section class="space-y-2.5 rounded-2xl border border-line bg-panel p-3">
    <div class="flex items-start justify-between gap-2">
      <div class="flex flex-col">
        <span class="text-sm font-medium text-ink">{{ control.label }}</span>
        <span v-if="control.description" class="text-[11px] leading-tight text-faint">
          {{ control.description }}
        </span>
      </div>

      <button
        v-if="control.kind === 'toggle'"
        class="relative mt-0.5 h-6 w-11 shrink-0 rounded-full transition"
        :class="isOn ? 'bg-teal-500' : 'bg-line'"
        role="switch"
        :aria-checked="isOn"
        :aria-label="control.label"
        @click="setControlValue(control, !isOn)"
      >
        <span
          class="absolute left-0.5 top-0.5 h-5 w-5 rounded-full bg-white shadow-sm transition-transform"
          :class="isOn ? 'translate-x-5' : 'translate-x-0'"
        />
      </button>
    </div>

    <div v-if="control.kind === 'select'" class="grid grid-cols-2 gap-1.5">
      <button
        v-for="option in control.options"
        :key="option"
        class="rounded-lg border px-2 py-1.5 text-sm font-medium transition"
        :class="
          selected === option
            ? 'border-accent/50 bg-accent/15 text-accent'
            : 'border-line bg-field text-muted hover:border-line-strong hover:text-ink'
        "
        @click="setControlValue(control, option)"
      >
        {{ option }}
      </button>
    </div>

    <div v-else-if="control.kind === 'segmented'" class="grid grid-cols-2 gap-1.5">
      <button
        class="rounded-lg border px-2 py-1.5 text-sm font-medium transition"
        :class="
          !isOn
            ? 'border-line-strong bg-faint/20 text-ink'
            : 'border-line bg-field text-muted hover:border-line-strong hover:text-ink'
        "
        @click="setControlValue(control, false)"
      >
        OFF
      </button>
      <button
        class="rounded-lg border px-2 py-1.5 text-sm font-medium transition"
        :class="
          isOn
            ? 'border-success/50 bg-success/15 text-success'
            : 'border-line bg-field text-muted hover:border-line-strong hover:text-ink'
        "
        @click="setControlValue(control, true)"
      >
        ON
      </button>
    </div>

    <div v-else-if="control.kind === 'range'" class="space-y-2">
      <div class="flex justify-between text-xs text-muted">
        <span class="font-mono text-ink">
          {{ formatControlValue(numeric) }}
          <span class="font-sans text-faint">{{ control.unit }}</span>
        </span>
        <span class="font-mono text-[10px] text-dim">
          {{ bounds.min }} … {{ bounds.max }}
        </span>
      </div>

      <input
        type="range"
        class="w-full accent-teal-500"
        :min="bounds.min"
        :max="bounds.max"
        :step="bounds.step"
        :value="numeric"
        :aria-label="control.label"
        @input="onNumericInput"
      />

      <input
        type="number"
        class="w-full rounded-lg border border-line bg-field px-3 py-2 text-sm text-ink outline-none transition focus:border-accent"
        :min="bounds.min"
        :max="bounds.max"
        :step="bounds.step"
        :value="numeric"
        :aria-label="`${control.label} value`"
        @change="onNumericInput"
      />
    </div>

    <div v-else-if="control.kind === 'stepper'" class="space-y-2">
      <div class="flex justify-between text-xs text-muted">
        <span class="font-mono text-ink">
          {{ formatControlValue(numeric) }}
          <span class="font-sans text-faint">{{ control.unit }}</span>
        </span>
        <span class="font-mono text-[10px] text-dim">
          step {{ bounds.step }} · {{ bounds.min }} … {{ bounds.max }}
        </span>
      </div>

      <div class="flex items-stretch gap-1.5">
        <button
          class="w-10 shrink-0 rounded-lg border border-line bg-field text-lg font-semibold text-muted transition hover:border-line-strong hover:text-ink disabled:cursor-not-allowed disabled:opacity-40"
          :disabled="numeric <= bounds.min"
          :aria-label="`Decrease ${control.label}`"
          @click="nudge(-1)"
        >
          −
        </button>

        <input
          type="number"
          class="min-w-0 flex-1 rounded-lg border border-line bg-field px-3 py-2 text-center font-mono text-sm text-ink outline-none transition focus:border-accent"
          :min="bounds.min"
          :max="bounds.max"
          :step="bounds.step"
          :value="numeric"
          :aria-label="`${control.label} value`"
          @change="onNumericInput"
        />

        <button
          class="w-10 shrink-0 rounded-lg border border-line bg-field text-lg font-semibold text-muted transition hover:border-line-strong hover:text-ink disabled:cursor-not-allowed disabled:opacity-40"
          :disabled="numeric >= bounds.max"
          :aria-label="`Increase ${control.label}`"
          @click="nudge(1)"
        >
          +
        </button>
      </div>
    </div>

    <div v-else-if="control.kind === 'knob'">
      <ControlKnob :control="control" />
    </div>

    <div v-if="numericKind && presets.length > 0" class="flex flex-wrap gap-1.5">
      <button
        v-for="preset in presets"
        :key="preset"
        class="rounded-lg border px-2 py-1 text-xs font-medium transition"
        :class="
          presetActive(preset)
            ? 'border-accent/50 bg-accent/15 text-accent'
            : 'border-line bg-field text-muted hover:border-line-strong hover:text-ink'
        "
        @click="setControlValue(control, preset)"
      >
        {{ formatControlValue(preset) }}
      </button>
    </div>
  </section>
</template>
