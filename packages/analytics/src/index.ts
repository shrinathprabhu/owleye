import { safely, guarded, cleanup, noop } from "./internal/safety";
import type {
  AnalyticsController,
  OwlConfig,
  OwlPage,
  OwlRecord,
  OwlTrackRecords,
} from "./types.js";
import { createEvent, mergeTrackRecords } from "./internal/events";
import { instances, instanceKey, firstPageStart } from "./internal/instances";
import { getPage } from "./internal/environment";
import { onNavigation } from "./internal/navigation";
import { createEmitter, type Emitter } from "./internal/transport";

export type {
  AnalyticsController,
  OwlConfig,
  OwlEnvironment,
  OwlEvent,
  OwlEventEnvelope,
  OwlEventType,
  OwlPage,
  OwlPerfRecords,
  OwlRecord,
  OwlRecordValue,
  OwlTrackRecords,
} from "./types.js";

/**
 * Create automatic page analytics and return its small lifecycle controller.
 *
 * Tracking starts immediately unless `autoStart` is false. Calling `stop()`
 * removes every listener installed by this controller; `start()` can safely
 * begin or resume it after a consent or lawful-basis decision.
 */
export function useAnalytics(
  siteId: string,
  config?: OwlConfig,
): AnalyticsController {
  return safely(
    "analytics initialization",
    () => {
      const key = instanceKey(siteId, config);
      const controllers = instances<AnalyticsController>("analytics");
      const existing = controllers.get(key);
      if (existing) {
        if (config?.autoStart !== false) existing.start();
        return existing;
      }

      let controller: OwlEyeAnalytics;
      controller = new OwlEyeAnalytics(siteId, config, key, () => {
        if (controllers.get(key) === controller) controllers.delete(key);
      });
      controllers.set(key, controller);
      return controller.startAndReturn();
    },
    {
      start: noop,
      stop: noop,
      track: noop,
      pageview: noop,
      setGlobalRecords: noop,
    },
  );
}

class OwlEyeAnalytics implements AnalyticsController {
  private active = false;
  private activePage?: ActivePage;
  private readonly emitter: Emitter;
  private globalRecords?: OwlRecord;
  private removeNavigationListener?: () => void;

  constructor(
    siteId: string,
    config: OwlConfig | undefined,
    private readonly key: string,
    private readonly release: () => void,
  ) {
    this.emitter = createEmitter(siteId, config);
  }

  startAndReturn(): AnalyticsController {
    if (this.emitter.config.autoStart) this.start();
    return this;
  }

  start = (): void => {
    return safely("analytics start", () => {
      if (
        this.active ||
        !this.emitter.enabled ||
        typeof window === "undefined" ||
        typeof document === "undefined"
      ) {
        return;
      }

      const key = this.key;
      const controllers = instances<AnalyticsController>("analytics");
      const existing = controllers.get(key);
      if (existing && existing !== this) return;
      controllers.set(key, this);
      this.active = true;
      const started = safely(
        "analytics startup",
        () => {
          this.beginPage(firstPageStart(this.key));

          this.removeNavigationListener = onNavigation((change) => {
            if (!this.active) return;
            this.closePage();
            firstPageStart(this.key);
            this.beginPage(true, change.from);
          });

          window.addEventListener("pagehide", this.handlePageExit);
          window.addEventListener("pageshow", this.handlePageShow);
          document.addEventListener(
            "visibilitychange",
            this.handleVisibilityChange,
          );
          return true;
        },
        false,
      );
      if (!started) this.stop();
    });
  };

  stop = (): void => {
    this.active = false;
    this.activePage = undefined;
    const removeNavigation = this.removeNavigationListener;
    this.removeNavigationListener = undefined;
    cleanup(
      this.release,
      () => removeNavigation?.(),
      () => window.removeEventListener("pagehide", this.handlePageExit),
      () => window.removeEventListener("pageshow", this.handlePageShow),
      () =>
        document.removeEventListener(
          "visibilitychange",
          this.handleVisibilityChange,
        ),
    );
  };

  track = (eventName: string, ...records: OwlTrackRecords): void => {
    return safely("custom event", () => {
      if (!this.active) return;

      const mergedRecords = mergeTrackRecords(records);
      const eventRecords = this.globalRecords
        ? mergeTrackRecords([{ ...this.globalRecords, ...mergedRecords }])
        : mergedRecords;

      this.emitter.emit(
        createEvent(
          "external",
          eventName,
          eventRecords
            ? {
                records: eventRecords,
              }
            : undefined,
          { pageCapture: this.emitter.config },
        ),
      );
    });
  };

  pageview = (page: Partial<OwlPage> = {}): void => {
    return safely("pageview", () => {
      if (!this.active) return;
      this.closePage();
      this.beginPage(true, undefined, {
        ...getPage(this.emitter.config),
        ...page,
      });
    });
  };

  setGlobalRecords = (records?: OwlRecord): void => {
    return safely("global records", () => {
      this.globalRecords = records ? mergeTrackRecords([records]) : undefined;
    });
  };

  private beginPage(
    trackPageView: boolean,
    referrerOverride?: string,
    pageOverride?: OwlPage,
  ): void {
    const page = pageOverride ?? getPage(this.emitter.config, referrerOverride);
    const hidden = document.visibilityState === "hidden";
    this.activePage = {
      closed: hidden,
      page,
      startedAt: now(),
    };

    if (trackPageView) {
      this.emitter.emit(
        createEvent("pageview", "page_view", undefined, {
          page,
          pageCapture: this.emitter.config,
        }),
      );
    }
  }

  private closePage(): void {
    const activePage = this.activePage;
    if (!activePage || activePage.closed) return;
    activePage.closed = true;

    this.emitter.emit(
      createEvent(
        "page_session",
        "page_session",
        {
          duration_ms: Math.max(0, Math.round(now() - activePage.startedAt)),
        },
        {
          page: activePage.page,
          pageCapture: this.emitter.config,
        },
      ),
    );
  }

  private readonly handlePageExit = guarded("handlePageExit", (): void => {
    if (!this.active) return;
    this.closePage();
  });

  private readonly handlePageShow = guarded("handlePageShow", (): void => {
    if (!this.active || !this.activePage?.closed) return;
    this.beginPage(false);
  });

  private readonly handleVisibilityChange = guarded(
    "handleVisibilityChange",
    (): void => {
      if (!this.active) return;

      if (document.visibilityState === "hidden") {
        this.closePage();
      } else if (this.activePage?.closed) {
        this.beginPage(false);
      }
    },
  );
}

interface ActivePage {
  closed: boolean;
  page: ReturnType<typeof getPage>;
  startedAt: number;
}

function now(): number {
  return typeof performance !== "undefined" ? performance.now() : Date.now();
}
