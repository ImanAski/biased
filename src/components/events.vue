<script setup lang="ts">
import { lastSeq, status, uiError } from "../lib/device";
import { rxTotals } from "../lib/packets";
</script>

<template>
  <div class="w-full space-y-3 rounded-2xl border border-line bg-panel p-5">
    <div class="flex justify-between">
      <span class="font-bold uppercase text-muted">Event Log</span>
      <span
        class="rounded-2xl px-2"
        :class="
          status.connected
            ? 'bg-success/15 text-success'
            : 'bg-faint/20 text-muted'
        "
      >
        {{ status.connected ? "live" : "idle" }}
      </span>
    </div>

    <ul class="space-y-1 font-mono text-xs text-muted">
      <li>
        <span class="text-faint">link</span>
        <span class="text-ink">
          {{ status.connected ? `${status.portName} @ ${status.baud}` : "down" }}
        </span>
      </li>
      <li>
        <span class="text-faint">frames</span>
        <span class="text-ink">{{ status.txs }}</span>
        <span class="text-dim">· last seq {{ lastSeq }}</span>
      </li>
      <li>
        <span class="text-faint">rx</span>
        <span class="text-ink">{{ rxTotals.chunks }}</span>
        <span class="text-dim">chunks · {{ rxTotals.bytes }} bytes</span>
      </li>
      <li v-if="status.lastError" class="text-danger">
        error {{ status.lastError }}
      </li>
      <li v-if="uiError" class="text-danger">ui {{ uiError }}</li>
    </ul>
  </div>
</template>
