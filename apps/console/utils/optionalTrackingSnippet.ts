export const ANALYTICS_SDK_VERSION = "1.0.3";
export type OptionalCollector = "performance" | "rules";
function htmlAttribute(value: string) {
  return value
    .replace(/&/g, "&amp;")
    .replace(/"/g, "&quot;")
    .replace(/</g, "&lt;")
    .replace(/>/g, "&gt;");
}
export function optionalTrackingSnippet(
  kind: OptionalCollector,
  siteId: string,
  apiBase: string,
) {
  const server = apiBase.trim().replace(/\/+$/, "");
  const options = server ? `, { server: ${JSON.stringify(server)} }` : "";
  const id = JSON.stringify(siteId);
  const npm =
    kind === "rules"
      ? `import { trackRules } from "@owleye/analytics/rules";

// Initialize once in a browser entrypoint, after any required consent.
const rules = trackRules(${id}${options});
// On teardown: rules.stop();`
      : `import { trackWebVitals, trackPerf } from "@owleye/analytics/performance";

// Initialize once in a browser entrypoint, after any required consent.
const vitals = trackWebVitals(${id}${options});
// On teardown: vitals.stop();

// Optional: measure one operation explicitly.
const performance = trackPerf(${id}${options});
const end = performance.start("load_results");
// ...perform the operation...
end();`;
  const cdn = `<script defer src="https://cdn.jsdelivr.net/npm/@owleye/analytics@${ANALYTICS_SDK_VERSION}/dist/owleye.${kind}.iife.js" data-owleye-id="${htmlAttribute(siteId)}"${server ? ` data-owleye-server="${htmlAttribute(server)}"` : ""}></script>`;
  return { npm, cdn };
}

export function optionalTrackingSetupPrompt(
  kind: OptionalCollector,
  siteId: string,
  apiBase: string,
) {
  const snippets = optionalTrackingSnippet(kind, siteId, apiBase);
  return `Add OwlEye ${kind === "rules" ? "tracking rules" : "Web Vitals and optional manual performance spans"} to this project. Inspect the framework, existing OwlEye initialization, package manager and consent controls first. Preserve working page analytics.

Public tracking ID: ${JSON.stringify(siteId)}
API base URL: ${JSON.stringify(apiBase.trim() || "https://api.owleye.dev")}
SDK reference: https://github.com/shrinathprabhu/owleye/blob/v${ANALYTICS_SDK_VERSION}/packages/analytics/README.md
CDN reference: https://owleye.dev/docs/cdn/

Choose npm OR CDN, matching the existing setup. If the full OwlEye bundle already provides this collector, reuse it; do not initialize a second collector. For npm, use @owleye/analytics@${ANALYTICS_SDK_VERSION} and initialize once in a browser-only entrypoint after any required consent. Keep the API server configuration identical to the existing integration. Stop the collector on actual teardown and avoid duplicate instances during navigation/remounts.

Suggested npm setup:
${snippets.npm}

Plain HTML alternative:
${snippets.cdn}

${
  kind === "rules"
    ? "Rules are managed in OwlEye but require the external rules entrypoint on the website. Verify enabled rules and their real selectors/triggers; matching interactions must actually occur. Keep capture_text disabled. Use static safe metadata, never input values, DOM text, emails, private URLs or secrets. Do not invent a successful signup or payment event."
    : "Web Vitals availability depends on the browser and interactions; the current collector reports estimates. Missing INP is not zero. Manual spans are optional: measure a real asynchronous operation, call the returned end function after it completes, and use a stable static operation name. Do not add artificial performance events, heartbeats, FPS loops or repeated collectors."
}

Preserve SDK Global Privacy Control defaults and existing consent gates. Keep full query/fragment capture disabled; bounded campaign fields are automatic, without a campaign configuration option. Do not add cookies, storage identifiers, personal data or secrets. Check allowed origins and only the CSP directives required for this integration. Run relevant project checks and explain how to verify a real ${kind === "rules" ? "matching interaction" : "supported measurement"} in OwlEye. State anything that could not be verified instead of claiming collection succeeded.`;
}
