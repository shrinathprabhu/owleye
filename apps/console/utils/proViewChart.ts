import type { EChartsCoreOption } from "echarts/core";
import type { ProViewPreviewResponse } from "../types/pro-view.ts";
import {
  chartBarCorners,
  chartLegend,
  chartPlotGrid,
  chartRingItemStyle,
  chartLineShape,
  chartLineStroke,
  chartZoom,
  type ChartTheme,
} from "./chartTheme.ts";

type PlotDefinition = {
  visualization: string;
  breakdown?: string;
  metric?: string;
};

/** Presentation only: report values, ordering, and unknown groups stay intact. */
export function proViewPlotOption(
  definition: PlotDefinition,
  preview: ProViewPreviewResponse,
  theme: ChartTheme,
): EChartsCoreOption {
  const legend = chartLegend(theme);

  if (
    ["pie", "donut"].includes(definition.visualization) &&
    !["visitors", "sessions"].includes(definition.metric ?? "events")
  ) {
    const hasPrevious = Boolean(preview.previous?.points?.length);
    const isDonut = definition.visualization === "donut";
    const names = Array.from(
      new Set([
        ...(preview.current?.points?.map((point) => point.period) ?? []),
        ...(preview.previous?.points?.map((point) => point.period) ?? []),
      ]),
    );
    // Comparison rings may arrive in a different rank order. A category must
    // retain its color, rather than taking the color of its array position.
    const categoryColor = new Map(
      names.map((name, index) => [
        name,
        theme.series[index % theme.series.length],
      ]),
    );
    const makeRing = (
      period: ProViewPreviewResponse["current"],
      previous: boolean,
    ) => ({
      avoidLabelOverlap: true,
      center: ["50%", "43%"],
      data: (period?.points ?? []).map((point) => ({
        name: point.period,
        value: point.value,
        itemStyle: { color: categoryColor.get(point.period) },
      })),
      emphasis: { scaleSize: 6 },
      itemStyle: {
        ...chartRingItemStyle(theme),
        opacity: previous ? 0.54 : 1,
      },
      label: {
        alignTo: "edge",
        color: theme.text,
        edgeDistance: 8,
        formatter: "{b}",
        overflow: "truncate",
        show: !previous,
        width: 96,
      },
      labelLine: { length: 12, length2: 8, show: !previous, smooth: 0.2 },
      name: period?.label ?? "Current",
      radius: previous
        ? isDonut
          ? ["24%", "44%"]
          : [0, "44%"]
        : hasPrevious
          ? ["53%", "73%"]
          : isDonut
            ? ["50%", "73%"]
            : [0, "73%"],
      stillShowZeroSum: false,
      type: "pie",
    });
    return {
      legend,
      series: [
        makeRing(preview.current, false),
        ...(hasPrevious ? [makeRing(preview.previous, true)] : []),
      ],
    };
  }

  const periods = Array.from(
    new Set([
      ...(preview.current?.points?.map((point) => point.period) ?? []),
      ...(preview.previous?.points?.map((point) => point.period) ?? []),
    ]),
  );
  const seriesType = ["pie", "donut"].includes(definition.visualization)
    ? "bar"
    : definition.visualization;
  const makeSeries = (
    period: ProViewPreviewResponse["current"],
    color: string,
  ) => {
    if (!period?.points) return null;
    const byPeriod = new Map(
      period.points.map((point) => [point.period, point.value] as const),
    );
    const values = periods.map((label) => byPeriod.get(label) ?? null);
    if (seriesType === "scatter") {
      return {
        data: values.map((value, index) => [index, value]),
        itemStyle: { color },
        name: period.label,
        symbolSize: 11,
        type: "scatter",
      };
    }
    if (seriesType === "bar") {
      return {
        barMaxWidth: 44,
        barMinHeight: 0,
        data: values.map((value) =>
          value === null
            ? null
            : {
                value,
                itemStyle: {
                  borderRadius: value === 0 ? 0 : chartBarCorners(value),
                },
              },
        ),
        itemStyle: { color },
        name: period.label,
        type: "bar",
      };
    }
    return {
      ...chartLineShape,
      data: values,
      itemStyle: { color },
      lineStyle: {
        ...chartLineStroke,
        color,
        shadowBlur: 12,
        shadowColor: color,
        width: 3,
      },
      name: period.label,
      showSymbol: periods.length === 1,
      type: "line",
    };
  };
  const hasZoom = !(
    definition.breakdown &&
    definition.breakdown !== "time" &&
    periods.length <= 8
  );
  return {
    dataZoom: hasZoom ? chartZoom(periods.length, theme) : [],
    grid: chartPlotGrid(hasZoom),
    legend,
    series: [
      makeSeries(preview.current, theme.series[0] ?? "#d8ff52"),
      makeSeries(preview.previous, theme.series[1] ?? "#a391ff"),
    ].filter(Boolean),
    xAxis: {
      axisLabel: { color: theme.text, hideOverlap: true },
      axisLine: { lineStyle: { color: theme.axis } },
      axisTick: { show: false },
      data: periods,
      type: "category",
    },
    yAxis: {
      axisLabel: { color: theme.text },
      axisLine: { show: false },
      axisTick: { show: false },
      minInterval: definition.metric === "p75" ? 0 : 1,
      splitLine: { lineStyle: { color: theme.grid } },
      type: "value",
    },
  };
}
