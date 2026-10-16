import type { CohortPoint, CohortSummary } from "../types/stats.ts";
import { metricSparkline } from "./metricSparkline.ts";

export type CohortMetric = "cohorts" | "bounce" | "span";
export const cohortMetrics = [
  {
    key: "cohorts",
    label: "Closed session cohorts",
    note: "Eligible pageview cohorts · fixed 30-minute buckets",
  },
  {
    key: "bounce",
    label: "Estimated bounce rate",
    note: "Single-page cohorts / eligible pageview cohorts",
  },
  {
    key: "span",
    label: "Observed session span",
    note: "Mean first-to-last activity · multi-observation sample",
  },
] as const;

function finite(value: unknown): value is number {
  return typeof value === "number" && Number.isFinite(value) && value >= 0;
}
export function cohortValue(
  value: Partial<CohortPoint> | CohortSummary | undefined,
  metric: CohortMetric,
): number | undefined {
  if (
    !value ||
    ("status" in value &&
      (value.status === "unavailable_in_demo" ||
        value.status === "query_failed"))
  )
    return;
  if (metric === "cohorts")
    return finite(value.eligible_cohorts) ? value.eligible_cohorts : undefined;
  if (metric === "bounce")
    return finite(value.eligible_cohorts) &&
      value.eligible_cohorts > 0 &&
      finite(value.estimated_bounce_rate) &&
      value.estimated_bounce_rate <= 100
      ? value.estimated_bounce_rate
      : undefined;
  return finite(value.observed_span_samples) &&
    value.observed_span_samples > 0 &&
    finite(value.avg_observed_span_ms)
    ? value.avg_observed_span_ms
    : undefined;
}
export function cohortMetricText(
  value: number | undefined,
  metric: CohortMetric,
) {
  if (!finite(value)) return "—";
  if (metric === "bounce")
    return `${value.toLocaleString(undefined, { maximumFractionDigits: 1 })}%`;
  if (metric === "cohorts") return value.toLocaleString();
  if (value < 1000)
    return `${value.toLocaleString(undefined, { maximumFractionDigits: 0 })} ms`;
  if (value < 60_000)
    return `${(value / 1000).toLocaleString(undefined, { maximumFractionDigits: 1 })} s`;
  const seconds = Math.round(value / 1000);
  return `${Math.floor(seconds / 60)}m ${seconds % 60}s`;
}
/** Independent scales keep percentages, cohort counts and duration units distinct. */
export function cohortScaleMaximum(
  points: readonly CohortPoint[],
  metric: CohortMetric,
) {
  return metric === "bounce"
    ? 100
    : Math.max(1, ...points.map((point) => cohortValue(point, metric) ?? 0));
}
export function cohortSparkline(
  points: readonly CohortPoint[],
  metric: CohortMetric,
) {
  const spark = metricSparkline(
    points.map((point) => ({
      date: point.date,
      events: cohortValue(point, metric),
    })),
    "events",
    cohortScaleMaximum(points, metric),
  );
  // Fill only contiguous observed runs. Closing each run independently keeps
  // unknown buckets empty instead of painting a bridge across missing data.
  const area = spark.segments
    .flatMap((segment) => {
      if (segment.count < 2) return [];
      return [
        `${segment.path} L${segment.end[0].toFixed(2)},52 L${segment.start[0].toFixed(2)},52 Z`,
      ];
    })
    .join(" ");
  return { ...spark, area };
}
