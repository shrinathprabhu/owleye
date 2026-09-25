import assert from "node:assert/strict";
import test from "node:test";
import {
  chartPoints,
  chartOptions,
  defaultChart,
  reportLabel,
  reportScope,
} from "../utils/aiReport.ts";
import type { AiEvidence } from "../types/ai.ts";
const evidence: AiEvidence = {
  report: "daily",
  start_date: "2026-09-14",
  end_date: "2026-09-16",
  timezone: "UTC",
  groups_suppressed_below_visitors: 5,
  country: "IN",
  metric: "visitors",
  rows: [
    {
      label: "2026-09-14",
      visitors: 8,
      sessions: 10,
      pageviews: 12,
      events: 15,
    },
    { label: "2026-09-16", visitors: 6, sessions: 7, pageviews: 9, events: 11 },
  ],
};
test("filtered chart gaps are unknown rather than invented zeros", () => {
  assert.deepEqual(
    chartPoints(evidence, "visitors").map((p) => p.value),
    [8, null, 6],
  );
  assert.deepEqual(
    chartPoints(
      { ...evidence, groups_suppressed_below_visitors: 0 },
      "sessions",
    ).map((p) => p.value),
    [10, 0, 7],
  );
});
test("weekly buckets retain server distinct counts and partial Monday labels", () => {
  const weekly = {
    ...evidence,
    report: "weekly",
    start_date: "2026-09-15",
    end_date: "2026-09-22",
    rows: [{ ...evidence.rows[0]!, label: "2026-09-14" }],
  };
  assert.deepEqual(chartPoints(weekly, "visitors"), [
    { label: "2026-09-14", value: 8 },
    { label: "2026-09-21", value: null },
  ]);
  assert.equal(defaultChart(weekly), "none");
  assert.equal(
    defaultChart({ ...weekly, chart: "line", rows: evidence.rows }),
    "line",
  );
});
test("country labels and scopes use friendly names without altering stored codes", () => {
  assert.equal(reportLabel("IN", "country"), "India (IN)");
  assert.equal(reportScope(evidence), "India (IN)");
  assert.equal(defaultChart({ ...evidence, report: "country" }), "none");
  assert.equal(
    defaultChart({ ...evidence, report: "country", chart: "donut" }),
    "donut",
  );
});

test("totals and single buckets never invite meaningless charts", () => {
  assert.equal(
    defaultChart({ ...evidence, report: "totals", chart: "pie" }),
    "none",
  );
  assert.deepEqual(
    chartOptions(
      {
        ...evidence,
        report: "country",
        chart: "pie",
        rows: [evidence.rows[0]!],
      },
      "visitors",
    ),
    [],
  );
  assert.deepEqual(chartOptions({ ...evidence, chart: "line" }, "visitors"), [
    "line",
    "bar",
  ]);
  assert.deepEqual(
    chartOptions({ ...evidence, report: "browser", chart: "bar" }, "visitors"),
    ["bar", "pie", "donut"],
  );
  assert.deepEqual(
    chartOptions({ ...evidence, chart: "auto" }, "visitors"),
    [],
  );
  assert.equal(
    reportScope({
      ...evidence,
      country: ["IN", "US"],
      browser: ["Chrome", "Safari"],
      os: ["macOS"],
    }),
    "India (IN) or United States (US) · Chrome or Safari · macOS",
  );
});
