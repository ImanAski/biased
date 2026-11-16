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
    segments?: number;
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
    segments: 20,
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

function fractionOf(mark: number): number {
  if (!hasRange.value) return 0;
  const ratio =
    (mark - (props.min as number)) / ((props.max as number) - (props.min as number));
  return Math.min(1, Math.max(0, ratio));
}

const cells = computed(() => Math.max(5, Math.min(40, props.segments)));
const activeCells = computed(() => {
  if (props.value === null) return 0;
  return Math.round(fractionOf(props.value) * cells.value);
});

const lowMark = computed(() =>
  props.low === null ? null : fractionOf(props.low) * 100,
);
const highMark = computed(() =>
  props.high === null ? null : fractionOf(props.high) * 100,
);

const rangeText = computed(() =>
  hasRange.value ? `${props.min} – ${props.max} ${props.unit}`.trim() : "",
);
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

    <div class="relative space-y-1">
      <div class="relative flex h-4 w-full gap-[2px]">
        <span
          v-for="cell in cells"
          :key="cell"
          class="h-full flex-1 rounded-[1px] transition-colors"
          :class="
            cell <= activeCells
              ? alert
                ? 'bg-warning'
                : 'bg-accent'
              : 'bg-line'
          "
        />

        <span
          v-if="lowMark !== null"
          class="absolute -top-1 h-6 w-px bg-warning/80"
          :style="{ left: `${lowMark}%` }"
          title="Low threshold"
        />
        <span
          v-if="highMark !== null"
          class="absolute -top-1 h-6 w-px bg-warning/80"
          :style="{ left: `${highMark}%` }"
          title="High threshold"
        />
      </div>

      <div v-if="hasRange" class="flex justify-between text-[10px] text-dim">
        <span>{{ min }}</span>
        <span>{{ max }}</span>
      </div>
    </div>

    <div class="relative flex justify-between gap-2 text-xs">
      <span class="text-faint">{{ notes }}</span>
      <span v-if="rangeText" class="shrink-0 text-dim">{{ rangeText }}</span>
    </div>
  </div>
</template>
