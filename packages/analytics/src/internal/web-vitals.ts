import { safely, guarded, cleanup, reportFailure } from "./safety";
import type { WebVitalsController } from "../types";
import { createEvent } from "./events";
import type { Emitter } from "./transport";

type MetricName = "CLS" | "FCP" | "INP" | "LCP" | "TTFB";
type VitalEntry = PerformanceEntry & {
  duration?: number;
  hadRecentInput?: boolean;
  interactionId?: number;
  value?: number;
};

/**
 * Lightweight PerformanceObserver estimates, not the reference Web Vitals
 * algorithms (for example CLS session windows and INP percentile selection).
 * Mark every event approximate so downstream reporting can preserve that scope.
 */
export function collectWebVitals(emitter: Emitter): WebVitalsController {
  if (
    !emitter.enabled ||
    typeof window === "undefined" ||
    typeof document === "undefined"
  ) {
    return { stop() {} };
  }

  const observers: PerformanceObserver[] = [];
  const sent = new Set<MetricName>();
  let cls = 0;
  let inp = 0;
  let lcp = 0;
  let stopped = false;

  const report = (metric: MetricName, value: number) => {
    if (stopped || sent.has(metric) || !Number.isFinite(value) || value < 0)
      return;
    sent.add(metric);
    const rounded =
      metric === "CLS"
        ? Math.round(value * 10_000) / 10_000
        : Math.round(value);
    emitter.emit(
      createEvent(
        "performance",
        `web_vital_${metric.toLowerCase()}`,
        {
          ...(metric === "CLS" ? {} : { duration_ms: rounded }),
          metric,
          approximate: true,
          rating: rating(metric, rounded),
          value: rounded,
        },
        { pageCapture: emitter.config },
      ),
    );
  };

  const observe = (type: string, callback: (entries: VitalEntry[]) => void) => {
    if (typeof PerformanceObserver === "undefined") return;
    let observer: PerformanceObserver | undefined;
    try {
      observer = new PerformanceObserver(
        guarded(
          "performance observation",
          (list: PerformanceObserverEntryList) =>
            callback(list.getEntries() as VitalEntry[]),
        ),
      );
      observer.observe({ buffered: true, type } as PerformanceObserverInit);
      observers.push(observer);
    } catch {
      safely("performance observer cleanup", () => observer?.disconnect());
      reportFailure("performance observer setup");
    }
  };

  observe("largest-contentful-paint", (entries) => {
    lcp = entries[entries.length - 1]?.startTime ?? lcp;
  });
  observe("layout-shift", (entries) => {
    for (const entry of entries) {
      if (!entry.hadRecentInput) cls += entry.value ?? 0;
    }
  });
  observe("event", (entries) => {
    for (const entry of entries) {
      if ((entry.interactionId ?? 0) > 0)
        inp = Math.max(inp, entry.duration ?? 0);
    }
  });
  observe("paint", (entries) => {
    const fcp = entries.find(
      (entry) => entry.name === "first-contentful-paint",
    );
    if (fcp) report("FCP", fcp.startTime);
  });

  safely("navigation timing", () => {
    const navigation = performance.getEntriesByType("navigation")[0] as
      PerformanceNavigationTiming | undefined;
    if (navigation?.responseStart !== undefined)
      report("TTFB", Math.max(0, navigation.responseStart));
  });

  const flush = guarded("performance flush", () => {
    if (lcp > 0) report("LCP", lcp);
    if (inp > 0) report("INP", inp);
    report("CLS", cls);
  });
  const handleVisibility = guarded("performance visibility", () => {
    if (document.visibilityState === "hidden") flush();
  });
  safely("performance page listener", () =>
    window.addEventListener("pagehide", flush, { capture: true }),
  );
  safely("performance visibility listener", () =>
    document.addEventListener("visibilitychange", handleVisibility, {
      capture: true,
    }),
  );

  return {
    stop() {
      if (stopped) return;
      stopped = true;
      cleanup(
        ...observers.map((observer) => () => observer.disconnect()),
        () => window.removeEventListener("pagehide", flush, { capture: true }),
        () =>
          document.removeEventListener("visibilitychange", handleVisibility, {
            capture: true,
          }),
      );
    },
  };
}

function rating(metric: MetricName, value: number) {
  const thresholds: Record<MetricName, [number, number]> = {
    CLS: [0.1, 0.25],
    FCP: [1_800, 3_000],
    INP: [200, 500],
    LCP: [2_500, 4_000],
    TTFB: [800, 1_800],
  };
  const [good, poor] = thresholds[metric];
  if (value <= good) return "good";
  if (value <= poor) return "needs_improvement";
  return "poor";
}
