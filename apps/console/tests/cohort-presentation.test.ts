import assert from "node:assert/strict";
import test from "node:test";
import type { CohortPoint, CohortSummary } from "../types/stats.ts";
import {
  cohortMetricText,
  cohortSparkline,
  cohortScaleMaximum,
  cohortValue,
} from "../utils/cohortPresentation.ts";

const summary = {
  status: "observed_estimate",
  eligible_cohorts: 10,
  estimated_bounce_rate: 40,
  observed_span_samples: 2,
  avg_observed_span_ms: 1_500,
} as CohortSummary;

test("cohort cards preserve unavailable, zero, units and sample eligibility", () => {
  assert.equal(cohortValue(undefined, "cohorts"), undefined);
  assert.equal(
    cohortValue(
      { ...summary, status: "query_failed", eligible_cohorts: 0 },
      "cohorts",
    ),
    undefined,
  );
  assert.equal(
    cohortValue({ ...summary, status: "unavailable_in_demo" }, "bounce"),
    undefined,
  );
  assert.equal(
    cohortValue(
      { ...summary, eligible_cohorts: 0, estimated_bounce_rate: 0 },
      "bounce",
    ),
    undefined,
  );
  assert.equal(cohortValue({ ...summary, eligible_cohorts: 0 }, "cohorts"), 0);
  assert.equal(
    cohortValue({ ...summary, estimated_bounce_rate: 0 }, "bounce"),
    0,
  );
  assert.equal(
    cohortValue({ ...summary, estimated_bounce_rate: 101 }, "bounce"),
    undefined,
  );
  assert.equal(
    cohortValue({ ...summary, observed_span_samples: 0 }, "span"),
    undefined,
  );
  assert.equal(
    cohortValue({ ...summary, avg_observed_span_ms: null }, "span"),
    undefined,
  );
  assert.equal(cohortValue({ ...summary, avg_observed_span_ms: 0 }, "span"), 0);
  assert.equal(cohortMetricText(undefined, "span"), "—");
  assert.equal(cohortMetricText(40, "bounce"), "40%");
  assert.equal(cohortMetricText(1_500, "span"), "1.5 s");
  assert.equal(cohortMetricText(75_000, "span"), "1m 15s");
  assert.equal(cohortMetricText(NaN, "cohorts"), "—");
});

test("cohort sparkline gaps stay unknown and use server rates rather than reconstructing them", () => {
  const points = [
    { ...summary, date: "1", estimated_bounce_rate: 40 },
    { ...summary, date: "2", eligible_cohorts: 0, estimated_bounce_rate: null },
    { ...summary, date: "3", estimated_bounce_rate: 80 },
  ] as CohortPoint[];
  assert.equal(
    cohortSparkline(points, "bounce").path,
    "M4.00,34.40 M236.00,16.80",
  );
  assert.equal(cohortValue(points[0], "bounce"), 40);
});

test("cohort comparison charts keep independently labelled count, percentage and time scales", () => {
  const points = [
    { ...summary, date: "1" },
    { ...summary, date: "2", eligible_cohorts: 20, avg_observed_span_ms: 3000 },
  ] as CohortPoint[];
  assert.equal(cohortScaleMaximum(points, "cohorts"), 20);
  assert.equal(cohortScaleMaximum(points, "bounce"), 100);
  assert.equal(cohortScaleMaximum(points, "span"), 3000);
  assert.equal(
    cohortMetricText(cohortScaleMaximum(points, "bounce"), "bounce"),
    "100%",
  );
  assert.equal(
    cohortMetricText(cohortScaleMaximum(points, "span"), "span"),
    "3 s",
  );
  assert.equal(
    cohortMetricText(cohortScaleMaximum(points, "cohorts"), "cohorts"),
    "20",
  );
  assert.equal(
    cohortSparkline(points, "cohorts").path,
    "M4.00,30.00 L236.00,8.00",
  );
  assert.equal(
    cohortSparkline(points, "span").path,
    "M4.00,30.00 L236.00,8.00",
  );
  assert.equal(
    cohortSparkline(points, "bounce").path,
    "M4.00,34.40 L236.00,34.40",
  );
});

test("cohort area fills retain gaps and never invent an area for isolated observations", () => {
  const points = [
    { ...summary, date: "1", estimated_bounce_rate: 40 },
    { ...summary, date: "2", estimated_bounce_rate: 50 },
    { ...summary, date: "3", eligible_cohorts: 0, estimated_bounce_rate: null },
    { ...summary, date: "4", estimated_bounce_rate: 70 },
    { ...summary, date: "5", estimated_bounce_rate: 80 },
  ] as CohortPoint[];
  const area = cohortSparkline(points, "bounce").area;
  assert.equal((area.match(/Z/g) ?? []).length, 2);
  assert.match(area, /L62\.00,52 L4\.00,52 Z/);
  assert.match(area, /L236\.00,52 L178\.00,52 Z/);
  assert.equal(cohortSparkline([points[0]!], "bounce").area, "");
  assert.equal(cohortSparkline([points[2]!], "bounce").area, "");
});

test("cohort areas close each rounded observed run using the same curve as the line", () => {
  const rates = [40, 80, 0, null, 20, 100, 60];
  const spark = cohortSparkline(
    rates.map((estimated_bounce_rate, index) => ({
      ...summary,
      date: String(index),
      estimated_bounce_rate,
      eligible_cohorts: estimated_bounce_rate === null ? 0 : 10,
    })) as CohortPoint[],
    "bounce",
  );
  assert.equal(spark.segments.length, 2);
  assert.equal((spark.area.match(/C/g) ?? []).length, 4);
  assert.equal((spark.area.match(/Z/g) ?? []).length, 2);
  assert.ok(spark.area.startsWith(spark.segments[0]!.path));
  assert.ok(spark.area.includes(spark.segments[1]!.path));
  assert.match(spark.area, /L81\.33,52 L4\.00,52 Z/);
  assert.match(spark.area, /L236\.00,52 L158\.67,52 Z/);
});
