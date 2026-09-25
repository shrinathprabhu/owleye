import { init, type ECharts } from "echarts/core";
import { onBeforeUnmount, onMounted, type Ref } from "vue";

type EChartLifecycleOptions = {
  observeTheme?: boolean;
};

/**
 * Owns the imperative browser resources shared by every ECharts component.
 * Chart option construction stays in each focused component; allocation,
 * resize scheduling, reduced motion, and cleanup stay here.
 */
export function useEChartLifecycle(
  host: Ref<HTMLElement | null>,
  render: () => void,
  options: EChartLifecycleOptions = {},
) {
  let chart: ECharts | undefined;
  let chartHost: HTMLElement | undefined;
  let resizeObserver: ResizeObserver | undefined;
  let themeObserver: MutationObserver | undefined;
  let reduceMotionQuery: MediaQueryList | undefined;
  let resizeFrame: number | undefined;
  let usesWindowResizeFallback = false;

  function scheduleResize() {
    if (resizeFrame !== undefined) cancelAnimationFrame(resizeFrame);
    resizeFrame = requestAnimationFrame(() => {
      resizeFrame = undefined;
      chart?.resize();
    });
  }

  function ensureChart() {
    if (!host.value) return undefined;
    if (chart && chartHost === host.value) return chart;

    disposeChart();
    chartHost = host.value;
    chart = init(chartHost, null, { renderer: "canvas" });

    if (typeof ResizeObserver === "undefined") {
      if (!usesWindowResizeFallback) {
        window.addEventListener("resize", scheduleResize, { passive: true });
        usesWindowResizeFallback = true;
      }
    } else {
      resizeObserver = new ResizeObserver(scheduleResize);
      resizeObserver.observe(chartHost);
    }

    return chart;
  }

  function clearChart() {
    // Vue removes a v-if chart host before its watcher runs. Release observers
    // and the detached canvas immediately instead of retaining it until the
    // next successful render or route unmount.
    if (!host.value || host.value !== chartHost) disposeChart();
    else chart?.clear();
  }

  function disposeChart() {
    if (resizeFrame !== undefined) cancelAnimationFrame(resizeFrame);
    resizeFrame = undefined;
    resizeObserver?.disconnect();
    resizeObserver = undefined;
    if (usesWindowResizeFallback) {
      window.removeEventListener("resize", scheduleResize);
      usesWindowResizeFallback = false;
    }
    chart?.dispose();
    chart = undefined;
    chartHost = undefined;
  }

  function prefersReducedMotion() {
    return reduceMotionQuery?.matches ?? false;
  }

  onMounted(() => {
    reduceMotionQuery = window.matchMedia("(prefers-reduced-motion: reduce)");
    reduceMotionQuery.addEventListener("change", render);

    if (options.observeTheme) {
      themeObserver = new MutationObserver(render);
      themeObserver.observe(document.documentElement, {
        attributeFilter: ["class", "data-theme", "style"],
        attributes: true,
      });
    }
  });

  onBeforeUnmount(() => {
    reduceMotionQuery?.removeEventListener("change", render);
    themeObserver?.disconnect();
    disposeChart();
  });

  return {
    clearChart,
    ensureChart,
    prefersReducedMotion,
  };
}
