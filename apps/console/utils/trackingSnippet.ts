export function trackingSetupSnippet(siteId: string, apiBase: string) {
  const server =
    apiBase.trim() ||
    (typeof window !== "undefined"
      ? window.location.origin
      : "https://analytics.example.com");
  const options = `, {\n  server: ${JSON.stringify(server)},\n}`;

  return `import { useAnalytics } from "@owleye/analytics";

const analytics = useAnalytics(${JSON.stringify(siteId)}${options});

analytics.track("signup_completed", { plan: "starter" });`;
}

export function trackingSetupPrompt(siteId: string, apiBase: string) {
  const server =
    apiBase.trim() ||
    (typeof window !== "undefined"
      ? window.location.origin
      : "https://analytics.example.com");
  return `Add OwlEye analytics to this project. Inspect the framework and existing analytics setup first; reuse the project's package manager and conventions.

Public tracking ID: ${JSON.stringify(siteId)}
OwlEye API base URL: ${JSON.stringify(server)}
SDK reference: https://github.com/shrinathprabhu/owleye/blob/main/packages/analytics/README.md
CDN reference: https://owleye.dev/docs/cdn/

For a bundled app, install @owleye/analytics and initialize useAnalytics from that package once in a browser-only entrypoint with this tracking ID and { server: the API base URL above }. Keep initialization out of server rendering and avoid duplicate instances during navigation or component remounts. Page views and SPA navigation are automatic.

For a plain HTML site without a bundler, use the documented CDN integration instead, with the same tracking ID and data-owleye-server set to the API base URL above. Use only one installation method. Do not install both CDN and npm tracking.

Keep query strings and URL fragments excluded and respect the SDK's Global Privacy Control default. Preserve any existing consent gate; do not initialize tracking before it permits analytics. Do not add cookies, local storage, or other persistent visitor identifiers. Never put secrets, account emails, or personal data into event properties or URLs.

Start with page analytics only. Add custom events, campaigns, rules, or performance tracking only if requested, following the SDK reference. If optional trackers are added, pass the same server configuration to each. Custom events allow at most 10 combined properties, using only strings, finite numbers, and booleans. Do not emit a sample signup event on page load.

Check that the deployed website domain is allowed in this app's OwlEye settings. If the project has a Content Security Policy, update only the directives needed for the chosen integration; do not broadly weaken it. Run the project's relevant checks and explain how to verify a real page view in OwlEye. Report any steps you cannot verify; do not claim ingestion succeeded without checking it.`;
}
