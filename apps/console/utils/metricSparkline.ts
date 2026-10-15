import type { TrafficChartPoint, TrafficMetric } from "./trafficMetrics";

// Each bucket keeps its own meaning; missing values break the line, never become zero.
// This is a visual path only. Range totals must come from the API, not summed buckets.
export function metricSparkline(
  points: readonly TrafficChartPoint[],
  metric: TrafficMetric,
) {
  const ordered = [...points].sort((a, b) => a.date.localeCompare(b.date));
  const values = ordered.map((point) => point[metric]);
  const finite = values.filter(
    (value): value is number =>
      typeof value === "number" && Number.isFinite(value) && value >= 0,
  );
  if (!finite.length) return { path: "", point: null };
  const maximum = Math.max(1, ...finite);
  let connected = false;
  const coordinates: [number, number][] = [];
  const path = values
    .map((value, index) => {
      if (typeof value !== "number" || !Number.isFinite(value) || value < 0) {
        connected = false;
        return "";
      }
      const x =
        values.length === 1 ? 120 : 4 + (index / (values.length - 1)) * 232;
      const y = 52 - (value / maximum) * 44;
      coordinates.push([x, y]);
      const command = connected ? "L" : "M";
      connected = true;
      return `${command}${x.toFixed(2)},${y.toFixed(2)}`;
    })
    .join(" ");
  return { path, point: finite.length === 1 ? coordinates[0]! : null };
}
