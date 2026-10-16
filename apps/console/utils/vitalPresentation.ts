import type { PerformanceMetric } from "../types/insights";

// Mirrors the API rating boundaries. This describes thresholds, not a sample distribution.
export const VITAL_PRESENTATION: Record<
  PerformanceMetric,
  { label: string; good: number; poor: number; core: boolean }
> = {
  LCP: { label: "Loading", good: 2500, poor: 4000, core: true },
  INP: { label: "Responsiveness", good: 200, poor: 500, core: true },
  CLS: { label: "Visual stability", good: 0.1, poor: 0.25, core: true },
  FCP: { label: "First content", good: 1800, poor: 3000, core: false },
  TTFB: { label: "Server response", good: 800, poor: 1800, core: false },
};

export function vitalValue(metric: PerformanceMetric, raw?: number | null) {
  if (raw == null || !Number.isFinite(raw) || raw < 0) return "—";
  return metric === "CLS"
    ? raw.toFixed(3)
    : `${Math.round(raw).toLocaleString()} ms`;
}

export function vitalPosition(metric: PerformanceMetric, raw?: number | null) {
  if (raw == null || !Number.isFinite(raw) || raw < 0) return null;
  const { good, poor } = VITAL_PRESENTATION[metric];
  // Three equal visual bands, with a bounded marker inside the open-ended poor band.
  return raw <= good
    ? ((raw / good) * 100) / 3
    : raw <= poor
      ? ((1 + (raw - good) / (poor - good)) * 100) / 3
      : Math.min(100, ((2 + (raw - poor) / poor) * 100) / 3);
}
