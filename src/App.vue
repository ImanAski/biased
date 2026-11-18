<script setup lang="ts">
import "./app.css";
import { defineAsyncComponent, onMounted, watch } from "vue";
import { initDevice } from "./lib/device";
import { initPackets } from "./lib/packets";
import { initWidgets } from "./lib/widgets";
import {
  currentView,
  navigate,
  packetLabAvailable,
} from "./lib/nav";
import InfoBar from "./components/info-bar.vue";
import OverviewPage from "./pages/main.vue";
import SettingsPage from "./pages/settings.vue";
import LogsPage from "./pages/logs.vue";
import RightBar from "./components/right-bar.vue";
import LeftBar from "./components/left-bar.vue";

// The Packet Lab is an editor-only tool. Loading it lazily keeps the page out
// of a customer build's runtime path entirely.
const PacketsPage = defineAsyncComponent(() => import("./pages/packets.vue"));

// Connects event listeners, loads ports and starts mirroring UI state to Rust.
onMounted(() => {
  void initDevice();
  // RX counters feed the status bars in every build; the page itself is gated.
  void initPackets();
  void initWidgets();
});

// If a customer build ever reaches the packets view (for example through a
// stale saved state), fall back to the overview.
watch(packetLabAvailable, (available) => {
  if (!available && currentView.value === "packets") navigate("overview");
});
</script>

<template>
  <main class="h-screen flex flex-col bg-app">
    <InfoBar />

    <div class="flex flex-1 min-h-0">
      <div id="left">
        <LeftBar />
      </div>
      <div
        id="center"
        class="flex-1 overflow-auto p-4 bg-center border-l border-r border-line"
      >
        <OverviewPage v-if="currentView === 'overview'" />
        <PacketsPage
          v-else-if="currentView === 'packets' && packetLabAvailable"
        />
        <SettingsPage v-else-if="currentView === 'settings'" />
        <LogsPage v-else-if="currentView === 'logs'" />
        <OverviewPage v-else />
      </div>
      <div id="right">
        <RightBar />
      </div>
    </div>
  </main>
</template>
