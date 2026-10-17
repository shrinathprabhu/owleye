# Overview polish · 17 October 2026

The public edition receives the shared Console presentation improvements: compact metric selection, consistent chart colors and rounded lines, selected-metric exports, explicit partial traffic buckets, cohort span gaps, and recognizable referrer labels with local icon fallbacks.

The private cloud's advanced source-spike report and authenticated annotation loading are not copied into this edition. Existing public features remain available, and this work adds no hosted authentication, pricing or usage gates. The SDK stays at **1.0.3**; this UI polish does not change SDK code or require another npm publication.

The public edition continues to use its own configured API. The standalone synthetic Overview click-test fixture and restart instructions live in the private repository's `docs/overview-polish-2026-10-17.md`; that fixture is development-only and is not a public API service or a production deployment change.

Chart/cache work bounds and local checks do not constitute a production resource benchmark. Final automated and rendered-browser acceptance results are recorded separately by the implementation review. Nothing is committed automatically.

## Final local validation

The later major-source presentation refinement passed **124 Console tests**, Nuxt typecheck and the static production build. Shared helpers now support up to three significant sources per spike date, preserve each source's individual safe post link and reject coarse channels as major sources. The advanced detector and its authenticated loading remain private-only. Automated browser verification of this new grouping was blocked by the browser tool's URL security policy; the earlier rendered checks below do not verify the new grouping.

Initial Overview polish validation, recorded earlier on 17 October 2026:

120 Console tests, Nuxt typecheck, static production build, changed-file Prettier and diff checks passed. Desktop and 390px mobile browser verification covered compact cards, themed rounded charts, partial-period hover values, cohort keyboard inspection and source icons with Direct/unknown globe fallbacks. Source icons are bundled, same-origin assets; unrecognized hosts never trigger arbitrary favicon requests. No SDK/API source or dependency changes were needed; SDK references remain 1.0.3. Nothing was committed or deployed, and remote GitHub CI was not triggered.

An additional whole-tree Prettier diagnostic flags 10 unchanged tracked files plus a generated `.wrangler` file; the public repository has no whole-tree formatting CI command. Those unrelated files were left unchanged. Formatting for every changed Console text file passes.
