export const API_UNAUTHORIZED_EVENT = "owleye:api-unauthorized";

export function useApi() {
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
    onResponseError({ response }) {
      if (response.status === 401 && import.meta.client) {
        window.dispatchEvent(new Event(API_UNAUTHORIZED_EVENT));
      }
    },
  });
  const publicApi = $fetch.create({
    baseURL: apiBase.value,
    credentials: "omit",
  });

  return { api, apiBase, publicApi };
}
