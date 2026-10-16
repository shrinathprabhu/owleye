import type { TrafficChartPoint, TrafficMetric } from "./trafficMetrics";

type Coordinate = [number, number];

export interface SparklineSegment {
  path: string;
  start: Coordinate;
  end: Coordinate;
  count: number;
}

function coordinate([x, y]: Coordinate) {
  return `${x.toFixed(2)},${y.toFixed(2)}`;
}

/** Monotone cubic interpolation rounds turns without inventing peaks or dips. */
function observedSegment(points: Coordinate[]): SparklineSegment {
  const start = points[0]!;
  const end = points.at(-1)!;
  let path = `M${coordinate(start)}`;
  if (points.length === 2) path += ` L${coordinate(end)}`;
  if (points.length > 2) {
    const slopes = points.slice(1).map(([x, y], index) => {
      const previous = points[index]!;
      return (y - previous[1]) / (x - previous[0]);
    });
    const tangents = points.map((_, index) => {
      if (index === 0) return slopes[0]!;
      if (index === points.length - 1) return slopes.at(-1)!;
      const before = slopes[index - 1]!;
      const after = slopes[index]!;
      // Flat sections and direction changes have horizontal tangents. The
      // harmonic mean bounds the other tangents by both neighbouring slopes.
      if (before === 0 || after === 0 || Math.sign(before) !== Math.sign(after))
        return 0;
      return (2 * before * after) / (before + after);
    });
    for (let index = 1; index < points.length; index++) {
      const previous = points[index - 1]!;
      const current = points[index]!;
      const third = (current[0] - previous[0]) / 3;
      const lower = Math.min(previous[1], current[1]);
      const upper = Math.max(previous[1], current[1]);
      const bounded = (value: number) =>
        Math.min(upper, Math.max(lower, value));
      const control1: Coordinate = [
        previous[0] + third,
        bounded(previous[1] + tangents[index - 1]! * third),
      ];
      const control2: Coordinate = [
        current[0] - third,
        bounded(current[1] - tangents[index]! * third),
      ];
      path += ` C${coordinate(control1)} ${coordinate(control2)} ${coordinate(current)}`;
    }
  }
  return { path, start, end, count: points.length };
}

// Each bucket keeps its own meaning; missing values break the line, never become zero.
// This is a visual path only. Range totals must come from the API, not summed buckets.
export function metricSparkline(
  points: readonly TrafficChartPoint[],
  metric: TrafficMetric,
  scaleMaximum?: number,
) {
  const ordered = [...points].sort((a, b) => a.date.localeCompare(b.date));
  const values = ordered.map((point) => point[metric]);
  const finite = values.filter(
    (value): value is number =>
      typeof value === "number" && Number.isFinite(value) && value >= 0,
  );
  if (!finite.length)
    return { path: "", point: null, segments: [] as SparklineSegment[] };
  const maximum = Math.max(
    1,
    Number.isFinite(scaleMaximum) ? (scaleMaximum ?? 1) : 1,
    ...finite,
  );
  const segments: SparklineSegment[] = [];
  let run: Coordinate[] = [];
  const closeRun = () => {
    if (run.length) segments.push(observedSegment(run));
    run = [];
  };
  values.forEach((value, index) => {
    if (typeof value !== "number" || !Number.isFinite(value) || value < 0) {
      closeRun();
      return;
    }
    const x =
      values.length === 1 ? 120 : 4 + (index / (values.length - 1)) * 232;
    run.push([x, 52 - (value / maximum) * 44]);
  });
  closeRun();
  return {
    path: segments.map((segment) => segment.path).join(" "),
    point: finite.length === 1 ? segments[0]!.start : null,
    segments,
  };
}
