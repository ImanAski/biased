<script setup lang="ts">
import MenuButton from "./menu-button.vue";
import { currentView, navigate, views } from "../lib/nav";
import { theme, toggleTheme } from "../lib/theme";
import { locked } from "../lib/widgets";
</script>

<template>
  <div class="flex h-full min-w-80 flex-col space-y-3">
    <div class="flex flex-col space-y-1 px-3 pt-3">
      <span class="text-xl font-semibold text-ink">Bias Controller</span>
      <span class="text-muted">desktop | v0.6.2</span>
      <span
        v-if="locked"
        class="w-fit rounded-full bg-success/15 px-2 py-0.5 text-[11px] text-success"
      >
        configuration locked
      </span>
    </div>

    <hr class="border-line" />

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

    <button
      class="mx-3 mb-3 mt-auto flex items-center gap-2 rounded-xl border border-line px-4 py-2 text-sm text-muted transition hover:border-line-strong hover:text-ink"
      :aria-label="theme === 'dark' ? 'Switch to light theme' : 'Switch to dark theme'"
      @click="toggleTheme"
    >
      <span aria-hidden="true">{{ theme === "dark" ? "☀" : "☾" }}</span>
      {{ theme === "dark" ? "Light mode" : "Dark mode" }}
    </button>
  </div>
</template>
