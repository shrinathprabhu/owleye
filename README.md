# OwlEye

Cookie-free web and product analytics. [Use hosted OwlEye](https://owleye.dev), or self-host this MIT-licensed distribution.

[Documentation](https://owleye.dev/docs/) · [npm](https://www.npmjs.com/package/@owleye/analytics) · [Status](https://status.owleye.dev) · [Source](https://github.com/shrinathprabhu/owleye)

## Why OwlEye?

- Understand pages, campaigns, and the product actions that matter.
- Install the rules entrypoint once, then change click, form, and visibility tracking from the Console.
- Build funnels to find where the measured journey stops.
- Use a dependency-free browser SDK with no cookies, localStorage, or sessionStorage writes.

## Add analytics

```sh
npm install @owleye/analytics
```

```ts
import { useAnalytics } from "@owleye/analytics";
const analytics = useAnalytics("your-public-tracking-id");
analytics.track("export_completed", { format: "pdf" });
```

That example uses the hosted endpoint. For self-hosting, pass `{ server: "https://analytics.example.com" }` as the second argument. Add the [rules entrypoint](packages/analytics/README.md) for Console-managed interactions; confirmed purchases and other business results should be tracked where they actually succeed.

| Browser defaults collect | Browser defaults avoid |
| --- | --- |
| Page paths, titles, external referrer hosts | URL query strings and fragments unless enabled |
| Browser/OS families, viewport, approximate location | Raw IP storage in analytics facts |
| Named events and explicitly supplied properties | Cookies, localStorage, sessionStorage, session replay |

Visitor counts use site-scoped pseudonyms and remain estimates. Cookie-free does not by itself establish a consent exemption. Do not send personal information or secrets in custom fields.

## Hosted or self-hosted?

|  | Hosted OwlEye | This self-hosted distribution |
| --- | --- | --- |
| Operations | OwlEye runs the service | You manage servers, updates, backups, and monitoring |
| Cost | Free and paid [plans](https://owleye.dev/pricing/) | MIT software; you pay infrastructure costs |
| Data retention | Plan-based hosted retention | Your deployment's configured policy |
| AI and billing | Managed service capabilities and plan eligibility | Do not assume hosted features or provider accounts are included |

One Rust server serves the Nuxt Console at `/` and APIs at `/v1`. SQLite stores accounts and configuration; ClickHouse stores analytics. This monorepo contains the API, Console, and SDK. Hosted OwlEye is operated from a separate service repository.

## Run your own instance

- [Self-hosting guide](docs/SELF_HOSTING.md): prerequisites, machine sizing, setup, redeploy, and update.
- [SDK usage guide](packages/analytics/README.md): npm and CDN setup, JavaScript globals, manual events, campaigns, rules, performance, and the 10-property contract.
- [Browser storage and visitor identification](packages/analytics/README.md#visitor-identification-and-browser-storage).
- [SDK release checklist](docs/SELF_HOSTING.md#publishing-the-sdk).

```sh
./setup.sh       # first installation and administrator account
./redeploy.sh    # build and restart using existing configuration
./update.sh      # git pull --ff-only, build, and restart
```

Recommended minimum: **4 vCPU, 8 GB RAM, and 20 GB free storage**. This is guidance, not an enforced restriction. Install Node.js 26.2+, pnpm 10.28.1, Rust 1.95+ with Cargo, ClickHouse, and SQLite before setup. Caddy is optional. Follow the [prerequisite installation links and commands](docs/SELF_HOSTING.md#installing-prerequisites), then run `./setup.sh --check`. Running setup again refuses to overwrite an existing installation; use redeploy to rebuild and restart.

Licensed under [MIT](LICENSE).

Maintained by **Shrinath** · [hello@shrinath.me](mailto:hello@shrinath.me) · [owleye.dev](https://owleye.dev).

Third-party dependencies and bundled assets retain their respective licenses, including the [amCharts map data](apps/console/public/maps/LICENSE.amcharts).

User-Agent detection includes [ua-parser rules](apps/api/vendor/uap-core/README.md), licensed under Apache-2.0.
