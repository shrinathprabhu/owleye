# Improvements implementation ledger

Date: 14 October 2026. Branch: `chore/improvements` in both repositories.

This branch implements the first **Trust and polish** slice of the approved roadmap, plus Console response caching and static metric widgets. It does **not** implement the entire multi-release roadmap. The user's implementation authorization supersedes the proposal's old “documentation only” status. Category 4 remains excluded. Later features keep their definition, privacy, cost and rollout gates.

Private baseline: `98f842c` on local `main`. Public baseline: `16e71b9`, after switching to `main` and pulling with fast-forward only. The public baseline is newer than the proposal's inspected revision. Existing setup snippets, themed fields/selects and OTP behavior are preserved. No SDK runtime source, database schema or production query semantics were changed. Security validation required targeted Rust lockfile patches: Rustls 0.23.46 in both editions and Quinn-proto 0.11.18 in public. JavaScript dependency versions and audit exceptions are unchanged.

## Category 1: implemented and remaining

| ID | Status in both repositories | Implementation / remaining work |
| --- | --- | --- |
| A01 | Implemented | Pageviews, pageview-scoped estimated visitors and events have lightweight SVG sparklines. Selecting a card selects the main chart. Range visitors use the API's deduplicated total, never the sum of daily uniques. Missing samples break lines; zero remains zero. Sessions/bounce/span cards await their API contracts. |
| A02 | Implemented; browser validation outstanding | Accessible country bars sit beside the existing lazy D3/canvas globe. Pointer/focus/touch selects a country and rotates once along a short arc. Reduced motion skips the animation; offscreen/hidden work stops; listeners and frames are cleaned up. No hover fetch. Mobile puts the list first. The former separate ECharts ranking was removed. |
| A03 | Implemented presentation; browser validation outstanding | Saved funnels open results before configuration. Numbered stages show counts, previous-stage losses, total conversion and existing timings, with a table fallback. Advanced ordering/window settings are collapsed. New-funnel starters require choosing actual paths/events; they do not invent success signals. Existing saved definitions/counts remain unchanged. B02's first-entry limitation is disclosed. |
| A04 | Implemented presentation; browser validation outstanding | Estimated p75 cards show thresholds, rating and sample count, selected-metric p50/p75/p95 trends, core/supporting labels, and unitless CLS. Threshold bands are explicitly not audience distributions. Device splits and distribution shares are not fabricated from unavailable data. |
| A05 | Partial | Overview missing-referrer labels say “Direct / unknown.” Existing event names, grouping, last-seen information and empty/error states are retained. Cache-backed reports improve loading consistency. A unified cross-report date/filter controller and broader tab/navigation redesign remain unimplemented; existing reports have different supported ranges and filter contracts. |
| A06 | Deferred | No new shared-filter persistence. This needs validated site/user metadata and permissions; existing saved Pro Views remain available. The roadmap explicitly makes this optional in release 1 if small. |
| A07 | Implemented | Explicitly running the ignored schema test without its ClickHouse URL now fails with an actionable message. Ordinary test runs still mark it ignored. Similar database tests inspected already fail on missing configuration rather than report a silent pass. |
| A08 | Implemented | SDK and deployment documentation explain endpoint rewrites, original-address trust, distorted geo/cohort/rate-limit behavior, independent script/ingestion routing, cookie stripping and the lack of a guaranteed blocker bypass. No managed proxy was added. |
| A09 | Implemented | Release-artifact tests validate package exports, documented versioned CDN targets, integrity bytes/SHA-384 and source-map references. Existing complete-consumer and IIFE size gates remain intact. Source maps remain packaged; no visitor-transfer saving is claimed. |

## Additional request: cache and background refresh

Implemented for **Overview, Events, Campaigns, Web Vitals and AI status** in both editions. The existing workspace/session cache continues to own authentication and site membership refresh.

- One bounded cache per Nuxt application instance; no process-global cross-request response cache, localStorage, IndexedDB or service-worker storage of analytics.
- Keys include report, site and query context. Identity and site permission/role/entitlement changes clear snapshots. API writes invalidate at start and completion; 401/403 responses invalidate them too.
- Fresh snapshots (under 30 seconds) avoid another request. Stale snapshots (up to five minutes) render immediately while a request refreshes them. Older entries are not reused.
- At most 32 entries, a two-million-byte accounting budget for serialized UTF-16 keys/payloads, and at most 300 KB per entry. This is a cache accounting limit, not a claim about total browser heap. JSON copies prevent callers from modifying stored responses.
- Cancellation/site/range guards and cache generations prevent old responses from refilling invalidated snapshots. First-load and access failures remain honest. Transient refresh errors disclose the failure; authorized stale snapshots can stay visible.
- AI status includes the existing disabled/no-access state, so returning to that page need not first show a loading layout. A cached status is never server authorization to submit a prompt. Prompts/history/provider results are not newly cached by this change.
- This is not a blanket cache for every API. Auth refreshes, secret/key operations, settings, Pro View definitions/previews, live polling and mutation responses retain their existing behavior. Extending caching to additional editable resources needs their own invalidation and navigation tests. There is no server/CDN cache for authenticated API responses in this slice.
- Remote revocation and changes made by another client are discovered through existing workspace/API revalidation, not instantly pushed to an open tab. No claim of instantaneous cross-client invalidation is made.

## Additional request: reusable/shareable widgets

Implemented a static export for the selected Overview metric: **PNG, SVG, or copyable SVG embed code**. It includes site name, date range and metric label. It exports only the aggregate already visible to the authorized user, and has explicit share intent in the UI. The export is generated locally with escaped text and no script, remote asset, API key, tracking ID, cookies or background requests. Blob URLs are revoked. The browser-rendered download/copy flow remains to be verified because the local preview was blocked by the browser tool.

Live widgets remain a separate expansion. Reuse public-sharing access checks, but design a revocable, narrowly scoped widget capability selecting exact metrics and periods, with rate limits, expiry/rotation, privacy checks and an explicit framing policy. Do not paste Console credentials or a broad site API key into client code. Existing public-share frame restrictions are unchanged. Static SVG code is useful immediately for developer docs, READMEs and app layouts that accept sanitized SVG; a React/Vue SDK or npm widget package is not included.

## Category 2: not shipped in this slice

| ID | Status and next gate |
| --- | --- |
| B01 | Investigated, no totals changed. Known crawlers are classified during UA parsing; the inspected analytics query paths do not provide a new shared exclusion policy. Define/version a consistent population across all surfaces before changing counts. |
| B02 | Confirmed first-entry limitation. `step_1` uses the first qualifying sequence/time; later exact/window steps never restart. Reproduce `A, X, A, B` and expired-first/later-valid attempts with real database tests, select a qualifying attempt deterministically, benchmark, and version saved semantics before release. No SQL fix is represented as done. |
| B03 | Not implemented. Versioned channel precedence and campaign dimensions require matching filters/shares. |
| B04 | Not implemented. Expanded dimensions need sparse/residual protection and edition-specific column checks. |
| B05 | Not implemented. Bounce/entry/exit require complete, eligible fixed 30-minute pageview cohorts and boundary tests. |
| B06 | Not implemented. Observed span and visible-page segments remain distinct; no inferred session-time sum was added. |
| B07 | Not implemented. New series, comparable deltas and vital distributions require bounded queries and correct denominators. |
| B08 | Not implemented. Goals require additive metadata, bounded evaluation, permissions and conversion-population tests. |
| B09 | Not implemented. Annotations/public-post mappings require safe URLs, edit permissions and contextual labels. |
| B10 | Not implemented. Property summaries require existing redaction/catalog integration and cardinality/privacy bounds. |
| B11 | No new stream. Existing aggregate live analytics is preserved; no raw person/session activity feed added. |
| B12 | Measurement only via A09. No minifier change or new performance entrypoint; actual artifact changes need a separate SDK release gate. |
| B13 | Not implemented. Campaign capture extensions depend on value validation and SDK headroom. |
| B14 | Not implemented. Safe manual rules/events remain available; automatic outbound capture needs modular collection and quota decisions. |
| B15 | Not implemented. Funnel source comparisons depend on B02/B03 and entry attribution. |
| B16 | Not implemented. Aggregate paths require bounded chronological processing and rare-path/residual protection. |
| B17 | Not implemented as a new collection-health report. Existing setup guidance remains; absent data is not called a broken installation. |
| B18 | Not implemented. Durable weekly delivery and alerts are separate operational releases with edition-specific delivery decisions. |
| B19 | Not implemented. Internal exclusions need sensitive-config access controls, transient ingestion handling and effective dates. |
| B20 | Not implemented. A server/edge crawler integration is a separate optional collector, not a human-traffic denominator. |
| B21 | Deferred as specified. Reference-aligned vitals need lifecycle tests, versioned measurements, overhead evidence and an SDK release. |
| B22 | Not implemented. Revenue totals require a merchant-side minimized adapter/import, currency/refund reconciliation and coverage. No browser purchase joins added. |
| B23 | Not implemented as new recipes. Existing safe custom-event/rule setup is preserved; no text/form-value collection added. |
| B24 | Deferred as specified. Rule versions/rollback precede an authenticated selector helper. |
| B25 | Optional later expansion, not implemented. OAuth/token lifecycle, quotas and query privacy required. |
| B26 | Optional later expansion, not implemented. Cross-site permissions and one-format aggregate-import provenance required. |
| B27 | No new metric definition was shipped, so no numerical version boundary was introduced. Full definition history remains later work. |
| B28 | Deferred as specified. No demonstrated routing need or authenticated relay pilot has been established. |

## Category 3: not shipped

C01 spike explanations, C02 exact-post evidence, C03 forecasts/scenarios, C04 modeled revenue contribution, C05 returning-cohort estimates and C06 coverage estimates are all unimplemented. Their measurement prerequisites, privacy rules, uncertainty disclosure and validation gates remain in force. No optimistic estimates, guessed URLs, invented causes, attribution percentages or “near perfect” claims were added.

## Category 4: intentionally excluded

All D01–D13 exclusions in the approved plan remain excluded: identified profiles/payment joins, persistent visitor identity/fingerprinting, replay/text/DOM capture, sensitive URLs, precise location, public raw streams, privacy-choice bypasses, always-on FPS, experimental identity/social enrichment, fabricated accuracy/causation/coverage, and blanket privacy/legal claims.

## Validation and limits

Validation results are recorded below as local results, not remote CI or production proof. No branch has been pushed, deployed or released by this task. No SDK release is needed: only SDK documentation/tests changed, and runtime exports/artifacts are unchanged.

### Completed local checks

| Check | Private | Public |
| --- | --- | --- |
| Fresh typecheck | All three workspace tasks passed, including 46 website files with zero diagnostics | Console passed |
| Console tests | 73 passed | 60 passed |
| SDK tests, including artifact contracts | 72 passed | 72 passed |
| Production frontend builds | Console Worker, website Worker and SDK passed | Static Console passed |
| Built Worker response policy checks | Console: 21 responses; website: 36 responses passed | Not applicable to the public static host |
| SDK size gates | All passed | All passed |
| Rust formatting and Clippy, warnings denied | Passed after lockfile patches | Passed after lockfile patches |
| RustSec audit | Passed after Rustls patch | Passed after Rustls and Quinn patches |
| JavaScript audit | Passed under the repository's three existing documented high-severity exceptions | **Fails: 22 existing advisories** (3 critical, 13 high, 5 moderate, 1 low) |
| Supply-chain, repository-secret checks, email checks | Passed; email tests: 4 passed | No matching hosted email/supply-chain jobs |
| Public deployment/setup script suite | Not applicable | 4 passed; 1 explicitly opt-in integration test skipped because the test API binary was not supplied |

The public JavaScript findings include Nuxt development/build dependencies (`simple-git`, its argument parser, shell quoting and transitive packages), with a mixture of patchable, major-version and no-fix findings. They were present in the unchanged JavaScript lockfile. They are **not resolved or newly ignored by this branch**, and should be handled as a separate compatibility-tested dependency update. Private's existing audit exceptions are not evidence that those packages have no vulnerability.

The private account administration (12), email blocklist (2) and load-test seed (6) tests passed. Shell syntax checks passed. The cloud config-only preflight passed 85 checks and correctly rejected the two-replica negative fixture; this uses fake CI configuration and is not live launch evidence.

**Private deployment-script test limitation:** `scripts/release/test-redeploy-api.py` reported 18 subprocess timeout errors across 11 tests on this macOS host. An unrestricted rerun also timed out and was interrupted; an isolated fixture reproduced the 20-second timeout while still processing mock Git/preflight commands, before real service operations (all service commands are mocked). No assertion failure established an application regression, but this suite is **not locally passing** and needs a successful run on the supported Linux CI runner. Its timeouts/assertions were not relaxed.

### Measured SDK headroom

Fresh builds produce the same sizes in both editions. Values below are gzip KiB, rounded to two decimals; the existing gate uses exact bytes. No limits were raised.

| Artifact                          | Current / budget (KiB) |
| --------------------------------- | ---------------------- |
| Full IIFE                         | 12.74 / 12.79          |
| Index entry module                | 1.42 / 1.56            |
| Rules entry module                | 5.79 / 6.35            |
| Analytics IIFE                    | 7.30 / 7.32            |
| Rules IIFE                        | 10.95 / 11.04          |
| Performance IIFE                  | 6.91 / 6.93            |
| Complete ESM core consumer        | 6.90 / 6.93            |
| Complete ESM rules consumer       | 10.87 / 10.94          |
| Complete ESM performance consumer | 6.52 / 6.54            |

The smallest remaining margins are only a few dozen bytes. New collectors need measured budget decisions rather than assuming there is spare space. Dashboard-only UI is outside the visitor SDK bundle.

### Remaining verification boundaries

- Final locked Rust tests passed after the dependency patches: private 372 API + 2 CLI + 2 crawler-control tests (30 API and 1 corpus audit explicitly ignored); public 150 API tests (17 opt-in tests ignored). The deterministic offline AI cases are included in the private unit run.
- Isolated real ClickHouse reruns passed after the dependency patches: private 24 tests; public 16 tests. Paid live-provider tests and the operator-seeded range benchmark were deliberately excluded. The explicitly selected schema test was also verified to fail without its required environment variable. These checks exercise existing metrics/privacy semantics; they do not claim to fix B02.
- Browser preview failed with `ERR_BLOCKED_BY_CLIENT`. No desktop/mobile screenshot, keyboard walkthrough, reduced-motion walkthrough, or real download/clipboard result is claimed. Temporary preview routes and configuration were removed. Unit/type/build checks do not replace this acceptance gate.
- No before/after browser heap, frame-time, layout-shift, long-task or production-query latency benchmark was obtained. Bounded caching, short-lived globe animation and cleanup are implementation safeguards; **zero UX/performance regression is not yet proven**.
- Docker is unavailable locally. Container image builds/scans, Compose/Caddy checks and the exact pinned Linux ClickHouse image remain unverified here. Local isolated database tests use ClickHouse 26.2; private CI pins 26.6.2.81.
- No remote GitHub Actions run, deployment, live-provider/paid AI evaluation or production validation was performed. Existing metric SQL and AI evidence algorithms are unchanged.

### Rollout and rollback

Keep this slice separate from later definition/collection changes. Finish browser acceptance and the remaining CI/container gates before release. Validate navigation through each cached report, fresh/stale/expired/error states, logout/site/access changes, accessible country selection, narrow layouts and static export in both editions. A rollback can restore these Console changes and tests without reversing analytics data migrations, because this slice introduces none; retain or independently reapply the Rust security patches.
