<script setup lang="ts">
import "./app.css";
import { onMounted } from "vue";
import { initDevice } from "./lib/device";
import { currentView } from "./lib/nav";
import InfoBar from "./components/info-bar.vue";
import OverviewPage from "./pages/main.vue";
import SettingsPage from "./pages/settings.vue";
import LogsPage from "./pages/logs.vue";
import RightBar from "./components/right-bar.vue";
import LeftBar from "./components/left-bar.vue";

// Connects event listeners, loads ports and starts mirroring UI state to Rust.
onMounted(initDevice);
</script>

<template>
  <main class="h-screen flex flex-col bg-[#1B2230]">
    <InfoBar />

    <div class="flex flex-1 min-h-0">
      <div id="left">
        <LeftBar />
      </div>
      <div
        id="center"
        class="flex-1 overflow-auto p-4 bg-[#12161F] border-l border-r border-slate-500"
      >
        <OverviewPage v-if="currentView === 'overview'" />
        <SettingsPage v-else-if="currentView === 'settings'" />
        <LogsPage v-else />
      </div>
      <div id="right">
        <RightBar />
      </div>
    </div>
  </main>
</template>
