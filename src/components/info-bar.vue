<script setup lang="ts">
import { lastSeq, status } from "../lib/device";
import { formatBytes, rxRate, rxTotals } from "../lib/packets";
</script>

<template>
  <div class="px-3 py-2 border-b border-line">
    <div
      v-if="status.connected"
      class="flex items-center justify-between text-xs text-muted"
    >
      <div class="flex items-center gap-2">
        <span class="inline-block h-2 w-2 rounded-full bg-success animate-pulse" />
        <span class="text-ink font-semibold">{{ status.portName }}</span>
        <span>@ {{ status.baud }} baud</span>
        <span class="text-faint">· master tick {{ status.tickMs }} ms</span>
      </div>
      <div class="flex items-center gap-4">
        <span>tx {{ status.txs.toLocaleString() }}</span>
        <span>rx {{ rxTotals.chunks.toLocaleString() }}</span>
        <span class="text-faint">
          {{ formatBytes(rxTotals.bytes) }} · {{ rxRate.toFixed(1) }}/s
        </span>
        <span class="text-faint">seq {{ lastSeq }}</span>
      </div>
    </div>

    <div v-else class="flex items-center justify-between text-xs">
      <div class="flex items-center gap-2">
        <span class="inline-block h-2 w-2 rounded-full bg-faint" />
        <span class="text-dim">Not Connected</span>
      </div>
      <span v-if="status.lastError" class="text-danger">{{
        status.lastError
      }}</span>
    </div>
  </div>
</template>
