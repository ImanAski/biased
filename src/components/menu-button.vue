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
        ? 'bg-accent/15 text-accent'
        : 'text-muted hover:bg-ink/5 hover:text-ink'
    "
  >
    <slot />
  </button>
</template>
