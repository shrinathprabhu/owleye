export type ChartTheme = {
  axis: string;
  grid: string;
  series: string[];
  text: string;
  tooltipBg: string;
  tooltipText: string;
};

export type TrafficChartTheme = ChartTheme & {
  traffic: {
    events: string;
    pageviews: string;
    visitors: string;
  };
};

export function readCssVariable(
  host: HTMLElement,
  name: string,
  fallback: string,
) {
  return getComputedStyle(host).getPropertyValue(name).trim() || fallback;
}

export function readChartTheme(host: HTMLElement): ChartTheme {
  return {
    axis: readCssVariable(host, "--chart-axis", "#8f887d"),
    grid: readCssVariable(host, "--chart-grid", "#d8d2c4"),
    series: [
      readCssVariable(host, "--chart-1", "#3d5afe"),
      readCssVariable(host, "--chart-2", "#007d84"),
      readCssVariable(host, "--chart-3", "#c23e35"),
      readCssVariable(host, "--chart-4", "#6048c8"),
      readCssVariable(host, "--chart-5", "#668500"),
      readCssVariable(host, "--coral", "#ff705c"),
      readCssVariable(host, "--violet", "#a391ff"),
    ],
    text: readCssVariable(host, "--chart-text", "#6a6861"),
    tooltipBg: readCssVariable(host, "--chart-tooltip-bg", "#111214"),
    tooltipText: readCssVariable(host, "--chart-tooltip-text", "#f7f7f2"),
  };
}

/**
 * The traffic chart is the dashboard's visual anchor, so its metric colors
 * mirror the landing-page demo instead of relying on generic series order.
 * Dedicated variables remain optional for downstream themes.
 */
export function readTrafficChartTheme(host: HTMLElement): TrafficChartTheme {
  const theme = readChartTheme(host);

  return {
    ...theme,
    traffic: {
      events: readCssVariable(host, "--traffic-events", "#ff705c"),
      pageviews: readCssVariable(host, "--traffic-pageviews", "#d6ff3f"),
      visitors: readCssVariable(host, "--traffic-visitors", "#7188ff"),
    },
  };
}

export function chartMotion(reducedMotion: boolean, updateDuration: number) {
  return {
    animation: !reducedMotion,
    animationDuration: reducedMotion ? 0 : 420,
    animationDurationUpdate: reducedMotion ? 0 : updateDuration,
    animationEasing: "cubicOut" as const,
    animationEasingUpdate: "cubicInOut" as const,
  };
}

/** Shared preview navigator for time-series charts. */
export function chartZoom(pointCount: number, theme: ChartTheme) {
  const start = 0;

  return [
    {
      filterMode: "filter" as const,
      minSpan: Math.min(100, Math.max(4, 200 / Math.max(pointCount, 2))),
      start,
      end: 100,
      rangeMode: ["percent", "percent"] as ["percent", "percent"],
      type: "inside" as const,
    },
    {
      backgroundColor: "rgba(255, 254, 249, 0.58)",
      borderColor: theme.grid,
      bottom: 31,
      brushSelect: false,
      dataBackground: {
        areaStyle: { color: "rgba(61, 90, 254, 0.09)" },
        lineStyle: { color: "rgba(61, 90, 254, 0.42)", width: 1.2 },
      },
      fillerColor: "rgba(61, 90, 254, 0.1)",
      handleIcon:
        "path://M8.2,0.5A2.7,2.7 0 0,0 5.5,3.2V20.8A2.7,2.7 0 0,0 8.2,23.5H15.8A2.7,2.7 0 0,0 18.5,20.8V3.2A2.7,2.7 0 0,0 15.8,0.5ZM9.5,7.2H11V16.8H9.5ZM13,7.2H14.5V16.8H13Z",
      handleSize: 22,
      handleStyle: {
        borderColor: theme.tooltipBg,
        borderWidth: 1,
        color: theme.tooltipBg,
      },
      height: 44,
      moveHandleSize: 0,
      selectedDataBackground: {
        areaStyle: { color: "rgba(61, 90, 254, 0.14)" },
        lineStyle: { color: "rgba(61, 90, 254, 0.74)", width: 1.4 },
      },
      showDetail: false,
      showDataShadow: true,
      start,
      end: 100,
      rangeMode: ["percent", "percent"] as ["percent", "percent"],
      textStyle: { color: theme.text },
      filterMode: "filter" as const,
      type: "slider" as const,
    },
  ];
}

export function stableSlug(value: string) {
  return value
    .toLocaleLowerCase()
    .replace(/[^a-z0-9]+/g, "-")
    .replace(/(^-|-$)/g, "");
}
