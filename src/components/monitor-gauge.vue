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

const RADIUS = 42;
const SWEEP = 0.75;
const CIRCUMFERENCE = 2 * Math.PI * RADIUS;

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

function fractionOf(mark: number): number {
  if (!hasRange.value) return 0;
  const ratio =
    (mark - (props.min as number)) / ((props.max as number) - (props.min as number));
  return Math.min(1, Math.max(0, ratio));
}

const fillFraction = computed(() =>
  props.value === null ? 0 : fractionOf(props.value),
);

const dash = computed(
  () => `${fillFraction.value * CIRCUMFERENCE * SWEEP} ${CIRCUMFERENCE}`,
);

const lowTick = computed(() => (props.low === null ? null : tick(props.low)));
const highTick = computed(() => (props.high === null ? null : tick(props.high)));

function tick(mark: number) {
  const angle = ((135 + 270 * fractionOf(mark)) * Math.PI) / 180;
  return {
    x1: 50 + (RADIUS - 7) * Math.cos(angle),
    y1: 50 + (RADIUS - 7) * Math.sin(angle),
    x2: 50 + (RADIUS + 1) * Math.cos(angle),
    y2: 50 + (RADIUS + 1) * Math.sin(angle),
  };
}
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

    <div class="relative flex items-center justify-center">
      <svg viewBox="0 0 100 100" class="h-36 w-36" role="img" :aria-label="title">
        <circle
          cx="50"
          cy="50"
          :r="RADIUS"
          fill="none"
          stroke="currentColor"
          class="text-line"
          stroke-width="8"
          stroke-linecap="round"
          :stroke-dasharray="`${CIRCUMFERENCE * SWEEP} ${CIRCUMFERENCE}`"
          transform="rotate(135 50 50)"
        />
        <circle
          cx="50"
          cy="50"
          :r="RADIUS"
          fill="none"
          stroke="currentColor"
          :class="alert ? 'text-warning' : 'text-accent'"
          stroke-width="8"
          stroke-linecap="round"
          :stroke-dasharray="dash"
          transform="rotate(135 50 50)"
        />

        <line
          v-if="lowTick"
          :x1="lowTick.x1"
          :y1="lowTick.y1"
          :x2="lowTick.x2"
          :y2="lowTick.y2"
          stroke="currentColor"
          class="text-warning/80"
          stroke-width="1.5"
        />
        <line
          v-if="highTick"
          :x1="highTick.x1"
          :y1="highTick.y1"
          :x2="highTick.x2"
          :y2="highTick.y2"
          stroke="currentColor"
          class="text-warning/80"
          stroke-width="1.5"
        />

        <text
          x="50"
          y="55"
          text-anchor="middle"
          class="fill-ink font-mono"
          style="font-size: 15px"
        >
          {{ display }}
        </text>
        <text
          x="50"
          y="66"
          text-anchor="middle"
          class="fill-muted"
          style="font-size: 7px"
        >
          {{ unit }}
        </text>
      </svg>
    </div>

    <div
      v-if="hasRange"
      class="relative flex justify-between text-[10px] text-dim"
    >
      <span>{{ min }} {{ unit }}</span>
      <span>{{ max }} {{ unit }}</span>
    </div>

    <div class="relative flex justify-between gap-2 text-xs">
      <span class="text-faint">{{ notes }}</span>
      <span
        v-if="low !== null || high !== null"
        class="shrink-0 text-warning/80"
      >
        limit {{ low ?? "−∞" }} … {{ high ?? "∞" }}
      </span>
    </div>
  </div>
</template>
