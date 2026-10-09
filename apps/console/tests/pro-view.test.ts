import assert from "node:assert/strict";
import test from "node:test";
import {
  defaultProViewDefinition,
  widgetDateRange,
  widgetRangeLabel,
  breakdownLabel,
  widgetRequestBody,
} from "../types/pro-view.ts";

test("new charts use duration only, and comparison is independent of grouping", () => {
  const chart = defaultProViewDefinition("chart");
  assert.deepEqual(chart.date_range, { days: 30, compare_previous: false });
  assert.equal(widgetRangeLabel(chart), "Last 30 days");
  chart.breakdown = "campaign";
  chart.date_range = { days: 45, compare_previous: true };
  assert.equal(widgetRangeLabel(chart), "Last 45 days vs previous period");
  assert.equal(breakdownLabel(chart), "Campaign");
  chart.visualization = "map";
  assert.equal(breakdownLabel(chart), "Country");
});
test("saved legacy chart labels retain their previous date meaning", () => {
  const chart = defaultProViewDefinition("chart");
  delete chart.date_range;
  chart.comparison = "today_vs_yesterday";
  assert.equal(widgetRangeLabel(chart), "Today vs yesterday · hourly");
  assert.deepEqual(widgetDateRange(chart), { days: 1, compare_previous: true });
  chart.comparison = "none";
  assert.equal(widgetRangeLabel(chart), "Last 30 days");
});

test("saved chart requests exclude server metadata and retain chart controls", () => {
  const saved = {
    ...defaultProViewDefinition("chart"),
    id: "saved-id",
    created_by: { email: "owner@example.com" },
    created_at: "2026-01-01",
    updated_at: "2026-01-02",
  };
  saved.breakdown = "city";
  const body = widgetRequestBody(saved);
  for (const field of ["id", "created_by", "created_at", "updated_at"])
    assert.equal(field in body, false);
  assert.equal(body.breakdown, "city");
  assert.deepEqual(body.date_range, { days: 30, compare_previous: false });
});

test("new chart controls survive save and reload", () => {
  const chart = defaultProViewDefinition("chart");
  assert.equal(chart.metric, "events");
  chart.metric = "visitors";
  chart.breakdown = "property";
  chart.breakdown_property = "format";
  chart.property_filters = {
    logic: "and",
    filters: [{ id: "a", key: "plan", operator: "equals", value: "pro" }],
  };
  const body = widgetRequestBody(chart);
  assert.equal(body.metric, "visitors");
  assert.equal(body.breakdown_property, "format");
  assert.deepEqual(body.property_filters, chart.property_filters);
  assert.equal(breakdownLabel(chart), "Property: format");
});
