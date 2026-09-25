/** Keep the API source exact. Configuration is never interpolated into CSP. */
export function consoleContentSecurityPolicy(apiBase: string, nonce: string) {
  if (!/^[A-Za-z0-9+/]{24,}={0,2}$/.test(nonce)) {
    throw new Error("Invalid CSP nonce");
  }
  let apiOrigin = "";
  if (!(apiBase.startsWith("/") && !apiBase.startsWith("//"))) {
    const url = new URL(apiBase);
    if (
      !["https:", "http:"].includes(url.protocol) ||
      url.username ||
      url.password
    ) {
      throw new Error(
        "The console API must use an HTTP(S) origin or same-origin path",
      );
    }
    apiOrigin = ` ${url.origin}`;
  }
  return [
    "default-src 'self'",
    "base-uri 'self'",
    `script-src 'self' 'nonce-${nonce}' 'strict-dynamic'`,
    "script-src-attr 'none'",
    "style-src 'self' 'unsafe-inline'",
    `connect-src 'self'${apiOrigin}`,
    "font-src 'self' data:",
    "img-src 'self' data: blob:",
    "form-action 'self'",
    "frame-ancestors 'none'",
    "object-src 'none'",
    "worker-src 'self' blob:",
  ].join("; ");
}
