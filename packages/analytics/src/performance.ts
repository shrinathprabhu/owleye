import { safely, guarded, cleanup, noop } from "./internal/safety";
import type {
  OwlConfig,
  OwlPerfRecords,
  PerfController,
  PerfEnd,
  WebVitalsController,
} from "./types.js";
import {
  instances,
  instanceKey,
  rememberConfig,
  checkConfig,
} from "./internal/instances";
import { normalizeEventName } from "./internal/bounds";
import { createEvent, mergePerfRecords } from "./internal/events";
import { createEmitter, type Emitter } from "./internal/transport";
import { getPage } from "./internal/environment";
import { collectWebVitals } from "./internal/web-vitals";

export type {
  OwlConfig,
  OwlPerfRecords,
  OwlRecord,
  OwlRecordValue,
  PerfController,
  PerfEnd,
  WebVitalsController,
} from "./types.js";

/**
 * Create an explicit performance tracker without starting page analytics.
 * Each `start()` call returns an idempotent function that ends that one span.
 */
export function trackPerf(siteId: string, config?: OwlConfig): PerfController {
  return safely<PerfController>(
    "performance initialization",
    () => {
      return new PerformanceTracker(siteId, config);
    },
    { start: () => noop, observeVitals: () => ({ stop: noop }) },
  );
}

/** Collect field Web Vitals without enabling page-view analytics. */
export function trackWebVitals(
  siteId: string,
  config?: OwlConfig,
): WebVitalsController {
  return safely(
    "Web Vitals initialization",
    () => {
      const key = instanceKey(siteId, config);
      const controllers = instances<WebVitalsController>("vitals");
      const existing = controllers.get(key);
      if (existing) {
        checkConfig(existing, config);
        return existing;
      }
      const observation = collectWebVitals(createEmitter(siteId, config));
      const controller = {
        stop() {
          cleanup(
            () => observation.stop(),
            () => {
              if (controllers.get(key) === controller) controllers.delete(key);
            },
          );
        },
      };
      if (typeof window !== "undefined")
        controllers.set(key, rememberConfig(controller, config));
      return controller;
    },
    { stop: noop },
  );
}

class PerformanceTracker implements PerfController {
  private readonly emitter: Emitter;

  constructor(siteId: string, config?: OwlConfig) {
    this.emitter = createEmitter(siteId, config);
  }

  start = (perfId: string, ...records: OwlPerfRecords): PerfEnd => {
    return safely(
      "performance start",
      () => {
        const normalizedPerfId = normalizeEventName(perfId);
        const startedAt = now();
        const page = getPage(this.emitter.config);
        const startData = mergePerfRecords(records);
        let ended = false;

        return guarded(
          "performance completion",
          (...endRecords: OwlPerfRecords) => {
            if (ended) return;
            ended = true;

            this.emitter.emit(
              createEvent(
                "performance",
                normalizedPerfId,
                {
                  duration_ms: Math.max(0, Math.round(now() - startedAt)),
                  end: mergePerfRecords(endRecords),
                  start: startData,
                },
                {
                  page,
                  pageCapture: this.emitter.config,
                },
              ),
            );
          },
        );
      },
      noop,
    );
  };

  observeVitals = (): WebVitalsController => {
    return trackWebVitals(this.emitter.siteId, this.emitter.config);
  };
}

function now(): number {
  return typeof performance !== "undefined" ? performance.now() : Date.now();
}
