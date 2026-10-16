import type { ECharts, EChartsCoreOption } from "echarts/core";

export type ChartSnapshot = { svg: string; width: number; height: number };

/** Render the live chart option only on export; the on-screen renderer stays canvas. */
export async function chartSnapshot(
  chart: Pick<ECharts, "getOption" | "getWidth" | "getHeight">,
): Promise<ChartSnapshot> {
  // Capture state before the lazy renderer loads; later navigation/selection cannot mix snapshots.
  const width = Math.round(chart.getWidth());
  const height = Math.round(chart.getHeight());
  if (width < 1 || height < 1 || width > 8192 || height > 8192)
    throw new Error("Chart dimensions unavailable.");
  const option = chart.getOption();
  const [{ init, use }, { SVGRenderer }] = await Promise.all([
    import("echarts/core"),
    import("echarts/renderers"),
  ]);
  use([SVGRenderer]);
  const mirror = init(null, undefined, {
    renderer: "svg",
    ssr: true,
    width,
    height,
  });
  try {
    // Formatter functions are kept in memory, never serialized into the exported code.
    mirror.setOption({ ...option, animation: false } as EChartsCoreOption, {
      notMerge: true,
    });
    return {
      svg: mirror.renderToSVGString({ useViewBox: true }),
      width,
      height,
    };
  } finally {
    mirror.dispose();
  }
}
