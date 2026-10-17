import assert from "node:assert/strict";
import test from "node:test";
import { init, use, type EChartsCoreOption } from "echarts/core";
import { LineChart } from "echarts/charts";
import {
  DataZoomComponent,
  GridComponent,
  LegendComponent,
  MarkPointComponent,
} from "echarts/components";
import { SVGRenderer } from "echarts/renderers";
import { toggleChartMetric } from "../utils/chartSelection.ts";
import { chartSnapshot } from "../utils/chartSnapshot.ts";
import {
  chartWidgetSvg,
  isSnapshotPngDataUrl,
  type SnapshotMetric,
} from "../utils/metricWidget.ts";
import {
  chartLineShape,
  chartLineStroke,
  chartPlotGrid,
  chartZoom,
} from "../utils/chartTheme.ts";

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
  MarkPointComponent,
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

const tinyPng =
  "data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mP8/x8AAwMCAO+tmfQAAAAASUVORK5CYII=";
function widgetWithSvg(svg: string) {
  return chartWidgetSvg({
    chart: { svg, width: 400, height: 300 },
    metrics: [series[0]],
    period: "UTC",
    site: "Example",
  });
}
function imageChart(symbol: string) {
  const chart = init(null, undefined, {
    renderer: "svg",
    ssr: true,
    width: 400,
    height: 300,
  });
  chart.setOption({
    animation: false,
    xAxis: { type: "category", data: ["A", "B"] },
    yAxis: {},
    series: [
      {
        type: "line",
        data: [10, 20],
        markPoint: {
          symbol: `image://${symbol}`,
          symbolSize: 28,
          data: [{ coord: ["A", 10] }, { coord: ["B", 20] }],
        },
      },
    ],
  });
  return chart;
}

test("static widget accepts only bounded inline PNG icons, rejecting remote sources, hostile attributes and disguised image data", () => {
  assert.equal(isSnapshotPngDataUrl(tinyPng), true);
  for (const attribute of ["href", "xlink:href"]) {
    const widget = widgetWithSvg(
      `<svg><image ${attribute}="${tinyPng}" width="2" height="2" x="-1" y="-1" transform="matrix(14,0,0,14,100,50)"/></svg>`,
    );
    assert.ok(widget.svg.includes(tinyPng));
    assert.doesNotMatch(widget.svg, /https?:\/\/(?!www\.w3\.org\/)/);
  }
  const oversized = Buffer.from(tinyPng.split(",")[1]!, "base64");
  oversized.writeUInt32BE(4096, 16);
  const invalidPng = `data:image/png;base64,${oversized.toString("base64")}`;
  for (const href of [
    "https://attacker.example/track.png",
    "//attacker.example/track.png",
    "/referrer-icons/google.ico",
    "data:image/svg+xml;base64,PHN2ZyBvbmxvYWQ9ImFsZXJ0KDEpIj48L3N2Zz4=",
    "data:image/png;base64,PHN2Zy8+",
    invalidPng,
    tinyPng + "A".repeat(17000),
    tinyPng.replace("base64,", "base64,\n"),
  ])
    assert.throws(() => widgetWithSvg(`<svg><image href="${href}"/></svg>`));
  for (const attributes of [
    `href="${tinyPng}" onload="alert(1)"`,
    `href="${tinyPng}" href="https://attacker.example"`,
    `href="${tinyPng}" xlink:href="${tinyPng}"`,
    `href="${tinyPng}" style="background:url(https://attacker.example)"`,
    `href="${tinyPng}" width="9999"`,
    `href="${tinyPng}" transform="scale(9999)"`,
    `href="${tinyPng}" xmlns="http://www.w3.org/1999/xhtml"`,
  ])
    assert.throws(() => widgetWithSvg(`<svg><image ${attributes}/></svg>`));
  for (const content of [
    '<use href="#unapproved"/>',
    '<g src="https://attacker.example"/>',
    '<style>@import "https://attacker.example";</style>',
    '<path fill="url(https://attacker.example)"/>',
    '<animate attributeName="href" to="https://attacker.example"/>',
    "<svg:script>alert(1)</svg:script>",
    '<!DOCTYPE svg [<!ENTITY x SYSTEM "https://attacker.example">]>',
  ])
    assert.throws(() => widgetWithSvg(`<svg>${content}</svg>`));
  assert.throws(() =>
    widgetWithSvg(`<svg>${`<image href="${tinyPng}"/>`.repeat(33)}</svg>`),
  );
});

test("ECharts source markpoint PNGs survive the captured chart and all widget formats without remote URLs", async () => {
  const chart = imageChart(tinyPng);
  try {
    const snapshot = await chartSnapshot(chart);
    assert.equal((snapshot.svg.match(/<image\b/g) ?? []).length, 2);
    assert.ok(snapshot.svg.includes(tinyPng));
    const widget = widgetWithSvg(snapshot.svg);
    assert.ok(widget.svg.includes(tinyPng));
    assert.doesNotMatch(widget.svg, /https?:\/\/(?!www\.w3\.org\/)/);
  } finally {
    chart.dispose();
  }
});

test("export rasterizes each allowlisted same-origin icon once at 32px and never requests arbitrary icon paths", async () => {
  const previousImage = Object.getOwnPropertyDescriptor(globalThis, "Image");
  const previousDocument = Object.getOwnPropertyDescriptor(
    globalThis,
    "document",
  );
  const requested: string[] = [],
    canvases: { width: number; height: number }[] = [],
    draws: number[][] = [];
  class StubImage {
    naturalWidth = 64;
    naturalHeight = 128;
    onload: (() => void) | null = null;
    onerror: (() => void) | null = null;
    referrerPolicy = "";
    set src(value: string) {
      requested.push(value);
      queueMicrotask(() => this.onload?.());
    }
  }
  Object.defineProperty(globalThis, "Image", {
    configurable: true,
    value: StubImage,
  });
  Object.defineProperty(globalThis, "document", {
    configurable: true,
    value: {
      location: { origin: "https://console.example" },
      createElement(tag: string) {
        assert.equal(tag, "canvas");
        const canvas = {
          width: 0,
          height: 0,
          getContext: () => ({
            drawImage(_image: unknown, ...coordinates: number[]) {
              draws.push(coordinates);
            },
          }),
          toDataURL: () => tinyPng,
        };
        canvases.push(canvas);
        return canvas;
      },
    },
  });
  try {
    const chart = imageChart("/referrer-icons/google.ico");
    try {
      const snapshot = await chartSnapshot(chart);
      assert.deepEqual(requested, [
        "https://console.example/referrer-icons/google.ico",
      ]);
      assert.equal(
        canvases.length,
        1,
        "two marks reuse one icon rasterization",
      );
      assert.deepEqual([canvases[0]!.width, canvases[0]!.height], [32, 32]);
      assert.deepEqual(
        draws,
        [[8, 0, 16, 32]],
        "rasterization keeps the aspect ratio",
      );
      assert.ok(widgetWithSvg(snapshot.svg).svg.includes(tinyPng));
      assert.doesNotMatch(snapshot.svg, /\/referrer-icons\//);
    } finally {
      chart.dispose();
    }
    for (const path of [
      "https://attacker.example/pixel",
      "//attacker.example/pixel",
      "/referrer-icons/unknown.ico",
      "/referrer-icons/../pixel",
      "/referrer-icons/google.ico?pixel=1",
    ]) {
      const invalid = imageChart(path);
      try {
        await assert.rejects(
          chartSnapshot(invalid),
          /Only bundled source icons/,
        );
      } finally {
        invalid.dispose();
      }
    }
    assert.equal(
      requested.length,
      1,
      "rejected sources never trigger an image request",
    );
  } finally {
    if (previousImage)
      Object.defineProperty(globalThis, "Image", previousImage);
    else Reflect.deleteProperty(globalThis, "Image");
    if (previousDocument)
      Object.defineProperty(globalThis, "document", previousDocument);
    else Reflect.deleteProperty(globalThis, "document");
  }
});

test("an unavailable bundled icon falls back to a local vector badge without breaking export", async () => {
  const chart = imageChart("/referrer-icons/google.ico");
  try {
    const snapshot = await chartSnapshot(chart);
    assert.doesNotMatch(snapshot.svg, /<image|href=/);
    assert.match(snapshot.svg, /rx="\.45" fill="#a391ff"/);
    assert.doesNotThrow(() => widgetWithSvg(snapshot.svg));
  } finally {
    chart.dispose();
  }
});

test("the actual traffic glow, layered source badges, dashed partial series and navigator export together", async () => {
  const theme = {
    axis: "#8f887d",
    grid: "#353638",
    text: "#c4c5c0",
    tooltipBg: "#111214",
    tooltipText: "#f7f7f2",
    surface: "#111214",
    series: series.map((item) => item.color),
  };
  const dates = [
    "2026-10-13",
    "2026-10-14",
    "2026-10-15",
    "2026-10-16",
    "2026-10-17",
  ];
  const chart = init(null, undefined, {
    renderer: "svg",
    ssr: true,
    width: 960,
    height: 400,
  });
  try {
    chart.setOption({
      animation: false,
      grid: chartPlotGrid(true),
      dataZoom: chartZoom(dates.length, theme),
      xAxis: { type: "category", boundaryGap: false, data: dates },
      yAxis: { type: "value" },
      series: series.flatMap((item) => {
        const base = {
          ...chartLineShape,
          emphasis: { focus: "series" },
          showSymbol: false,
          symbol: "circle",
          symbolSize: 8,
          type: "line",
          itemStyle: { color: item.color },
          lineStyle: {
            ...chartLineStroke,
            color: item.color,
            shadowBlur: item.key === "pageviews" ? 14 : 10,
            shadowColor: item.color,
            width: item.key === "pageviews" ? 3.25 : 2.75,
          },
          name: item.label,
        };
        return [
          {
            ...base,
            id: `traffic-${item.key}`,
            data: item.data.map((value, index) =>
              index === dates.length - 1 ? null : value,
            ),
            ...(item.key === "pageviews"
              ? {
                  markPoint: {
                    symbolSize: 24,
                    symbolOffset: [0, -20],
                    label: { show: false },
                    data: [
                      {
                        name: dates[2],
                        coord: [dates[2], item.data[2]],
                        symbol: "circle",
                        symbolSize: 30,
                        itemStyle: {
                          color: "#fffef9",
                          borderColor: item.color,
                          borderWidth: 2,
                        },
                      },
                      {
                        name: dates[2],
                        coord: [dates[2], item.data[2]],
                        symbol: `image://${tinyPng}`,
                        symbolSize: 20,
                      },
                    ],
                  },
                }
              : {}),
          },
          {
            ...base,
            id: `traffic-${item.key}-partial`,
            lineStyle: { ...base.lineStyle, type: "dashed", shadowBlur: 0 },
            data: item.data.map((value, index) =>
              index >= dates.length - 2 ? value : null,
            ),
          },
        ];
      }),
    } as EChartsCoreOption);
    const snapshot = await chartSnapshot(chart);
    assert.match(snapshot.svg, /<filter\b/);
    assert.match(snapshot.svg, /<feDropShadow[^>]*stdDeviation="7 7"/);
    assert.match(snapshot.svg, /<feDropShadow[^>]*stdDeviation="5 5"/);
    assert.match(snapshot.svg, /stroke-dasharray=/);
    assert.match(snapshot.svg, /fill="#fffef9"/);
    assert.ok(snapshot.svg.includes(tinyPng));
    assert.doesNotMatch(snapshot.svg, /<style\b/);
    assert.doesNotThrow(() =>
      chartWidgetSvg({
        chart: snapshot,
        metrics: series,
        period: "Last 5 days UTC · current period partial",
        site: "Example",
      }),
    );
    assert.deepEqual(
      (chart.getOption().series as { data: unknown[] }[])[1]!.data,
      [null, null, null, 40, 20],
    );
  } finally {
    chart.dispose();
  }
});

test("traffic shadow support rejects image filters, remote paint and unbounded blur or filter regions", () => {
  const safe =
    '<filter id="glow" x="-100%" y="-100%" width="300%" height="300%"><feDropShadow dx="0" dy="0" stdDeviation="7 7" flood-color="#d8ff52" flood-opacity="1"/></filter>';
  assert.doesNotThrow(() =>
    widgetWithSvg(
      `<svg><defs>${safe}</defs><path filter="url(#glow)" d="M0 0L10 10"/></svg>`,
    ),
  );
  for (const content of [
    safe.replace('stdDeviation="7 7"', 'stdDeviation="70000 70000"'),
    safe.replace('width="300%"', 'width="999999%"'),
    safe.replace('flood-opacity="1"', 'flood-opacity="-1"'),
    safe.replace(
      'flood-color="#d8ff52"',
      'flood-color="url(https://attacker.example)"',
    ),
    safe.replace('dx="0"', 'href="https://attacker.example" dx="0"'),
    '<filter id="glow"><feImage href="https://attacker.example"/></filter>',
  ])
    assert.throws(() => widgetWithSvg(`<svg><defs>${content}</defs></svg>`));
});
