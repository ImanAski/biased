<script setup lang="ts">
import { lastSeq, status, uiError } from "../lib/device";
</script>

<template>
  <div class="w-full space-y-3 rounded-2xl border border-slate-400 bg-[#222A3A] p-5">
    <div class="flex justify-between">
      <span class="font-bold uppercase text-slate-400">Event Log</span>
      <span
        class="rounded-2xl px-2"
        :class="
          status.connected
            ? 'bg-green-500/20 text-green-300'
            : 'bg-slate-500/20 text-slate-400'
        "
      >
        {{ status.connected ? "live" : "idle" }}
      </span>
    </div>

    <ul class="space-y-1 font-mono text-xs text-slate-400">
      <li>
        <span class="text-slate-500">link</span>
        <span class="text-white">
          {{ status.connected ? `${status.portName} @ ${status.baud}` : "down" }}
        </span>
      </li>
      <li>
        <span class="text-slate-500">frames</span>
        <span class="text-white">{{ status.txs }}</span>
        <span class="text-slate-600">· last seq {{ lastSeq }}</span>
      </li>
      <li v-if="status.lastError" class="text-red-400">
        error {{ status.lastError }}
      </li>
      <li v-if="uiError" class="text-red-400">ui {{ uiError }}</li>
    </ul>
  </div>
</template>
