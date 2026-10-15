# OwlEye self-hosted: remaining improvements implementation plan

**Date:** 15 October 2026  
**Status:** Planning only. No additional feature implemented by this document.  
**Source:** [Implementation ledger](improvements-implementation-2026-10-14.md), reviewed alongside the private edition's matching ledger and original item definitions.

## Scope and priorities

Preserve the current snippets, themed fields/OTP, dashboard polish, bounded Console memory cache and static PNG/SVG exports. Finish their browser/CI acceptance first. Implement remaining core analytics in small independent slices, then select optional collection and sharing expansions. The private repository has the detailed cross-edition plan under the same filename; this document is the standalone self-hosted scope.

The public branch is `chore/improvements`, based on `16e71b9` plus existing uncommitted work. Both ledgers currently contain the same unfinished IDs. Core analytics should share definitions and acceptance criteria across editions, with explicit audits of source differences. Preserve the public SDK + Console + Rust API architecture with SQLite/ClickHouse. Do not introduce private billing, organization models, mail services or hosted operations to achieve UI parity.

AI expansion remains subject to the user's separate manual approval of the AI plan. Analytics prerequisites listed here do not authorize implementing that expansion.

## Implementation sequence

| Phase | Remaining IDs | Deliverable / prerequisite |
| --- | --- | --- |
| P0 | Acceptance of implemented A01–A04, cache and exports | Rendered browser/accessibility/performance checks; resolve public dependency audit findings and outstanding CI/container checks |
| P1 | Reporting foundation, minimum B27, remaining A05 | Bounded authenticated report queries, definition versions, consistent supported date/filter context and cache keys |
| P2 | B01, B02, B03 | Consistent known-crawler population, correct later funnel attempts, channels and existing campaign dimensions |
| P3 | B05, B06, B07; remaining A01/A04 | Fixed-cohort bounce/entry/last-observed pages, observed span, matched comparisons, missing series and actual vital sample distributions |
| P4 | B08, B09, B10, B04, A06 | Goals, annotations/post mappings, property summaries, safe audience dimensions and filter presets |
| P5 | B15, B16, C01, evidence-first C02 | Source funnels, protected aggregate journeys and selective spike explanations with supported links |
| P6 | B17, B11, B23, B19, B18 | Collection health, aggregate recent activity, safe recipes, internal exclusions; summaries and alerts in separate releases |
| P7 | B12, B13, B14, optional B21 | Measured SDK headroom, safe opt-in campaign fields, outbound reports/module, optional improved vital collection |
| P8 | B22, live widgets; optional B20/B24/B25/B26/full B27 | One independently supportable expansion at a time |
| P9 | C03–C06, optional B28/research | Evaluated aggregate predictions and conditional investigations; no guaranteed shipment if their data/privacy gate fails |

No runtime SDK release is needed for initial report/UI changes. Runtime/packaging changes in P7 require a package/CDN/SRI release. Existing A07/A08/A09 are retained, not rebuilt as missing features.

## P0/P1: foundations before heavier reports

1. Verify current UI in a working browser: desktop/mobile, keyboard/touch, reduced motion, globe cleanup, funnel controls, vital units, static download/copy, forms and OTP/recovery. Exercise zero, unavailable, withheld, error and stale states. The ledger does not establish that these browser gates passed.
2. Recheck cache navigation, fresh/stale/expired behavior, in-flight deduplication, writes, logout/site/access changes and late responses. Measure repeated navigation for jumps and retained memory. Cached AI status never grants execution access.
3. Resolve public dependency advisories with tested updates and finish applicable Linux/container/pinned ClickHouse/remote CI gates. Do not rename unrun checks as passing.
4. Measure representative query latency, scans, peak memory, response bytes, concurrent fan-out and browser transfer/long tasks/heap before setting new workload budgets.
5. Add a typed authenticated-report executor with bound site/range/filter inputs, allowlisted server-compiled SELECTs, read-only execution, scan/time/memory/output/concurrency limits and capped streaming responses. Accept no caller-supplied SQL. Existing AI/public-report caps do not automatically cover generic `query_json_each_row` reads. Do not apply AI's 400-row output shape to unrelated operational reads. Fail visibly on limits rather than return truncated totals.
6. Migrate report families incrementally; Overview currently fans out into twelve SQL queries. Batch compatible scans only after before/after cost evidence. Bound total work and tenant fairness, and verify database cancellation separately from browser abort.
7. Add minimum B27 definition/population versions, collection-policy effective dates and comparability markers in additive metadata. Include applicable versions, range/timezone, as-of and provisional status in report/cache contracts. Full history UI can follow.
8. Complete A05 through shared supported date/filter context and per-report capabilities. Unsupported filters must be visibly unavailable or explicitly cleared. Keep UTC semantics until timezone-aware bucketing is actually supported. Port public/private filter and property-suppression contracts deliberately before extending reports.

Keep the existing bounded memory cache for read reports; do not persist analytics in browser storage. New editable resources need write invalidation. Preserve the live endpoint's existing short-lived bounded server cache and authorization-before-cache checks. Additional server caching follows measured need: bounded coalescing first, short-lived authorized aggregates only after site/access/query/version/deletion/retention keys and invalidation are designed. TTL alone does not promise instant revocation/erasure. Do not add a new infrastructure service solely for caching or publicly cache authenticated responses through a CDN.

## P2/P3: definitions and metric correctness

| ID | Implementation | Acceptance |
| --- | --- | --- |
| B01 | Shared known-crawler treatment using existing classification, applied across human reports, shares/exports and evidence. | Version changed counts; retain unknown agents and separate crawler reports. UA recognition is not complete bot detection. |
| B02 | Versioned later-attempt evaluation and deterministic attempt selection for exact/ordered modes, with coherent counts/timings. Specify passive/historical performance scope and preserve intentional independent-stage semantics in open mode. | Real database tests for `A, unrelated, A, B`, expired-first/later-valid, duplicates, ties, repeat steps, boundaries/privacy, plus open-mode later stages absent A/out-of-order. Do not force open mode into closed semantics or show unsupported drop-offs/timings. Preserve v1 meaning; intentional v2 upgrade. Benchmark each mode. |
| B03 | Mutually exclusive versioned channel precedence; separate source/medium/campaign reports using existing fields. | Counts and filters reconcile. UTM labels do not prove paid traffic; AI referrals differ from crawlers/mentions. Direct / unknown stays honest. |
| B05 | Complete-context eligible fixed 30-minute cohorts; estimated single-page bounce, entry and last-observed page. | Group before entry filters; closed buckets/grace/as-of and retained boundary context. Zero denominator unavailable. Custom/vital/duration events do not automatically cancel a single-page bounce. |
| B06 | Observed first-to-last eligible activity span, with sample/coverage; keep visible-page segments separate. | One observation is unknown; no duration-packet sum claimed as real session time. Test overlapping tabs, boundaries and late data. |
| B07 | Bounded current/prior totals and series, equal completed portions, percentage-point rate deltas; actual vital sample buckets/device splits. | Deduplicated range visitors, no sum of bucket uniques or averaged daily p75s. CLS unitless, missing/withheld distinct, sample distributions not visitor percentages. Extend A01/A04 only once APIs exist. |

Closed cohorts remain observations that late data can revise. Existing fixed buckets must not be described as rolling inactivity sessions. Support timezone/DST consistently or visibly decline unsupported combinations.

## P4/P5: goals and acquisition insight

| ID | Small implementation slice | API/DB and release gate |
| --- | --- | --- |
| B08 | Goal CRUD/revisions, page/event/rule conditions, occurrences versus converting cohorts, selected trends. | Additive SQLite goal metadata (not compatibility-reserved tables) and batched bounded evaluation. Same-population denominators, real site success templates, repeat-fire/privacy/permissions/deletion tests. |
| B09 | Plain-text notes and campaign-code → public-post mappings. | SQLite CRUD, safe bounded public URLs, explicit access and provenance; no URL fetching or causal claims. Can follow P1 independently. |
| B10 | Approved event-property categorical cards, count/share/missing coverage/Other. | Extend bounded reports; redact unsafe legacy values, cap cardinality and prevent sparse/residual/comparison leakage across exports/shares. Public suppression parity is prerequisite. |
| B04 | Existing country/region/coarse device and coarse screen/viewport-size buckets first; language/timezone then separately gated cities. | Check per-edition columns, protect small groups/combinations, preserve unknown/unsupported and safe Other. No raw high-cardinality size combinations or new fingerprinting inputs. |
| A06 | Validated user/site report presets. | Additive SQLite definitions, not result rows or people. Revalidate permissions/filters on apply; one coherent refresh with write invalidation. |
| B15 | Selected entry-source/campaign funnel comparisons. | B02/B03 first; bounded categories, same selected attempt for attribution/timings. No later-referrer reassignment or eager funnel × source fan-out. Do not sum overlapping distinct visitors. |
| B16 | Three-step aggregate paths to/from one selected page/goal; repeated pages collapsed. | Bound scans/sorts/per-cohort input; stable timestamp/event-ID order, rare/residual suppression, table fallback. Output `LIMIT` alone is insufficient. No individual lookup. |
| C01 | Deterministic pageview spikes with zero to three independently unusual material contributors. | B01/B03/B07/B09/minimum B17/B27; pull basic health forward from P6. Completed comparable buckets, absolute/relative excess, one acquisition partition per event. Suppress sparse distinct cohorts before exposing/ranking sources; reconcile source expectations with the site baseline. Keep safe Other/unexplained. No forced filling. Evaluate false positives/misses before alerts; no hover recomputation or mandatory LLM. |
| C02 | Link actual safe observed evidence or owner campaign-post mappings. | Label provenance and owner judgment. Origin-only referrers cannot identify a post. Optional outside research is a separate opt-in/provider-verified scope, with no visitor/private-report context sent externally. |

## P6/P7: health and optional collection

| ID | Implementation | Boundary |
| --- | --- | --- |
| B17 | Last query-visible activity, counts, setup guidance and existing SDK name/version coverage; received/persisted-stage counters only where separately measured. | Both editions already store `received_at`, `sdk_name` and `sdk_version`; no SDK change for initial coverage. Unknown versions stay unknown. No blocker probes or absent-data installation verdict. |
| B11 | Improve coarse recent event/page presentation by reusing existing live aggregates, cache and bounded polling. | Preserve authorization/privacy/concurrency controls. Raw debug/person/session streams are outside this recommended scope; public sharing remains aggregate. |
| B23 | Static submit-success, section-viewed, search-used/zero-results recipes using existing SDK/rules. | No form values/search text/DOM capture. Actual success signal required. |
| B19 | New protected site exclusions evaluated transiently at ingestion, aggregate counts and effective dates. | Do not reuse compatibility-reserved `site_exclusions`. No raw-IP analytics; protect/delete sensitive rule configuration, preserve proxy trust and decide metering. No browser exclusion identifier. |
| B18a | In-console/on-demand weekly summary first. | Durable external delivery requires explicit operator-configured transport, job/outbox, authorization, idempotency, preferences and unsubscribe. No mandatory hosted email dependency. |
| B18b | Threshold alerts as a separate release. | B18a delivery durability, cooldown/recovery/sample/outage handling; anomaly alerts additionally require evaluated C01. |
| B12 | Measured minifier/packaging/optional span-entrypoint experiments. | Preserve current budgets, transport, lifecycle, exports/maps/SRI. Existing artifacts have very little headroom. No assumed savings. |
| B13 | Opt-in owner-defined campaign/placement codes. | Validate values as well as keys; exclude IDs/emails/tokens/free-form text. Existing dormant `utm_content`/`utm_term` columns need actual SDK/API row population. Add other columns only as needed, no invented historical values. |
| B14 | Explicit safe rule/event outbound reports first; optional delegated-listener module later. | Domain default; only configured safe public paths/static labels. No raw text/query/private/mail/tel content. Bounded transport/teardown, owned-domain and quota decision. |
| B21 | Optional reference-aligned vitals collector. | Version estimated/new measurements separately; lifecycle/BFCache/observer tests and measured size/runtime cost. No FPS. |

All SDK changes retain opt-in/privacy controls and mixed-version API compatibility. Build/test/size-gate and release changed artifacts separately from Console work.

## P8/P9: optional expansions and conditional feasibility

| ID / request | Smallest feasible deliverable | Prerequisite / stopping condition |
| --- | --- | --- |
| B22 | Merchant-side minimized site/day/currency money and transaction/refund aggregate import, reconciliation and traffic overlay. | SQLite manifests/config plus dedicated ClickHouse aggregate facts, integer minor units/currency precision and explicit latest-revision reads. Idempotent corrections, separate currencies, net/refund/tax/timezone/coverage definitions. No customer/order/visitor joins. Native connectors later. |
| Live widgets | Narrow published metric/range capability, read-only aggregate endpoint and one safe iframe/SVG embed mode. | SQLite hashed capability/config, expiry/revoke/rotation, auth/publication checks before caches, privacy limits/rate limits/CSP. Dedicated embed route only; preserve Console framing protection. Never expose Console credentials or broad site keys. Explicit sharing preview and token/cache/deletion tests. Static exports already exist. |
| B20 | Optional minimized server/edge known-crawler adapter and separate aggregate dataset/report. | Scoped authenticated ingestion, identity-verification labels and bounded replay/dedup/transport. No visitor headers or claims about training/answers. Not a human-coverage denominator. |
| B24 | Immutable rules, publish/rollback as a new revision, then explicit authenticated selector helper. | Optimistic concurrency/cache invalidation. No default visitor bundle overhead, DOM upload/input capture or unrestricted secret-bearing helper. Preview states actual version/data. |
| B25 | One independently configured Search Console connector/import. | Verify provider/OAuth/API contracts when selected; protected credentials, revoke, quotas, delayed/safe aggregates. No query-to-visitor joins. |
| B26a | Authorized selected-site aggregate overview. | Per-site permissions, comparable definitions, bounded fan-out and partial failures. Do not sum site visitors as people. |
| B26b | One aggregate historical import format. | Separate imported ClickHouse facts, SQLite manifests, provenance/idempotency/erasure and visible definition/timezone boundary. No reconstructed journeys/funnels. |
| B27 full | Read-only definition/collection history UI. | Minimum version metadata is already required in P1. |
| C03 | One projected remainder, seasonal forecast or prior-rate goal scenario. | B07/minimum B17/B27, goals when applicable, comparable retained history, definition breaks, rolling holdout/baseline evaluation and uncertainty; abstain on inadequate data/outages. |
| C04 | Aggregate revenue scenario/model. | Reconciled B22, held-out evaluation and identifiable effects. Preserve modeled/unattributed result; otherwise no result. No observed source revenue/ROAS or buyer matching. |
| C05 | Same-cohort timing first; only permitted existing-window aggregate returning/retention estimates later. | Audit hash stability/retention, protect sparse groups and right-censoring. First observed is not first ever. No longer identity life or reconstructed people. Unsupported windows remain unsupported. |
| C06 | Comparable independent server/edge pageview aggregate ratio; optional transparent engagement formula separately. | Align bots/routes/SPA/cache/retries/time. B20 bot counts are insufficient. No exact blocker rate or claimed measured attention. Without denominator, ship B17 health only. |
| B28 | One explicit supported proxy pilot after demonstrated routing need. | Full body/route/site/client-metadata-bound authentication and replay limits, trusted forwarding, origin controls, stripped credentials, safe logs, bounded resources; no ingestion caching. Test count/geo/rate-limit fidelity, spoofing, failures and added cost. No hidden collection-choice bypass. |

These optional items are investigations until their gate passes. Lack of suitable data or an acceptable privacy/cost design is a valid reason to leave one unimplemented.

## Validation, architecture and exclusions

- SQLite stores control metadata/revisions/config/jobs/import manifests; ClickHouse stores analytics/aggregate facts. Prefer additive migrations. Preaggregation/server caches need measured justification plus erasure/retention and invalidation design.
- Preserve pageview-scoped visitors, range deduplication and distinct-visitor privacy checks. Test late data, ties, duplicates, timezones/DST, boundaries, sparse/residual disclosures, site/access revocation and unsafe legacy values in real database fixtures.
- Set explicit per-slice query and browser budgets from P0 measurements; no silent SDK/query limit increases. Check response sizes/fan-out, long tasks, layout and retained heap in rendered desktop/mobile flows. Do not promise zero regression from compilation alone.
- Run applicable formatting/fresh types/tests/builds, SDK gates, Rust fmt/clippy/locked tests, isolated pinned ClickHouse, audits and container/release/remote CI jobs. Record skipped/unavailable checks. AI evidence changes need deterministic regression tests; live-provider calls remain opt-in.
- Ship small slices with version/availability dates, a separate SDK-release decision and rollback. Every derived store participates in deletion/retention before use. No fabricated backfills or destructive rollback migrations.
- D01–D13 remain excluded: profiles/payment joins, persistent/stronger visitor identity, replay/input/DOM capture, sensitive URLs, precise location, public raw streams, covert choice bypasses, FPS and fabricated causal/accuracy claims.

**First batch:** complete P0; implement P1 report bounds/versioning; ship B02 correctness independently; then shared context and B01/B03. Follow with B05/B06/B07, goals and annotations. No new collector or hosted service is a dependency of those core reports.

This turn added a plan only. It did not run new application tests/benchmarks or implement the proposed APIs, schema, collectors or features.
