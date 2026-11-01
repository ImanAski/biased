import { ref } from "vue";

export type View = "overview" | "packets" | "settings" | "logs";

export interface ViewItem {
  id: View;
  label: string;
}

export const views: ViewItem[] = [
  { id: "overview", label: "Overview" },
  { id: "packets", label: "Packet Lab" },
  { id: "settings", label: "Settings" },
  { id: "logs", label: "Logs" },
];

/** Which page the center pane renders. */
export const currentView = ref<View>("overview");

export function navigate(view: View) {
  currentView.value = view;
}
