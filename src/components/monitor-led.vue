<script setup lang="ts">
import { computed } from "vue";

const props = withDefaults(
  defineProps<{
    title?: string;
    value?: number | null;
    unit?: string;
    notes?: string;
  }>(),
  {
    title: "",
    value: null,
    unit: "",
    notes: "",
  },
);

/** LED sources use 0/1; anything at or above 0.5 counts as on. */
const on = computed(() => props.value !== null && props.value >= 0.5);
const known = computed(() => props.value !== null);

const badge = computed(() => {
  if (!known.value) return "no data";
  return on.value ? "on" : "off";
});

const badgeClass = computed(() => {
  if (!known.value) return "bg-faint/20 text-muted";
  return on.value ? "bg-success/15 text-success" : "bg-faint/20 text-muted";
});
</script>

<template>
  <div class="relative w-full space-y-3 overflow-hidden rounded-2xl border border-line bg-panel p-5 transition">
    <div class="relative flex items-start justify-between gap-2">
      <span class="font-bold uppercase text-muted">{{ title }}</span>
      <span
        class="rounded-2xl px-2 py-0.5 text-xs transition"
        :class="badgeClass"
        >{{ badge }}</span
      >
    </div>

    <div class="relative flex items-center gap-4 py-2">
      <span class="relative flex h-12 w-12 items-center justify-center">
        <span
          v-if="on"
          class="absolute h-12 w-12 animate-ping rounded-full bg-success/20"
        />
        <span
          class="relative h-8 w-8 rounded-full border-2 transition"
          :class="
            on
              ? 'border-success/60 bg-success shadow-[0_0_16px] shadow-success/50'
              : 'border-line bg-faint/20'
          "
        />
      </span>

      <div class="flex flex-col">
        <span class="font-mono text-2xl text-ink">{{ badge.toUpperCase() }}</span>
        <span v-if="unit" class="text-xs text-dim">{{ unit }}</span>
      </div>
    </div>

    <div class="relative flex justify-between gap-2 text-xs">
      <span class="text-faint">{{ notes }}</span>
      <span class="shrink-0 text-dim">
        {{ known ? value?.toFixed(0) : "—" }}
      </span>
    </div>
  </div>
</template>
