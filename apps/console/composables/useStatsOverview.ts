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
const CACHE_LIMIT = 24;
const CACHE_TTL_MS = 60_000;
const PREFETCH_DELAY_MS = 350;

type CacheEntry = {
  expiresAt: number;
  value: StatsOverviewResponse;
};

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
  const { api } = useApi();
  const stats = shallowRef<StatsOverviewResponse | null>(null);
  const statsError = ref("");
  const statsPending = ref(false);
  const cache = new Map<string, CacheEntry>();
  const prefetchControllers = new Map<string, AbortController>();

  let foregroundController: AbortController | undefined;
  let prefetchTimer: ReturnType<typeof setTimeout> | undefined;
  let requestVersion = 0;

  const hasLoadedSite = computed(() =>
    Boolean(stats.value?.site_id === options.siteId.value),
  );

  watch(options.siteId, (nextSiteId) => {
    if (stats.value?.site_id !== nextSiteId) stats.value = null;
    statsError.value = "";
  });

  onBeforeUnmount(() => {
    foregroundController?.abort();
    for (const controller of prefetchControllers.values()) controller.abort();
    if (prefetchTimer) clearTimeout(prefetchTimer);
  });

  async function refreshStats(force = false) {
    const requestedSiteId = options.siteId.value.trim();
    if (!requestedSiteId) {
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
        stats.value = cached;
        statsError.value = "";
        statsPending.value = false;
        scheduleDemoPrefetch(requestedSiteId, requestedDays);
        return;
      }
    }

    if (stats.value?.site_id !== requestedSiteId) stats.value = null;
    statsError.value = "";
    statsPending.value = true;

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
        isCurrentRequest(version, requestedSiteId, requestedDays) &&
        JSON.stringify(requestedDates) === JSON.stringify(options.dates?.value)
      ) {
        writeCache(key, response);
        stats.value = response;
        scheduleDemoPrefetch(requestedSiteId, requestedDays);
      }
    } catch (error) {
      if (!isAbortError(error) && version === requestVersion) {
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
        writeCache(key, response);
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
    const entry = cache.get(key);
    if (!entry) return undefined;
    if (entry.expiresAt <= Date.now()) {
      cache.delete(key);
      return undefined;
    }

    // Touch the entry so Map insertion order acts as a tiny LRU.
    cache.delete(key);
    cache.set(key, entry);
    return entry.value;
  }

  function writeCache(key: string, value: StatsOverviewResponse) {
    cache.delete(key);
    cache.set(key, { expiresAt: Date.now() + CACHE_TTL_MS, value });
    while (cache.size > CACHE_LIMIT) {
      const oldestKey = cache.keys().next().value;
      if (!oldestKey) break;
      cache.delete(oldestKey);
    }
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
