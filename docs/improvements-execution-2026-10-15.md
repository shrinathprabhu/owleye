# OwlEye self-hosted improvements: implementation and review record

> 16 October update: the newly authorized SDK 1.0.3 release supersedes this document's opt-in three-tag campaign policy and earlier no-SDK-release assessment. Five bounded UTMs are automatic; arbitrary query/fragment capture remains opt-in. See `packages/analytics/CHANGELOG.md`.

**Date:** 15 October 2026 · **Branch:** `chore/improvements` · **State:** uncommitted.

No package publication, deployment, remote CI or production performance result is implied by local tests.

This follows [the public remaining-improvements plan](remaining-improvements-implementation-plan-2026-10-15.md) and preserves [the earlier implementation baseline](improvements-implementation-2026-10-14.md). Existing self-hosted features are retained. These changes introduce no cloud login, subscription, price or AI-credit gates into public. Existing local authorization/security and bounded resource limits remain.

## Shared core delivered

- Text-token syntax highlighting for JavaScript, HTML, shell and plain text, rendered safely without `v-html` or a heavyweight runtime dependency. Optional performance/rules setup uses the same themed components as Overview and offers copyable coding-assistant instructions. Public API-base/same-origin setup remains self-host appropriate.
- Memory-only response caching, shared in-flight reads and quiet background refresh. At most four default reports for the current authorized app are warmed at idle, with two concurrent requests and cancellation on scope changes. No all-app/API flood, AI/model work, secrets or writes on page entry. Hidden/data-saving contexts pause background work. Auth/site/permission changes and mutations invalidate the bounded cache.
- Read-only bounded ClickHouse reporting: two shared execution slots, bounded admission/queue, 5 seconds per query, 5M input rows, 256MiB input bytes, 96MiB aggregation, one thread, 2,000 output rows/512KiB streamed output and 8-second HTTP timeout. Existing operational readers are separate. The Overview still performs several bounded scans; it is not a single-scan optimization or proof of target-hardware throughput.
- One authorized request shares a maximum 20M input rows / 1GiB scan bytes and 8,000 result rows / 2MiB streamed output across its declared statement slots. Scan ceilings are divided before execution, failures/cancellation do not refund work, nested same-site scopes cannot reset the budget, and cross-site/spawn escapes refuse. Eight global admitted statements, two per site and FIFO two-slot execution prevent one site filling the global queue; request child concurrency is two. Safe retryable 503 responses explain resource refusals. Existing public AI only gains these resource protections, without new cloud AI features or gates.
- Versioned measurement metadata, UTC/as-of/grace, unknown retention/coverage and late-data caveats. Optional cohort/acquisition failures do not turn into zero activity or replace legacy totals.
- Exclusive channel groups, controlled UTM source/medium categories and protected deduplicated Other; existing referrer/campaign presentation remains. No new SDK collection.
- Closed fixed 30-minute pageview cohort estimates: eligible counts, single-page bounce, first-to-last observed span with sample/median context, entry and last-observed pages. Open/query-boundary buckets are excluded when detectable. There is no heartbeat, persistent identity, exact attention measure or inferred final-page dwell time.
- Versioned crawler populations: legacy Overview totals/series and saved funnel v1 keep their existing all-agent population; new cohort/channel reports and funnel v2 exclude stored known crawlers. Unknown agents stay included and are not verified humans. UI/API/export labels distinguish these populations. The full legacy Overview migration remains additional work.
- Funnel v2 coherent attempts, later valid attempts, exact/ordered selection and timing, with bounded work and fail-closed overflow. V1 matching semantics remain available. Five distinct visitors per releasable stage are mandatory in both versions; a large number of sessions from one visitor cannot bypass privacy.
- Web Vitals rating distributions use actual sample counts with whole-distribution withholding when any nonempty band lacks five known distinct visitors. Bounded privacy-floor aggregates avoid retaining unnecessary exact visitor sets. Empty/unknown data is not shown as zero; percentile gauges remain threshold presentations, not distribution estimates.

## Edition boundary and remaining work

| IDs / feature | Public status |
| --- | --- |
| A01/A04/B05–B07 | Initial cohort metrics, series and vital distribution work delivered. Independent preceding-period Overview series/sparkline comparisons remain. |
| A02/A03/A07–A09 | Existing geography, funnel presentation, input/OTP primitives, static exports and setup guidance preserved. |
| A05/A06 | Full shared custom-date/filter navigation and durable preset application remain. Existing report APIs use different calendar/trailing-window meanings; labels are not silently treated as equivalent dates. |
| B01 | New report policy delivered; a versioned opt-in migration of legacy Overview and downstream share/cache/developer contracts remains. |
| B02/B03 | Funnel correctness/privacy and channel core delivered. Advanced source-at-entry segmentation is a private addition in this batch; no existing public capability is removed. |
| B04 | Additional protected audience dimensions/filter combinations remain. No new fingerprinting fields or unrestricted city combinations. |
| B08–B10/B15–B17 | New goals/context/property/journey/health/advanced acquisition workspace remains private in this batch. Existing public reports/rules/funnels are preserved. |
| B11 | Existing aggregate activity behavior remains; no raw visitor/session stream is introduced. |
| B12–B14/B21/B23 | Existing explicit SDK APIs and optional collector recipes remain. Automatic outbound capture, extra UTM fields/observers and broader recipes require separate size/privacy/performance tests. |
| B18/B19 | Durable summaries/alerts and protected ingestion-time internal exclusions remain separate work. Public external delivery needs an operator transport decision, preferences/idempotency/retry/deletion controls; no hosted mail dependency or per-ingest decrypt/query loop is imported. |
| B20/B22/B24–B26 | Bot adapter, aggregate merchant connector/import, rule publication revisions/helper, SEO connector, multi-site and historical import remain independently scoped expansions. No hosted credentials, buyer joins or source-revenue guesses are introduced. |
| B27 | Minimum metric/policy context delivered; complete durable historical comparability timeline remains. |
| B28 | Proxy pilot remains conditional on real need and relay-authentication/spoofing/replay/metadata/outage/latency tests. No generic forwarding or guaranteed blocker-bypass claim. |
| C01/C02 | New provisional source-change preview and explicit post registry remain private. Public does not guess an X/Reddit/LinkedIn post from an origin-only referrer. |
| C03–C06 | Forecast/money/returning/coverage estimates await adequate permitted data, independent denominators and held-out evaluation. No longer-lived identifiers or near-perfect accuracy claim. |
| Reusable widgets | Existing static exports remain. New scoped expiring live embed/API access needs a separate revocation/cache/framing/abuse contract. No API secret is included in public export code. |
| AI expansion / abuse sanctions | The consolidated AI expansion, encrypted incident/block/operator workflow and new private Growth work are private only. Existing public AI files/features are not replaced or gated by this batch. |

## SDK and storage

SDK **1.0.2 remains the published installation target; no SDK release is needed** for these Console/API changes. The unnecessary prepared version bump and changelog entry were removed at the user’s request. Package metadata, install/CDN snippets and coding-assistant instructions target 1.0.2; runtime source, collection defaults, public APIs, payloads, dependencies and byte budgets remain unchanged. No tag, commit, publication or frontend dependency/lockfile upgrade is needed.

No new public control-plane tables or ClickHouse fact columns are needed for this shared core. Private workspace/security migrations are not copied into self-hosted. Resource limits are operational safeguards rather than pricing restrictions.

## Screenshot feedback follow-up

Both editions share the Web Vitals-style selected-card border/offset ink shadow. Numeric fields retain native validation/keyboard editing with spinner arrows hidden. Profile forms use natural grid rows rather than masonry measurements, preventing password/authenticator/account sections or async errors from overlapping. Globe selection has a thin subdued border. Web Vitals uses the existing OwlEye lime/violet/coral and chart palette, with readable state labels; no new palette or UI dependency was added.

Traffic defaults to all three metrics. Cards isolate one series, comparison toggles and Show all support every nonempty one/two/three combination, and the descriptive legend cannot diverge from those controls. SVG, PNG and static embed code capture the same selected chart, axes, formatter and zoom. Header totals remain full-range API totals and are labelled accordingly; range visitors are never summed across buckets. Export captures chart/header state before lazy rendering so concurrent selection/navigation cannot mix them. No live endpoint or secret is embedded.

Engagement uses the same dark grid surface and multi-selection controls, with independently labelled count, percentage and duration small multiples. Areas fill contiguous observed runs independently; unknown gaps and isolated observations do not invent filled measurements. Bounce stays on a 0–100% scale.

## Acceptance evidence

Node 26.2.0 and existing installed dependencies were used. This implementation run performed no installation, staging, commit, tag, package publication or deployment. The existing checkout HEAD at final review is `b7f8f7a`; it was preserved, and these changes remain unstaged above it.

| Gate | Result |
| --- | --- |
| Final Console tests / typecheck / production build | 79 tests passed; typecheck passed; static build passed for 20 routes after the final screenshot/area-fill changes. |
| SDK tests / size / release inspection | Rebuilt 1.0.2: 72 tests passed; unchanged byte budgets, matching-tag release check and local package dry-run passed. No new SDK release is required. |
| Rust formatting / strict clippy | Passed workspace formatting and all-target clippy with warnings denied. |
| Rust workspace tests | 175 passed; 26 opt-in tests ignored. |
| Final explicit ClickHouse fixtures | 14 selected fixtures passed, covering bounded readers, populations/cohorts/channels, coherent funnel attempts and actual vital samples/privacy. Earlier targeted module runs are in the logs. |
| Incremental Web Vitals checks | Five focused insights tests passed, including two ClickHouse fixtures covering actual sample bands, 4/5/6 visitors, unknown-only and mixed known/unknown sparse bands. |
| Existing public AI offline checks | Two passed; only resource protection was changed, not AI capability. |
| Script tests | Four passed; one optional platform/deployment test skipped under its default opt-in contract. |
| Final diff whitespace check | Passed; changes remain unstaged on `chore/improvements`. |

The final backend follow-up includes eleven new focused resource regressions per edition. Failed, oversized, malformed or interrupted output is charged/exhausts the shared request budget before it can be caught and retried. Its logs and exact commands are `/tmp/owleye-report-budget-final-ci-20261015/public/summary.json` and `results.jsonl`. Client cancellation releases local permits and requests server cancellation, without claiming an acknowledged database kill.

The initial sandboxed Rust run could not bind local mock TCP listeners; the loopback-enabled rerun passed. The disposable macOS ClickHouse 26.6.2.81 fixture required request-only JIT flags to avoid its `_bzero` symbol failure. No production query settings were changed. These fixture runs test SQL semantics, not target-host throughput.

After the Mac was unlocked, root inspected the built public Console in the actual browser using synthetic local data. Overview fit a 390px viewport without horizontal overflow, including its themed range menu. Web Vitals showed actual rating-band samples and themed npm/CDN highlighted setup; the copy action showed a success state, but isolated-browser clipboard payload delivery was not established. Synthetic sign-in exercised six themed OTP slots, numeric mode and one-time-code autocomplete, then returned to Overview after the fake code. No real credentials or account data was used. Browser viewport overrides were reset.

Screenshots: `/tmp/owleye-ui-review-20261015/public-otp.png`, `public-performance-setup.png` and `public-overview-mobile.png`. Local command/evidence summary: `/tmp/owleye-public-final-ci-20261015/validation-summary.md` with final `freeze-console-*` logs. Root's rendered observations supersede that subagent log's earlier browser-unavailable note. Temporary local fixture logs/screenshots are workstation evidence, not remote CI.

Request-log inspection showed bounded default warming and no AI/history/secret/live polling loop. Cache and queue tests cover scope invalidation, cancellation, joined foreground/background requests and a maximum of two concurrent warm reads. This does not establish total heap or server capacity.

**Not established:** remote CI, container-image validation, paid-provider behavior, production load/memory, every responsive report combination or deployment. The existing static-build large-chunk warning remains. The requested small API/SQLite and ClickHouse host sizing has not been benchmarked or deployed. Remaining and private-only work is recorded above; the entire roadmap is not claimed complete.

### Final screenshot follow-up evidence

The final source passed **79 Console tests**, typecheck and the 20-route static build: `/tmp/owleye-public-console-feedback-area-final.log`. Earlier 73/78-test snapshots are superseded. Real ECharts SSR tests cover every nonempty one/two/three-series selection, hidden/stale series exclusion, zoom/axes/formatter fidelity, static output safety and export state races. The shared three-series browser SVG export was parsed with all selected labels, correct API totals and no script/external links; PNG and clipboard delivery were not separately asserted in the browser.

The final public build was reloaded in the real browser with synthetic data. Profile/password/authenticator/account cards do not overlap at 1280px and stack at 390px without overflow. Overview defaults to all three; clicking Page views isolated it and the compare toggle added Estimated visitors, with matching table/chart labels. Globe country selection and the final dark engagement area panel were inspected. Number spinners, shared card selection and Web Vitals palette use the same verified shared changes as private.

Screenshots under `/tmp/owleye-ui-review-20261015/`: `public-profile-fixed.png`, `public-profile-mobile-fixed.png`, `public-two-series-mobile.png`, `public-globe-highlight-fixed.png` and `public-engagement-theme-fixed.png`. Synthetic local previews at 6173/6175 remain open for the user's continued review; they do not query production. The disposable ClickHouse fixture was stopped after all backend gates passed. No final source changes were staged or committed.

### SDK version correction and final rebuild

At the user's request, the unnecessary prepared SDK bump was removed. The manifest and changelog match the existing 1.0.2 baseline; README/install/CDN examples, assistant prompts and their assertions target published 1.0.2. SDK runtime sources, dependencies, lockfiles and byte budgets are unchanged. No package publication, release tag or dependency upgrade is required for this Console/API work.

Fresh local validation after that correction passed: **72 SDK tests**, all existing size budgets, `RELEASE_TAG=v1.0.2 node scripts/check-sdk-release.mjs`, local `npm pack --dry-run --ignore-scripts`, **79 Console tests**, typecheck and the 20-route production static build. Built SDK integrity metadata reports 1.0.2; regenerated Console assets contain the 1.0.2 install/reference targets. The existing synthetic preview at 6175 serves this rebuilt output and remains available for review after reload.

Logs: `/tmp/owleye-public-sdk-102-revert-test.log`, `-size.log`, `-release.log` and `-pack.json`; Console logs use `/tmp/owleye-public-console-sdk-102-{test,typecheck,build}.log`. The existing nonfatal duplicate `ChartSnapshot` auto-import warning and static-build warnings remain; this correction does not rename unrelated UI types. Remote CI and publication were not run.

## 16 October: automatic campaigns and compact funnels

The newly authorized SDK **1.0.3** patch supersedes the earlier no-release assessment for UI-only work. This update is implemented in both sibling repositories; it is not committed, tagged, published, or deployed.

- Core, rules, performance, and full SDK builds automatically capture only the five bounded UTM fields: source, medium, campaign, term, and content. No campaign enablement option is needed. Legacy `captureCampaigns` flags/attributes are ignored, including `false`; a deprecated no-op TypeScript field keeps existing source compatible. This deliberately changes the collection default.
- Arbitrary query/fragment capture remains an explicit opt-in. GPC and consent gating remain unchanged. Automatic capture uses bounded work, UTF-8 limits, first-key precedence, and recognizable sensitive-value checks; these checks do not make arbitrary user text safe. There is no campaign browser storage or persistent first-touch attribution.
- API extraction and ClickHouse insertion now populate the existing `utm_term` and `utm_content` compatibility columns. Optional campaign definition fields are added through SQLite schema **9**, with empty defaults for existing definitions. No historical attribution is invented. Startup migrations are additive; no ClickHouse backfill, reindex, materialized view, or manual optimization is required for this patch.
- Campaign link generation and reports include optional keyword/content variants. One bounded SQL report provides independently deduplicated totals, campaign rows, and up to 20 released variants per campaign (200 per response). Variants require five distinct visitors. Existing tenant, retention, read-memory, and query admission limits remain enforced. Variant visitor counts must not be summed as people.
- A Detailed/Compact toggle in the shared funnel renderer uses the existing lime/violet/coral palette and rounded bars. Detailed is the default; both views use the same server counts and accessible table. Toggling does not fetch data.
- All newly proposed `ref`/`referrer` URL hint capture, declared-referral storage/reporting, API fields, and UI were removed at the user's request. The existing observed browser referrer remains. Unknown query parameters are excluded by default, as before.
- Current SDK installation/CDN examples, Console coding-assistant prompts, privacy wording, and release instructions have been updated. Historical plans carry a superseding note rather than rewriting past implementation results.

### Local verification

| Gate | Result |
| --- | --- |
| SDK builds and tests | 78 passed in each repo, including strict Bundler/NodeNext declarations |
| SDK size and tree-shaking | Passed; new CDN validation logic adds 332–352 gzip bytes, with a strict 400-byte incremental allowance and no dependencies or extra requests |
| Console tests | 93 passed |
| Console typecheck / production build | Passed in both repos, with fresh Nuxt types |
| Rust workspace tests | 181 API tests passed; existing opt-in tests are separate |
| Strict workspace Clippy / rustfmt | Passed in both repos |
| ClickHouse integration / local counting | 27 passed; isolated official 26.6.2.81 server and synthetic fixtures |
| SDK release metadata / package inspection | Both source version checks and public npm pack dry-run passed for 1.0.3; no publication |
| Rendered verification | Both Console compact funnels and campaign forms/reports checked with synthetic local data; 390 px mobile funnel has no page overflow |
| Repository diff checks | Passed; private frozen install preserved its lockfile, public pre-existing changes preserved |

Private website production/LLM output checks and 37 Worker response checks passed, together with 21 Console Worker response checks. Private supply-chain policy and 44 policy tests passed. Public operations tests passed (6, with its provider/deployment fixture skipped by default). This is local validation, not a claim of a new GitHub CI run, production load test, live provider evaluation, or Docker image validation.

### Release ordering and remaining operations

1. Review and commit the changes manually in both repositories.
2. Deploy the matching API so startup applies the additive SQLite changes and verifies ClickHouse compatibility columns.
3. Publish SDK 1.0.3 from the public repository through the existing release workflow after the manual source review.
4. Update the private website and Console dependency pins from published 1.0.2 to 1.0.3, regenerate the lockfile, and rerun their checks. They intentionally remain installable at 1.0.2 while 1.0.3 is unpublished.
5. Deploy the updated docs, Console snippets, and hosted frontends with the published SDK. Review visitor notices because campaign collection now happens automatically.

No commits, release tags, npm publication, production database operations, or deployments were performed. Deferred URL referral hints are intentionally absent.
