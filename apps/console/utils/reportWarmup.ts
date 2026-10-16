import { routes } from "./routes.ts";

type WarmupTask = {
  key: string;
  run: (signal: AbortSignal) => Promise<unknown>;
};

/** Opportunistic work starts only after visible reads settle. No polling/retry loop. */
export class ReportWarmupQueue {
  private queued = new Map<string, WarmupTask>();
  private active = new Map<string, AbortController>();
  private attempted = new Set<string>();
  private foreground = 0;
  private timer: ReturnType<typeof setTimeout> | undefined;
  private generation = 0;
  private delay: number;
  constructor(delay = 800) {
    this.delay = delay;
  }

  enqueue(task: WarmupTask) {
    if (
      this.queued.has(task.key) ||
      this.active.has(task.key) ||
      this.attempted.has(task.key) ||
      this.queued.size >= 4
    )
      return;
    this.queued.set(task.key, task);
    this.schedule();
  }
  beginForeground() {
    this.foreground++;
    if (this.timer) clearTimeout(this.timer);
    this.timer = undefined;
  }
  endForeground() {
    this.foreground = Math.max(0, this.foreground - 1);
    this.schedule();
  }
  clear() {
    this.generation++;
    this.queued.clear();
    this.attempted.clear();
    if (this.timer) clearTimeout(this.timer);
    this.timer = undefined;
    // Keep the slots occupied until aborted requests settle; abort is not proof
    // that the remote server stopped executing its query.
    for (const controller of this.active.values()) controller.abort();
  }
  private schedule() {
    if (
      this.timer ||
      this.foreground ||
      this.active.size >= 2 ||
      !this.queued.size
    )
      return;
    this.timer = setTimeout(() => {
      this.timer = undefined;
      this.drain();
    }, this.delay);
  }
  private drain() {
    if (this.foreground) return;
    while (this.active.size < 2 && this.queued.size) {
      const [key, task] = this.queued.entries().next().value!;
      this.queued.delete(key);
      const controller = new AbortController();
      const generation = this.generation;
      this.active.set(key, controller);
      this.attempted.add(key);
      // Catch sync failures as well as network rejection; visible routes retry
      // normally and never inherit an invisible warmup error.
      void Promise.resolve()
        .then(() => task.run(controller.signal))
        .catch(() => {})
        .finally(() => {
          this.active.delete(key);
          if (generation !== this.generation) this.attempted.delete(key);
          this.schedule();
        });
    }
  }
}

export type ReportWarmup = {
  key: string;
  url: string;
  query: Record<string, string | number>;
};

/** Exact default read reports only. No settings, secrets, AI, live or mutations. */
export function reportWarmups(
  siteId: string,
  visiblePath: string,
): ReportWarmup[] {
  if (!siteId) return [];
  const reports: Array<ReportWarmup & { page: string }> = [
    {
      page: "/",
      key: `overview:${siteId}:30::`,
      url: "/v1/stats/overview",
      query: { site_id: siteId, days: 30, group_by: "day" },
    },
    {
      page: "/events",
      key: `events:${siteId}:${JSON.stringify({ days: 30, group_by: "event_name", order: "desc", sort_by: "count" })}`,
      url: routes.sites.events(siteId),
      query: {
        days: 30,
        group_by: "event_name",
        order: "desc",
        sort_by: "count",
      },
    },
    {
      page: "/campaigns",
      key: `campaigns:${siteId}:30`,
      url: routes.sites.campaigns(siteId),
      query: { days: 30 },
    },
    {
      page: "/performance",
      key: `performance:${siteId}:30:LCP`,
      url: routes.sites.performance(siteId),
      query: { days: 30, metric: "LCP" },
    },
  ];
  return reports
    .filter((report) => report.page !== visiblePath)
    .map(({ page: _, ...report }) => report);
}

export function reportRequestKey(url: string, query?: unknown) {
  const sorted = Object.entries((query ?? {}) as Record<string, unknown>)
    .filter(([, value]) => value !== undefined)
    .sort(([a], [b]) => a.localeCompare(b));
  return `${url}:${JSON.stringify(sorted)}`;
}

/** Stop one consumer without aborting another consumer's shared network read. */
export function waitForReport<T>(
  promise: Promise<T>,
  signal?: AbortSignal,
): Promise<T> {
  if (!signal) return promise;
  if (signal.aborted)
    return Promise.reject(new DOMException("Request aborted", "AbortError"));
  return new Promise((resolve, reject) => {
    const abort = () => {
      cleanup();
      reject(new DOMException("Request aborted", "AbortError"));
    };
    const cleanup = () => signal.removeEventListener("abort", abort);
    signal.addEventListener("abort", abort, { once: true });
    void promise.then(
      (value) => {
        cleanup();
        resolve(value);
      },
      (error) => {
        cleanup();
        reject(error);
      },
    );
  });
}
