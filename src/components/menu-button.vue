<script setup lang="ts">
defineProps<{
  active?: boolean;
}>();
</script>

<template>
  <button
    class="w-full rounded-xl px-4 py-2 text-start transition"
    :class="
      active
        ? 'bg-[#10343F] text-white'
        : 'text-slate-300 hover:bg-white/5 hover:text-white'
    "
  >
    <slot />
  </button>
</template>
