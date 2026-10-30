<script setup lang="ts">
import { lastSeq, status } from "../lib/device";
</script>

<template>
  <div class="px-3 py-2 border-b border-slate-700/70">
    <div
      v-if="status.connected"
      class="flex items-center justify-between text-xs text-slate-300"
    >
      <div class="flex items-center gap-2">
        <span class="inline-block h-2 w-2 rounded-full bg-green-400 animate-pulse" />
        <span class="text-white font-semibold">{{ status.portName }}</span>
        <span class="text-slate-400">@ {{ status.baud }} baud</span>
        <span class="text-slate-500">· master tick {{ status.tickMs }} ms</span>
      </div>
      <div class="flex items-center gap-4">
        <span>tx {{ status.txs.toLocaleString() }}</span>
        <span class="text-slate-500">seq {{ lastSeq }}</span>
      </div>
    </div>

    <div v-else class="flex items-center justify-between text-xs">
      <div class="flex items-center gap-2">
        <span class="inline-block h-2 w-2 rounded-full bg-slate-500" />
        <span class="text-[#56607A]">Not Connected</span>
      </div>
      <span v-if="status.lastError" class="text-red-400">{{
        status.lastError
      }}</span>
    </div>
  </div>
</template>
