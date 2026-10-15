import {
  computed,
  onBeforeUnmount,
  ref,
  shallowRef,
  watch,
  type Ref,
} from "vue";

import { analyticsQuery } from "~/utils/analyticsRange";
import type { StatsOverviewResponse } from "~/types/stats";

const DEMO_RANGES = [7, 30, 90] as const;
const PREFETCH_DELAY_MS = 350;

type StatsOverviewOptions = {
  days: Readonly<Ref<number>>;
  dates?: Readonly<Ref<{ start_date: string; end_date: string } | undefined>>;
  prefetchDemoRanges: Readonly<Ref<boolean>>;
  siteId: Readonly<Ref<string>>;
  toErrorMessage: (error: unknown, fallback: string) => string;
};

/**
 * Fetches overview data without letting request, cache, cancellation, and
 * prefetch concerns leak into the dashboard page component.
 */
export function useStatsOverview(options: StatsOverviewOptions) {
  const { api, responseCache: cache } = useApi();
  const stats = shallowRef<StatsOverviewResponse | null>(null);
  const statsError = ref("");
  const statsPending = ref(false);
  const prefetchControllers = new Map<string, AbortController>();

  let foregroundController: AbortController | undefined;
  let foregroundRequest: { key: string; promise: Promise<void> } | undefined;
  let prefetchTimer: ReturnType<typeof setTimeout> | undefined;
  let requestVersion = 0;
  let displayedKey = "";

  const hasLoadedSite = computed(() =>
    Boolean(stats.value?.site_id === options.siteId.value),
  );

  watch(options.siteId, (nextSiteId) => {
    if (stats.value?.site_id !== nextSiteId) stats.value = null;
    statsError.value = "";
  });

  onBeforeUnmount(() => {
    requestVersion += 1;
    foregroundRequest = undefined;
    foregroundController?.abort();
    for (const controller of prefetchControllers.values()) controller.abort();
    if (prefetchTimer) clearTimeout(prefetchTimer);
  });

  function refreshStats(force = false): Promise<void> {
    const key = cacheKey(
      options.siteId.value.trim(),
      options.days.value,
      options.dates?.value,
    );
    // Mount and the site watcher can request the same range together. Join it
    // without aborting/restarting the API's ClickHouse work. Explicit refreshes
    // and changes of site/range still replace the current request.
    if (!force && foregroundRequest?.key === key)
      return foregroundRequest.promise;
    const request = { key, promise: loadStats(force) };
    foregroundRequest = request;
    const clear = () => {
      if (foregroundRequest === request) foregroundRequest = undefined;
    };
    void request.promise.then(clear, clear);
    return request.promise;
  }

  async function loadStats(force: boolean) {
    const requestedSiteId = options.siteId.value.trim();
    if (!requestedSiteId) {
      requestVersion += 1;
      foregroundController?.abort();
      stats.value = null;
      statsError.value = "Enter a site ID before loading analytics.";
      statsPending.value = false;
      return;
    }

    const requestedDays = options.days.value;
    const requestedDates = options.dates?.value;
    const key = cacheKey(requestedSiteId, requestedDays, requestedDates);
    const version = ++requestVersion;
    foregroundController?.abort();
    foregroundController = undefined;

    if (!force) {
      const cached = readCache(key);
      if (cached) {
        stats.value = cached.value;
        displayedKey = key;
        statsError.value = "";
        statsPending.value = false;
        scheduleDemoPrefetch(requestedSiteId, requestedDays);
        if (cached.fresh) return;
      }
    }

    if (stats.value?.site_id !== requestedSiteId || displayedKey !== key)
      stats.value = null;
    statsError.value = "";
    statsPending.value = !stats.value;
    const generation = cache.generation;

    const controller = new AbortController();
    foregroundController = controller;

    try {
      const response = await fetchStats(
        requestedSiteId,
        requestedDays,
        controller.signal,
        requestedDates,
      );

      if (
        generation === cache.generation &&
        isCurrentRequest(version, requestedSiteId, requestedDays) &&
        JSON.stringify(requestedDates) === JSON.stringify(options.dates?.value)
      ) {
        writeCache(key, response, generation);
        stats.value = response;
        displayedKey = key;
        scheduleDemoPrefetch(requestedSiteId, requestedDays);
      }
    } catch (error) {
      if (!isAbortError(error) && version === requestVersion) {
        const code =
          (error as { statusCode?: number; status?: number })?.statusCode ??
          (error as { status?: number })?.status;
        if (code === 401 || code === 403) stats.value = null;
        statsError.value = options.toErrorMessage(
          error,
          "Analytics could not be loaded for this site. Check the site ID and try again.",
        );
      }
    } finally {
      if (foregroundController === controller) foregroundController = undefined;
      if (version === requestVersion) statsPending.value = false;
    }
  }

  function resetStats() {
    requestVersion += 1;
    foregroundRequest = undefined;
    foregroundController?.abort();
    foregroundController = undefined;
    for (const controller of prefetchControllers.values()) controller.abort();
    prefetchControllers.clear();
    if (prefetchTimer) clearTimeout(prefetchTimer);
    prefetchTimer = undefined;
    cache.clear();
    stats.value = null;
    statsError.value = "";
    statsPending.value = false;
  }

  function scheduleDemoPrefetch(requestedSiteId: string, currentDays: number) {
    if (
      typeof window === "undefined" ||
      !options.prefetchDemoRanges.value ||
      options.dates?.value
    ) {
      return;
    }
    if (prefetchTimer) clearTimeout(prefetchTimer);

    prefetchTimer = setTimeout(() => {
      for (const days of DEMO_RANGES) {
        if (days !== currentDays) void prefetchRange(requestedSiteId, days);
      }
    }, PREFETCH_DELAY_MS);
  }

  async function prefetchRange(requestedSiteId: string, days: number) {
    const key = cacheKey(requestedSiteId, days);
    if (readCache(key) || prefetchControllers.has(key)) return;

    const version = requestVersion;
    const generation = cache.generation;
    const controller = new AbortController();
    prefetchControllers.set(key, controller);
    try {
      const response = await fetchStats(
        requestedSiteId,
        days,
        controller.signal,
      );
      if (
        version === requestVersion &&
        requestedSiteId === options.siteId.value
      ) {
        writeCache(key, response, generation);
      }
    } catch (error) {
      if (!isAbortError(error)) {
        // Prefetch is opportunistic; an explicit range change retries normally.
      }
    } finally {
      if (prefetchControllers.get(key) === controller) {
        prefetchControllers.delete(key);
      }
    }
  }

  function fetchStats(
    siteId: string,
    days: number,
    signal: AbortSignal,
    dates?: { start_date: string; end_date: string },
  ) {
    return api<StatsOverviewResponse>("/v1/stats/overview", {
      query: analyticsQuery(siteId, days, dates),
      signal,
    });
  }

  function isCurrentRequest(version: number, siteId: string, days: number) {
    return (
      version === requestVersion &&
      siteId === options.siteId.value &&
      days === options.days.value
    );
  }

  function readCache(key: string) {
    return cache.read<StatsOverviewResponse>(`overview:${key}`);
  }

  function writeCache(
    key: string,
    value: StatsOverviewResponse,
    generation: number,
  ) {
    cache.write(`overview:${key}`, value, generation);
  }

  return {
    hasLoadedSite,
    refreshStats,
    resetStats,
    stats,
    statsError,
    statsPending,
  };
}

function cacheKey(
  siteId: string,
  days: number,
  dates?: { start_date: string; end_date: string },
) {
  return `${siteId}:${days}:${dates?.start_date ?? ""}:${dates?.end_date ?? ""}`;
}

function isAbortError(error: unknown) {
  return (
    typeof error === "object" &&
    error !== null &&
    "name" in error &&
    error.name === "AbortError"
  );
}
