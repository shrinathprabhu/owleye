type ApiErrorPayload = {
  error?: string | { code?: string; message?: string };
};

type ApiErrorLike = {
  data?: ApiErrorPayload;
  response?: { status?: number };
  status?: number;
  statusCode?: number;
};

/**
 * Returns the server's safe error message when one is available, otherwise the
 * caller-owned fallback. Keeping this parser in one place makes every console
 * surface handle API failures consistently without depending on a specific
 * fetch implementation's error class.
 */
export function apiErrorMessage(error: unknown, fallback: string) {
  if (!isErrorLike(error)) return fallback;

  const payload = error.data;
  if (typeof payload?.error === "string") return payload.error;
  if (payload?.error?.message) return payload.error.message;
  return fallback;
}

export function apiErrorStatus(error: unknown) {
  if (!isErrorLike(error)) return undefined;
  return error.status ?? error.statusCode ?? error.response?.status;
}

export function apiErrorCode(error: unknown) {
  if (!isErrorLike(error)) return undefined;
  const payload = error.data;
  return typeof payload?.error === "object" ? payload.error.code : undefined;
}

export function isAbortError(error: unknown) {
  return (
    typeof error === "object" &&
    error !== null &&
    "name" in error &&
    error.name === "AbortError"
  );
}

function isErrorLike(error: unknown): error is ApiErrorLike {
  return typeof error === "object" && error !== null;
}
