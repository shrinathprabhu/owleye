export const ANALYTICS_SDK_VERSION = "1.0.2";
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
