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
    notes?: string;
  }>(),
  {
    title: "",
    value: null,
    unit: "",
    digits: 1,
    min: null,
    max: null,
    notes: "",
  },
);

const display = computed(() =>
  props.value === null ? "—" : props.value.toFixed(props.digits),
);

const hasRange = computed(
  () =>
    props.value !== null &&
    props.min !== null &&
    props.max !== null &&
    props.max > props.min,
);

const inRange = computed<boolean | null>(() => {
  if (props.value === null || props.min === null || props.max === null) {
    return null;
  }
  return props.value >= props.min && props.value <= props.max;
});

const fillPercent = computed(() => {
  if (
    !hasRange.value ||
    props.value === null ||
    props.min === null ||
    props.max === null
  ) {
    return 0;
  }
  const ratio = (props.value - props.min) / (props.max - props.min);
  return Math.min(100, Math.max(0, ratio * 100));
});

const badge = computed(() => {
  if (props.value === null) return "no data";
  if (inRange.value === false) return "out of range";
  return "live";
});

const badgeClass = computed(() => {
  if (props.value === null) return "bg-faint/20 text-muted";
  if (inRange.value === false) return "bg-warning/15 text-warning";
  return "bg-success/15 text-success";
});

const rangeText = computed(() => {
  if (props.min === null || props.max === null) return "";
  return `${props.min} – ${props.max} ${props.unit}`.trim();
});
</script>

<template>
  <div class="w-full space-y-3 rounded-2xl border border-line bg-panel p-5">
    <div class="flex justify-between">
      <span class="font-bold uppercase text-muted">{{ title }}</span>
      <span class="rounded-2xl px-2" :class="badgeClass">{{ badge }}</span>
    </div>

    <div class="flex items-end justify-start">
      <span
        class="text-4xl"
        :class="inRange === false ? 'text-warning' : 'text-ink'"
        >{{ display }}</span
      >
      <span class="text-sm text-muted">{{ unit }}</span>
    </div>

    <div
      v-if="hasRange"
      class="h-1.5 w-full overflow-hidden rounded-full bg-line"
    >
      <div
        class="h-full rounded-full transition-all"
        :class="inRange === false ? 'bg-warning' : 'bg-accent'"
        :style="{ width: `${fillPercent}%` }"
      />
    </div>

    <div class="flex justify-between gap-2 text-xs">
      <span class="text-faint">{{ notes }}</span>
      <span v-if="rangeText" class="shrink-0 text-dim">{{ rangeText }}</span>
    </div>
  </div>
</template>
