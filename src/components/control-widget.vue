<script setup lang="ts">
import { computed } from "vue";
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

function onNumericInput(event: Event) {
  const target = event.target as HTMLInputElement;
  setControlValue(props.control, clampToControl(props.control, Number(target.value)));
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
  </section>
</template>
