import { ResponseCache } from "../utils/responseCache.ts";
import type { WorkspaceSession, WorkspaceSite } from "~/types/workspace";

const caches = new WeakMap<object, ResponseCache>();

export const API_UNAUTHORIZED_EVENT = "owleye:api-unauthorized";

export function useApi() {
  const app = useNuxtApp();
  let responseCache = caches.get(app);
  if (!responseCache) {
    responseCache = new ResponseCache();
    caches.set(app, responseCache);
  }
  const cache = responseCache;
  const session = useState<WorkspaceSession | null>(
    "console-workspace-session",
    () => null,
  );
  const sites = useState<WorkspaceSite[]>("console-workspace-sites", () => []);
  // Access snapshots are a cache boundary, never a replacement for server authorization.
  watch(
    () =>
      JSON.stringify([
        session.value?.authenticated ? session.value.user.id : null,
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
  const apiBase = computed(() =>
    (import.meta.client
      ? window.location.origin
      : String(config.public.apiBase)
    ).replace(/\/$/, ""),
  );
  const api = $fetch.create({
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
      if (response.status === 401 || response.status === 403) cache.clear();
      if (response.status === 401 && import.meta.client) {
        window.dispatchEvent(new Event(API_UNAUTHORIZED_EVENT));
      }
    },
  });
  const publicApi = $fetch.create({
    baseURL: apiBase.value,
    credentials: "omit",
  });

  return { api, apiBase, publicApi, responseCache: cache };
}
