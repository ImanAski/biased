/** Units and metadata shared by the monitoring cards, controls and settings. */
export const POWER_UNIT = "W";

/* -------------------------------------------------------------------------- */
/* Monitoring                                                                 */
/* -------------------------------------------------------------------------- */

export type MetricKey = "theta0" | "temp" | "inputPower" | "outputPower";

export interface MetricMeta {
  key: MetricKey;
  label: string;
  unit: string;
  /** Decimal places shown on the monitoring card. */
  digits: number;
}

/** Order matches the four overview cards. */
export const METRICS: MetricMeta[] = [
  { key: "theta0", label: "Θ₀", unit: "°", digits: 1 },
  { key: "temp", label: "Temp", unit: "°C", digits: 1 },
  { key: "inputPower", label: "Input Power", unit: POWER_UNIT, digits: 2 },
  { key: "outputPower", label: "Output Power", unit: POWER_UNIT, digits: 2 },
];

/** Format a monitored value, or an em dash before the first sample arrives. */
export function formatMetric(value: number | null, digits: number): string {
  return value === null ? "—" : value.toFixed(digits);
}

/* -------------------------------------------------------------------------- */
/* Controls                                                                   */
/* -------------------------------------------------------------------------- */

export const BIAS_POINTS = ["QUAD+", "QUAD-", "MAX", "NULL"] as const;
export type BiasPoint = (typeof BIAS_POINTS)[number];

export type DitherKey = "amp" | "freq";

export interface DitherMeta {
  key: DitherKey;
  label: string;
  unit: string;
}

export const DITHER_FIELDS: DitherMeta[] = [
  { key: "amp", label: "Amplitude", unit: "V" },
  { key: "freq", label: "Frequency", unit: "Hz" },
];
