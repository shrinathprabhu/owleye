# OwlEye

[![CI](https://github.com/shrinathprabhu/owleye/actions/workflows/ci.yml/badge.svg)](https://github.com/shrinathprabhu/owleye/actions/workflows/ci.yml) [![npm version](https://img.shields.io/npm/v/%40owleye%2Fanalytics)](https://www.npmjs.com/package/@owleye/analytics) [![MIT license](https://img.shields.io/npm/l/%40owleye%2Fanalytics)](https://github.com/shrinathprabhu/owleye/blob/HEAD/LICENSE)

Web analytics that shows why visitors don't convert. [Use hosted OwlEye](https://owleye.dev).

[Documentation](https://owleye.dev/docs/) · [npm](https://www.npmjs.com/package/@owleye/analytics) · [Status](https://status.owleye.dev) · [GitHub](https://github.com/shrinathprabhu/owleye)

## See where visitors drop off

- **Add events from your dashboard.** Install the optional rules entrypoint once, then create click, form-submit and section-visible rules. New page loads pick up enabled changes without a code change for each event.
- **Find the step that loses the signup.** Build ordered funnels over recorded events, then compare pages, referrers and campaigns to decide what to investigate.
- **Ask your recorded data questions.** Hosted OwlEye supports plain-English analytics on eligible paid plans when AI is available and enabled. Availability, permissions and credit limits apply.

## Start with hosted OwlEye

Create an app in the [OwlEye Console](https://app.owleye.dev) and copy its public tracking ID. Install the browser SDK:

```sh
npm install @owleye/analytics@1.0.3
```

```ts
import { useAnalytics } from "@owleye/analytics";
const analytics = useAnalytics("your-public-tracking-id");
// Call after the export actually succeeds:
analytics.track("export_completed", { format: "pdf" });
```

That example uses the default OwlEye API. Add the [optional rules entrypoint](packages/analytics/README.md) once to manage click, form and visibility events from the dashboard. Confirmed purchases and other business results should be tracked where they actually succeed. Funnels show drop-off among recorded steps; they do not establish why an individual visitor left.

[Start free](https://app.owleye.dev) · [See plans](https://owleye.dev/pricing/)

## Collection boundaries

| Available measurements | Collection boundaries |
| --- | --- |
| Page paths, titles, external referrer hosts | URL query strings and fragments stay excluded unless enabled |
| Browser/OS families, viewport, approximate location | No raw IP storage in analytics facts |
| Named events and explicitly supplied properties | No cookies, localStorage, sessionStorage or session replay from the visitor script |

Visitor counts use site-scoped pseudonyms and remain estimates. Cookie-free does not by itself establish a consent exemption. Do not send personal information or secrets in custom fields.

Maintained by **Shrinath** · [hello@shrinath.me](mailto:hello@shrinath.me) · [owleye.dev](https://owleye.dev).

## Running it yourself

This repository contains the API, Console and SDK. One Rust server serves the Nuxt Console at `/` and APIs at `/v1`. SQLite stores accounts and configuration; ClickHouse stores analytics. Hosted OwlEye is operated from a separate service repository.

|  | Hosted OwlEye | Running this repository |
| --- | --- | --- |
| Operations | OwlEye runs the service | You manage servers, updates, backups and monitoring |
| Cost | Free and paid [plans](https://owleye.dev/pricing/) | Software terms are in [LICENSE](LICENSE); you pay infrastructure costs |
| Data retention | Plan-based hosted retention | Your deployment's configured policy |
| AI and billing | Managed capabilities, subject to availability and plan eligibility | Do not assume hosted features or provider accounts are included |

- [Setup guide](docs/SELF_HOSTING.md): prerequisites, machine sizing, setup, redeploy and update.
- [SDK usage guide](packages/analytics/README.md): npm and CDN setup, JavaScript globals, manual events, campaigns, rules, performance and the 10-property contract.
- [Browser storage and visitor identification](packages/analytics/README.md#visitor-identification-and-browser-storage).
- [SDK release checklist](docs/SELF_HOSTING.md#publishing-the-sdk).

```sh
./setup.sh       # first installation and administrator account
./redeploy.sh    # build and restart using existing configuration
./update.sh      # git pull --ff-only, build, and restart
```

Recommended minimum: **4 vCPU, 8 GB RAM, and 20 GB free storage**. This is guidance, not an enforced restriction. Install Node.js 26.12.0, pnpm 12.11.2, Rust 1.95+ with Cargo, ClickHouse, and SQLite before setup. Caddy is optional. Follow the [prerequisite installation links and commands](docs/SELF_HOSTING.md#installing-prerequisites), then run `./setup.sh --check`. Running setup again refuses to overwrite an existing installation; use redeploy to rebuild and restart.

### API origin and proxy boundaries

To send events to a custom API origin, pass `{ server: "https://analytics.example.com" }` as the second SDK argument. Set the same API origin on each optional entrypoint you use. A `server` override or same-origin rewrite is not an automatically supported first-party proxy. Forwarded addresses are trusted only when the immediate peer is configured in `OWLEYE_TRUSTED_PROXIES`; trust only proxy infrastructure you control and sanitize client-supplied forwarding headers there. An untrusted or incorrectly forwarded request can use the proxy address for visitor/session derivation and geography, and share rate limits with other visitors. Proxies add operational cost and latency, do not guarantee blocker recovery, and must preserve visitor privacy choices. See the [SDK proxy boundaries and release checks](packages/analytics/README.md#endpoint-overrides-and-proxies) before changing delivery paths.

Licensed under [MIT](LICENSE).

Third-party dependencies and bundled assets retain their respective licenses, including the [amCharts map data](apps/console/public/maps/LICENSE.amcharts).

User-Agent detection includes [ua-parser rules](apps/api/vendor/uap-core/README.md), licensed under Apache-2.0.
