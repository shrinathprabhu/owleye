import type { DimensionStat } from "../types/stats";
export type BreakdownMetric = "pageviews" | "visitors";
export function breakdownLabel(metric: BreakdownMetric) {
  return metric === "visitors" ? "Unique visitors" : "Page views";
}
export function breakdownItems(
  items: DimensionStat[] | undefined,
  metric: BreakdownMetric,
): DimensionStat[] {
  return (items ?? [])
    .map((item) => ({
      ...item,
      count: metric === "visitors" ? (item.visitors ?? 0) : item.count,
    }))
    .sort(
      (left, right) =>
        right.count - left.count || left.name.localeCompare(right.name),
    );
}
