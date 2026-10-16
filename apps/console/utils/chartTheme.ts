export type ChartTheme = {
  surface?: string;
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

/** Shared curve geometry; unknown measurements must remain visible gaps. */
export const chartLineShape = {
  smooth: 0.36,
  smoothMonotone: "x",
  connectNulls: false,
} as const;

export const chartLineStroke = { cap: "round", join: "round" } as const;

/** Round only the value end; zero values keep their true zero height. */
export function chartBarCorners(
  value: number | null | undefined,
): [number, number, number, number] {
  return typeof value !== "number" || !Number.isFinite(value) || value === 0
    ? [0, 0, 0, 0]
    : value < 0
      ? [0, 0, 8, 8]
      : [8, 8, 0, 0];
}

export function chartRingItemStyle(theme: ChartTheme) {
  return {
    borderRadius: 8,
    borderWidth: 3,
    borderColor: theme.surface ?? "#111214",
  };
}

export function chartLegend(theme: ChartTheme) {
  return {
    type: "scroll" as const,
    bottom: 0,
    icon: "circle",
    itemHeight: 9,
    itemWidth: 9,
    itemGap: 16,
    pageIconColor: theme.text,
    pageIconInactiveColor: theme.axis,
    pageTextStyle: { color: theme.text },
    textStyle: { color: theme.text, fontFamily: "Geist Variable, sans-serif" },
  };
}

export function chartPlotGrid(withZoom = false) {
  return {
    top: 24,
    left: 8,
    right: 14,
    bottom: withZoom ? 112 : 48,
    outerBoundsMode: "same" as const,
    outerBoundsContain: "axisLabel" as const,
  };
}

export function readCssVariable(
  host: HTMLElement,
  name: string,
  fallback: string,
) {
  return getComputedStyle(host).getPropertyValue(name).trim() || fallback;
}

export function readChartTheme(host: HTMLElement): ChartTheme {
  return {
    surface: readCssVariable(host, "--chart-surface-bg", "#111214"),
    axis: readCssVariable(host, "--chart-axis", "#8f887d"),
    grid: readCssVariable(host, "--chart-grid", "#d8d2c4"),
    series: [
      readCssVariable(host, "--chart-1", "#d8ff52"),
      readCssVariable(host, "--chart-2", "#a391ff"),
      readCssVariable(host, "--chart-3", "#ff705c"),
      readCssVariable(host, "--chart-4", "#8295ff"),
      readCssVariable(host, "--chart-5", "#66d9cc"),
      readCssVariable(host, "--chart-6", "#ffc56b"),
      readCssVariable(host, "--chart-7", "#c2b8ff"),
      readCssVariable(host, "--chart-8", "#ff9b8b"),
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
      events: readCssVariable(
        host,
        "--traffic-events",
        readCssVariable(host, "--owleye-coral", "#ff705c"),
      ),
      pageviews: readCssVariable(
        host,
        "--traffic-pageviews",
        readCssVariable(host, "--owleye-lime", "#d8ff52"),
      ),
      visitors: readCssVariable(
        host,
        "--traffic-visitors",
        readCssVariable(host, "--owleye-violet", "#a391ff"),
      ),
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
      backgroundColor: theme.surface ?? "#111214",
      borderColor: theme.grid,
      borderRadius: 8,
      bottom: 31,
      brushSelect: false,
      dataBackground: {
        areaStyle: { color: theme.series[0], opacity: 0.09 },
        lineStyle: {
          ...chartLineStroke,
          color: theme.series[1],
          opacity: 0.42,
          width: 1.2,
        },
      },
      fillerColor: "rgba(163, 145, 255, 0.12)",
      handleIcon:
        "path://M8.2,0.5A2.7,2.7 0 0,0 5.5,3.2V20.8A2.7,2.7 0 0,0 8.2,23.5H15.8A2.7,2.7 0 0,0 18.5,20.8V3.2A2.7,2.7 0 0,0 15.8,0.5ZM9.5,7.2H11V16.8H9.5ZM13,7.2H14.5V16.8H13Z",
      handleSize: 22,
      handleStyle: {
        borderColor: theme.surface ?? "#111214",
        borderWidth: 1,
        color: theme.text,
      },
      height: 44,
      moveHandleSize: 0,
      selectedDataBackground: {
        areaStyle: { color: theme.series[0], opacity: 0.14 },
        lineStyle: {
          ...chartLineStroke,
          color: theme.series[1],
          opacity: 0.74,
          width: 1.4,
        },
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
