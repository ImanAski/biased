<script setup lang="ts">
import { computed, ref } from "vue";
import { invoke } from "@tauri-apps/api/core";
import { open, save } from "@tauri-apps/plugin-dialog";
import { inTauri, status } from "../lib/device";
import { addLog } from "../lib/logs";
import { resetSettings, settings } from "../lib/settings";
import {
  configErrors,
  configLoading,
  configPath,
  configSource,
  debugBuild,
  exportLockedBuild,
  exportName,
  loadWidgetConfig,
  locked,
  useDefaultConfig,
  widgetConfig,
} from "../lib/widgets";

const fieldClass =
  "w-full rounded-lg border border-line bg-field px-3 py-2 text-sm text-ink outline-none transition focus:border-accent";

const busy = ref(false);
const notice = ref<string | null>(null);
const exportError = ref<string | null>(null);

const isPreview = computed(() => configPath.value === "built-in preview");
const canExport = computed(
  () => inTauri && !locked.value && !debugBuild.value,
);

const actionClass =
  "rounded-lg border border-line px-3 py-2 text-sm text-muted transition hover:border-line-strong hover:text-ink disabled:opacity-40";

function flash(message: string) {
  notice.value = message;
  window.setTimeout(() => {
    if (notice.value === message) notice.value = null;
  }, 2500);
}

async function openConfig() {
  if (!inTauri) return;
  busy.value = true;
  try {
    const picked = await open({
      title: "Open widget layout",
      multiple: false,
      directory: false,
      filters: [{ name: "TOML", extensions: ["toml"] }],
    });
    const path = Array.isArray(picked) ? picked[0] : picked;
    if (!path) return;
    const ok = await loadWidgetConfig(path);
    if (ok) flash("Layout loaded");
  } catch (err) {
    addLog("error", `Open config failed: ${String(err)}`);
  } finally {
    busy.value = false;
  }
}

async function reloadConfig() {
  busy.value = true;
  try {
    const ok = await loadWidgetConfig(isPreview.value ? null : configPath.value);
    if (ok) flash("Layout reloaded");
  } finally {
    busy.value = false;
  }
}

async function restoreDefault() {
  busy.value = true;
  try {
    const ok = await useDefaultConfig();
    if (ok) flash("Using the app's widgets.toml");
  } finally {
    busy.value = false;
  }
}

async function copyPath() {
  try {
    await navigator.clipboard.writeText(configPath.value);
    flash("Path copied");
  } catch {
    addLog("warn", "Clipboard is unavailable; select the path manually");
  }
}

async function saveCopy() {
  if (!inTauri || isPreview.value) return;
  busy.value = true;
  try {
    const contents = await invoke<string>("read_text_file", {
      path: configPath.value,
    });
    const target = await save({
      title: "Save a copy of the widget layout",
      defaultPath: "widgets.toml",
      filters: [{ name: "TOML", extensions: ["toml"] }],
    });
    if (!target) return;
    await invoke("write_text_file", { path: target, contents });
    addLog("info", `Widget layout copied to ${target}`);
    flash("Copy saved");
  } catch (err) {
    addLog("error", `Save config copy failed: ${String(err)}`);
  } finally {
    busy.value = false;
  }
}

async function exportCustomerBuild() {
  if (!canExport.value) return;
  exportError.value = null;
  busy.value = true;
  try {
    const target = await save({
      title: "Export configuration-locked build",
      defaultPath: exportName.value,
    });
    if (!target) return;

    const result = await exportLockedBuild(target);
    addLog(
      "info",
      `Customer build exported: ${result.path} (${result.bytes.toLocaleString()} bytes)`,
    );
    flash("Customer build exported");
  } catch (err) {
    const message = String(err);
    exportError.value = message;
    addLog("error", `Customer build export failed: ${message}`);
  } finally {
    busy.value = false;
  }
}

function resetDefaults() {
  resetSettings();
  addLog("info", "Settings reset to defaults");
}
</script>

<template>
  <div class="flex flex-col gap-4">
    <div class="flex flex-col">
      <span class="text-4xl font-semibold text-ink">Settings</span>
      <span class="text-muted"
        >Widget layout, link defaults and device information</span
      >
    </div>

    <!-- Customer build -->
    <section class="space-y-3 rounded-2xl border border-line bg-panel p-5">
      <div class="flex items-start justify-between gap-3">
        <div class="flex flex-col">
          <span class="text-xs font-bold uppercase text-muted"
            >Customer build</span
          >
          <p class="text-xs text-faint">
            Package the current layout into a standalone executable that
            customers can use but cannot reconfigure.
          </p>
        </div>
        <span
          class="shrink-0 rounded-full px-2 py-0.5 text-[11px]"
          :class="
            locked
              ? 'bg-success/15 text-success'
              : 'bg-faint/20 text-muted'
          "
        >
          {{ locked ? "configuration locked" : "editable" }}
        </span>
      </div>

      <template v-if="locked">
        <p class="text-sm text-muted">
          This executable carries its layout internally. Opening, reloading and
          saving a different <code class="text-ink">widgets.toml</code> are
          disabled.
        </p>
        <p class="break-all text-xs text-faint">
          Executable: <code>{{ configSource ?? configPath }}</code>
        </p>
      </template>

      <template v-else>
        <p class="text-sm text-muted">
          The export is a copy of this executable with the active
          <code class="text-ink">widgets.toml</code> embedded. The layout cannot
          be replaced in the copy, so the customer always gets exactly this
          configuration.
        </p>
        <div class="flex flex-wrap items-center gap-2">
          <button
            :class="actionClass"
            :disabled="busy || !canExport"
            @click="exportCustomerBuild"
          >
            Export customer build…
          </button>
          <span v-if="!inTauri" class="text-xs text-faint">
            Available in the desktop app.
          </span>
          <span v-else-if="debugBuild" class="text-xs text-faint">
            Available in a production build — run
            <code>bun run tauri build</code> and launch the built app.
          </span>
          <span v-else class="text-xs text-faint">
            Creates an executable for this operating system.
          </span>
        </div>
        <p
          v-if="exportError"
          class="whitespace-pre-wrap rounded-xl border border-danger/30 bg-danger/10 p-3 text-xs text-danger"
        >
          {{ exportError }}
        </p>
      </template>
    </section>

    <!-- Widget layout -->
    <section class="space-y-3 rounded-2xl border border-line bg-panel p-5">
      <div class="flex items-start justify-between gap-3">
        <div class="flex flex-col">
          <span class="text-xs font-bold uppercase text-muted"
            >Widget layout</span
          >
          <p class="text-xs text-faint">
            <template v-if="locked">
              This build uses the layout embedded in its executable; it cannot
              be edited.
            </template>
            <template v-else>
              Monitor cards and controls are declared in
              <code class="text-ink">widgets.toml</code>. Edit the file, then
              press Reload.
            </template>
          </p>
        </div>
        <span
          class="shrink-0 rounded-full px-2 py-0.5 text-[11px]"
          :class="
            configErrors.length > 0
              ? 'bg-warning/15 text-warning'
              : 'bg-success/15 text-success'
          "
        >
          {{ configErrors.length > 0 ? "invalid" : "active" }}
        </span>
      </div>

      <template v-if="!locked">
        <div
          class="flex items-center gap-2 rounded-xl border border-line bg-field px-3 py-2"
        >
          <code class="min-w-0 flex-1 break-all font-mono text-xs text-muted">{{
            configPath
          }}</code>
          <button
            class="shrink-0 rounded px-2 py-1 text-xs text-muted transition hover:text-ink"
            title="Copy path"
            @click="copyPath"
          >
            Copy
          </button>
        </div>

        <div class="flex flex-wrap gap-2">
          <button :class="actionClass" :disabled="busy" @click="reloadConfig">
            {{ configLoading ? "Loading…" : "Reload" }}
          </button>
          <button :class="actionClass" :disabled="busy" @click="openConfig">
            Open file…
          </button>
          <button :class="actionClass" :disabled="busy" @click="restoreDefault">
            Use app default
          </button>
          <button
            :class="actionClass"
            :disabled="busy || isPreview"
            @click="saveCopy"
          >
            Save a copy…
          </button>
        </div>
      </template>

      <div class="flex flex-wrap gap-4 text-xs text-muted">
        <span>
          <span class="font-mono text-ink">{{ widgetConfig.monitors.length }}</span>
          monitors
        </span>
        <span>
          <span class="font-mono text-ink">{{ widgetConfig.controls.length }}</span>
          controls
        </span>
        <span class="text-faint">title: {{ widgetConfig.title }}</span>
      </div>

      <p v-if="notice" class="text-xs text-success">{{ notice }}</p>

      <div
        v-if="configErrors.length > 0"
        class="space-y-1 rounded-xl border border-warning/40 bg-warning/10 p-3"
      >
        <p class="text-xs font-semibold text-warning">
          <template v-if="locked">
            The embedded layout was rejected — contact the supplier.
          </template>
          <template v-else>
            The file was rejected — the last good layout is still active.
          </template>
        </p>
        <ul class="list-inside list-disc space-y-0.5 font-mono text-[11px] text-warning">
          <li
            v-for="(error, index) in configErrors.slice(0, 8)"
            :key="index"
            class="whitespace-pre-wrap break-words"
          >
            {{ error }}
          </li>
        </ul>
        <p v-if="configErrors.length > 8" class="text-[11px] text-warning">
          …and {{ configErrors.length - 8 }} more.
        </p>
      </div>
    </section>

    <section class="space-y-3 rounded-2xl border border-line bg-panel p-5">
      <span class="text-xs font-bold uppercase text-muted"
        >Serial defaults</span
      >

      <label class="block max-w-xs space-y-1">
        <span class="text-sm text-muted">Default baud rate</span>
        <input
          v-model.number="settings.defaultBaud"
          type="number"
          min="300"
          step="100"
          :class="fieldClass"
        />
        <span class="block text-xs text-faint">
          Pre-filled when connecting. The live link uses the value chosen in the
          control panel.
        </span>
      </label>
    </section>

    <section class="space-y-2 rounded-2xl border border-line bg-panel p-5">
      <span class="text-xs font-bold uppercase text-muted">Master clock</span>
      <div class="flex justify-between text-sm">
        <span class="text-muted">State frame period</span>
        <span class="text-ink">{{ status.tickMs }} ms</span>
      </div>
      <div class="flex justify-between text-sm">
        <span class="text-muted">Role</span>
        <span class="text-ink">Serial master</span>
      </div>
    </section>

    <section class="space-y-2 rounded-2xl border border-line bg-panel p-5">
      <span class="text-xs font-bold uppercase text-muted">Device</span>
      <div class="flex justify-between text-sm">
        <span class="text-muted">Controller</span>
        <span class="text-ink">MCU-04</span>
      </div>
      <div class="flex justify-between text-sm">
        <span class="text-muted">Application</span>
        <span class="text-ink">v0.2.0</span>
      </div>
    </section>

    <button
      class="w-fit rounded-lg border border-line px-4 py-2 text-sm text-muted transition hover:border-line-strong hover:text-ink"
      @click="resetDefaults"
    >
      Reset app settings
    </button>
  </div>
</template>
