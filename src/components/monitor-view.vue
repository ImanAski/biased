<script setup lang="ts">
import { computed } from "vue";
import { monitorValue, type MonitorWidget } from "../lib/widgets";
import MonitorCard from "./monitor-card.vue";
import MonitorGauge from "./monitor-gauge.vue";
import MonitorLed from "./monitor-led.vue";
import MonitorMeter from "./monitor-meter.vue";
import MonitorSparkline from "./monitor-sparkline.vue";
import MonitorTile from "./monitor-tile.vue";

const props = defineProps<{ monitor: MonitorWidget }>();

const views = {
  bar: MonitorCard,
  gauge: MonitorGauge,
  sparkline: MonitorSparkline,
  led: MonitorLed,
  tile: MonitorTile,
  meter: MonitorMeter,
};

const view = computed(() => views[props.monitor.variant] ?? MonitorCard);

const bindings = computed(() => {
  const monitor = props.monitor;
  return {
    title: monitor.label,
    value: monitorValue(monitor),
    unit: monitor.unit,
    digits: monitor.digits,
    min: monitor.range?.min ?? null,
    max: monitor.range?.max ?? null,
    low: monitor.threshold?.low ?? null,
    high: monitor.threshold?.high ?? null,
    notes: monitor.description,
    ...(monitor.variant === "sparkline" ? { points: monitor.history } : {}),
    ...(monitor.variant === "meter" ? { segments: monitor.segments } : {}),
  };
});
</script>

<template>
  <component :is="view" v-bind="bindings" />
</template>
