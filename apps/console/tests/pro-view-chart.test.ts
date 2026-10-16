import assert from "node:assert/strict";
import test from "node:test";
import { BarChart, LineChart, PieChart, ScatterChart } from "echarts/charts";
import {
  DataZoomComponent,
  GridComponent,
  LegendComponent,
} from "echarts/components";
import { init, use } from "echarts/core";
import { SVGRenderer } from "echarts/renderers";
import type { ProViewPreviewResponse } from "../types/pro-view.ts";
import type { ChartTheme } from "../utils/chartTheme.ts";
import { proViewPlotOption } from "../utils/proViewChart.ts";

use([
  BarChart,
  LineChart,
  PieChart,
  ScatterChart,
  DataZoomComponent,
  GridComponent,
  LegendComponent,
  SVGRenderer,
]);

type PresentedSeries = {
  type: string;
  data: Array<
    | {
        name: string;
        value: number;
        itemStyle: { color: string; borderRadius: number | number[] };
      }
    | number[]
    | null
  >;
  itemStyle: {
    color: string;
    borderRadius: number;
    borderWidth: number;
    borderColor: string;
  };
  center: string[];
  barMaxWidth: number;
  barMinHeight: number;
  minAngle?: number;
  stillShowZeroSum: boolean;
  label: { show: boolean };
  connectNulls: boolean;
  smoothMonotone: string;
  lineStyle: { join: string };
};

// EChartsCoreOption exposes series as unknown, so narrow the fields being
// verified here without depending on the renderer's internal model classes.
function plot(...args: Parameters<typeof proViewPlotOption>) {
  return proViewPlotOption(...args) as {
    series: PresentedSeries[];
    xAxis: { data: string[] };
    yAxis: { minInterval: number };
    grid: { bottom: number };
    legend: { type: string; icon: string };
    dataZoom: unknown[];
  };
}
function item(series: PresentedSeries, index: number) {
  const point = series.data[index];
  assert.ok(point && !Array.isArray(point));
  return point;
}

const theme: ChartTheme = {
  axis: "#737780",
  grid: "#34363d",
  series: ["#d8ff52", "#a391ff", "#ff705c", "#3d5afe"],
  text: "#c4c5c0",
  tooltipBg: "#111214",
  tooltipText: "#f7f7f2",
};
const preview: ProViewPreviewResponse = {
  granularity: "day",
  current: {
    label: "Current",
    points: [
      { period: "SG", value: 39 },
      { period: "IN", value: 4 },
      { period: "US", value: 0 },
      { period: "GB", value: -3 },
    ],
  },
  previous: {
    label: "Previous",
    points: [
      { period: "IN", value: 10 },
      { period: "SG", value: 20 },
      { period: "DE", value: 5 },
    ],
  },
};

test("category bars use theme colors and rounded ends while retaining zeros, negatives and missing groups", () => {
  const original = structuredClone(preview);
  const option = plot(
    { visualization: "bar", breakdown: "country" },
    preview,
    theme,
  );
  assert.deepEqual(preview, original);
  const current = option.series[0]!;
  const previous = option.series[1]!;
  assert.deepEqual(option.xAxis.data, ["SG", "IN", "US", "GB", "DE"]);
  assert.equal(current.itemStyle.color, theme.series[0]);
  assert.equal(previous.itemStyle.color, theme.series[1]);
  assert.equal(current.barMaxWidth, 44);
  assert.equal(current.barMinHeight, 0);
  assert.deepEqual(current.data, [
    { value: 39, itemStyle: { borderRadius: [8, 8, 0, 0] } },
    { value: 4, itemStyle: { borderRadius: [8, 8, 0, 0] } },
    { value: 0, itemStyle: { borderRadius: 0 } },
    { value: -3, itemStyle: { borderRadius: [0, 0, 8, 8] } },
    null,
  ]);
  assert.equal(previous.data[2], null);
  assert.deepEqual(option.dataZoom, []);
  assert.equal(option.grid.bottom, 48);
});

test("pie comparisons keep category colors across differently ordered rings and preserve values", () => {
  for (const visualization of ["pie", "donut"]) {
    const option = plot(
      { visualization, breakdown: "country" },
      preview,
      theme,
    );
    const current = option.series[0]!;
    const previous = option.series[1]!;
    assert.equal(item(current, 0).name, "SG");
    assert.equal(item(current, 0).itemStyle.color, theme.series[0]);
    assert.equal(item(previous, 0).name, "IN");
    assert.equal(item(previous, 0).itemStyle.color, theme.series[1]);
    assert.equal(
      item(previous, 1).itemStyle.color,
      item(current, 0).itemStyle.color,
    );
    assert.deepEqual(
      current.data.map((_, index) => item(current, index).value),
      [39, 4, 0, -3],
    );
    assert.deepEqual(current.center, ["50%", "43%"]);
    assert.equal(current.itemStyle.borderRadius, 8);
    assert.equal(current.itemStyle.borderWidth, 3);
    assert.equal(current.itemStyle.borderColor, theme.tooltipBg);
    assert.equal(current.stillShowZeroSum, false);
    assert.equal(current.minAngle, undefined);
    assert.equal(previous.label.show, false);
    assert.equal(option.legend.type, "scroll");
    assert.equal(option.legend.icon, "circle");
  }
});

test("overlapping audience metrics retain their bar fallback instead of suggesting parts of a whole", () => {
  for (const metric of ["visitors", "sessions"]) {
    const option = plot(
      { visualization: "donut", breakdown: "country", metric },
      preview,
      theme,
    );
    assert.equal(option.series[0]!.type, "bar");
    assert.equal(item(option.series[0]!, 0).value, 39);
    assert.equal(option.series[0]!.barMinHeight, 0);
  }
});

test("line and scatter views preserve unknown comparison buckets and reserve navigator space only when used", () => {
  for (const visualization of ["line", "scatter"]) {
    const option = plot(
      { visualization, breakdown: "time", metric: "p75" },
      preview,
      theme,
    );
    assert.equal(option.dataZoom.length, 2);
    assert.equal(option.grid.bottom, 112);
    assert.equal(option.yAxis.minInterval, 0);
    if (visualization === "line") {
      assert.equal(option.series[0]!.connectNulls, false);
      assert.equal(option.series[0]!.smoothMonotone, "x");
      assert.equal(option.series[0]!.lineStyle.join, "round");
      assert.equal(option.series[0]!.data[4], null);
    } else {
      assert.deepEqual(option.series[0]!.data[4], [4, null]);
    }
  }
});

test("the actual chart renderer accepts themed category and comparison layouts at compact widths", () => {
  for (const visualization of ["bar", "donut", "pie", "line", "scatter"]) {
    const chart = init(null, undefined, {
      height: 330,
      renderer: "svg",
      ssr: true,
      width: 360,
    });
    try {
      chart.setOption({
        animation: false,
        color: theme.series,
        ...proViewPlotOption(
          { visualization, breakdown: "country" },
          preview,
          theme,
        ),
      });
      const svg = chart.renderToSVGString();
      assert.ok(svg.includes("<svg"));
      assert.ok(svg.includes(theme.series[0]!));
      assert.ok(svg.includes(theme.series[1]!));
      assert.doesNotMatch(svg, /NaN|undefined/);
      if (visualization === "line") assert.match(svg, /C[\d., -]+/);
    } finally {
      chart.dispose();
    }
  }
});
