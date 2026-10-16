import assert from "node:assert/strict";
import test from "node:test";
import { init, use, type EChartsCoreOption } from "echarts/core";
import { LineChart } from "echarts/charts";
import {
  DataZoomComponent,
  GridComponent,
  LegendComponent,
} from "echarts/components";
import { SVGRenderer } from "echarts/renderers";
import { toggleChartMetric } from "../utils/chartSelection.ts";
import { chartSnapshot } from "../utils/chartSnapshot.ts";
import { chartWidgetSvg, type SnapshotMetric } from "../utils/metricWidget.ts";
import { chartLineShape, chartLineStroke } from "../utils/chartTheme.ts";

const keys = ["pageviews", "visitors", "events"] as const;
const series = [
  {
    key: "pageviews",
    label: "Page views",
    color: "#d8ff52",
    unit: "page views",
    value: 120,
    data: [10, 20, 30, 40, 20],
  },
  {
    key: "visitors",
    label: "Estimated visitors",
    color: "#a391ff",
    unit: "range-deduplicated estimates",
    value: 41,
    data: [8, 15, 20, 22, 11],
  },
  {
    key: "events",
    label: "Tracked events",
    color: "#ff705c",
    unit: "events",
    value: 160,
    data: [15, 25, 40, 50, 30],
  },
] as const;

test("chart comparison toggles keep every 1/2/3 combination nonempty and ordered", () => {
  for (let mask = 1; mask < 8; mask++) {
    const selected = keys.filter((_, index) => (mask & (1 << index)) !== 0);
    for (const key of keys) {
      const original = [...selected];
      const next = toggleChartMetric(selected, key, keys);
      assert.ok(next.length >= 1 && next.length <= 3);
      assert.deepEqual(selected, original);
      assert.deepEqual(
        next,
        keys.filter((item) => next.includes(item)),
      );
      assert.equal(
        next.includes(key),
        selected.length === 1 && selected.includes(key)
          ? true
          : !selected.includes(key),
      );
    }
  }
  assert.deepEqual(toggleChartMetric([], "visitors", keys), ["visitors"]);
});

use([
  LineChart,
  GridComponent,
  LegendComponent,
  DataZoomComponent,
  SVGRenderer,
]);

test("each 1/2/3-series export captures the live selection, axes and manual zoom without hidden series", async () => {
  const chart = init(null, undefined, {
    renderer: "svg",
    ssr: true,
    width: 720,
    height: 420,
  });
  const dates = ["DAY_ONE", "DAY_TWO", "DAY_THREE", "DAY_FOUR", "DAY_FIVE"];
  try {
    for (const mask of [7, 1, 3, 2, 6, 4, 5, 7]) {
      const selected = series.filter((_, index) => (mask & (1 << index)) !== 0);
      chart.setOption(
        {
          animation: false,
          grid: { left: 60, right: 15, top: 25, bottom: 90 },
          legend: {
            bottom: 0,
            selectedMode: false,
          },
          xAxis: {
            type: "category",
            data: dates,
            axisLabel: {
              interval: 0,
              formatter: (value: string) => `UTC_${value}`,
            },
          },
          yAxis: {
            type: "value",
            min: 5,
            max: 35,
            interval: 10,
            axisLabel: { formatter: (value: number) => `COUNT_${value}` },
          },
          dataZoom: [{ type: "inside", startValue: 1, endValue: 3 }],
          series: selected.map((item) => ({
            ...chartLineShape,
            id: item.key,
            name: item.label,
            type: "line",
            data: [...item.data],
            showSymbol: false,
            lineStyle: { ...chartLineStroke, color: item.color },
            itemStyle: { color: item.color },
          })),
        } as EChartsCoreOption,
        { replaceMerge: ["series"], notMerge: false },
      );
      chart.dispatchAction({ type: "dataZoom", startValue: 1, endValue: 3 });
      const before = chart.getOption();
      const snapshot = await chartSnapshot(chart);
      assert.equal(snapshot.width, 720);
      assert.equal(snapshot.height, 420);
      assert.deepEqual(
        chart.getOption().dataZoom,
        before.dataZoom,
        "export never changes the visible zoom",
      );
      assert.match(snapshot.svg, /UTC_DAY_TWO/);
      assert.match(snapshot.svg, /UTC_DAY_FOUR/);
      assert.doesNotMatch(snapshot.svg, /UTC_DAY_ONE|UTC_DAY_FIVE/);
      assert.match(snapshot.svg, /COUNT_5/);
      assert.match(snapshot.svg, /COUNT_35/);
      assert.match(snapshot.svg, /<path[^>]+d="[^"]*C[^"]*"[^>]+stroke="#/);
      for (const item of series) {
        assert.equal(
          snapshot.svg.includes(item.label),
          selected.includes(item),
          `legend selection for ${item.key}`,
        );
        assert.equal(
          snapshot.svg.includes(`stroke="${item.color}"`),
          selected.includes(item),
          `visible path for ${item.key}`,
        );
      }
      const widget = chartWidgetSvg({
        chart: snapshot,
        metrics: selected as unknown as SnapshotMetric[],
        period: "1–5 October 2026 · daily UTC",
        site: "<script>private & site</script>",
      });
      assert.ok(widget.svg.includes("1–5 October 2026 · daily UTC"));
      assert.ok(
        widget.svg.includes("&lt;script&gt;private &amp; site&lt;/script&gt;"),
      );
      assert.ok(
        widget.svg.includes(
          snapshot.svg.replace(/^<svg\b[^>]*>/, "").replace(/<\/svg>\s*$/, ""),
        ),
        "SVG/PNG/code share the captured chart, not a reconstructed series",
      );
      assert.doesNotMatch(
        widget.svg,
        /<(?:script|image|foreignObject)\b|(?:href|src)\s*=/i,
      );
      if (selected.some((item) => item.key === "visitors")) {
        assert.match(
          widget.svg,
          /Estimated visitors: 41 range-deduplicated estimates/,
        );
        assert.doesNotMatch(widget.svg, /Estimated visitors: 76/);
      }
      for (const item of series.filter((item) => !selected.includes(item)))
        assert.ok(
          !widget.svg.includes(item.label),
          "hidden series is absent from static code too",
        );
    }
  } finally {
    chart.dispose();
  }
});

test("rounded chart exports keep unknown buckets as gaps and retain measured values", async () => {
  const chart = init(null, undefined, {
    renderer: "svg",
    ssr: true,
    width: 720,
    height: 300,
  });
  const values = [0, 100, 0, null, 80, 0, 60];
  try {
    chart.setOption({
      animation: false,
      xAxis: { type: "category", data: values.map((_, index) => index) },
      yAxis: { type: "value", min: 0, max: 100 },
      series: [
        {
          ...chartLineShape,
          type: "line",
          data: values,
          showSymbol: false,
          areaStyle: { color: "#d8ff52" },
          lineStyle: { ...chartLineStroke, color: "#d8ff52" },
        },
      ],
    } as EChartsCoreOption);
    const snapshot = await chartSnapshot(chart);
    const paths =
      snapshot.svg.match(/<path\b[^>]*stroke="#d8ff52"[^>]*>/g) ?? [];
    assert.equal(paths.length, 1);
    const path = /\bd="([^"]+)"/.exec(paths[0]!)![1]!;
    assert.equal(
      (path.match(/M/g) ?? []).length,
      2,
      "the missing bucket separates both observed runs",
    );
    assert.match(path, /C/);
    assert.match(paths[0]!, /stroke-linecap="round"/);
    assert.match(paths[0]!, /stroke-linejoin="round"/);
    assert.deepEqual(
      (chart.getOption().series as { data: unknown[] }[])[0]!.data,
      values,
    );
  } finally {
    chart.dispose();
  }
});

test("chart widget refuses missing totals, hostile SVG and invalid dimensions", () => {
  const input = {
    chart: {
      svg: '<svg xmlns="http://www.w3.org/2000/svg"><path d="M0 0"/></svg>',
      width: 720,
      height: 420,
    },
    metrics: [series[0]],
    site: "Example",
    period: "UTC",
  };
  assert.throws(() => chartWidgetSvg({ ...input, metrics: [] }));
  assert.throws(() =>
    chartWidgetSvg({ ...input, metrics: [{ ...series[0], value: NaN }] }),
  );
  for (const svg of [
    "<svg><script>alert(1)</script></svg>",
    '<svg onload="alert(1)"></svg>',
    '<svg><image href="https://example.com/track"/></svg>',
    "<svg><foreignObject/></svg>",
  ])
    assert.throws(() =>
      chartWidgetSvg({ ...input, chart: { ...input.chart, svg } }),
    );
  assert.throws(() =>
    chartWidgetSvg({ ...input, chart: { ...input.chart, width: 0 } }),
  );
});

test("an in-flight lazy export keeps the selection captured before later chart updates", async () => {
  const chart = init(null, undefined, {
    renderer: "svg",
    ssr: true,
    width: 600,
    height: 300,
  });
  try {
    chart.setOption({
      animation: false,
      xAxis: { type: "category", data: ["A", "B"] },
      yAxis: { type: "value" },
      legend: {},
      series: [
        { id: "pageviews", name: "Page views", type: "line", data: [10, 20] },
      ],
    });
    const pending = chartSnapshot(chart);
    chart.setOption(
      {
        series: [
          {
            id: "events",
            name: "Tracked events",
            type: "line",
            data: [30, 40],
          },
        ],
      },
      { replaceMerge: ["series"] },
    );
    const snapshot = await pending;
    assert.match(snapshot.svg, /Page views/);
    assert.doesNotMatch(snapshot.svg, /Tracked events/);
  } finally {
    chart.dispose();
  }
});
