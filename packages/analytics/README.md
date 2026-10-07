# @owleye/analytics

[![npm version](https://img.shields.io/npm/v/%40owleye%2Fanalytics)](https://www.npmjs.com/package/@owleye/analytics)
[![CI](https://github.com/shrinathprabhu/owleye/actions/workflows/ci.yml/badge.svg)](https://github.com/shrinathprabhu/owleye/actions/workflows/ci.yml)
[![Core bundle size (minified + gzip)](https://img.shields.io/bundlejs/size/%40owleye/analytics?format=minzip&label=core%20min%2Bgzip)](https://bundlejs.com/?q=%40owleye%2Fanalytics)
[![MIT license](https://img.shields.io/npm/l/%40owleye%2Fanalytics)](https://github.com/shrinathprabhu/owleye/blob/HEAD/packages/analytics/LICENSE)

A dependency-free, cookie-free browser analytics SDK for OwlEye.

See [the changelog](./CHANGELOG.md) for SDK 1.0.2 changes. Version-pinned 1.0.2 examples below require publication to npm first.

## Install

```sh
pnpm add @owleye/analytics@1.0.2
# or: npm install @owleye/analytics@1.0.2
# or: bun add @owleye/analytics@1.0.2
# or: yarn add @owleye/analytics@1.0.2
```

```ts
import { useAnalytics } from "@owleye/analytics";

const analytics = useAnalytics("owl_your_tracking_id", {
  captureCampaigns: true,
});

analytics.track("signup_completed", {
  plan: "founder",
});

// Call during app teardown or when tracking permission is withdrawn.
function stopAnalytics() {
  analytics.stop();
}
```

See [Config](#config) for all optional settings and their defaults.

## Browser script

Load the dependency-free CDN build with your site ID. Pin a published version in production rather than using a moving tag.

```html
<script
  defer
  crossorigin="anonymous"
  src="https://cdn.jsdelivr.net/npm/@owleye/analytics@1.0.2/dist/owleye.analytics.iife.js"
  data-owleye-id="owl_your_tracking_id"
></script>
<script>
  window.addEventListener("load", () => {
    window.OwlEyeAnalytics.track("signup_completed", { plan: "founder" });
  });
</script>
```

The default SDK excludes URL query strings and fragments, writes no cookies, `localStorage`, or `sessionStorage`, omits request credentials, and suppresses tracking for Global Privacy Control (GPC) by default. Do Not Track (DNT) is no longer consulted. Optional entrypoints keep rule tracking and performance tracking out of the main bundle:

```ts
import { trackPerf } from "@owleye/analytics/performance";
import { trackRules } from "@owleye/analytics/rules";
```

Import `trackWebVitals` from `@owleye/analytics/performance` to opt into PerformanceObserver estimates. The pre-release `performance: true` core option has been removed so the core bundle actually excludes the optional collector. These estimates are marked `approximate: true`; they do not implement the full reference Web Vitals algorithms and must not be presented as CrUX-equivalent CLS or INP scores. The standalone performance entrypoint enables those field metrics automatically and also exposes manual spans. Measurements obey the same SDK GPC, URL-redaction, origin and privacy controls as other events. `captureCampaigns: true` retains only `utm_source`, `utm_medium`, and `utm_campaign`; unrelated query parameters remain excluded.

Create the site in the console first. Allowed domains are optional: leave them blank to allow all browser origins, or add multiple domains to restrict tracking and rule delivery to matching hosts. Use the public tracking ID in browser code; keep API/developer secrets on your server. Verify `POST /v1/events` in the Network panel and check the response's `accepted` count: HTTP 202 can also acknowledge a privacy opt-out with zero events. Then confirm the event in the console. See the [quickstart](https://owleye.dev/docs/quickstart/) for framework setup and troubleshooting.

The size badge estimates the minified and gzipped core npm entrypoint using bundlejs; optional rules, performance, and full CDN bundles have separate sizes. CI links to the public repository workflow, which includes the SDK tests.

`pnpm size` checks the complete ESM consumer bundles, including shared chunks, and verifies that unused imports disappear. The core has no runtime dependencies and does not include the optional rules or performance collectors.

Cookie-free does not determine your lawful basis. If your policy or applicable law requires a decision before analytics starts, gate it explicitly:

```ts
const analytics = useAnalytics("owl_your_tracking_id", {
  autoStart: false,
});

// Call only after your consent/lawful-basis flow permits analytics.
analytics.start();

// Withdraw immediately without sending the active page-session segment.
analytics.stop();
```

Create the optional rule and performance trackers only after that same decision; `autoStart` controls the automatic page-analytics entrypoint, not explicit optional entrypoints.

For consent-gated CDN usage, add `data-owleye-auto-start="false"` and later call `window.OwlEyeAnalytics.start()`. `respectGlobalPrivacyControl` defaults to `true`. The SDK checks the browser setting before sending events; the ingestion API does not filter `DNT` or `Sec-GPC` headers. DNT is ignored and has no SDK configuration option. Server-side senders must apply the appropriate collection choices before submitting events.

The browser controls the `Sec-GPC` header; installing OwlEye does not enable it. OwlEye checks `navigator.globalPrivacyControl` when a controller is initialized. An integration can explicitly set `respectGlobalPrivacyControl: false`, or `data-owleye-respect-global-privacy-control="false"` for CDN scripts, to disable SDK suppression. This does not remove the browser header. Configure each optional rules/performance entrypoint separately. GPC expresses a sale/sharing opt-out; OwlEye's default of suppressing all collection is a conservative product choice, not a claim that every analytics use is prohibited.

## Full CDN bundle

Use `dist/owleye.full.iife.js` when you want page analytics, Console-managed rules, Web Vitals, and manual performance timers together. It is one self-contained JavaScript payload with shared code, not a loader that downloads three more scripts. It still makes the normal rules and event API requests. Keep the smaller individual bundles when you only need some features.

Load the full bundle from the version-pinned CDN URL (available after 1.0.2 is published):

```html
<script
  defer
  src="https://cdn.jsdelivr.net/npm/@owleye/analytics@1.0.2/dist/owleye.full.iife.js"
  data-owleye-id="owl_your_tracking_id"
  data-owleye-capture-campaigns="true"
></script>
```

The npm export is `@owleye/analytics/cdn/full`. The full bundle is included starting with SDK 1.0.1. The version-pinned CDN URL becomes available after the corresponding version is published to npm. You can also copy the built file to your own static assets. Its SHA-384 value is included in `dist/integrity.json` alongside the individual bundles.

All documented script attributes apply to the full bundle. They are read from the script tag and passed to all included modules. Analytics-only options (`autoStart`, `autoTrackPageviews`, `trackQueryChanges`, and `trackHashChanges`) still affect only page analytics. GPC, URL capture, mock, debug, and endpoint settings apply to all three modules. Function-based `enrichRule` still requires the module API; it cannot be supplied as an HTML attribute.

After the script loads, use the same globals as the individual bundles:

```js
window.OwlEyeAnalytics.track("signup_completed", { plan: "free" });
const end = window.OwlEyePerformance.start("report_generation");
// Call after the operation completes:
end({ status: "ok" });
```

Rules and Web Vitals start on load. `data-owleye-auto-start="false"` only delays page analytics; it is not a gate for the whole bundle. If all collection must wait, load the full script only after that decision. Loading full plus individual bundles does not duplicate automatic trackers for the same Tracking ID and endpoint, but normally choose one approach to avoid redundant downloads.

To clean up the automatic trackers:

```js
window.OwlEyeAnalytics.stop();
window.OwlEyeRules.stop();
window.OwlEyePerformance.observeVitals().stop();
```

There is no aggregate `OwlEyeFull` controller or global performance `stop()`. Do not invoke previously returned custom timer end functions after collection permission is withdrawn. Stopping trackers does not retract requests already started.

## CDN campaigns, custom data, rules, and performance

The CDN globals are `window.OwlEyeAnalytics`, `window.OwlEyeRules`, and `window.OwlEyePerformance`. Each is available only after its corresponding bundle loads successfully. There is no single `window.OwlEye` object and no pre-load queue.

Load these scripts once with the same Tracking ID and endpoint. Keep `defer` (rather than `async`) when a later script depends on their order:

```html
<script
  defer
  src="https://cdn.jsdelivr.net/npm/@owleye/analytics@1.0.2/dist/owleye.analytics.iife.js"
  data-owleye-id="owl_your_tracking_id"
  data-owleye-capture-campaigns="true"
></script>
<!-- Optional: enabled rules configured in Console -->
<script
  defer
  src="https://cdn.jsdelivr.net/npm/@owleye/analytics@1.0.2/dist/owleye.rules.iife.js"
  data-owleye-id="owl_your_tracking_id"
  data-owleye-capture-campaigns="true"
></script>
<!-- Optional: automatic Web Vitals and manual timings -->
<script
  defer
  src="https://cdn.jsdelivr.net/npm/@owleye/analytics@1.0.2/dist/owleye.performance.iife.js"
  data-owleye-id="owl_your_tracking_id"
  data-owleye-capture-campaigns="true"
></script>
<script defer src="/analytics-events.js"></script>
```

`data-owleye-capture-campaigns="true"` opts each bundle into capturing only `utm_source`, `utm_medium`, and `utm_campaign` from the current page URL, for example `?utm_source=newsletter&utm_medium=email&utm_campaign=autumn_launch`. Leave full query capture disabled. Campaigns are attribution on normal events, not a fourth SDK bundle or a separate manual tracking call. This does not persist first-touch attribution after those parameters disappear from the URL.

In your own **analytics-events.js**, use the initialized globals directly:

```js
const analytics = window.OwlEyeAnalytics;

// Optional fields reused by custom events. Never put personal identifiers here.
analytics?.setGlobalRecords({
  app_version: "1.0.0",
  environment: "production",
});

// A manual custom event: values retain their string / number / boolean types.
document.getElementById("download-report")?.addEventListener("click", () => {
  analytics?.track("report_downloaded", {
    format: "pdf",
    page_count: 12,
    included_charts: true,
  });
});

// Time your real asynchronous operation; no automatic page analytics needed.
async function loadReport() {
  const end = window.OwlEyePerformance?.start("report_load", { format: "pdf" });
  try {
    const response = await fetch("/reports/latest");
    end?.({ ok: response.ok, status_code: response.status });
    return response;
  } catch (error) {
    end?.({ ok: false, failure: "network" });
    throw error;
  }
}
```

For an inline `<script>`, wrap setup code in a `DOMContentLoaded` listener: inline scripts execute immediately even when preceded by deferred external scripts. For a dynamically injected CDN script, wait for its `load` event before using the global. Optional chaining prevents an exception when a script is blocked; it drops the call and does not retry it. If scripts can arrive later, read the global at call time instead of caching an initially undefined reference.

Rules need no manual JS handler: configure and enable a rule in Console, load the rules bundle, and trigger the matching interaction. Configure its custom-property selectors there too. Do not add a manual `.track()` for the same action unless you intend two events. A JavaScript `enrichRule` callback is a module-import option; it cannot be passed as a CDN HTML attribute. Use manual `.track()` for computed custom data in plain JavaScript.

The performance bundle starts Web Vitals automatically. Manual spans use `window.OwlEyePerformance.start()` as shown above. To stop Web Vitals, call `window.OwlEyePerformance?.observeVitals().stop()`; to stop rules, call `window.OwlEyeRules?.stop()`. The performance global itself has no `.stop()`. Only load optional bundles after any required tracking-permission decision; `data-owleye-auto-start="false"` delays the analytics bundle alone.

### Custom-property contract

- Custom events: **10 fields total**, combining global and event-specific fields; event-specific values override a matching global key.
- Rules: **10 fields total**, combining configured DOM captures and optional module-based enrichment. Configured captures take precedence.
- Performance: **10 fields at start and 10 at end**. Built-in timing fields are separate; the entire event data still must fit within **16 KiB**.
- Values: **string, finite number, or boolean** only. No `null`, `undefined`, arrays, nested objects, `Date` instances, bigint, functions, `NaN`, or infinities. Missing rule elements are omitted. Use an ISO string or numeric timestamp for a date; use named flat fields for structured concepts. JSON encoded as a string stays an opaque string, not queryable nested properties.

Invalid manual data is logged and dropped by the SDK without throwing into your application. The API also validates custom properties. Check the request to `/v1/events` and its `accepted` count; look for the event in Console's Events page, UTM attribution in Campaigns, and automatic measurements in Web Vitals.

## Browser identification

The SDK sends a bounded browser name and version from supported UA tokens and low-entropy client-hint brands. Chrome, Google Chrome, and Chromium are normalized to `Chrome`; recognized specific identities take precedence over generic engine hints. Existing Brave API detection checks for the presence of `navigator.brave.isBrave`, without calling it, and leaves the product version unset unless a matching brand or token supplies one.

A browser that exposes only Chrome identifiers remains `Chrome`, including Arc, Comet, Dia, or Brave in that situation. Explicit-token test fixtures do not prove that every shipping browser exposes a unique identity. The SDK does not probe for hidden browser identities or request high-entropy hints. OS, device, crawler/AI-agent, and IP geolocation parsing happen on the backend.

## Visitor identification and browser storage

All SDK bundles use in-memory state only. They do not read or write cookies, localStorage, sessionStorage, IndexedDB, or Cache Storage, and they do not install a service worker or persist an event queue. Event and rule requests use `credentials: "omit"`, including when the API shares the page's origin.

The API derives site-scoped, keyed pseudonyms from the truncated IP prefix (IPv4 /24 or IPv6 /64), user agent, timezone, and server salt. The visitor pseudonym can remain stable across days while those inputs stay unchanged; the user pseudonym changes daily and session pseudonyms use fixed 30-minute buckets. These are approximate identifiers: different people can share one, and the same person can receive another after their network or browser changes. Cookie-free does not mean there is no server-side identification.

The authenticated Console uses HttpOnly, SameSite login cookies, with Secure on HTTPS. Its localStorage entries contain only refresh timestamps/expiry and a random timestamped tab-notification signal, not user IDs, passwords, or tokens. Those entries coordinate Console tabs and are not used for analytics visitors. Custom event data and rule captures are controlled by the integrator: do not send personal identifiers or copy a site's own stored identifiers into event fields.

## Repeated initialization and runtime limits

Each browser document shares one active analytics controller, rule tracker, and Web Vitals observation per normalized tracking ID and API base, including across ESM and CDN copies. Repeated initialization reuses them; the first configuration (including enrichment) wins until that tracker is stopped. Repeated initialization never starts a controller created with `autoStart: false`; call its `start()` explicitly. Supplying conflicting options produces a bounded warning with the option names, without applying the new values. Stop and recreate a tracker to change its options. Own this lifecycle at the app root: calling `stop()` from any consumer stops the shared tracker for every consumer.

Cleanup/remount on the same URL does not send another automatic pageview. Pathname navigation (plus opted-in query/hash changes) and explicit `pageview()` calls still count. Explicit `track()` calls and distinct matching rule IDs remain separate events; the SDK cannot infer whether your application intended those calls. Duplicate rule IDs from a rules response are ignored. A replaced DOM element may legitimately produce a new view impression.

Delivery is best effort, with no persistent queue or automatic retries. Across the document, at most eight event requests and 60,000 bytes are in flight, with a five-second timeout and a 100-events/second budget (burst capacity 200). Excess events are dropped. Rule loading times out after ten seconds. Pending rule debounce timers and added DOM roots are bounded at 250 each; mutation overflow coalesces into a refresh. Stopping rules clears timers, listeners, and observers.

SDK failures are contained at public calls and background callbacks. Invalid initialization returns a safe no-op controller; invalid events are dropped and valid later calls can continue. Console warnings identify the failed operation, without event payloads or raw exception messages, and are limited to ten per minute per document. Logging failures are also contained. Browser matching, serialization, and synchronous enrichment execute on the main thread: keep selectors narrow and enrichment fast. These safeguards do not guarantee zero CPU cost or prevent a slow application callback from blocking its own page.

## SPA and memory routers

Automatic pageviews count pathname changes by default. Query-only and hash-only changes do not create pageviews or close the current page segment. To count those route changes, enable the relevant option:

```ts
const analytics = useAnalytics("owl_your_tracking_id", {
  trackQueryChanges: true,
  trackHashChanges: true,
});
```

These options control counting, not captured URL content. `captureQuery` and `captureHash` remain separate opt-ins. Leave them off unless those values are safe to send. Conversely, enabling URL capture alone does not enable query/hash routing. Repeated browser notifications for the same URL count once.

Memory routers have no standard browser navigation event. Connect your router's committed-route callback to `pageview()` and disable automatic views so the browser URL is not also counted:

```ts
const analytics = useAnalytics("owl_your_tracking_id", {
  autoTrackPageviews: false,
});

// Call once for the initial screen and once after each completed navigation.
// Wire this function to your router's subscription or framework lifecycle.
function onRouteCommitted(path: string, title: string) {
  analytics.pageview({ path, title });
}
```

Use a sanitized virtual pathname without secrets. Supplying `path` also derives the page URL when no explicit `url` is given. In manual mode, page sessions and custom events use the last explicit page context. OwlEye cannot infer memory-router transitions from DOM changes, and repeated explicit calls each count as a view. Optional rules and performance modules still use browser/document context, not the memory router's virtual route.

For CDN, use `data-owleye-track-query-changes="true"`, `data-owleye-track-hash-changes="true"`, or `data-owleye-auto-track-pageviews="false"`, and call `window.OwlEyeAnalytics.pageview({ path, title })` from the same route callback. Configure tracking permission separately; `autoTrackPageviews: false` does not disable custom events.

With `debug: true`, messages distinguish privacy/inactive suppression, mock mode, delivery-budget drops, HTTP/network failures, and API acknowledgements (including `accepted: 0`). A submitted request is not proof of acceptance or storage. Acknowledgement inspection is limited to 4 KiB; detailed messages are limited to 100 per minute per document. Normal mode skips response parsing and emits only bounded failure/configuration warnings.

Web Vitals retain document-level attribution, including buffered measurements observed after SPA navigation. If the browser exposes navigation timing, its document URL is used; otherwise the URL at observation startup is retained. These are not separate per-SPA-screen Web Vitals. Custom timings retain the browser page where `start()` was called. Web Vitals `stop()` discards pending values and disconnects observers/listeners; hidden/page-exit lifecycle events flush values while observation is active. Stopping does not retract requests already started.

## API

| Entry point | Purpose |
| --- | --- |
| `useAnalytics(siteId, config)` | Returns an idempotent page and custom-event controller. |
| `controller.track(name, records?)` | Sends a custom event with at most ten primitive fields. |
| `controller.pageview(page?)` | Records a route change that did not update browser history. |
| `controller.setGlobalRecords(records?)` | Replaces fields merged into future custom events; the combined limit is ten. |
| `controller.start()` / `stop()` | Idempotently installs or removes automatic browser tracking. |
| `trackPerf(siteId, config).start(name, records?)` | Starts a measurement with at most ten fields and returns its idempotent end function. |
| `trackPerf(siteId, config).observeVitals()` | Starts automatic LCP, INP, CLS, FCP, and TTFB field collection. |
| `trackRules(siteId, config)` | Loads server-managed rules and returns a stoppable controller. |

```ts
import { trackPerf } from "@owleye/analytics/performance";
import { trackRules } from "@owleye/analytics/rules";

const perf = trackPerf("owl_your_tracking_id");
const endCheckout = perf.start("checkout_latency", { plan: "pro" });
endCheckout({ outcome: "success" });

const rules = trackRules("owl_your_tracking_id");
rules.stop();
```

Event and performance records must be flat objects containing only strings, finite numbers, or booleans. Custom events accept at most ten fields and performance start/end records accept at most ten fields each. Invalid names, oversized payloads, nested values, and excess fields are logged and dropped before a request is sent. These validation failures do not throw into your application.

Avoid sending names, email addresses, tokens, or other personal data. SDK constructors validate the site ID and server URL immediately, including during server rendering; create browser controllers in client-only lifecycle code. See the [SDK guide](https://owleye.dev/docs/sdk/) for configuration, browser/CDN usage, SPA behavior, payload limits, and cleanup.

## Config

All configuration fields are optional. Pass an `OwlConfig` object as the second argument to `useAnalytics`, `trackPerf`, or `trackWebVitals`. `trackRules` accepts `OwlRulesConfig`, which adds `enrichRule` to the same options. Each entrypoint has its own configuration; options are not automatically copied between trackers.

| Field | Type | Default | Behavior |
| --- | --- | --- | --- |
| `autoStart` | `boolean` | `true` | Start automatic page analytics when `useAnalytics` is called. Set to `false` to wait until `controller.start()`. Does not delay rules or performance tracking. |
| `autoTrackPageviews` | `boolean` | `true` | Automatically count initial and browser navigation views. Disable for manual/memory-router views; custom events remain available. |
| `trackQueryChanges` | `boolean` | `false` | Count query-only navigation. Does not enable query capture. |
| `trackHashChanges` | `boolean` | `false` | Count hash-only navigation. Does not enable hash capture. |
| `debug` | `boolean` | `false` | Explain collection/delivery decisions and numeric API acknowledgements without event payloads or raw response text. Failures still produce bounded warnings when disabled. |
| `mock` | `boolean` | `false` | Skip network requests; useful with `debug` during development. Remote rules are not loaded. |
| `captureQuery` | `boolean` | `false` | Include full query strings in page and referrer fields. Takes precedence over `captureCampaigns`; query strings may contain sensitive information. |
| `captureCampaigns` | `boolean` | `false` | Include only `utm_source`, `utm_medium`, and `utm_campaign` from the current URL. Does not persist attribution after the parameters disappear. |
| `captureHash` | `boolean` | `false` | Include URL fragments in page and referrer fields. Fragments may contain sensitive information. |
| `respectGlobalPrivacyControl` | `boolean` | `true` | Disable tracking when the browser enables Global Privacy Control. This check runs in the SDK only; the ingestion API does not enforce the header. |
| `server` | `string \| URL` | `https://api.owleye.dev` | Override the API base with an absolute HTTP(S) URL or same-origin path. Credentials, query strings, and fragments are not allowed. |
| `enrichRule` | `RuleEnricher` | Unset | `trackRules` only: synchronously return custom fields from `(rule, context)`, or return nothing. The context contains `element`, optional `event`, and interaction `type`. |

Rule enrichment shares the ten-field limit with configured DOM captures; configured captures take precedence. Return only string, finite number, or boolean values. Throwing or returning invalid data does not prevent the base rule event.

The examples use the default hosted API. For self-hosting, set `server` to your own OwlEye origin on each SDK entrypoint you use. Copy the installation snippet from your Console for the correct endpoint; omitting the override sends requests to the hosted API.

### CDN configuration

Set the corresponding attributes on each bundle's script tag. Omitted attributes use the defaults above. Boolean attributes accept `"true"` or `"1"` for enabled and `"false"` or `"0"` for disabled; an empty attribute also enables the option.

| Configuration field | CDN attribute |
| --- | --- |
| `autoStart` | `data-owleye-auto-start` |
| `autoTrackPageviews` | `data-owleye-auto-track-pageviews` |
| `trackQueryChanges` | `data-owleye-track-query-changes` |
| `trackHashChanges` | `data-owleye-track-hash-changes` |
| `debug` | `data-owleye-debug` |
| `mock` | `data-owleye-mock` |
| `captureQuery` | `data-owleye-capture-query` |
| `captureCampaigns` | `data-owleye-capture-campaigns` |
| `captureHash` | `data-owleye-capture-hash` |
| `respectGlobalPrivacyControl` | `data-owleye-respect-global-privacy-control` |
| `server` | `data-owleye-server` |
| `enrichRule` | Not available as an HTML attribute; use the module API. |

The tracking ID is supplied separately using `data-owleye-id`. See [CDN campaigns, custom data, rules, and performance](#cdn-campaigns-custom-data-rules-and-performance) for accessing the browser globals.

Source: <https://github.com/shrinathprabhu/owleye>

License: [MIT](LICENSE)

## Reproducible release artifacts

The package is published on npm. Source-checkout builds generate `dist/integrity.json` with SHA-384 digests for each standalone IIFE. For a CDN install, use an exact published version, its matching integrity value, and `crossorigin="anonymous"`. Do not copy a digest from a different build.

The GitHub release workflow builds and publishes the version in this manifest with npm provenance. Configure the npm trusted publisher as described in the [self-hosting guide](https://github.com/shrinathprabhu/owleye/blob/HEAD/docs/SELF_HOSTING.md#sdk), then publish a matching release tag.

Package: [@owleye/analytics](https://www.npmjs.com/package/@owleye/analytics) · [Source](https://github.com/shrinathprabhu/owleye/tree/HEAD/packages/analytics) · [Website](https://owleye.dev)

Maintained by **Shrinath** · [hello@shrinath.me](mailto:hello@shrinath.me) · [owleye.dev](https://owleye.dev).

The npm package description uses this SDK README from `packages/analytics`, not the monorepo root README. README changes reach npm with the next published package version.

### Page duration and referrers

The browser measures visible page time using its monotonic clock. Each time the page becomes hidden, navigates, or exits, it sends a `page_session` duration segment. Returning to the same tab starts another segment without another page view. The server aggregates these durations; counting segments is not counting visitor sessions. Background time is excluded, visibility does not prove attention, and browsers can drop exit deliveries.

Same-origin referrers are omitted from new SDK payloads and normalized away by the API. Different subdomains, schemes, or ports are different origins and remain eligible as referral sources. Browser privacy settings can prevent any referrer from being available. OwlEye's outbound HTTP `Referrer-Policy` does not remove an external referrer already captured in the event payload.

Web Vitals and explicit performance spans use separate measurement storage and do not appear in ordinary event lists. Web Vitals charts browser metrics; custom span summaries are available through AI reports. Collection still requires loading the optional performance entrypoint. Self-hosted installations have no monthly event allowance.
