<script setup lang="ts">
import { computed, ref, watch } from "vue";

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
    points?: number;
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
    points: 60,
  },
);

const samples = ref<number[]>([]);

watch(
  () => props.value,
  (value) => {
    if (value === null || !Number.isFinite(value)) return;
    samples.value.push(value);
    const limit = Math.max(2, props.points);
    if (samples.value.length > limit) {
      samples.value.splice(0, samples.value.length - limit);
    }
  },
  { immediate: true },
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

const scale = computed(() => {
  if (hasRange.value) {
    return { min: props.min as number, max: props.max as number };
  }
  if (samples.value.length === 0) return { min: 0, max: 1 };

  let min = Math.min(...samples.value);
  let max = Math.max(...samples.value);
  if (min === max) {
    min -= 1;
    max += 1;
  }
  const pad = (max - min) * 0.1;
  return { min: min - pad, max: max + pad };
});

function y(value: number): number {
  const { min, max } = scale.value;
  const ratio = (value - min) / (max - min);
  return 34 - Math.min(1, Math.max(0, ratio)) * 30;
}

function x(index: number): number {
  const total = samples.value.length;
  if (total <= 1) return 100;
  return (index / (total - 1)) * 100;
}

const line = computed(() =>
  samples.value
    .map((value, index) => `${x(index).toFixed(2)},${y(value).toFixed(2)}`)
    .join(" "),
);

const area = computed(() =>
  samples.value.length === 0
    ? ""
    : `0,36 ${line.value} 100,36`,
);

function thresholdY(mark: number | null): number | null {
  if (mark === null) return null;
  const { min, max } = scale.value;
  if (mark < min || mark > max) return null;
  return y(mark);
}

const lowY = computed(() => thresholdY(props.low));
const highY = computed(() => thresholdY(props.high));
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

    <div class="relative">
      <svg viewBox="0 0 100 36" class="h-16 w-full" preserveAspectRatio="none">
        <line
          v-if="lowY !== null"
          x1="0"
          :y1="lowY"
          x2="100"
          :y2="lowY"
          stroke="currentColor"
          class="text-warning/30"
          stroke-width="0.6"
          stroke-dasharray="2 2"
          vector-effect="non-scaling-stroke"
        />
        <line
          v-if="highY !== null"
          x1="0"
          :y1="highY"
          x2="100"
          :y2="highY"
          stroke="currentColor"
          class="text-warning/30"
          stroke-width="0.6"
          stroke-dasharray="2 2"
          vector-effect="non-scaling-stroke"
        />

        <polygon
          v-if="area"
          :points="area"
          class="fill-accent/10"
        />
        <polyline
          v-if="line"
          :points="line"
          fill="none"
          stroke="currentColor"
          :class="alert ? 'text-warning' : 'text-accent'"
          stroke-width="1.5"
          stroke-linejoin="round"
          stroke-linecap="round"
          vector-effect="non-scaling-stroke"
        />
      </svg>

      <div
        v-if="samples.length === 0"
        class="absolute inset-0 flex items-center justify-center text-xs text-faint"
      >
        waiting for samples…
      </div>
    </div>

    <div class="relative flex justify-between gap-2 text-xs">
      <span class="text-faint">{{ notes }}</span>
      <span class="shrink-0 text-dim">
        {{ samples.length }} / {{ points }} samples
      </span>
    </div>
  </div>
</template>
