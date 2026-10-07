# Changelog

## 1.0.2

Patch release of `@owleye/analytics`, compared with the published npm `1.0.1` package. No public API, configuration, or event-schema changes are required.

### Browser identification fixes

- Normalize Chrome, Google Chrome, and Chromium identities to `Chrome`; normalize Edge to `Edge` instead of `Edg` or a client-hint brand string.
- Prefer recognized browser brands and explicit product tokens over generic Chrome/Chromium engine hints. This prevents Edge, Opera, Samsung Internet, and identifiable browser forks from being grouped under their engine.
- Recognize explicit identities for Arc/Arc Search, Comet, Dia, Zen, Floorp, Brave, Vivaldi, UC Browser, Headless Chrome, and Headless Edge. These rules apply only when the browser supplies a matching token or supported client-hint brand.
- Recognize Chrome (`CriOS`), Firefox (`FxiOS`), Edge (`EdgiOS`/`EdgA`), and Opera (`OPiOS`) mobile tokens, plus versionless Brave and ArcMobile2 tokens.
- When Brave is identified solely through the existing `navigator.brave.isBrave` API-presence check, omit its unknown product version instead of assigning the Chromium engine version. The detector does not call or await `isBrave()`.
- Prefer the matching UA product version to a generic Chrome/Chromium hint version when available.

### Release maintenance

- Add browser regression tests covering canonical names, 22 explicit-token/fallback fixtures, and 12 client-hint aliases in both brand orders.
- Adjust affected gzip budgets by 100 bytes for the expanded detection rules.
- Clarify the existing keepalive transport comment; delivery behavior is unchanged.
- Refresh version-pinned installation/CDN examples and document detection limits.

### Detection limits and compatibility

Arc, Comet, Dia, Brave, and other forks can expose the same UA as Chrome. Without a distinguishing supported hint, token, or the existing Brave API-presence signal, they remain `Chrome`. Synthetic identity fixtures verify the rules, not that every shipping browser exposes those identities. The SDK does not add fingerprinting probes or high-entropy client-hint requests.

OS, device, crawler/AI-agent, and IP geolocation parsing remain backend responsibilities. This release does not merge historical analytics rows or change sessions, page-view counting, privacy defaults, storage, or transport behavior. There are no new runtime dependencies.

### Publishing

Publish from the public `shrinathprabhu/owleye` repository after committing the SDK changes there. Its existing release workflow accepts `v1.0.2` or `@owleye/analytics@1.0.2`, then rebuilds, tests, checks bundle sizes, inspects the package, and publishes with npm provenance. Preparing these files does not publish the package or create a tag/release.

Repository package versions and current installation snippets are aligned to `1.0.2`. Hosted frontends pin the published npm package `@owleye/analytics@1.0.2`, including its bundled JavaScript and declarations. Their builds do not require compiling the local SDK workspace.
