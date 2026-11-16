<script setup lang="ts">
import { computed, ref } from "vue";
import {
  clampToControl,
  controlValue,
  formatControlValue,
  setControlValue,
  type ControlWidget,
} from "../lib/widgets";

const props = defineProps<{ control: ControlWidget }>();

const RADIUS = 40;
const SWEEP = 0.75;
const CIRCUMFERENCE = 2 * Math.PI * RADIUS;

const value = computed(() => Number(controlValue(props.control)));

const bounds = computed(() => {
  const range = props.control.range;
  if (!range) return { min: 0, max: 1, step: 1 };
  return {
    min: Math.min(range.min, range.max),
    max: Math.max(range.min, range.max),
    step: range.step && range.step > 0 ? range.step : 1,
  };
});

const fraction = computed(() => {
  const span = bounds.value.max - bounds.value.min;
  if (span <= 0) return 0;
  return Math.min(1, Math.max(0, (value.value - bounds.value.min) / span));
});

const dash = computed(
  () => `${fraction.value * CIRCUMFERENCE * SWEEP} ${CIRCUMFERENCE}`,
);

const dragging = ref(false);
let startY = 0;
let startValue = 0;

function nudge(direction: number) {
  setControlValue(
    props.control,
    clampToControl(props.control, value.value + direction * bounds.value.step),
  );
}

function onPointerDown(event: PointerEvent) {
  dragging.value = true;
  startY = event.clientY;
  startValue = value.value;
  (event.currentTarget as Element).setPointerCapture(event.pointerId);
}

function onPointerMove(event: PointerEvent) {
  if (!dragging.value) return;
  const span = bounds.value.max - bounds.value.min;
  const delta = ((startY - event.clientY) / 140) * span;
  setControlValue(props.control, clampToControl(props.control, startValue + delta));
}

function onPointerUp(event: PointerEvent) {
  dragging.value = false;
  const target = event.currentTarget as Element;
  if (target.hasPointerCapture(event.pointerId)) {
    target.releasePointerCapture(event.pointerId);
  }
}

function onKeydown(event: KeyboardEvent) {
  switch (event.key) {
    case "ArrowUp":
    case "ArrowRight":
      event.preventDefault();
      nudge(1);
      break;
    case "ArrowDown":
    case "ArrowLeft":
      event.preventDefault();
      nudge(-1);
      break;
    case "Home":
      event.preventDefault();
      setControlValue(props.control, bounds.value.min);
      break;
    case "End":
      event.preventDefault();
      setControlValue(props.control, bounds.value.max);
      break;
  }
}
</script>

<template>
  <div class="flex items-center gap-4">
    <svg
      viewBox="0 0 100 100"
      class="h-24 w-24 shrink-0 cursor-ns-resize touch-none select-none"
      role="slider"
      tabindex="0"
      :aria-label="control.label"
      :aria-valuemin="bounds.min"
      :aria-valuemax="bounds.max"
      :aria-valuenow="value"
      @pointerdown.prevent="onPointerDown"
      @pointermove="onPointerMove"
      @pointerup="onPointerUp"
      @pointercancel="onPointerUp"
      @keydown="onKeydown"
    >
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
        class="text-accent"
        stroke-width="8"
        stroke-linecap="round"
        :stroke-dasharray="dash"
        transform="rotate(135 50 50)"
      />
      <circle
        cx="50"
        cy="50"
        r="22"
        fill="currentColor"
        class="text-field"
        stroke="currentColor"
        stroke-width="1"
        stroke-opacity="0.2"
      />
      <line
        x1="50"
        y1="50"
        :x2="50 + 16 * Math.cos(((135 + 270 * fraction) * Math.PI) / 180)"
        :y2="50 + 16 * Math.sin(((135 + 270 * fraction) * Math.PI) / 180)"
        stroke="currentColor"
        class="text-accent"
        stroke-width="2"
        stroke-linecap="round"
      />
      <circle cx="50" cy="50" r="3" fill="currentColor" class="text-accent" />
    </svg>

    <div class="flex min-w-0 flex-col gap-1">
      <span class="font-mono text-2xl text-ink">
        {{ formatControlValue(value) }}
        <span class="font-sans text-sm text-faint">{{ control.unit }}</span>
      </span>
      <span class="text-[10px] text-dim">
        drag ↕ or use arrow keys · step {{ bounds.step }}
      </span>
      <span class="font-mono text-[10px] text-dim">
        {{ bounds.min }} … {{ bounds.max }}
      </span>
    </div>
  </div>
</template>
