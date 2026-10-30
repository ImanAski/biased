<script setup lang="ts">
import MenuButton from "./menu-button.vue";
import { currentView, navigate, views } from "../lib/nav";
</script>

<template>
  <div class="flex h-full min-w-80 flex-col space-y-3">
    <div class="flex flex-col space-y-1 px-3 pt-3">
      <span class="text-xl font-semibold text-white">Bias Controller</span>
      <span class="text-slate-400">desktop | v0.0.1a</span>
    </div>

    <hr class="border-slate-700" />

    <div class="flex w-full flex-col space-y-2.5 px-3">
      <MenuButton
        v-for="view in views"
        :key="view.id"
        :active="currentView === view.id"
        @click="navigate(view.id)"
      >
        {{ view.label }}
      </MenuButton>
    </div>
  </div>
</template>
