import { ResponseCache } from "../utils/responseCache.ts";
import {
  ReportWarmupQueue,
  reportRequestKey,
  reportWarmups,
  waitForReport,
  type ReportWarmup,
} from "../utils/reportWarmup.ts";
import {
  sitePermissions,
  type WorkspaceSession,
  type WorkspaceSite,
} from "../types/workspace.ts";

const caches = new WeakMap<object, ResponseCache>();
const warmups = new WeakMap<
  object,
  {
    queue: ReportWarmupQueue;
    reads: Map<string, Promise<unknown>>;
  }
>();

import type { NitroFetchOptions } from "nitropack/types";
// This API is external to Nitro; do not infer its URLs from Nuxt server routes.
type ExternalApi = <T = unknown>(
  url: string,
  options?: NitroFetchOptions<string>,
) => Promise<T>;
// Do not compare Nuxt's generated route-dependent $fetch overloads against an
// external service's generic URLs. Keep the public options Nitro-compatible.
type ExternalFetchFactory = (options: {
  baseURL: string;
  credentials: RequestCredentials;
  retry?: number;
  timeout?: number;
  onRequest?: (event: { options: { method?: string } }) => void;
  onResponse?: (event: { options: { method?: string } }) => void;
  onResponseError?: (event: { response: { status: number } }) => void;
}) => ExternalApi;
export const API_UNAUTHORIZED_EVENT = "owleye:api-unauthorized";

export function useApi() {
  const app = useNuxtApp();
  let responseCache = caches.get(app);
  if (!responseCache) {
    responseCache = new ResponseCache();
    caches.set(app, responseCache);
  }
  const cache = responseCache;
  let warming = warmups.get(app);
  if (!warming) {
    warming = { queue: new ReportWarmupQueue(), reads: new Map() };
    const { queue, reads } = warming;
    cache.onInvalidate(() => {
      queue.clear();
      reads.clear();
    });
    warmups.set(app, warming);
  }
  const { queue, reads } = warming;
  const session = useState<WorkspaceSession | null>(
    "console-workspace-session",
    () => null,
  );
  const sites = useState<WorkspaceSite[]>("console-workspace-sites", () => []);
  const selectedSite = useState<string>(
    "console-workspace-selected-site",
    () => "",
  );
  // Access snapshots are a cache boundary, never a replacement for server authorization.
  watch(
    () =>
      JSON.stringify([
        session.value?.authenticated ? session.value.user.id : null,
        selectedSite.value,
        sites.value.map((site) => [
          site.id,
          site.permissions,
          site.role,
          site.entitlements,
        ]),
      ]),
    (scope) => cache.setScope(scope),
    { immediate: true, flush: "sync" },
  );
  const config = useRuntimeConfig();
  const createExternalApi = $fetch.create as unknown as ExternalFetchFactory;
  const apiBase = computed(() =>
    (import.meta.client
      ? window.location.origin
      : String(config.public.apiBase)
    ).replace(/\/$/, ""),
  );
  const responseError = (status: number) => {
    if (status === 401 || status === 403) cache.clear();
    if (status === 401 && import.meta.client)
      window.dispatchEvent(new Event(API_UNAUTHORIZED_EVENT));
  };
  const foregroundApi = createExternalApi({
    baseURL: apiBase.value,
    credentials: "include",
    onRequest({ options }) {
      if (options.method && options.method.toUpperCase() !== "GET")
        cache.clear();
    },
    onResponse({ options }) {
      // A concurrent read during a write must not survive its completion.
      if (options.method && options.method.toUpperCase() !== "GET")
        cache.clear();
    },
    onResponseError({ response }) {
      responseError(response.status);
    },
  });
  const backgroundApi = createExternalApi({
    baseURL: apiBase.value,
    credentials: "include",
    retry: 0,
    timeout: 15_000,
    onResponseError({ response }) {
      responseError(response.status);
    },
  });
  const api: ExternalApi = <T>(
    url: string,
    options?: NitroFetchOptions<string>,
  ) => {
    queue.beginForeground();
    if (!options?.method || options.method.toUpperCase() === "GET") {
      const pending = reads.get(reportRequestKey(url, options?.query));
      if (pending) {
        return waitForReport(
          pending as Promise<T>,
          options?.signal ?? undefined,
        ).finally(() => queue.endForeground());
      }
    }
    return foregroundApi<T>(url, options).finally(() => queue.endForeground());
  };
  const publicApi = createExternalApi({
    baseURL: apiBase.value,
    credentials: "omit",
  });

  function warmReport(report: ReportWarmup) {
    const site = sites.value.find(
      (item) =>
        item.id === selectedSite.value ||
        item.tracking_id === selectedSite.value,
    );
    if (
      !session.value?.authenticated ||
      !site ||
      !sitePermissions(site).analytics_read
    )
      return;
    const allowed = reportWarmups(site.tracking_id, "").some(
      (candidate) =>
        candidate.key === report.key &&
        reportRequestKey(candidate.url, candidate.query) ===
          reportRequestKey(report.url, report.query),
    );
    if (!allowed) return;
    queue.enqueue({
      key: report.key,
      run: async (signal) => {
        if (signal.aborted || cache.read(report.key)?.fresh) return;
        const generation = cache.generation;
        const requestKey = reportRequestKey(report.url, report.query);
        const pending = backgroundApi(report.url, {
          query: report.query,
          signal,
        });
        reads.set(requestKey, pending);
        try {
          const value = await pending;
          if (!signal.aborted) cache.write(report.key, value, generation);
        } finally {
          if (reads.get(requestKey) === pending) reads.delete(requestKey);
        }
      },
    });
  }
  return {
    api,
    apiBase,
    publicApi,
    responseCache: cache,
    warmReport,
    reportWarmupQueue: queue,
  };
}
