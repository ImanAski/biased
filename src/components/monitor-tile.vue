<script setup lang="ts">
import { computed } from "vue";

const props = withDefaults(
  defineProps<{
    title?: string;
    value?: number | null;
    unit?: string;
    digits?: number;
    low?: number | null;
    high?: number | null;
    notes?: string;
  }>(),
  {
    title: "",
    value: null,
    unit: "",
    digits: 1,
    low: null,
    high: null,
    notes: "",
  },
);

const display = computed(() =>
  props.value === null ? "—" : props.value.toFixed(props.digits),
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
</script>

<template>
  <div
    class="relative flex w-full flex-col justify-between gap-3 overflow-hidden rounded-2xl border bg-panel p-4 transition"
    :class="alert ? 'border-warning/50' : 'border-line'"
  >
    <div
      v-if="alert"
      class="pointer-events-none absolute inset-0 bg-warning/[0.04]"
    />

    <div class="relative flex items-start justify-between gap-2">
      <span class="text-xs font-bold uppercase text-muted">{{ title }}</span>
      <span
        class="rounded-2xl px-2 py-0.5 text-[10px] transition"
        :class="badgeClass"
        >{{ badge }}</span
      >
    </div>

    <div class="relative flex items-baseline gap-1">
      <span
        class="font-mono text-3xl tabular-nums"
        :class="alert ? 'text-warning' : 'text-ink'"
        >{{ display }}</span
      >
      <span class="text-xs text-muted">{{ unit }}</span>
    </div>

    <div class="relative flex items-center justify-between gap-2 text-[10px]">
      <span class="truncate text-faint">{{ notes }}</span>
      <span
        v-if="low !== null || high !== null"
        class="shrink-0 font-mono text-warning/80"
      >
        {{ low ?? "−∞" }} … {{ high ?? "∞" }}
      </span>
    </div>
  </div>
</template>
