import assert from "node:assert/strict";
import test from "node:test";
import { init, use } from "echarts/core";
import { LineChart } from "echarts/charts";
import { GridComponent, DataZoomComponent } from "echarts/components";
import { SVGRenderer } from "echarts/renderers";
import {
  analyticsDateBounds,
  analyticsQuery,
  analyticsInterval,
  customAnalyticsRange,
  trafficPeriodLabel,
} from "../utils/analyticsRange.ts";
import { chartZoom } from "../utils/chartTheme.ts";

test("preset and custom durations choose the same granularity", () => {
  for (const [days, expected] of [
    [7, "day"],
    [30, "day"],
    [31, "week"],
    [90, "week"],
    [180, "week"],
    [181, "month"],
    [365, "month"],
  ] as const) {
    assert.equal(analyticsInterval(days), expected);
  }
  assert.equal(
    customAnalyticsRange("2026-05-01", "2026-06-30", "2026-09-19")?.days,
    61,
  );
  assert.equal(
    customAnalyticsRange("2024-02-28", "2024-03-01", "2024-03-01")?.days,
    3,
  );
  for (const [start, end] of [
    ["2026-02-30", "2026-03-01"],
    ["2026-09-20", "2026-09-20"],
    ["2026-09-19", "2026-09-18"],
    ["2025-09-19", "2026-09-19"],
    ["", ""],
  ] as const) {
    assert.equal(customAnalyticsRange(start, end, "2026-09-19"), null);
  }
  assert.deepEqual(analyticsDateBounds("2026-09-19"), {
    min: "2025-09-20",
    max: "2026-09-19",
  });
  assert.equal(
    customAnalyticsRange("2025-09-20", "2026-09-19", "2026-09-19")?.days,
    365,
  );
  assert.equal(
    customAnalyticsRange("2025-01-01", "2025-01-07", "2026-09-19"),
    null,
  );
  assert.equal(trafficPeriodLabel("2026-01-01", "month"), "Jan 2026");
  assert.equal(
    trafficPeriodLabel("2026-09-14", "week"),
    "Week of Sep 14, 2026",
  );
});

test("overview requests explicitly select grouping for presets and custom dates", () => {
  for (const [days, group_by] of [
    [7, "day"],
    [30, "day"],
    [90, "week"],
    [180, "week"],
    [365, "month"],
  ] as const) {
    assert.deepEqual(analyticsQuery("site", days), {
      site_id: "site",
      days,
      group_by,
    });
  }
  const dates = { start_date: "2026-05-01", end_date: "2026-06-30" };
  assert.deepEqual(analyticsQuery("site", 61, dates), {
    site_id: "site",
    days: 61,
    ...dates,
    group_by: "week",
  });
});

test("ECharts keeps full extent when changing duration, including after manual zoom", () => {
  use([LineChart, GridComponent, DataZoomComponent, SVGRenderer]);
  const chart = init(null, undefined, {
    renderer: "svg",
    ssr: true,
    width: 900,
    height: 400,
  });
  const theme = {
    axis: "#000",
    grid: "#ccc",
    series: ["#00f"],
    text: "#000",
    tooltipBg: "#fff",
    tooltipText: "#000",
  };
  try {
    for (const count of [7, 30, 90, 180, 365, 7]) {
      chart.setOption({
        animation: false,
        xAxis: {
          type: "category",
          data: Array.from({ length: count }, (_, i) => String(i)),
        },
        yAxis: { type: "value" },
        dataZoom: chartZoom(count, theme),
        series: [
          {
            type: "line",
            data: Array.from({ length: count }, (_, i) => i + 1),
          },
        ],
      });
      chart.dispatchAction({ type: "dataZoom", start: 0, end: 100 });
      for (const zoom of chart.getOption().dataZoom as {
        start: number;
        end: number;
      }[]) {
        assert.equal(zoom.start, 0);
        assert.equal(zoom.end, 100);
      }
      chart.dispatchAction({ type: "dataZoom", start: 30, end: 70 });
    }
  } finally {
    chart.dispose();
  }
});
