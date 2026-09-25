import type { WorkspaceSession } from "~/types/workspace";
import { apiErrorStatus } from "~/utils/apiError";
import {
  AUTH_SESSION_INVALIDATED_EVENT,
  refreshAtForExpiry,
} from "~/utils/authSchedule";

const SCHEDULER_TICK_MS = 60 * 1000;
const FALLBACK_RETRY_MS = 60 * 1000;
const REFRESH_COMPLETED_KEY = "owleye:auth-refresh-completed";
const REFRESH_REQUEST_TIMEOUT_MS = 10 * 1000;

interface StoredRefreshCoordination {
  accessExpiresAt?: number;
  completedAt?: number;
}

let consumers = 0;
let intervalId: number | null = null;
let visibilityHandler: (() => void) | null = null;
let nextRefreshAt = 0;
let knownAccessExpiry = 0;
let activeApiBase = "";
let refreshInFlight: Promise<boolean> | null = null;

export async function fetchSessionWithRefresh(
  apiBase: string,
): Promise<WorkspaceSession> {
  if (import.meta.server) {
    throw new Error("Browser session refresh cannot run during SSR.");
  }
  const session = await fetchSession(apiBase);
  if (session.authenticated) return session;

  const refreshed = await refreshCredentials(apiBase);
  return refreshed ? fetchSession(apiBase) : session;
}

export function useAuthSessionRefresh(
  apiBase: Readonly<Ref<string>>,
  authenticated: Readonly<Ref<boolean>>,
  accessExpiresAt: Readonly<Ref<string | null>>,
) {
  let registered = false;

  onMounted(() => {
    if (registered) return;
    registered = true;
    consumers += 1;
    activeApiBase = apiBase.value;
    if (authenticated.value) scheduleFromExpiry(accessExpiresAt.value);
    startScheduler();
  });

  watch(apiBase, (value) => {
    activeApiBase = value;
  });
  watch(authenticated, (value) => {
    if (!value) {
      nextRefreshAt = 0;
      knownAccessExpiry = 0;
    } else scheduleFromExpiry(accessExpiresAt.value);
  });
  watch(accessExpiresAt, (value) => {
    if (authenticated.value) scheduleFromExpiry(value);
  });

  onBeforeUnmount(() => {
    if (!registered) return;
    registered = false;
    consumers = Math.max(0, consumers - 1);
    if (consumers === 0) stopScheduler();
  });
}

async function fetchSession(apiBase: string): Promise<WorkspaceSession> {
  return $fetch<WorkspaceSession>("/v1/auth/session", {
    baseURL: apiBase,
    credentials: "include",
  });
}

async function refreshCredentials(apiBase: string): Promise<boolean> {
  if (refreshInFlight) return refreshInFlight;
  refreshInFlight = withCrossTabRefreshLock(async () => {
    try {
      const response = await $fetch<{ access_expires_at?: string }>(
        "/v1/auth/refresh",
        {
          baseURL: apiBase,
          credentials: "include",
          method: "POST",
          timeout: REFRESH_REQUEST_TIMEOUT_MS,
        },
      );
      const expiry = response.access_expires_at
        ? Date.parse(response.access_expires_at)
        : Number.NaN;
      knownAccessExpiry = Number.isFinite(expiry) ? expiry : 0;
      nextRefreshAt = knownAccessExpiry
        ? refreshAtForExpiry(knownAccessExpiry)
        : Date.now() + FALLBACK_RETRY_MS;
      return true;
    } catch (error) {
      const status = apiErrorStatus(error);
      if (status === 401 || status === 403) {
        window.dispatchEvent(new Event(AUTH_SESSION_INVALIDATED_EVENT));
      }
      // Invalid credentials cannot recover by retrying. Transport failures can,
      // so keep the scheduler alive without hammering the API.
      nextRefreshAt =
        status === 401 || status === 403 ? 0 : Date.now() + FALLBACK_RETRY_MS;
      return false;
    }
  }).finally(() => {
    refreshInFlight = null;
  });
  return refreshInFlight;
}

async function withCrossTabRefreshLock(
  refresh: () => Promise<boolean>,
): Promise<boolean> {
  const attemptStartedAt = Date.now();
  if (typeof navigator !== "undefined" && "locks" in navigator) {
    return navigator.locks.request("owleye-auth-refresh", async () => {
      if (adoptRecentRefresh(attemptStartedAt)) return true;
      const refreshed = await refresh();
      if (refreshed) publishRefreshCompletion();
      return refreshed;
    });
  }
  // Refresh credentials rotate, so an approximate localStorage lease is not
  // safe. Browsers without Web Locks must let the next session bootstrap retry.
  return false;
}

function startScheduler() {
  if (intervalId || typeof window === "undefined") return;
  intervalId = window.setInterval(() => void refreshIfDue(), SCHEDULER_TICK_MS);
  visibilityHandler = () => {
    if (document.visibilityState === "visible") void refreshIfDue();
  };
  document.addEventListener("visibilitychange", visibilityHandler);
}

function stopScheduler() {
  if (intervalId) window.clearInterval(intervalId);
  intervalId = null;
  if (visibilityHandler) {
    document.removeEventListener("visibilitychange", visibilityHandler);
  }
  visibilityHandler = null;
  activeApiBase = "";
}

async function refreshIfDue() {
  if (
    !activeApiBase ||
    !nextRefreshAt ||
    Date.now() < nextRefreshAt ||
    document.visibilityState !== "visible"
  ) {
    return;
  }
  await refreshCredentials(activeApiBase);
}

function scheduleFromExpiry(expiresAt: string | null) {
  const expiry = expiresAt ? Date.parse(expiresAt) : Number.NaN;
  if (Number.isFinite(expiry)) {
    knownAccessExpiry = Math.max(knownAccessExpiry, expiry);
  }
  nextRefreshAt = knownAccessExpiry
    ? refreshAtForExpiry(knownAccessExpiry)
    : Date.now() + FALLBACK_RETRY_MS;
  if (nextRefreshAt <= Date.now() && document.visibilityState === "visible") {
    queueMicrotask(() => void refreshIfDue());
  }
}

function publishRefreshCompletion() {
  try {
    localStorage.setItem(
      REFRESH_COMPLETED_KEY,
      JSON.stringify({
        accessExpiresAt: knownAccessExpiry,
        completedAt: Date.now(),
      }),
    );
  } catch {
    // Web Locks still serializes refreshes when storage is unavailable.
  }
}

function adoptRecentRefresh(attemptStartedAt: number) {
  try {
    const completed = parseStoredValue(
      localStorage.getItem(REFRESH_COMPLETED_KEY),
    );
    if (
      !completed ||
      typeof completed.completedAt !== "number" ||
      completed.completedAt < attemptStartedAt - 5_000
    ) {
      return false;
    }
    if (
      typeof completed.accessExpiresAt === "number" &&
      completed.accessExpiresAt > Date.now()
    ) {
      knownAccessExpiry = Math.max(
        knownAccessExpiry,
        completed.accessExpiresAt,
      );
      nextRefreshAt = refreshAtForExpiry(knownAccessExpiry);
    }
    return true;
  } catch {
    return false;
  }
}

function parseStoredValue(
  value: string | null,
): StoredRefreshCoordination | null {
  if (!value) return null;
  try {
    const parsed: unknown = JSON.parse(value);
    return parsed && typeof parsed === "object"
      ? (parsed as StoredRefreshCoordination)
      : null;
  } catch {
    return null;
  }
}
