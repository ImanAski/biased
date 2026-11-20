<script setup lang="ts">
import { computed, ref, watch } from "vue";
import { telemetry } from "../lib/device";
import { plotValue, type PlotWidget } from "../lib/widgets";

const props = defineProps<{ plot: PlotWidget }>();

const samples = ref<number[]>([]);

// Changing the source or its transform makes existing samples incomparable, so
// start a fresh trace. `telemetry.updatedAt` changes on every sample even when
// the value is unchanged, keeping flat traces moving.
const transformKey = computed(() =>
  [
    props.plot.source,
    props.plot.scale,
    props.plot.multiply,
    props.plot.offset,
    props.plot.offsetFirst,
  ].join("|"),
);
let lastTransform = transformKey.value;

watch(
  () => [telemetry.updatedAt, transformKey.value] as const,
  ([, key]) => {
    if (key !== lastTransform) {
      samples.value = [];
      lastTransform = key;
    }
    const value = plotValue(props.plot);
    if (value === null) return;
    samples.value.push(value);
    const limit = Math.max(2, props.plot.history);
    if (samples.value.length > limit) {
      samples.value.splice(0, samples.value.length - limit);
    }
  },
  { immediate: true },
);

const current = computed(() => plotValue(props.plot));
const display = computed(() =>
  current.value === null ? "—" : current.value.toFixed(props.plot.digits),
);

const bounds = computed(() => {
  const range = props.plot.range;
  if (
    range &&
    Number.isFinite(range.min) &&
    Number.isFinite(range.max) &&
    range.min < range.max
  ) {
    return { min: range.min, max: range.max };
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

function x(index: number): number {
  const total = samples.value.length;
  if (total <= 1) return 100;
  return (index / (total - 1)) * 100;
}

function y(value: number): number {
  const { min, max } = bounds.value;
  const ratio = (value - min) / (max - min);
  return 38 - Math.min(1, Math.max(0, ratio)) * 34;
}

const line = computed(() =>
  samples.value
    .map((value, index) => `${x(index).toFixed(2)},${y(value).toFixed(2)}`)
    .join(" "),
);

const area = computed(() =>
  props.plot.fill !== false && samples.value.length > 0
    ? `0,40 ${line.value} 100,40`
    : "",
);

const lastPoint = computed(() => {
  if (props.plot.marker === false || samples.value.length === 0) return null;
  const index = samples.value.length - 1;
  return { x: x(index), y: y(samples.value[index]) };
});

const gridLines = computed(() => (props.plot.grid === false ? [] : [4, 21, 38]));

const referenceLines = computed(() => {
  const threshold = props.plot.threshold;
  if (!threshold) return [];
  const { min, max } = bounds.value;
  const lines: { y: number; key: string }[] = [];
  if (
    threshold.low !== null &&
    Number.isFinite(threshold.low) &&
    threshold.low >= min &&
    threshold.low <= max
  ) {
    lines.push({ key: "low", y: y(threshold.low) });
  }
  if (
    threshold.high !== null &&
    Number.isFinite(threshold.high) &&
    threshold.high >= min &&
    threshold.high <= max
  ) {
    lines.push({ key: "high", y: y(threshold.high) });
  }
  return lines;
});
</script>

<template>
  <div class="w-full space-y-3 rounded-2xl border border-line bg-panel p-5">
    <div class="flex items-start justify-between gap-3">
      <div class="flex flex-col">
        <span class="font-bold uppercase text-muted">{{ plot.label }}</span>
        <span class="text-xs text-faint">
          {{ plot.description || "Rolling telemetry history" }}
        </span>
      </div>
      <span class="font-mono text-2xl tabular-nums text-ink">
        {{ display }} {{ plot.unit }}
      </span>
    </div>

    <div class="relative h-48 w-full">
      <svg
        viewBox="0 0 100 40"
        class="h-full w-full"
        preserveAspectRatio="none"
        role="img"
        :aria-label="`${plot.label} trend`"
      >
        <line
          v-for="mark in gridLines"
          :key="mark"
          x1="0"
          :y1="mark"
          x2="100"
          :y2="mark"
          stroke="currentColor"
          class="text-line/60"
          stroke-width="0.4"
          vector-effect="non-scaling-stroke"
        />
        <line
          v-for="reference in referenceLines"
          :key="reference.key"
          x1="0"
          :y1="reference.y"
          x2="100"
          :y2="reference.y"
          stroke="currentColor"
          class="text-warning/60"
          stroke-width="0.5"
          stroke-dasharray="2 2"
          vector-effect="non-scaling-stroke"
        />
        <polygon v-if="area" :points="area" class="fill-accent/10" />
        <polyline
          v-if="line"
          :points="line"
          fill="none"
          stroke="currentColor"
          class="text-accent"
          stroke-width="1.5"
          stroke-linejoin="round"
          stroke-linecap="round"
          vector-effect="non-scaling-stroke"
        />
        <circle
          v-if="lastPoint"
          :cx="lastPoint.x"
          :cy="lastPoint.y"
          r="1.2"
          class="fill-accent"
        />
      </svg>

      <div
        v-if="samples.length === 0"
        class="absolute inset-0 flex items-center justify-center rounded-xl border border-dashed border-line text-sm text-faint"
      >
        waiting for {{ plot.source }} samples…
      </div>
    </div>

    <div class="flex justify-between gap-2 text-xs">
      <span class="text-faint">
        {{ bounds.min.toFixed(plot.digits) }} – {{ bounds.max.toFixed(plot.digits) }} {{ plot.unit }}
      </span>
      <span class="shrink-0 text-dim">
        {{ samples.length }} / {{ plot.history }} samples
      </span>
    </div>
  </div>
</template>
