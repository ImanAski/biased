<script setup lang="ts">
import { computed, ref, watch } from "vue";
import { telemetry } from "../lib/device";

const props = withDefaults(
  defineProps<{
    title?: string;
    unit?: string;
    digits?: number;
    points?: number;
  }>(),
  {
    title: "Bias Voltage",
    unit: "V",
    digits: 2,
    points: 90,
  },
);

const samples = ref<number[]>([]);

watch(
  // `updatedAt` changes on every telemetry sample, even when the value is the
  // same, so a flat trace still advances instead of freezing on one point.
  () => telemetry.updatedAt,
  () => {
    const value = telemetry.biasVoltage;
    if (value === null || !Number.isFinite(value)) return;
    samples.value.push(value);
    const limit = Math.max(2, props.points);
    if (samples.value.length > limit) {
      samples.value.splice(0, samples.value.length - limit);
    }
  },
  { immediate: true },
);

const current = computed(() => telemetry.biasVoltage);
const display = computed(() =>
  current.value === null ? "—" : current.value.toFixed(props.digits),
);

const bounds = computed(() => {
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
  samples.value.length === 0 ? "" : `0,40 ${line.value} 100,40`,
);

const lastPoint = computed(() => {
  if (samples.value.length === 0) return null;
  const index = samples.value.length - 1;
  return { x: x(index), y: y(samples.value[index]) };
});
</script>

<template>
  <div class="w-full space-y-3 rounded-2xl border border-line bg-panel p-5">
    <div class="flex items-start justify-between gap-3">
      <div class="flex flex-col">
        <span class="font-bold uppercase text-muted">{{ title }}</span>
        <span class="text-xs text-faint">Rolling telemetry history</span>
      </div>
      <span class="font-mono text-2xl tabular-nums text-ink">
        {{ display }} {{ unit }}
      </span>
    </div>

    <div class="relative h-48 w-full">
      <svg
        viewBox="0 0 100 40"
        class="h-full w-full"
        preserveAspectRatio="none"
        role="img"
        :aria-label="`${title} trend`"
      >
        <line
          v-for="mark in [4, 21, 38]"
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
        waiting for bias voltage samples…
      </div>
    </div>

    <div class="flex justify-between gap-2 text-xs">
      <span class="text-faint">
        {{ bounds.min.toFixed(digits) }} – {{ bounds.max.toFixed(digits) }} {{ unit }}
      </span>
      <span class="shrink-0 text-dim">
        {{ samples.length }} / {{ points }} samples
      </span>
    </div>
  </div>
</template>
