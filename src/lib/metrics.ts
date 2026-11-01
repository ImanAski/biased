/** Bias operating points understood by the MCU and the widget config. */
export const BIAS_POINTS = ["QUAD+", "QUAD-", "MAX", "NULL"] as const;

export type BiasPoint = (typeof BIAS_POINTS)[number];
