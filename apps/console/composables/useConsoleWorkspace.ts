import type { ConsoleSelectOption } from "~/types/console";
import {
  PREVIEW_USER_ID,
  consoleRole,
  sitePermissions,
  type WorkspaceHealth,
  type WorkspaceSession,
  type WorkspaceSite,
} from "../types/workspace.ts";
import {
  apiErrorMessage as requestError,
  apiErrorStatus,
} from "../utils/apiError.ts";
import { API_UNAUTHORIZED_EVENT } from "./useApi.ts";
import { AUTH_SESSION_INVALIDATED_EVENT } from "../utils/authSchedule.ts";
import { routes } from "../utils/routes.ts";

type WorkspaceState = ReturnType<typeof useWorkspaceState>;
const WORKSPACE_FRESH_MS = 30_000;
const IDENTITY_CHANGE_KEY = "owleye:identity-change";
// Transient requests belong to this Nuxt app, never to a process-wide user.
const requests = new WeakMap<object, Promise<void>>();

export function useConsoleWorkspace() {
  const app = useNuxtApp();
  const route = useRoute();
  const router = useRouter();
  const state = useWorkspaceState();
  const { api, apiBase, publicApi } = useApi();
  const signOutPending = ref(false);
  // Keep this tab’s validated workspace while routes revalidate in the background.
  const lastSelection = useState<{ userId: string; siteId: string } | null>(
    "console-last-site-selection",
    () => null,
  );

  let ownsWorkspace = false;
  onMounted(() => {
    window.addEventListener("focus", refreshWorkspace);
    window.addEventListener(API_UNAUTHORIZED_EVENT, recoverSession);
    window.addEventListener(
      AUTH_SESSION_INVALIDATED_EVENT,
      clearRejectedSession,
    );
    window.addEventListener("storage", handleIdentityChange);
  });
  onBeforeUnmount(() => {
    ownsWorkspace = false;
    window.removeEventListener("focus", refreshWorkspace);
    window.removeEventListener(API_UNAUTHORIZED_EVENT, recoverSession);
    window.removeEventListener(
      AUTH_SESSION_INVALIDATED_EVENT,
      clearRejectedSession,
    );
    window.removeEventListener("storage", handleIdentityChange);
  });

  function refreshWorkspace() {
    if (ownsWorkspace && state.session.value?.authenticated) void initialize();
  }

  function clearRejectedSession() {
    if (!ownsWorkspace) return;
    requests.delete(app);
    invalidateWorkspace(state);
    lastSelection.value = null;
    state.session.value = { authenticated: false };
  }

  function recoverSession() {
    if (!ownsWorkspace || !state.session.value?.authenticated) return;
    const wasValidating = requests.has(app);
    clearRejectedSession();
    // A rejected catalog request during validation must fail closed, not start
    // a recursive validation loop against the same rejected credentials.
    if (!wasValidating) void initialize();
  }

  function handleIdentityChange(event: StorageEvent) {
    if (event.key === IDENTITY_CHANGE_KEY && ownsWorkspace) {
      clearRejectedSession();
      void initialize();
    }
  }

  function publishIdentityChange() {
    try {
      localStorage.setItem(
        IDENTITY_CHANGE_KEY,
        `${Date.now()}:${Math.random()}`,
      );
    } catch {
      // Storage is optional; focus and API responses still revalidate access.
    }
  }

  const isAuthenticated = computed(
    () => state.session.value?.authenticated === true,
  );
  const accessExpiresAt = computed(() =>
    state.session.value?.authenticated
      ? state.session.value.access_expires_at
      : null,
  );
  useAuthSessionRefresh(apiBase, isAuthenticated, accessExpiresAt);
  const user = computed(() =>
    state.session.value?.authenticated ? state.session.value.user : null,
  );
  const selectedSite = computed(
    () =>
      state.sites.value.find(
        (site) =>
          site.id === state.selectedSiteId.value ||
          site.tracking_id === state.selectedSiteId.value,
      ) ?? null,
  );
  const currentRole = computed(() => consoleRole(selectedSite.value));
  const permissions = computed(() => sitePermissions(selectedSite.value));
  const isDemoUser = computed(() => user.value?.id === PREVIEW_USER_ID);
  const siteOptions = computed<ConsoleSelectOption[]>(() =>
    state.sites.value.map((site) => {
      const role = consoleRole(site);
      return {
        badge:
          role === "viewer"
            ? "Read only"
            : role === "owner"
              ? "Owner"
              : "Admin",
        description: site.domain,
        label: site.name,
        meta: `Public tracking ID · ${site.tracking_id}`,
        value: site.tracking_id,
      };
    }),
  );

  watch(
    () => route.query.site,
    () => {
      if (state.sites.value.length) syncSelectedSite();
    },
  );

  /** Reuse only this tab’s workspace; sign-in and logout invalidate explicitly. */
  function prepareForNavigation() {
    if (state.session.value?.authenticated) syncSelectedSite();
  }

  async function initialize({ force = true }: { force?: boolean } = {}) {
    ownsWorkspace = true;
    if (force || state.validatedAt.value <= Date.now() - WORKSPACE_FRESH_MS) {
      let pending = requests.get(app);
      if (!pending) {
        pending = validateWorkspace();
        requests.set(app, pending);
        void pending.finally(() => {
          if (requests.get(app) === pending) requests.delete(app);
        });
      }
      await pending;
    }
    // A request may outlive its original route. Only the mounted owner changes
    // the URL; the next route can share its network request and result.
    if (!ownsWorkspace) return;
    const session = state.session.value;
    if (
      session?.authenticated &&
      session.onboarding.next_step !== "complete" &&
      route.path !== "/" &&
      route.meta.siteScoped !== false
    ) {
      await navigateTo("/");
      return;
    }
    if (session?.authenticated) syncSelectedSite();
  }

  async function validateWorkspace() {
    const validationId = ++state.validationId.value;
    const previousUserId = user.value?.id;
    const hadWorkspace = Boolean(previousUserId && state.validatedAt.value);
    state.loading.value = !hadWorkspace;
    state.error.value = "";
    try {
      // Health is optional and must not hold up authenticated navigation.
      void publicApi<WorkspaceHealth>(routes.health)
        .then((health) => {
          state.health.value = health;
        })
        .catch(() => {});
      const sessionResponse = await fetchSessionWithRefresh(apiBase.value);
      if (!isCurrentValidation(state, validationId)) return;
      if (!sessionResponse.authenticated) {
        clearIdentityState(state);
        state.session.value = sessionResponse;
        lastSelection.value = null;
        state.validatedAt.value = Date.now();
        return;
      }
      if (previousUserId && previousUserId !== sessionResponse.user.id) {
        clearIdentityState(state);
        lastSelection.value = null;
        state.loading.value = true;
      }
      state.session.value = sessionResponse;
      const siteResponse = await api<WorkspaceSite[]>(routes.sites.list);
      if (!isCurrentValidation(state, validationId)) return;
      // Unchanged site objects keep child watchers from clearing/reloading their
      // own data on an otherwise invisible workspace refresh.
      state.sites.value = siteResponse.map((site) => {
        const previous = state.sites.value.find(
          (entry) => entry.id === site.id,
        );
        return previous && JSON.stringify(previous) === JSON.stringify(site)
          ? previous
          : site;
      });
      state.selectedSiteId.value = selectedTrackingId(
        state.sites.value,
        state.selectedSiteId.value ||
          routeSite(route.query.site) ||
          (lastSelection.value?.userId === sessionResponse.user.id
            ? lastSelection.value.siteId
            : ""),
      );
      rememberSelection();
      state.validatedAt.value = Date.now();
    } catch (cause) {
      if (!isCurrentValidation(state, validationId)) return;
      // Keep already displayed data through transport errors. A rejected
      // session is cleared immediately by the auth-event handlers above.
      if (apiErrorStatus(cause) === 401 || apiErrorStatus(cause) === 403) {
        clearIdentityState(state);
        lastSelection.value = null;
        state.session.value = { authenticated: false };
      } else if (!state.session.value?.authenticated) clearIdentityState(state);
      state.error.value = requestError(
        cause,
        "The workspace could not be refreshed. Please try again.",
      );
    } finally {
      if (isCurrentValidation(state, validationId)) state.loading.value = false;
    }
  }

  function resetIdentity() {
    requests.delete(app);
    invalidateWorkspace(state, { loading: true });
    lastSelection.value = null;
    publishIdentityChange();
  }

  function syncSelectedSite() {
    if (route.meta.siteScoped === false) return;
    const querySite = routeSite(route.query.site);
    state.selectedSiteId.value = selectedTrackingId(
      state.sites.value,
      querySite || state.selectedSiteId.value,
    );
    rememberSelection();
    void syncSiteQuery();
  }

  async function syncSiteQuery() {
    if (route.meta.siteScoped === false) return;
    const querySite = routeSite(route.query.site);
    if (querySite === state.selectedSiteId.value) return;
    await router.replace({
      query: {
        ...route.query,
        site: state.selectedSiteId.value || undefined,
      },
    });
  }

  function rememberSelection() {
    if (user.value)
      lastSelection.value = {
        userId: user.value.id,
        siteId: state.selectedSiteId.value,
      };
  }

  async function selectSite(value: number | string) {
    state.selectedSiteId.value = String(value);
    rememberSelection();
    await syncSiteQuery();
  }

  async function signOut() {
    signOutPending.value = true;
    state.error.value = "";
    try {
      await api(routes.auth.logout, {
        method: "POST",
      });
      requests.delete(app);
      lastSelection.value = null;
      invalidateWorkspace(state);
      publishIdentityChange();
      state.session.value = { authenticated: false };
      await navigateTo("/");
    } catch (cause) {
      state.error.value = requestError(cause, "We could not sign you out.");
    } finally {
      signOutPending.value = false;
    }
  }

  return {
    apiBase,
    currentRole,
    error: state.error,
    health: state.health,
    initialize,
    isAuthenticated,
    isDemoUser,
    loading: state.loading,
    permissions,
    prepareForNavigation,
    resetIdentity,
    selectedSite,
    selectedSiteId: state.selectedSiteId,
    selectSite,
    session: state.session,
    signOut,
    signOutPending,
    siteOptions,
    sites: state.sites,
    user,
  };
}

function useWorkspaceState() {
  const session = useState<WorkspaceSession | null>(
    "console-workspace-session",
    () => null,
  );
  const health = useState<WorkspaceHealth | null>(
    "console-workspace-health",
    () => null,
  );
  const sites = useState<WorkspaceSite[]>("console-workspace-sites", () => []);
  const selectedSiteId = useState<string>(
    "console-workspace-selected-site",
    () => "",
  );
  const loading = useState<boolean>("console-workspace-loading", () => true);
  const error = useState<string>("console-workspace-error", () => "");
  const validatedAt = useState<number>(
    "console-workspace-validated-at",
    () => 0,
  );
  const validationId = useState<number>(
    "console-workspace-validation-id",
    () => 0,
  );

  return {
    error,
    health,
    loading,
    selectedSiteId,
    session,
    sites,
    validationId,
    validatedAt,
  };
}

function isCurrentValidation(state: WorkspaceState, validationId: number) {
  return state.validationId.value === validationId;
}

function invalidateWorkspace(
  state: WorkspaceState,
  { loading = false }: { loading?: boolean } = {},
) {
  state.validationId.value += 1;
  state.loading.value = loading;
  state.error.value = "";
  clearIdentityState(state);
}

function clearIdentityState(state: WorkspaceState) {
  state.validatedAt.value = 0;
  state.session.value = null;
  state.sites.value = [];
  state.selectedSiteId.value = "";
}

function routeSite(value: unknown) {
  return typeof value === "string" ? value.trim() : "";
}

function selectedTrackingId(sites: WorkspaceSite[], preferred = "") {
  const selected = sites.find(
    (site) => site.id === preferred || site.tracking_id === preferred,
  );
  return selected?.tracking_id ?? sites[0]?.tracking_id ?? "";
}
