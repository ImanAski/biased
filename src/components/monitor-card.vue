<script setup lang="ts">
import { computed } from "vue";

const props = withDefaults(
  defineProps<{
    title?: string;
    value?: number | null;
    unit?: string;
    digits?: number;
    min?: number | null;
    max?: number | null;
    low?: number | null;
    high?: number | null;
    notes?: string;
  }>(),
  {
    title: "",
    value: null,
    unit: "",
    digits: 1,
    min: null,
    max: null,
    low: null,
    high: null,
    notes: "",
  },
);

const display = computed(() =>
  props.value === null ? "—" : props.value.toFixed(props.digits),
);

const hasRange = computed(
  () =>
    props.min !== null &&
    props.max !== null &&
    Number.isFinite(props.min) &&
    Number.isFinite(props.max) &&
    props.max > props.min,
);

const state = computed<"empty" | "low" | "high" | "normal">(() => {
  if (props.value === null) return "empty";
  if (props.low !== null && props.value < props.low) return "low";
  if (props.high !== null && props.value > props.high) return "high";
  return "normal";
});

const alert = computed(() => state.value === "low" || state.value === "high");

const badge = computed(() => {
  switch (state.value) {
    case "empty":
      return "no data";
    case "low":
      return "low";
    case "high":
      return "high";
    default:
      return "nominal";
  }
});

const badgeClass = computed(() => {
  switch (state.value) {
    case "empty":
      return "bg-faint/20 text-muted";
    case "low":
    case "high":
      return "bg-warning/15 text-warning";
    default:
      return "bg-success/15 text-success";
  }
});

const fillPercent = computed(() => {
  if (!hasRange.value || props.value === null) return 0;
  const ratio = (props.value - (props.min as number)) / ((props.max as number) - (props.min as number));
  return Math.min(100, Math.max(0, ratio * 100));
});

function markPercent(mark: number | null): number | null {
  if (mark === null || !hasRange.value) return null;
  const ratio = (mark - (props.min as number)) / ((props.max as number) - (props.min as number));
  return Math.min(100, Math.max(0, ratio * 100));
}

const lowMark = computed(() => markPercent(props.low));
const highMark = computed(() => markPercent(props.high));

const rangeText = computed(() => {
  if (!hasRange.value) return "";
  return `${props.min} – ${props.max} ${props.unit}`.trim();
});

const thresholdText = computed(() => {
  if (props.low === null && props.high === null) return "";
  const low = props.low === null ? "−∞" : props.low;
  const high = props.high === null ? "∞" : props.high;
  return `limit ${low} … ${high} ${props.unit}`.trim();
});
</script>

<template>
  <div
    class="relative w-full space-y-3 overflow-hidden rounded-2xl border bg-panel p-5 transition"
    :class="alert ? 'border-warning/50' : 'border-line'"
  >
    <div
      v-if="alert"
      class="pointer-events-none absolute inset-0 bg-warning/[0.04]"
    />

    <div class="relative flex items-start justify-between gap-2">
      <span class="font-bold uppercase text-muted">{{ title }}</span>
      <span
        class="rounded-2xl px-2 py-0.5 text-xs transition"
        :class="badgeClass"
        >{{ badge }}</span
      >
    </div>

    <div class="relative flex items-end justify-start gap-1">
      <span
        class="font-mono text-4xl tabular-nums"
        :class="alert ? 'text-warning' : 'text-ink'"
        >{{ display }}</span
      >
      <span class="pb-1 text-sm text-muted">{{ unit }}</span>
    </div>

    <div v-if="hasRange" class="relative space-y-1">
      <div class="relative h-1.5 w-full rounded-full bg-line">
        <div
          class="h-full rounded-full transition-all duration-300"
          :class="alert ? 'bg-warning' : 'bg-accent'"
          :style="{ width: `${fillPercent}%` }"
        />

        <!-- Threshold markers -->
        <span
          v-if="lowMark !== null"
          class="absolute top-[-3px] h-[12px] w-px bg-warning/80"
          :style="{ left: `${lowMark}%` }"
          title="Low threshold"
        />
        <span
          v-if="highMark !== null"
          class="absolute top-[-3px] h-[12px] w-px bg-warning/80"
          :style="{ left: `${highMark}%` }"
          title="High threshold"
        />

        <!-- Value dot -->
        <span
          v-if="value !== null"
          class="absolute top-1/2 h-2.5 w-2.5 -translate-y-1/2 rounded-full border-2 border-panel"
          :class="alert ? 'bg-warning' : 'bg-accent'"
          :style="{ left: `calc(${fillPercent}% - 5px)` }"
        />
      </div>

      <div class="flex justify-between text-[10px] text-dim">
        <span>{{ min }}</span>
        <span v-if="thresholdText" class="text-warning/80">{{ thresholdText }}</span>
        <span>{{ max }}</span>
      </div>
    </div>

    <div class="relative flex justify-between gap-2 text-xs">
      <span class="text-faint">{{ notes }}</span>
      <span v-if="rangeText" class="shrink-0 text-dim">{{ rangeText }}</span>
    </div>
  </div>
</template>
