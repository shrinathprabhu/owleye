type ApiDownloadError = {
  data?: unknown;
  response: { status: number };
  status: number;
};

type BrowserDownloadOptions = {
  shouldDownload?: () => boolean;
  signal?: AbortSignal;
};

/**
 * Downloads an authenticated API response without navigating the console away
 * from its current route. Non-success payloads retain the same structural
 * shape as `$fetch` errors so the shared API error formatter can read them.
 */
export async function downloadBrowserFile(
  url: string,
  fileName: string,
  { shouldDownload, signal }: BrowserDownloadOptions = {},
) {
  if (!import.meta.client) return;

  const response = await fetch(url, {
    credentials: "include",
    signal,
  });
  if (!response.ok) {
    let data: unknown;
    try {
      data = await response.clone().json();
    } catch {
      data = undefined;
    }
    throw {
      data,
      response: { status: response.status },
      status: response.status,
    } satisfies ApiDownloadError;
  }

  const blob = await response.blob();
  signal?.throwIfAborted();
  if (shouldDownload && !shouldDownload()) return;
  const objectUrl = URL.createObjectURL(blob);
  const anchor = document.createElement("a");
  anchor.href = objectUrl;
  anchor.download = fileName;
  document.body.append(anchor);
  anchor.click();
  anchor.remove();
  window.setTimeout(() => URL.revokeObjectURL(objectUrl), 1_000);
}
