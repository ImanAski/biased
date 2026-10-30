<script setup lang="ts">
import { telemetry } from "../lib/device";

function display(value: number | null): string {
  return value === null ? "—" : value.toFixed(2);
}
</script>

<template>
  <div class="w-full space-y-3 rounded-2xl border border-slate-400 bg-[#222A3A] p-5">
    <div class="flex justify-between">
      <span class="font-bold uppercase text-slate-400">Bias Voltage</span>
      <span class="font-mono text-white">{{ display(telemetry.biasVoltage) }} V</span>
    </div>
    <div
      class="flex h-40 items-center justify-center rounded-xl border border-dashed border-slate-600 text-sm text-slate-500"
    >
      Telemetry plot placeholder
    </div>
  </div>
</template>
