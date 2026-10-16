export const trafficMetricLabels = {
  pageviews: "Page views",
  visitors: "Estimated visitors",
  events: "Tracked events",
  sessions: "Sessions",
} as const;

export type TrafficMetric = keyof typeof trafficMetricLabels;
export type TrafficChartPoint = { date: string } & Partial<
  Record<TrafficMetric, number>
>;

// Use a stable presentation order without adding metrics absent from a share.
export function orderedTrafficMetrics(
  metrics: readonly TrafficMetric[],
): TrafficMetric[] {
  return (Object.keys(trafficMetricLabels) as TrafficMetric[]).filter(
    (metric) => metrics.includes(metric),
  );
}
