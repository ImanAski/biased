<script setup lang="ts">
import { computed, nextTick, onMounted, onUnmounted, ref, watch } from "vue";
import { status } from "../lib/device";
import {
  analyzePacket,
  capturePaused,
  capturedBytes,
  clearRx,
  composerPackets,
  decodePayload,
  formatAge,
  formatBytes,
  formatClock,
  hasTemplate,
  hexDump,
  injectPacket,
  MAX_RX,
  packetError,
  packetLabel,
  packetProblem,
  packetScript,
  rxLastAt,
  rxPackets,
  rxRate,
  rxTotals,
  scriptErrors,
  sendIntervalMs,
  sendLimit,
  sendOnce,
  sender,
  senderBusy,
  startSending,
  stopSending,
  toAscii,
  toHex,
  txMarks,
  type ComposerPacket,
  type RxPacket,
} from "../lib/packets";

const filter = ref("");
const view = ref<"hex" | "ascii">("hex");
const autoscroll = ref(true);
const selectedId = ref<number | null>(null);
const copied = ref(false);
const now = ref(Date.now());

const listEl = ref<HTMLElement | null>(null);
const detailEl = ref<HTMLElement | null>(null);

const intervalPresets = [100, 300, 500, 1000];

const filteredPackets = computed(() => {
  const needle = filter.value.trim().toLowerCase();
  if (!needle) return rxPackets;

  return rxPackets.filter((packet) => {
    const haystack = [
      formatClock(packet.at),
      String(packet.seq),
      toHex(packet.bytes),
      toAscii(packet.bytes),
      packet.synthetic ? "sim injected" : "rx",
    ]
      .join(" ")
      .toLowerCase();
    return haystack.includes(needle);
  });
});

const selected = computed<RxPacket | null>(
  () => rxPackets.find((packet) => packet.id === selectedId.value) ?? null,
);

const detailRows = computed(() =>
  selected.value ? hexDump(selected.value.bytes) : [],
);

const detailAnalysis = computed(() =>
  selected.value ? analyzePacket(selected.value.bytes) : null,
);

const lastAge = computed(() =>
  rxLastAt.value === null ? null : Math.max(0, now.value - rxLastAt.value),
);

const hasLimit = computed({
  get: () => sendLimit.value > 0,
  set: (on: boolean) => {
    sendLimit.value = on ? Math.max(1, sendLimit.value || 100) : 0;
  },
});

const parsedBytes = computed(() =>
  composerPackets.value.reduce(
    (total, packet, index) =>
      total + decodePayload(packet, index).bytes.length,
    0,
  ),
);

const templatedCount = computed(
  () =>
    composerPackets.value.filter((packet) => hasTemplate(packet.payload)).length,
);

const countdown = computed(() => {
  if (!sender.running || sender.nextAt === null) {
    return { remaining: 0, progress: 0 };
  }
  const interval = Math.max(1, sender.intervalMs);
  const remaining = Math.max(0, sender.nextAt - now.value);
  return {
    remaining,
    progress: Math.min(1, Math.max(0, 1 - remaining / interval)),
  };
});

const dashOffset = computed(() => 97.4 * (1 - countdown.value.progress));

const currentLabel = computed(() => {
  if (!sender.running || sender.labels.length === 0) return null;
  return sender.labels[sender.sent % sender.labels.length];
});

const scheduledMarks = computed(() => txMarks.filter((mark) => !mark.manual));
const recentMarks = computed(() => txMarks.slice(-40));

const lastActualInterval = computed(() => {
  const marks = scheduledMarks.value;
  if (marks.length < 2) return null;
  return marks[marks.length - 1].at - marks[marks.length - 2].at;
});

let clockTimer: ReturnType<typeof setInterval> | null = null;

onMounted(() => {
  clockTimer = setInterval(() => {
    now.value = Date.now();
  }, 100);
});

onUnmounted(() => {
  if (clockTimer) clearInterval(clockTimer);
});

watch(
  () => (rxPackets.length > 0 ? rxPackets[rxPackets.length - 1].id : 0),
  async () => {
    if (!autoscroll.value || capturePaused.value) return;
    await nextTick();
    const element = listEl.value;
    if (element) element.scrollTop = element.scrollHeight;
  },
);

function onListScroll() {
  const element = listEl.value;
  if (!element) return;
  autoscroll.value =
    element.scrollHeight - element.scrollTop - element.clientHeight < 32;
}

function selectPacket(packet: RxPacket) {
  selectedId.value = selectedId.value === packet.id ? null : packet.id;
  autocrollDetail();
}

async function autocrollDetail() {
  await nextTick();
  detailEl.value?.scrollIntoView({ block: "nearest", behavior: "smooth" });
}

function clearInbound() {
  clearRx();
  selectedId.value = null;
}

function payloadInfo(packet: ComposerPacket, index = 0) {
  return decodePayload(packet, index);
}

function payloadSize(packet: ComposerPacket, index = 0): string {
  const info = payloadInfo(packet, index);
  return info.error ? "—" : `${info.bytes.length} B`;
}

function intervalInput(event: Event) {
  const value = Number((event.target as HTMLInputElement).value);
  sendIntervalMs.value = Number.isFinite(value)
    ? Math.min(60_000, Math.max(10, Math.round(value)))
    : 300;
}

function limitInput(event: Event) {
  const value = Number((event.target as HTMLInputElement).value);
  sendLimit.value = Number.isFinite(value)
    ? Math.max(0, Math.min(10_000_000, Math.floor(value)))
    : 0;
}

async function copySelected() {
  if (!selected.value) return;
  try {
    await navigator.clipboard.writeText(toHex(selected.value.bytes));
    copied.value = true;
    window.setTimeout(() => (copied.value = false), 1500);
  } catch {
    copied.value = false;
  }
}

function preview(packet: RxPacket): string {
  return view.value === "hex"
    ? `${toHex(packet.bytes, 24)}${packet.bytes.length > 24 ? " …" : ""}`
    : `${toAscii(packet.bytes, 40)}${packet.bytes.length > 40 ? "…" : ""}`;
}
</script>

<template>
  <div class="flex flex-col gap-4">
    <!-- Header -->
    <div class="flex items-end justify-between gap-3">
      <div class="flex flex-col">
        <span class="text-4xl font-semibold text-ink">Packet Lab</span>
        <span class="text-muted">
          Inspect inbound bytes and schedule raw packets on the wire
        </span>
      </div>

      <div class="flex items-center gap-2 text-xs">
        <span
          class="rounded-full px-2.5 py-1"
          :class="
            status.connected
              ? 'bg-success/15 text-success'
              : 'bg-faint/20 text-muted'
          "
        >
          {{ status.connected ? status.portName : "no link" }}
        </span>
        <span
          class="rounded-full px-2.5 py-1"
          :class="
            capturePaused
              ? 'bg-warning/15 text-warning'
              : 'bg-accent/15 text-accent'
          "
        >
          {{ capturePaused ? "capture paused" : "capture live" }}
        </span>
      </div>
    </div>

    <!-- Stats -->
    <div class="grid grid-cols-2 gap-3 lg:grid-cols-4">
      <div class="rounded-2xl border border-line bg-panel px-4 py-3">
        <div class="text-[10px] font-bold uppercase tracking-wide text-faint">
          Inbound chunks
        </div>
        <div class="font-mono text-2xl text-ink">
          {{ rxPackets.length.toLocaleString() }}
        </div>
        <div class="text-[11px] text-dim">
          wire {{ rxTotals.chunks.toLocaleString() }}
        </div>
      </div>

      <div class="rounded-2xl border border-line bg-panel px-4 py-3">
        <div class="text-[10px] font-bold uppercase tracking-wide text-faint">
          Captured bytes
        </div>
        <div class="font-mono text-2xl text-ink">
          {{ formatBytes(capturedBytes) }}
        </div>
        <div class="text-[11px] text-dim">
          wire {{ formatBytes(rxTotals.bytes) }}
        </div>
      </div>

      <div class="rounded-2xl border border-line bg-panel px-4 py-3">
        <div class="text-[10px] font-bold uppercase tracking-wide text-faint">
          Inbound rate
        </div>
        <div class="font-mono text-2xl text-ink">
          {{ rxRate.toFixed(1) }}
        </div>
        <div class="text-[11px] text-dim">chunks / second</div>
      </div>

      <div class="rounded-2xl border border-line bg-panel px-4 py-3">
        <div class="text-[10px] font-bold uppercase tracking-wide text-faint">
          Last packet
        </div>
        <div class="font-mono text-2xl text-ink">
          {{ lastAge === null ? "—" : formatAge(lastAge) }}
        </div>
        <div class="text-[11px] text-dim">ago</div>
      </div>
    </div>

    <div class="grid gap-4 xl:grid-cols-5">
      <!-- Inbound -->
      <section
        class="flex min-h-[30rem] flex-col overflow-hidden rounded-2xl border border-line bg-panel xl:col-span-3"
      >
        <div class="flex items-center justify-between border-b border-line px-4 py-2.5">
          <span class="text-xs font-bold uppercase text-muted"
            >Inbound stream</span
          >
          <span class="font-mono text-[10px] text-faint">
            buffer {{ rxPackets.length }} / {{ MAX_RX }} · showing
            {{ filteredPackets.length }}
          </span>
        </div>

        <div class="flex flex-wrap items-center gap-2 border-b border-line px-3 py-2">
          <input
            v-model="filter"
            type="text"
            placeholder="Filter hex, ascii, seq or time…"
            class="min-w-40 flex-1 rounded-lg border border-line bg-field px-2.5 py-1.5 font-mono text-xs text-ink outline-none transition focus:border-accent"
          />

          <div class="inline-flex overflow-hidden rounded-lg border border-line text-[11px]">
            <button
              class="px-2 py-1.5 transition"
              :class="
                view === 'hex'
                  ? 'bg-accent/15 text-accent'
                  : 'text-muted hover:text-ink'
              "
              @click="view = 'hex'"
            >
              HEX
            </button>
            <button
              class="px-2 py-1.5 transition"
              :class="
                view === 'ascii'
                  ? 'bg-accent/15 text-accent'
                  : 'text-muted hover:text-ink'
              "
              @click="view = 'ascii'"
            >
              ASCII
            </button>
          </div>

          <button
            class="rounded-lg border border-line px-2.5 py-1.5 text-[11px] text-muted transition hover:border-line-strong hover:text-ink"
            @click="capturePaused = !capturePaused"
          >
            {{ capturePaused ? "Resume" : "Pause" }}
          </button>

          <button
            class="rounded-lg border border-line px-2.5 py-1.5 text-[11px] text-muted transition hover:border-line-strong hover:text-ink disabled:opacity-40"
            :disabled="rxPackets.length === 0"
            @click="clearInbound"
          >
            Clear
          </button>

          <label class="flex items-center gap-1.5 text-[11px] text-muted">
            <input v-model="autoscroll" type="checkbox" class="accent-teal-500" />
            follow
          </label>
        </div>

        <div
          ref="listEl"
          class="max-h-[26rem] flex-1 overflow-auto font-mono text-[11px]"
          @scroll="onListScroll"
        >
          <button
            v-for="packet in filteredPackets"
            :key="packet.id"
            class="flex w-full items-center gap-2 border-b border-line/50 px-3 py-1.5 text-left transition hover:bg-ink/[0.03]"
            :class="selectedId === packet.id ? 'bg-accent/10' : ''"
            @click="selectPacket(packet)"
          >
            <span class="w-[5.5rem] shrink-0 text-faint">{{
              formatClock(packet.at)
            }}</span>
            <span class="w-14 shrink-0 text-right text-dim">
              {{ packet.dt === null ? "—" : `+${Math.round(packet.dt)}` }}
            </span>
            <span
              class="w-9 shrink-0 rounded text-center text-[9px] uppercase"
              :class="
                packet.synthetic
                  ? 'bg-warning/15 text-warning'
                  : 'bg-accent/10 text-accent'
              "
            >
              {{ packet.synthetic ? "sim" : "rx" }}
            </span>
            <span class="min-w-0 flex-1 truncate text-ink">{{ preview(packet) }}</span>
            <span class="w-10 shrink-0 text-right text-faint"
              >{{ packet.bytes.length }}B</span
            >
          </button>

          <div
            v-if="filteredPackets.length === 0"
            class="flex h-56 flex-col items-center justify-center gap-2 px-6 text-center font-sans"
          >
            <span class="text-2xl text-faint">⌁</span>
            <span class="text-sm text-muted">
              {{
                rxPackets.length === 0
                  ? "Waiting for inbound bytes…"
                  : "No packet matches this filter."
              }}
            </span>
            <span class="max-w-xs text-xs text-faint">
              Connect a port, or use “Inject” in the packet script to feed
              synthetic RX data without hardware.
            </span>
          </div>
        </div>

        <!-- Selected packet detail -->
        <div
          ref="detailEl"
          class="border-t border-line px-4 py-3"
          :class="selected ? 'bg-field/40' : ''"
        >
          <template v-if="selected">
            <div class="mb-2 flex flex-wrap items-center justify-between gap-2">
              <div class="flex flex-wrap items-center gap-2 text-xs">
                <span class="font-semibold text-ink">
                  Chunk #{{ selected.seq }}
                </span>
                <span class="text-faint">{{ formatClock(selected.at) }}</span>
                <span class="rounded bg-faint/20 px-1.5 py-0.5 text-[10px] text-muted">
                  {{ selected.bytes.length }} bytes
                </span>
                <span
                  v-if="selected.synthetic"
                  class="rounded bg-warning/15 px-1.5 py-0.5 text-[10px] text-warning"
                  >injected</span
                >
              </div>
              <div class="flex items-center gap-2">
                <button
                  class="rounded border border-line px-2 py-1 text-[10px] text-muted transition hover:text-ink"
                  @click="copySelected"
                >
                  {{ copied ? "Copied" : "Copy hex" }}
                </button>
                <button
                  class="rounded border border-line px-2 py-1 text-[10px] text-muted transition hover:text-ink"
                  @click="selectedId = null"
                >
                  Close
                </button>
              </div>
            </div>

            <div class="max-h-48 overflow-auto rounded-lg border border-line bg-panel p-2 font-mono text-[11px] leading-5">
              <div v-for="row in detailRows" :key="row.offset" class="flex gap-3">
                <span class="shrink-0 text-dim">{{ row.offset }}</span>
                <span class="whitespace-pre text-ink">{{ row.hex }}</span>
                <span class="whitespace-pre text-muted">{{ row.ascii }}</span>
              </div>
              <div v-if="detailRows.length === 0" class="text-faint">
                empty payload
              </div>
            </div>

            <div
              v-if="detailAnalysis"
              class="mt-2 grid gap-2 text-[11px] sm:grid-cols-2"
            >
              <div class="rounded-lg border border-line bg-panel p-2">
                <div class="mb-1 flex items-center justify-between">
                  <span class="font-semibold text-muted">Structure</span>
                  <span
                    class="rounded bg-faint/20 px-1.5 py-0.5 font-mono text-[9px] uppercase text-dim"
                  >
                    {{ detailAnalysis.kind }}
                  </span>
                </div>
                <div
                  class="grid grid-cols-[auto_1fr] gap-x-3 gap-y-0.5 font-mono text-[10px] text-dim"
                >
                  <span>framing</span>
                  <span class="text-ink">{{ detailAnalysis.framing }}</span>
                  <span>printable</span>
                  <span class="text-ink">
                    {{ Math.round(detailAnalysis.printable * 100) }}%
                  </span>
                  <span>lines</span>
                  <span class="text-ink">{{ detailAnalysis.lines }}</span>
                  <span>command</span>
                  <span class="truncate text-ink">
                    {{ detailAnalysis.command ?? "—" }}
                  </span>
                </div>
              </div>

              <div class="rounded-lg border border-line bg-panel p-2">
                <div class="mb-1 flex items-center justify-between">
                  <span class="font-semibold text-muted">Fields</span>
                  <span class="font-mono text-[9px] text-faint">
                    {{ detailAnalysis.fields.length }}
                  </span>
                </div>
                <div
                  v-if="detailAnalysis.fields.length"
                  class="flex flex-wrap gap-1"
                >
                  <span
                    v-for="(field, fieldIndex) in detailAnalysis.fields"
                    :key="`${field.key}-${fieldIndex}`"
                    class="rounded bg-faint/15 px-1.5 py-0.5 font-mono text-[10px] text-ink"
                  >
                    <span class="text-muted">{{ field.key }}</span>=<span>{{
                      field.value
                    }}</span>
                  </span>
                </div>
                <p v-else class="text-[10px] text-faint">
                  No whitespace-separated <code>key=value</code> fields.
                </p>
              </div>
            </div>
          </template>

          <div v-else class="text-xs text-faint">
            Select a chunk above to inspect it byte by byte.
          </div>
        </div>
      </section>

      <!-- Composer -->
      <section class="space-y-3 rounded-2xl border border-line bg-panel p-4 xl:col-span-2">
        <div class="flex items-center justify-between">
          <div class="flex flex-col">
            <span class="text-xs font-bold uppercase text-muted"
              >Packet script</span
            >
            <span class="text-[11px] text-faint">
              One packet per line, in send order.
            </span>
          </div>
          <span
            class="rounded-full px-2 py-0.5 text-[10px]"
            :class="
              sender.running
                ? 'bg-accent/15 text-accent'
                : 'bg-faint/20 text-muted'
            "
          >
            {{ sender.running ? "scheduled" : "idle" }}
          </span>
        </div>

        <!-- Clock -->
        <div class="flex items-center gap-3 rounded-xl border border-line bg-field p-3">
          <div class="relative h-16 w-16 shrink-0">
            <svg viewBox="0 0 36 36" class="h-16 w-16 -rotate-90">
              <circle
                cx="18"
                cy="18"
                r="15.5"
                fill="none"
                stroke="currentColor"
                stroke-width="3"
                class="text-line"
              />
              <circle
                cx="18"
                cy="18"
                r="15.5"
                fill="none"
                stroke="currentColor"
                stroke-width="3"
                stroke-linecap="round"
                stroke-dasharray="97.4"
                :stroke-dashoffset="dashOffset"
                class="text-accent transition-[stroke-dashoffset] duration-100 ease-linear"
              />
            </svg>
            <div class="absolute inset-0 flex flex-col items-center justify-center">
              <span class="font-mono text-xs text-ink">
                {{ Math.ceil(countdown.remaining) }}
              </span>
              <span class="text-[8px] uppercase text-dim">ms</span>
            </div>
          </div>

          <div class="min-w-0">
            <div class="text-sm font-medium text-ink">
              {{ sender.running ? "Next packet" : "Schedule idle" }}
            </div>
            <div class="truncate text-xs text-muted">
              {{ currentLabel ?? "Press start to transmit" }}
            </div>
            <div class="mt-1 font-mono text-[10px] text-dim">
              sent {{ sender.sent.toLocaleString() }} ·
              {{ sender.intervalMs }} ms · {{ sender.packetCount }} packet{{
                sender.packetCount === 1 ? "" : "s"
              }}
            </div>
          </div>
        </div>

        <!-- Interval -->
        <div class="space-y-2 rounded-xl border border-line p-3">
          <div class="flex items-center justify-between text-xs">
            <span class="font-semibold text-muted">Interval</span>
            <span class="font-mono text-ink">{{ sendIntervalMs }} ms</span>
          </div>

          <input
            v-model.number="sendIntervalMs"
            type="range"
            min="10"
            max="5000"
            step="10"
            class="w-full accent-teal-500"
          />

          <div class="flex flex-wrap items-center gap-1.5">
            <button
              v-for="preset in intervalPresets"
              :key="preset"
              class="rounded-md border px-2 py-1 text-[10px] transition"
              :class="
                sendIntervalMs === preset
                  ? 'border-accent/50 bg-accent/15 text-accent'
                  : 'border-line text-muted hover:text-ink'
              "
              @click="sendIntervalMs = preset"
            >
              {{ preset }} ms
            </button>
            <input
              :value="sendIntervalMs"
              type="number"
              min="10"
              max="60000"
              step="10"
              class="ml-auto w-20 rounded-md border border-line bg-field px-2 py-1 text-right font-mono text-[11px] text-ink outline-none focus:border-accent"
              aria-label="Interval in milliseconds"
              @change="intervalInput"
            />
          </div>
        </div>

        <!-- Script -->
        <div class="space-y-2">
          <div class="flex flex-wrap items-center justify-between gap-2">
            <span class="text-xs font-bold uppercase text-muted">Packets</span>
            <span
              class="font-mono text-[10px]"
              :class="scriptErrors.length ? 'text-danger' : 'text-faint'"
            >
              {{ composerPackets.length }} packet{{
                composerPackets.length === 1 ? "" : "s"
              }}
              · {{ formatBytes(parsedBytes) }}
              <span v-if="templatedCount > 0">
                · {{ templatedCount }} live
              </span>
            </span>
          </div>

          <textarea
            v-model="packetScript"
            rows="12"
            spellcheck="false"
            aria-label="Packet script"
            class="w-full resize-y rounded-xl border border-line bg-field px-3 py-2 font-mono text-[11px] leading-5 text-ink outline-none transition focus:border-accent"
          />

          <div
            v-if="scriptErrors.length > 0"
            class="space-y-1 rounded-xl border border-danger/30 bg-danger/10 p-3"
          >
            <p class="text-xs font-semibold text-danger">
              {{ scriptErrors.length }} script problem{{
                scriptErrors.length === 1 ? "" : "s"
              }}
            </p>
            <ul class="space-y-0.5 font-mono text-[11px] text-danger">
              <li
                v-for="(problem, index) in scriptErrors.slice(0, 6)"
                :key="index"
              >
                line {{ problem.line }}: {{ problem.message }}
              </li>
            </ul>
            <p v-if="scriptErrors.length > 6" class="text-[11px] text-danger">
              …and {{ scriptErrors.length - 6 }} more.
            </p>
          </div>

          <details
            class="rounded-xl border border-line bg-field/50 px-3 py-2 text-[11px] text-muted"
          >
            <summary class="cursor-pointer select-none text-faint">
              Script format
            </summary>
            <div class="mt-2 space-y-1 font-mono text-[10px] leading-5">
              <p>
                <span class="text-ink">Ping =&gt; PING\n</span>
                <span class="text-faint">text (default)</span>
              </p>
              <p>
                <span class="text-ink">Cal =&gt; [hex] 01 02 A0 FF</span>
                <span class="text-faint">hex bytes</span>
              </p>
              <p>
                <span class="text-ink">Note =&gt; [raw] C:\path\file</span>
                <span class="text-faint">literal text</span>
              </p>
              <p>
                <span class="text-ink">TLM seq=1 ...\n</span>
                <span class="text-faint">unlabelled text</span>
              </p>
              <p>
                <span class="text-ink"
                  >Live =&gt; seq={seq} temp={temp:.2|0.0}\n</span
                >
                <span class="text-faint">template</span>
              </p>
            </div>
            <p class="mt-2 leading-relaxed text-faint">
              <code>#</code> and <code>//</code> start a comment. Text supports
              <code>\n</code>, <code>\r</code>, <code>\t</code>,
              <code>\0</code>, <code>\\</code> and <code>\xNN</code>. A line
              containing <code>=&gt;</code> uses the part before it as the
              label. Invalid lines are reported above and are never sent.
            </p>
            <p class="mt-1 leading-relaxed text-faint">
              <code>{name}</code> placeholders expand at send time from the
              live device state and newest telemetry sample. Use
              <code>{name:.2}</code> for a float, <code>{name:04}</code> for a
              padded integer and <code>{name|fallback}</code> when a value may
              be missing. Write <code>&#123;&#123;</code> and
              <code>&#125;&#125;</code> for literal braces.
            </p>
          </details>

          <div class="max-h-[16rem] space-y-1 overflow-auto pr-1">
            <div
              v-for="(packet, index) in composerPackets"
              :key="packet.id"
              class="flex items-center gap-2 rounded-lg border border-line bg-field px-2 py-1.5"
            >
              <span class="w-5 shrink-0 text-center font-mono text-[10px] text-dim">
                {{ index + 1 }}
              </span>
              <span class="min-w-0 flex-1 truncate text-xs text-ink">{{
                packetLabel(packet, index)
              }}</span>
              <span
                class="shrink-0 rounded bg-faint/20 px-1.5 py-0.5 font-mono text-[9px] uppercase text-muted"
              >
                {{ packet.encoding }}
              </span>
              <span
                v-if="hasTemplate(packet.payload)"
                class="shrink-0 rounded bg-accent/15 px-1.5 py-0.5 font-mono text-[9px] uppercase text-accent"
                title="Placeholders expand at send time"
              >
                tpl
              </span>
              <span
                class="shrink-0 font-mono text-[10px] text-faint"
                :title="payloadInfo(packet, index).error ?? undefined"
              >
                {{ payloadSize(packet, index) }}
              </span>
              <button
                class="shrink-0 rounded-md border border-line px-2 py-1 text-[10px] text-muted transition hover:text-ink disabled:opacity-40"
                :disabled="!status.connected || !!packetProblem(packet)"
                title="Write this packet now"
                @click="sendOnce(packet)"
              >
                Send
              </button>
              <button
                class="shrink-0 rounded-md border border-accent/40 bg-accent/10 px-2 py-1 text-[10px] text-accent transition hover:bg-accent/20 disabled:opacity-40"
                :disabled="!!packetProblem(packet)"
                title="Inject as inbound data (no hardware needed)"
                @click="injectPacket(packet)"
              >
                Inject
              </button>
            </div>

            <div
              v-if="composerPackets.length === 0"
              class="rounded-xl border border-dashed border-line px-3 py-6 text-center text-xs text-faint"
            >
              No packets in the script yet.
            </div>
          </div>
        </div>

        <!-- Limit -->
        <div class="flex items-center justify-between gap-2 rounded-xl border border-line px-3 py-2">
          <label class="flex items-center gap-2 text-xs text-muted">
            <input v-model="hasLimit" type="checkbox" class="accent-teal-500" />
            Stop after
          </label>
          <input
            v-if="hasLimit"
            :value="sendLimit"
            type="number"
            min="1"
            max="10000000"
            step="1"
            class="w-24 rounded-md border border-line bg-field px-2 py-1 text-right font-mono text-[11px] text-ink outline-none focus:border-accent"
            aria-label="Packet limit"
            @change="limitInput"
          />
          <span v-else class="font-mono text-[10px] text-dim">continuous</span>
        </div>

        <button
          class="w-full rounded-lg py-2 text-sm font-semibold text-white transition disabled:opacity-40"
          :class="
            sender.running
              ? 'bg-danger/80 hover:bg-danger'
              : 'bg-teal-600 hover:bg-teal-500'
          "
          :disabled="
            senderBusy ||
            (!sender.running && (!status.connected || scriptErrors.length > 0))
          "
          @click="sender.running ? stopSending() : startSending()"
        >
          {{
            sender.running
              ? "Stop schedule"
              : status.connected
                ? "Start schedule"
                : "Connect to start"
          }}
        </button>

        <p v-if="packetError" class="text-xs text-danger">{{ packetError }}</p>
        <p v-else-if="sender.lastError" class="text-xs text-danger">
          {{ sender.lastError }}
        </p>
        <p v-if="sender.running" class="text-[10px] text-warning/80">
          The running schedule re-expands templates at send time. Stop and
          start again to change the packet definitions.
        </p>
        <p class="text-[10px] leading-relaxed text-faint">
          The script is parsed live; invalid lines are reported above and are
          never sent. Text payloads support <code>\n</code>, <code>\r</code>,
          <code>\t</code>, <code>\0</code>, <code>\\</code> and
          <code>\xNN</code>. Hex accepts spaces, commas and <code>0x</code>
          prefixes. Raw sends the line as UTF-8 without escape processing.
          Placeholders such as <code>{seq}</code> and <code>{temp:.2}</code> are
          filled from live state and telemetry on every send. “Inject”
          simulates an inbound chunk and feeds the monitor widgets.
        </p>
      </section>
    </div>

    <!-- Cadence -->
    <section class="rounded-2xl border border-line bg-panel p-4">
      <div class="mb-3 flex flex-wrap items-center justify-between gap-2">
        <div class="flex flex-col">
          <span class="text-xs font-bold uppercase text-muted">TX cadence</span>
          <span class="text-[11px] text-faint">
            Each bar is one send; amber bars are manual one-shots.
          </span>
        </div>
        <div class="flex items-center gap-3 font-mono text-[10px] text-dim">
          <span>last actual {{ lastActualInterval === null ? "—" : `${lastActualInterval} ms` }}</span>
          <span>scheduled {{ sender.sent.toLocaleString() }}</span>
        </div>
      </div>

      <div class="flex h-14 items-end gap-1 overflow-hidden rounded-lg border border-line bg-field px-2 py-2">
        <div
          v-for="mark in recentMarks"
          :key="mark.id"
          class="min-w-1 flex-1 rounded-t transition-all"
          :class="mark.manual ? 'bg-warning/70' : 'bg-accent/70'"
          :style="{
            height: `${Math.min(100, 18 + Math.min(64, mark.bytes) * 1.1)}%`,
          }"
          :title="`${mark.label} · ${mark.bytes} B · ${formatClock(mark.at)}${mark.manual ? ' · manual' : ''}`"
        />
        <div
          v-if="recentMarks.length === 0"
          class="flex w-full items-center justify-center text-[11px] text-faint"
        >
          No packets sent yet.
        </div>
      </div>
    </section>
  </div>
</template>
