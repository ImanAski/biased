import { computed, ref } from "vue";
import { buildInfoReady, packetLab } from "./widgets";

export type View = "overview" | "packets" | "settings" | "logs";

export interface ViewItem {
  id: View;
  label: string;
}

const allViews: ViewItem[] = [
  { id: "overview", label: "Overview" },
  { id: "packets", label: "Packet Lab" },
  { id: "settings", label: "Settings" },
  { id: "logs", label: "Logs" },
];

/**
 * The Packet Lab is an engineering tool. It is not part of a
 * configuration-locked customer build, so it only appears once build
 * information has confirmed this is an editable build.
 */
export const packetLabAvailable = computed(
  () => buildInfoReady.value && packetLab.value,
);

export const views = computed<ViewItem[]>(() =>
  allViews.filter(
    (view) => view.id !== "packets" || packetLabAvailable.value,
  ),
);

/** Which page the center pane renders. */
export const currentView = ref<View>("overview");

export function navigate(view: View) {
  if (view === "packets" && !packetLabAvailable.value) return;
  currentView.value = view;
}
