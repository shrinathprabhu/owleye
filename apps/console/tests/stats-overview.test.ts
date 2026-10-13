import assert from "node:assert/strict";
import { registerHooks } from "node:module";
import test from "node:test";
import { createRenderer, h, nextTick, ref } from "vue";
import type { StatsOverviewResponse } from "../types/stats.ts";

// Nuxt resolves this alias in production. Exercise the real composable under
// Vue's lifecycle without requiring a browser or mocking its request logic.
registerHooks({
  resolve(specifier, context, nextResolve) {
    if (specifier === "~/utils/analyticsRange") {
      return nextResolve(
        new URL("../utils/analyticsRange.ts", import.meta.url).href,
        context,
      );
    }
    return nextResolve(specifier, context);
  },
});
const { useStatsOverview } = await import("../composables/useStatsOverview.ts");

function harness() {
  const requests: Array<{
    query: Record<string, unknown>;
    signal: AbortSignal;
    resolve: (response: StatsOverviewResponse) => void;
    reject: (error: Error) => void;
  }> = [];
  Object.assign(globalThis, {
    useApi: () => ({
      api: (
        _path: string,
        options: { query: Record<string, unknown>; signal: AbortSignal },
      ) =>
        new Promise<StatsOverviewResponse>((resolve, reject) => {
          requests.push({ ...options, resolve, reject });
        }),
    }),
  });
  const renderer = createRenderer<object, object>({
    createElement: () => ({}),
    createText: () => ({}),
    createComment: () => ({}),
    insert() {},
    remove() {},
    setText() {},
    setElementText() {},
    patchProp() {},
    parentNode: () => null,
    nextSibling: () => null,
  });
  const siteId = ref("site-a");
  const days = ref(30);
  const dates = ref<{ start_date: string; end_date: string }>();
  let stats!: ReturnType<typeof useStatsOverview>;
  const app = renderer.createApp({
    setup() {
      stats = useStatsOverview({
        siteId,
        days,
        dates,
        prefetchDemoRanges: ref(false),
        toErrorMessage: () => "Unable to load",
      });
      return () => h("div");
    },
  });
  app.mount({});
  const respond = (index: number) => {
    const request = requests[index]!;
    const response = {
      site_id: request.query.site_id,
      days: request.query.days,
    } as StatsOverviewResponse;
    request.resolve(response);
    return response;
  };
  return { app, stats, requests, respond, siteId, days, dates };
}

test("mount and watcher join one overview request, then reuse the existing cache", async () => {
  const f = harness();
  try {
    const first = f.stats.refreshStats();
    const second = f.stats.refreshStats();
    assert.equal(first, second);
    assert.equal(f.requests.length, 1);
    assert.equal(f.requests[0]!.signal.aborted, false);
    const response = f.respond(0);
    await Promise.all([first, second]);
    assert.equal(f.stats.stats.value, response);
    assert.equal(f.stats.statsPending.value, false);
    await f.stats.refreshStats();
    assert.equal(f.requests.length, 1);
  } finally {
    f.app.unmount();
  }
});

test("site and range changes abort old requests and ignore their late responses", async () => {
  const f = harness();
  try {
    const old = f.stats.refreshStats();
    f.siteId.value = "site-b";
    await nextTick();
    const selected = f.stats.refreshStats();
    assert.equal(f.requests[0]!.signal.aborted, true);
    f.respond(0);
    await old;
    assert.equal(f.stats.stats.value, null);
    assert.equal(f.stats.statsPending.value, true);
    const response = f.respond(1);
    await selected;
    assert.equal(f.stats.stats.value, response);
    f.dates.value = { start_date: "2026-08-01", end_date: "2026-08-30" };
    const firstRange = f.stats.refreshStats();
    f.dates.value = { start_date: "2026-09-01", end_date: "2026-09-30" };
    const nextRange = f.stats.refreshStats();
    assert.equal(f.requests[2]!.signal.aborted, true);
    f.respond(3);
    await nextRange;
    const current = f.stats.stats.value;
    f.respond(2);
    await firstRange;
    assert.equal(f.stats.stats.value, current);
  } finally {
    f.app.unmount();
  }
});

test("force refresh replaces in-flight work and failures remain retryable", async () => {
  const f = harness();
  try {
    const first = f.stats.refreshStats();
    const forced = f.stats.refreshStats(true);
    assert.equal(f.requests.length, 2);
    assert.equal(f.requests[0]!.signal.aborted, true);
    f.requests[0]!.reject(new Error("stale failure"));
    await first;
    assert.equal(f.stats.statsError.value, "");
    f.requests[1]!.reject(new Error("failure"));
    await forced;
    assert.equal(f.stats.statsError.value, "Unable to load");
    const retry = f.stats.refreshStats();
    f.respond(2);
    await retry;
    assert.equal(f.stats.statsError.value, "");
  } finally {
    f.app.unmount();
  }
});

test("reset and unmount prevent old work from repopulating the screen or cache", async () => {
  const f = harness();
  const first = f.stats.refreshStats();
  f.stats.resetStats();
  assert.equal(f.requests[0]!.signal.aborted, true);
  f.respond(0);
  await first;
  assert.equal(f.stats.stats.value, null);
  const second = f.stats.refreshStats();
  assert.equal(f.requests.length, 2);
  f.app.unmount();
  assert.equal(f.requests[1]!.signal.aborted, true);
  f.respond(1);
  await second;
  assert.equal(f.stats.stats.value, null);
});
