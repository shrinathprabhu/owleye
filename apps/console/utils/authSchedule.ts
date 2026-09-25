export const AUTH_SESSION_INVALIDATED_EVENT = "owleye:session-invalidated";

const MIN_REFRESH_SKEW_MS = 30 * 1000;
const MAX_REFRESH_SKEW_MS = 10 * 60 * 1000;

/** Schedules refresh before expiry without hammering very short sessions. */
export function refreshAtForExpiry(expiry: number, now = Date.now()) {
  const remaining = Math.max(0, expiry - now);
  const skew = Math.min(
    MAX_REFRESH_SKEW_MS,
    Math.max(MIN_REFRESH_SKEW_MS, remaining * 0.15),
  );
  return Math.max(now, expiry - skew);
}
