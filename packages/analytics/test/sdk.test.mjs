import assert from "node:assert/strict";
import { afterEach, test } from "node:test";
import { readFileSync } from "node:fs";
import { runInThisContext } from "node:vm";

const analyticsModule = import("../dist/index.js");
const performanceModule = import("../dist/performance.js");
const rulesModule = import("../dist/rules.js");
const cleanups = [];

afterEach(() => {
  while (cleanups.length > 0) cleanups.pop()?.();
});

test("all CDN entrypoints track without cookies or persistent browser storage", async () => {
  const browser = installBrowser("https://app.example/");
  const requests = captureRequests({
    rules: {
      rules: [
        {
          id: "cdn_click",
          name: "cta_clicked",
          selector: "button",
          type: "click",
          capture_text: false,
        },
      ],
    },
  });
  browser.document.currentScript = {
    dataset: { owleyeId: "site_cdn", owleyeServer: "https://app.example" },
  };
  for (const entry of ["analytics", "rules", "performance"]) {
    runInThisContext(
      readFileSync(
        new URL(`../dist/owleye.${entry}.iife.js`, import.meta.url),
        "utf8",
      ),
    );
  }
  await flush();
  await flush();
  browser.window.OwlEyeAnalytics.track("cdn_signup", { plan: "free" });
  browser.document.emit("click", {
    target: new FakeElement("button"),
    type: "click",
  });
  browser.window.OwlEyePerformance.start("cdn_work")();
  browser.document.visibilityState = "hidden";
  browser.document.emit("visibilitychange");
  browser.window.OwlEyeAnalytics.stop();
  browser.window.OwlEyeRules.stop();
  browser.window.OwlEyePerformance.observeVitals().stop();
  await flush();
  const events = requestEvents(requests.calls);
  for (const type of [
    "pageview",
    "page_session",
    "external",
    "rule",
    "performance",
  ]) {
    assert.ok(
      events.some((event) => event.type === type),
      `missing ${type} from CDN bundle`,
    );
  }
  assert.ok(
    requests.calls.some((request) => request.url.includes("/v1/rules")),
  );
  assert.ok(
    requests.calls.every((request) => request.options.credentials === "omit"),
  );
  assert.equal(requests.beacons.length, 0);
});

test("attributes SPA sessions to their starting page and closes each segment once", async () => {
  const browser = installBrowser(
    "https://shop.example/first?customer=secret#private",
    "https://search.example/results?q=private#answer",
  );
  const requests = captureRequests();
  const { useAnalytics } = await analyticsModule;
  const originalPushState = browser.window.history.pushState;
  const analytics = useAnalytics("site_test", {
    server: "https://api.example",
  });

  browser.window.history.pushState({}, "", "/second?token=secret#checkout");
  browser.document.visibilityState = "hidden";
  browser.document.emit("visibilitychange");
  browser.window.emit("pagehide");
  analytics.stop();
  await flush();

  const events = requestEvents(requests.calls);
  assert.deepEqual(events[0].sdk, {
    name: "owleye-js",
    version: JSON.parse(
      readFileSync(new URL("../package.json", import.meta.url), "utf8"),
    ).version,
  });
  assert.deepEqual(
    events.map((event) => event.type),
    ["pageview", "page_session", "pageview", "page_session"],
  );
  assert.equal(events[1].page.path, "/first");
  assert.equal(events[1].page.url, "https://shop.example/first");
  assert.equal(events[2].page.path, "/second");
  assert.equal(events[2].page.referrer, "https://shop.example/first");
  assert.equal(events[2].page.search, undefined);
  assert.equal(events[2].page.hash, undefined);
  assert.equal(events[3].page.path, "/second");
  assert.equal(
    requests.beacons.length,
    0,
    "cross-origin lifecycle delivery must avoid Beacon",
  );

  for (const request of requests.calls.filter(isEventRequest)) {
    assert.equal(request.options.credentials, "omit");
    assert.equal(request.options.keepalive, true);
    assert.equal(request.options.referrerPolicy, "no-referrer");
  }
  assert.equal(browser.window.history.pushState, originalPushState);
});

test("reuses an active analytics controller without duplicate page views", async () => {
  installBrowser("https://app.example/");
  const requests = captureRequests();
  const { useAnalytics } = await analyticsModule;

  const first = useAnalytics("site_idempotent", {
    server: "https://api.example",
  });
  const second = useAnalytics("site_idempotent", {
    server: "https://api.example",
  });
  await flush();

  assert.equal(first, second);
  assert.equal(
    requestEvents(requests.calls).filter((event) => event.type === "pageview")
      .length,
    1,
  );
  first.stop();
});

test("normalizes initialization identity and does not count framework remounts as navigation", async () => {
  const browser = installBrowser("https://app.example/start");
  const requests = captureRequests();
  const { useAnalytics } = await analyticsModule;
  const first = useAnalytics(" site_rerender ", {
    server: "https://api.example/",
  });
  for (let i = 0; i < 100; i++) {
    assert.equal(
      useAnalytics("site_rerender", {
        server: new URL("https://api.example"),
        autoStart: true,
        captureQuery: false,
      }),
      first,
    );
  }
  assert.equal(browser.window.listeners.get("pagehide").size, 1);
  first.stop();
  const remounted = useAnalytics("site_rerender", {
    server: "https://api.example",
  });
  first.start(); // A stale controller cannot install another set of listeners.
  await flush();
  assert.equal(
    requestEvents(requests.calls).filter((e) => e.type === "pageview").length,
    1,
  );
  browser.window.history.replaceState({}, "", "/start");
  browser.window.history.pushState({}, "", "/next");
  browser.window.history.pushState({}, "", "/start");
  await flush();
  assert.equal(
    requestEvents(requests.calls).filter((e) => e.type === "pageview").length,
    3,
  );
  remounted.stop();
  assert.equal(browser.window.listeners.get("pagehide").size, 0);
});

test("ESM and repeated CDN loads share analytics, rules, and Web Vitals observation", async () => {
  const browser = installBrowser("https://app.example/");
  const rule = { id: "once", name: "cta", selector: "button", type: "click" };
  const requests = captureRequests({ rules: { rules: [rule, rule] } });
  const { useAnalytics } = await analyticsModule;
  const { trackRules } = await rulesModule;
  const { trackPerf, trackWebVitals } = await performanceModule;
  const config = { server: "https://api.example/" };
  const analytics = useAnalytics("shared", config);
  const rules = trackRules("shared", config);
  const vitals = trackWebVitals("shared", config);
  browser.document.currentScript = {
    dataset: { owleyeId: "shared", owleyeServer: "https://api.example" },
  };
  for (let i = 0; i < 4; i++) {
    for (const entry of ["analytics", "rules", "performance"]) {
      runInThisContext(
        readFileSync(
          new URL(`../dist/owleye.${entry}.iife.js`, import.meta.url),
          "utf8",
        ),
      );
    }
    assert.equal(
      trackRules("shared", { ...config, enrichRule: () => ({}) }),
      rules,
    );
    assert.equal(trackPerf("shared", config).observeVitals(), vitals);
  }
  await flush();
  assert.equal(browser.window.OwlEyeAnalytics, analytics);
  assert.equal(browser.window.OwlEyeRules, rules);
  assert.equal(
    requests.calls.filter((c) => c.url.includes("/v1/rules")).length,
    1,
  );
  assert.equal(FakePerformanceObserver.instances.length, 4);
  assert.equal(browser.document.listeners.get("click").size, 1);
  const button = new FakeElement("button");
  browser.document.emit("click", { target: button, type: "click" });
  browser.document.emit("click", { target: button, type: "click" });
  await flush();
  assert.equal(
    requestEvents(requests.calls).filter((e) => e.type === "pageview").length,
    1,
  );
  assert.equal(
    requestEvents(requests.calls).filter((e) => e.type === "rule").length,
    2,
    "one event per actual click, not per duplicated rule or initializer",
  );
  analytics.stop();
  rules.stop();
  vitals.stop();
  assert.equal(browser.document.listeners.get("click").size, 0);
  assert.ok(FakePerformanceObserver.instances.every((o) => o.disconnected));
});

test("transport bounds pending work and releases capacity after failure", async () => {
  installBrowser("https://app.example/");
  const previousFetch = globalThis.fetch;
  const pending = [];
  globalThis.fetch = (_url, options) =>
    new Promise((resolve, reject) =>
      pending.push({ resolve, reject, options }),
    );
  cleanups.push(() => {
    globalThis.fetch = previousFetch;
  });
  const { useAnalytics } = await analyticsModule;
  const analytics = useAnalytics("bounded");
  for (let i = 0; i < 1000; i++) analytics.track("burst");
  assert.equal(pending.length, 8);
  assert.ok(
    pending.reduce((n, p) => n + Buffer.byteLength(p.options.body), 0) <=
      60_000,
  );
  for (const p of pending) p.reject(new Error("offline"));
  await flush();
  analytics.track("recovered");
  assert.equal(pending.length, 9);
  pending.at(-1).resolve({ ok: true });
  await flush();
  globalThis.fetch = () => {
    throw new Error("synchronous host fetch failure");
  };
  assert.doesNotThrow(() => analytics.track("sync_failure"));
  analytics.stop();
});

test("stalled transport is aborted and recovers without an event backlog", async (t) => {
  installBrowser("https://app.example/");
  const previousFetch = globalThis.fetch;
  const calls = [];
  globalThis.fetch = (_url, options) =>
    new Promise((_resolve, reject) => {
      calls.push(options);
      options.signal.addEventListener(
        "abort",
        () => reject(new Error("timeout")),
        { once: true },
      );
    });
  cleanups.push(() => {
    globalThis.fetch = previousFetch;
  });
  t.mock.timers.enable({ apis: ["setTimeout"] });
  const { useAnalytics } = await analyticsModule;
  const analytics = useAnalytics("timeouts");
  for (let i = 0; i < 50; i++) analytics.track("event");
  assert.equal(calls.length, 8);
  t.mock.timers.tick(5000);
  assert.ok(calls.every((call) => call.signal.aborted));
  analytics.track("after_timeout");
  assert.equal(calls.length, 9);
  t.mock.timers.tick(5000);
  analytics.stop();
});

test("pending rule debounce work is bounded and completely discarded on stop", async (t) => {
  const browser = installBrowser("https://app.example/");
  const requests = captureRequests({
    rules: {
      rules: [
        {
          id: "debounce",
          name: "debounce",
          selector: "input",
          type: "keyup",
          trigger_config: { key_condition: "debounce", debounce_seconds: 0.1 },
        },
      ],
    },
  });
  const { trackRules } = await rulesModule;
  const tracker = trackRules("bounded_rules");
  await flush();
  await flush();
  t.mock.timers.enable({ apis: ["setTimeout"] });
  let scheduled = 0;
  const realSetTimeout = globalThis.setTimeout;
  globalThis.setTimeout = (...args) => {
    scheduled++;
    return realSetTimeout(...args);
  };
  try {
    for (let i = 0; i < 1000; i++) {
      browser.document.emit("keyup", {
        type: "keyup",
        target: new FakeElement("input"),
      });
    }
    assert.equal(scheduled, 250);
    tracker.stop();
    t.mock.timers.tick(200);
    assert.equal(requestEvents(requests.calls).length, 0);
    assert.equal(browser.document.listeners.get("keyup").size, 0);
  } finally {
    globalThis.setTimeout = realSetTimeout;
  }
});

test("separate sites stay independent and shared selectors are matched once per DOM event", async () => {
  const browser = installBrowser("https://app.example/");
  const requests = captureRequests({
    rules: {
      rules: [
        { id: "one", name: "one", selector: "button", type: "click" },
        { id: "two", name: "two", selector: "button", type: "click" },
      ],
    },
  });
  const { useAnalytics } = await analyticsModule;
  const { trackRules } = await rulesModule;
  const a = useAnalytics("site_a");
  const b = useAnalytics("site_b");
  assert.notEqual(a, b);
  const tracker = trackRules("site_a");
  await flush();
  await flush();
  const button = new FakeElement("button");
  let matches = 0;
  button.closest = () => {
    matches++;
    return button;
  };
  browser.document.emit("click", { type: "click", target: button });
  await flush();
  assert.equal(matches, 1);
  assert.equal(
    requestEvents(requests.calls).filter((e) => e.type === "rule").length,
    2,
    "distinct configured rules remain intentional events",
  );
  a.stop();
  b.stop();
  tracker.stop();
});

test("fast successful delivery still respects the event-rate budget", async (t) => {
  installBrowser("https://app.example/");
  const requests = captureRequests();
  t.mock.timers.enable({ apis: ["Date"], now: 1_000 });
  const { useAnalytics } = await analyticsModule;
  const analytics = useAnalytics("rate_budget");
  for (let i = 0; i < 220; i++) {
    analytics.track("burst");
    await flush();
  }
  assert.equal(requestEvents(requests.calls).length, 200);
  t.mock.timers.tick(10);
  analytics.track("one_refilled_token");
  await flush();
  assert.equal(requestEvents(requests.calls).length, 201);
  analytics.stop();
});

test("mutation storms coalesce to a refresh instead of retaining unlimited roots", async () => {
  const browser = installBrowser("https://app.example/");
  captureRequests({
    rules: {
      rules: [{ id: "view", name: "view", type: "view", selector: ".tile" }],
    },
  });
  const { trackRules } = await rulesModule;
  const tracker = trackRules("mutation_budget");
  await flush();
  await flush();
  FakeMutationObserver.instances[0].emit([
    {
      addedNodes: Array.from({ length: 1000 }, () => new FakeElement("div")),
      removedNodes: [],
    },
  ]);
  await flush(240);
  assert.equal(
    browser.document.queryCounts.get(".tile"),
    2,
    "initial scan and one overflow refresh",
  );
  assert.equal(FakeIntersectionObserver.instances.length, 2);
  assert.equal(FakeIntersectionObserver.instances[0].disconnected, true);
  tracker.stop();
});

test("supports manual page views and reusable global records", async () => {
  installBrowser("https://app.example/start");
  const requests = captureRequests();
  const { useAnalytics } = await analyticsModule;
  const analytics = useAnalytics("site_manual", {
    server: "https://api.example",
  });

  analytics.setGlobalRecords({ plan: "pro", tenant: "acme" });
  analytics.track("checkout", {
    currency: "USD",
    experiment: "pricing-v2",
    plan: "business",
    seats: 5,
  });
  analytics.pageview({ path: "/checkout", title: "Checkout" });
  await flush();
  analytics.stop();

  const events = requestEvents(requests.calls);
  const checkout = events.find((event) => event.name === "checkout");
  assert.deepEqual(checkout.data.records, {
    currency: "USD",
    experiment: "pricing-v2",
    plan: "business",
    seats: 5,
    tenant: "acme",
  });
  assert.equal(
    events.findLast((event) => event.type === "pageview").page.path,
    "/checkout",
  );
});

test("global and event records share the server's ten-field budget", async () => {
  installBrowser("https://app.example/");
  const requests = captureRequests();
  const { useAnalytics } = await analyticsModule;
  const analytics = useAnalytics("site_global_limit", {
    server: "https://api.example",
  });
  analytics.setGlobalRecords({ plan: "pro", category: "retail" });
  assert.doesNotThrow(() =>
    analytics.track(
      "checkout",
      Object.fromEntries(Array.from({ length: 9 }, (_, i) => [`field${i}`, i])),
    ),
  );
  await flush();
  analytics.stop();
  assert.equal(
    requestEvents(requests.calls).filter((event) => event.name === "checkout")
      .length,
    0,
  );
});

test("tracks hash navigation once and exposes query/hash only by explicit opt-in", async () => {
  const browser = installBrowser(
    "https://app.example/dashboard?view=week#overview",
  );
  const requests = captureRequests();
  const { useAnalytics } = await analyticsModule;
  const analytics = useAnalytics("site_test", {
    captureHash: true,
    captureQuery: true,
    server: "https://api.example",
  });

  browser.location.hash = "#details";
  browser.window.emit("popstate");
  browser.window.emit("hashchange");
  browser.window.history.replaceState({}, "", browser.location.href);
  analytics.stop();
  await flush();

  const events = requestEvents(requests.calls);
  assert.deepEqual(
    events.map((event) => event.type),
    ["pageview", "page_session", "pageview"],
  );
  assert.equal(events[0].page.search, "?view=week");
  assert.equal(events[0].page.hash, "#overview");
  assert.equal(events[2].page.hash, "#details");
  assert.equal(
    events[2].page.url,
    "https://app.example/dashboard?view=week#details",
  );
});

test("can opt into bounded campaign parameters without exposing arbitrary query data", async () => {
  installBrowser(
    "https://app.example/pricing?utm_source=newsletter&email=secret%40example.com&utm_medium=email&utm_campaign=launch-week#private",
  );
  const requests = captureRequests();
  const { useAnalytics } = await analyticsModule;
  const analytics = useAnalytics("site_test", {
    captureCampaigns: true,
    server: "https://api.example",
  });
  analytics.stop();
  await flush();

  const [pageView] = requestEvents(requests.calls);
  assert.equal(
    pageView.page.search,
    "?utm_source=newsletter&utm_medium=email&utm_campaign=launch-week",
  );
  assert.equal(
    pageView.page.url,
    "https://app.example/pricing?utm_source=newsletter&utm_medium=email&utm_campaign=launch-week",
  );
  assert.ok(!pageView.page.url.includes("email="));
  assert.equal(pageView.page.hash, undefined);
});

test("full query capture takes precedence over campaign-only capture", async () => {
  installBrowser(
    "https://app.example/pricing?utm_source=newsletter&plan=business#private",
  );
  const requests = captureRequests();
  const { useAnalytics } = await analyticsModule;
  const analytics = useAnalytics("site_test", {
    captureCampaigns: true,
    captureQuery: true,
    server: "https://api.example",
  });
  analytics.stop();
  await flush();

  const [pageView] = requestEvents(requests.calls);
  assert.equal(pageView.page.search, "?utm_source=newsletter&plan=business");
  assert.equal(pageView.page.hash, undefined);
});

test("emits one performance event and merges start and end records", async () => {
  installBrowser("https://app.example/checkout");
  const requests = captureRequests();
  const { trackPerf } = await performanceModule;
  const performance = trackPerf("site_test", { server: "https://api.example" });
  const end = performance.start("checkout_latency", { plan: "pro" });

  end({ outcome: "success" });
  end({ outcome: "duplicate" });
  await flush();

  const events = requestEvents(requests.calls);
  assert.equal(events.length, 1);
  assert.equal(events[0].type, "performance");
  assert.equal(events[0].name, "checkout_latency");
  assert.deepEqual(events[0].data.start, { plan: "pro" });
  assert.deepEqual(events[0].data.end, { outcome: "success" });
  assert.ok(events[0].data.duration_ms >= 0);
});

test("collects web vitals only after explicit performance opt-in", async () => {
  const browser = installBrowser("https://app.example/pricing?token=private");
  const requests = captureRequests();
  const { trackWebVitals } = await performanceModule;
  const vitals = trackWebVitals("site_vitals", {
    server: "https://api.example",
  });

  FakePerformanceObserver.forType("paint").emit([
    { name: "first-contentful-paint", startTime: 1_420 },
  ]);
  FakePerformanceObserver.forType("largest-contentful-paint").emit([
    { name: "largest-contentful-paint", startTime: 2_310 },
  ]);
  FakePerformanceObserver.forType("layout-shift").emit([
    { hadRecentInput: false, value: 0.04 },
    { hadRecentInput: true, value: 0.9 },
    { hadRecentInput: false, value: 0.03 },
  ]);
  FakePerformanceObserver.forType("event").emit([
    { duration: 184, interactionId: 7, name: "click" },
  ]);
  browser.document.visibilityState = "hidden";
  browser.document.emit("visibilitychange");
  vitals.stop();
  await flush();

  const events = requestEvents(requests.calls).filter(
    (event) => event.type === "performance",
  );
  assert.deepEqual(events.map((event) => event.name).sort(), [
    "web_vital_cls",
    "web_vital_fcp",
    "web_vital_inp",
    "web_vital_lcp",
    "web_vital_ttfb",
  ]);
  assert.equal(
    events.find((event) => event.name === "web_vital_cls").data.value,
    0.07,
  );
  assert.equal(
    events.find((event) => event.name === "web_vital_lcp").data.duration_ms,
    2_310,
  );
  assert.ok(events.every((event) => !event.page.url.includes("token=")));
  assert.ok(
    FakePerformanceObserver.instances.every(
      (observer) => observer.disconnected,
    ),
  );
});

test("stopping performance observation discards unsent measurements", async () => {
  installBrowser("https://app.example/");
  const requests = captureRequests();
  const { trackWebVitals } = await performanceModule;
  const vitals = trackWebVitals("site_stop_vitals");
  await flush();
  const before = requestEvents(requests.calls).length;
  const paint = FakePerformanceObserver.forType("paint");
  vitals.stop();
  paint.emit([{ name: "first-contentful-paint", startTime: 1200 }]);
  await flush();
  assert.equal(requestEvents(requests.calls).length, before);
});

test("reports transport failures by default and honors mock mode", async () => {
  installBrowser("https://app.example/");
  const warnings = [];
  const originalWarn = console.warn;
  const originalInfo = console.info;
  console.warn = (...args) => warnings.push(args);
  console.info = () => undefined;
  cleanups.push(() => {
    console.warn = originalWarn;
    console.info = originalInfo;
  });
  const requests = captureRequests({ eventStatus: 503 });
  const { useAnalytics } = await analyticsModule;

  const failing = useAnalytics("site_transport_http", {
    autoStart: false,
    server: "https://api.example",
  });
  failing.start();
  await flush();
  failing.stop();
  assert.ok(
    warnings.some(([message]) => String(message).includes("HTTP delivery")),
  );

  const beforeMock = requests.calls.length;
  const mocked = useAnalytics("site_transport_mock", {
    mock: true,
    server: "https://api.example",
  });
  mocked.track("mocked_event");
  mocked.stop();
  await flush();
  assert.equal(requests.calls.length, beforeMock);
});

test("contains rejected network transports without throwing", async () => {
  installBrowser("https://app.example/");
  const warnings = [];
  const originalWarn = console.warn;
  const originalInfo = console.info;
  console.warn = (...args) => warnings.push(args);
  console.info = () => undefined;
  cleanups.push(() => {
    console.warn = originalWarn;
    console.info = originalInfo;
  });
  captureRequests({ eventError: new Error("offline") });
  const { useAnalytics } = await analyticsModule;
  const analytics = useAnalytics("site_transport_network", {
    debug: true,
    server: "https://api.example",
  });
  await flush();
  analytics.stop();

  assert.ok(
    warnings.some(([message]) => String(message).includes("network delivery")),
  );
});

test("bounds opted-in page fields to the ingestion contract", async () => {
  const browser = installBrowser(
    `https://app.example/${"p".repeat(3_000)}?detail=${"q".repeat(3_000)}#${"h".repeat(3_000)}`,
  );
  browser.document.title = "🦉".repeat(400);
  const requests = captureRequests();
  const { useAnalytics } = await analyticsModule;
  const analytics = useAnalytics("site_test", {
    captureHash: true,
    captureQuery: true,
    server: "https://api.example",
  });
  analytics.stop();
  await flush();

  const [pageView] = requestEvents(requests.calls);
  assert.ok(Buffer.byteLength(pageView.page.path) <= 2_048);
  assert.ok(Buffer.byteLength(pageView.page.search) <= 2_048);
  assert.ok(Buffer.byteLength(pageView.page.hash) <= 2_048);
  assert.ok(Buffer.byteLength(pageView.page.title) <= 512);
  assert.ok(Buffer.byteLength(pageView.page.url) <= 4_096);
});

test("obsolete DNT settings do not suppress analytics", async () => {
  installBrowser("https://app.example/", undefined, "1");
  navigator.msDoNotTrack = "1";
  window.doNotTrack = "yes";
  const requests = captureRequests();
  const { useAnalytics } = await analyticsModule;

  const analytics = useAnalytics("site_test", {
    respectDoNotTrack: true,
    server: "https://api.example",
  });
  analytics.track("dnt_does_not_block");
  analytics.stop();
  await flush();

  assert.deepEqual(
    requestEvents(requests.calls).map((event) => event.type),
    ["pageview", "external"],
  );
});

test("honors Global Privacy Control by default with an explicit SDK override", async () => {
  installBrowser("https://app.example/", undefined, "0", true);
  const requests = captureRequests();
  const { useAnalytics } = await analyticsModule;
  const { trackRules } = await rulesModule;

  const disabledAnalytics = useAnalytics("site_test", {
    server: "https://api.example",
  });
  const disabledRules = trackRules("site_test", {
    server: "https://api.example",
  });
  disabledAnalytics.track("ignored");
  disabledAnalytics.stop();
  disabledRules.stop();
  await flush();

  assert.equal(requests.calls.length, 0);

  const enabledAnalytics = useAnalytics("site_test", {
    respectGlobalPrivacyControl: false,
    server: "https://api.example",
  });
  enabledAnalytics.stop();
  await flush();
  assert.equal(requestEvents(requests.calls).length, 1);
});

test("can wait for an explicit consent decision before starting analytics", async () => {
  installBrowser("https://app.example/pricing");
  const requests = captureRequests();
  const { useAnalytics } = await analyticsModule;
  const analytics = useAnalytics("site_test", {
    autoStart: false,
    server: "https://api.example",
  });

  analytics.track("ignored_before_start");
  await flush();
  assert.equal(requests.calls.length, 0);

  analytics.start();
  analytics.start();
  analytics.track("pricing_viewed");
  const requestsBeforeStop = requests.calls.length;
  analytics.stop();
  await flush();
  assert.equal(
    requests.calls.length,
    requestsBeforeStop,
    "stop must not emit or queue a final page-session event",
  );

  const events = requestEvents(requests.calls);
  assert.deepEqual(
    events.map((event) => event.type),
    ["pageview", "external"],
  );
  assert.equal(events[1].name, "pricing_viewed");
});

test("adds valid rule enrichment and safely drops invalid or throwing enrichment", async () => {
  const browser = installBrowser("https://app.example/");
  const requests = captureRequests({
    rules: {
      rules: [
        {
          capture_text: false,
          id: "rule_1",
          name: "cta_clicked",
          selector: "button",
          type: "click",
        },
      ],
    },
  });
  const { trackRules } = await rulesModule;
  let hookCalls = 0;
  let receivedContext;
  const rules = trackRules("site_test", {
    enrichRule(_rule, context) {
      hookCalls += 1;
      receivedContext = context;
      if (hookCalls === 1) return { active: true, plan: "pro", seats: 3 };
      if (hookCalls === 2)
        return Object.fromEntries(
          Array.from({ length: 11 }, (_, i) => [`field${i}`, i]),
        );
      throw new Error("application hook failed");
    },
    server: "https://api.example",
  });
  await flush();
  await flush();

  const ruleRequest = requests.calls.find((request) =>
    request.url.includes("/v1/rules"),
  );
  assert.equal(ruleRequest.options.credentials, "omit");
  assert.equal(ruleRequest.options.referrerPolicy, "strict-origin");

  const button = new FakeElement("button");
  browser.document.emit("click", { target: button, type: "click" });
  browser.document.emit("click", { target: button, type: "click" });
  browser.document.emit("click", { target: button, type: "click" });
  rules.stop();
  await flush();

  const events = requestEvents(requests.calls).filter(
    (event) => event.type === "rule",
  );
  assert.equal(
    events.length,
    3,
    "hook failures must not suppress the base rule event",
  );
  assert.deepEqual(events[0].data.records, {
    active: true,
    plan: "pro",
    seats: 3,
  });
  assert.equal(events[1].data.records, undefined);
  assert.equal(events[2].data.records, undefined);
  assert.equal(receivedContext.element, button);
  assert.equal(receivedContext.type, "click");
});

test("honors rule sampling without persisting a browser identifier", async () => {
  const browser = installBrowser("https://app.example/");
  const requests = captureRequests({
    rules: {
      rules: [
        {
          id: "rule_never",
          name: "never_sampled",
          sample_rate: 0,
          selector: "button",
          type: "click",
        },
        {
          id: "rule_always",
          name: "always_sampled",
          sample_rate: 1,
          selector: "button",
          type: "click",
        },
      ],
    },
  });
  const { trackRules } = await rulesModule;
  const rules = trackRules("site_test", {
    server: "https://api.example",
  });
  await flush();
  await flush();

  browser.document.emit("click", {
    target: new FakeElement("button"),
    type: "click",
  });
  rules.stop();
  await flush();

  const events = requestEvents(requests.calls).filter(
    (event) => event.type === "rule",
  );
  assert.deepEqual(
    events.map((event) => event.name),
    ["always_sampled"],
  );
});

test("guards required remote rule fields while preserving valid rules", async () => {
  const browser = installBrowser("https://app.example/");
  const requests = captureRequests({
    rules: {
      rules: [
        null,
        {
          id: "bad_sample_rate",
          name: "bad_sample_rate",
          type: "click",
        },
        {
          id: "bad_dom_event",
          name: "bad_dom_event",
          selector: "button",
          type: "pointerdown",
        },
        {
          id: "valid_rule",
          name: "valid_rule",
          selector: "button",
          type: "click",
        },
      ],
    },
  });
  const warnings = [];
  const originalWarn = console.warn;
  const originalInfo = console.info;
  console.warn = (...args) => warnings.push(args);
  console.info = () => undefined;
  cleanups.push(() => {
    console.warn = originalWarn;
    console.info = originalInfo;
  });

  const { trackRules } = await rulesModule;
  const rules = trackRules("site_test", {
    debug: true,
    server: "https://api.example",
  });
  await flush();
  await flush();

  browser.document.emit("click", {
    target: new FakeElement("button"),
    type: "click",
  });
  rules.stop();
  await flush();

  const events = requestEvents(requests.calls).filter(
    (event) => event.type === "rule",
  );
  assert.deepEqual(
    events.map((event) => event.name),
    ["valid_rule"],
  );
  assert.equal(warnings.length, 1);
  assert.ok(
    warnings.every(([message]) =>
      String(message).startsWith("[OwlEye] rule validation"),
    ),
  );

  const ruleRequest = requests.calls.find((request) =>
    request.url.includes("/v1/rules"),
  );
  assert.equal(ruleRequest.options.cache, "no-store");
  assert.equal(ruleRequest.options.method, "GET");
  assert.equal(ruleRequest.options.headers.accept, "application/json");
});

test("debounces matched rules and releases pending timers on stop", async () => {
  const browser = installBrowser("https://app.example/");
  const requests = captureRequests({
    rules: {
      rules: [
        {
          id: "debounced_rule",
          name: "debounced_rule",
          selector: "input",
          trigger_config: {
            debounce_seconds: 0.1,
            dom_event: "keyup",
            key_condition: "debounce",
          },
          type: "click",
        },
      ],
    },
  });
  const { trackRules } = await rulesModule;
  const rules = trackRules("site_test", { server: "https://api.example" });
  await flush();
  await flush();

  const input = new FakeElement("input");
  browser.document.emit("keyup", { key: "a", target: input, type: "keyup" });
  browser.document.emit("keyup", { key: "b", target: input, type: "keyup" });
  await flush(120);

  browser.document.emit("keyup", { key: "c", target: input, type: "keyup" });
  rules.stop();
  await flush(120);

  const events = requestEvents(requests.calls).filter(
    (event) => event.type === "rule",
  );
  assert.deepEqual(
    events.map((event) => event.name),
    ["debounced_rule"],
  );
});

test("validates site IDs consistently and can stop rule tracking outside a browser", async () => {
  const { useAnalytics } = await analyticsModule;
  const { trackPerf } = await performanceModule;
  const { trackRules } = await rulesModule;

  for (const createController of [useAnalytics, trackPerf, trackRules]) {
    assert.doesNotThrow(() => createController("  "));
  }

  const rules = trackRules("site_test");
  assert.doesNotThrow(() => rules.stop());
});

test("normalizes valid site IDs and rejects identifiers the API cannot accept", async () => {
  installBrowser("https://app.example/");
  const requests = captureRequests();
  const { useAnalytics } = await analyticsModule;

  for (const siteId of ["site id", "site/slash", "é", "a".repeat(129)]) {
    assert.doesNotThrow(() => useAnalytics(siteId));
  }

  const analytics = useAnalytics("  site_test.v1  ", {
    server: "https://api.example",
  });
  analytics.stop();
  await flush();
  const envelopes = requests.calls
    .filter(isEventRequest)
    .map((request) => JSON.parse(request.options.body));
  assert.ok(envelopes.length > 0);
  assert.ok(envelopes.every((envelope) => envelope.site_id === "site_test.v1"));
});

test("validates API bases, event names, and payload size before transport", async () => {
  installBrowser("https://app.example/");
  const requests = captureRequests();
  const { useAnalytics } = await analyticsModule;
  const { trackPerf } = await performanceModule;

  for (const server of [
    "javascript:alert(1)",
    "//api.example",
    "https://user:secret@api.example",
    "https://api.example?tenant=private",
  ]) {
    assert.doesNotThrow(() => useAnalytics("site_test", { server }));
  }

  const analytics = useAnalytics("site_test", { server: "/telemetry/" });
  for (const name of [" ", "line\nbreak", "x".repeat(129)]) {
    assert.doesNotThrow(() => analytics.track(name));
  }
  assert.doesNotThrow(() =>
    analytics.track("oversized", { value: "x".repeat(17_000) }),
  );
  assert.doesNotThrow(() => trackPerf("site_test").start("\u0000bad"));
  analytics.track("  checkout_completed  ", { plan: "founder" });
  analytics.stop();
  await flush();

  const eventRequests = requests.calls.filter(isEventRequest);
  assert.ok(
    eventRequests.every((request) => request.url === "/telemetry/v1/events"),
  );
  assert.ok(
    requestEvents(eventRequests).some(
      (event) => event.name === "checkout_completed",
    ),
  );
});

test("tracks every view rule on a shared element without mutating its dataset", async () => {
  const browser = installBrowser("https://app.example/");
  const hero = new FakeElement("section");
  browser.document.queryResults.set(".hero", [hero]);
  const requests = captureRequests({
    rules: {
      rules: [
        {
          id: "rule_view_a",
          name: "hero_seen",
          selector: ".hero",
          type: "view",
        },
        {
          id: "rule_view_b",
          name: "campaign_seen",
          selector: ".hero",
          type: "view",
        },
      ],
    },
  });
  const { trackRules } = await rulesModule;
  const rules = trackRules("site_test", { server: "https://api.example" });
  await flush();
  await flush();

  const observer = FakeIntersectionObserver.instances.at(-1);
  assert.ok(observer);
  observer.emit(hero);
  rules.stop();
  await flush();

  const events = requestEvents(requests.calls).filter(
    (event) => event.type === "rule",
  );
  assert.deepEqual(
    events.map((event) => event.name),
    ["hero_seen", "campaign_seen"],
  );
  assert.equal(browser.document.queryCounts.get(".hero"), 1);
  assert.equal(hero.dataset.owlRuleId, undefined);
  assert.equal(observer.disconnected, true);
});

test("tracks view rules when one element matches different selectors", async () => {
  const browser = installBrowser("https://app.example/");
  const hero = new FakeElement("section");
  browser.document.queryResults.set(".hero", [hero]);
  browser.document.queryResults.set("section", [hero]);
  const requests = captureRequests({
    rules: {
      rules: [
        {
          id: "rule_by_class",
          name: "hero_seen",
          selector: ".hero",
          type: "view",
        },
        {
          id: "rule_by_tag",
          name: "section_seen",
          selector: "section",
          type: "view",
        },
      ],
    },
  });
  const { trackRules } = await rulesModule;
  const rules = trackRules("site_test", { server: "https://api.example" });
  await flush();
  await flush();

  const observer = FakeIntersectionObserver.instances.at(-1);
  assert.ok(observer);
  observer.emit(hero);
  rules.stop();
  await flush();

  const events = requestEvents(requests.calls).filter(
    (event) => event.type === "rule",
  );
  assert.deepEqual(
    events.map((event) => event.name),
    ["hero_seen", "section_seen"],
  );
});

test("supports text and XPath view selectors", async () => {
  const browser = installBrowser("https://app.example/");
  const textTarget = new FakeElement("h2");
  textTarget.textContent = "Private analytics for everyone";
  const xpathTarget = new FakeElement("button");
  browser.document.queryResults.set("*", [textTarget]);
  browser.document.xpathResults.set("//button[@data-upgrade]", [xpathTarget]);
  const requests = captureRequests({
    rules: {
      rules: [
        {
          id: "rule_text",
          name: "privacy_heading_seen",
          selector: "private analytics",
          trigger_config: { selector_type: "text" },
          type: "view",
        },
        {
          id: "rule_xpath",
          name: "upgrade_seen",
          selector: "//button[@data-upgrade]",
          trigger_config: { selector_type: "xpath" },
          type: "view",
        },
      ],
    },
  });
  const { trackRules } = await rulesModule;
  const rules = trackRules("site_test", { server: "https://api.example" });
  await flush();
  await flush();

  const observer = FakeIntersectionObserver.instances.at(-1);
  assert.ok(observer.observed.has(textTarget));
  assert.ok(observer.observed.has(xpathTarget));
  observer.emit(textTarget);
  observer.emit(xpathTarget);
  rules.stop();
  await flush();

  assert.deepEqual(
    requestEvents(requests.calls)
      .filter((event) => event.type === "rule")
      .map((event) => event.name),
    ["privacy_heading_seen", "upgrade_seen"],
  );
});

test("rebuilds page-scoped view rules after SPA navigation", async () => {
  const browser = installBrowser("https://app.example/a");
  const hero = new FakeElement("section");
  browser.document.queryResults.set(".hero", [hero]);
  const requests = captureRequests({
    rules: {
      rules: [
        {
          id: "rule_a",
          name: "hero_a_seen",
          selector: ".hero",
          trigger_config: { page_path: "/a" },
          type: "view",
        },
        {
          id: "rule_b",
          name: "hero_b_seen",
          selector: ".hero",
          trigger_config: { page_path: "/b" },
          type: "view",
        },
      ],
    },
  });
  const originalPushState = browser.window.history.pushState;
  const { trackRules } = await rulesModule;
  const rules = trackRules("site_test", { server: "https://api.example" });
  await flush();
  await flush();

  const firstObserver = FakeIntersectionObserver.instances.at(-1);
  assert.ok(firstObserver?.observed.has(hero));
  firstObserver.emit(hero);

  browser.window.history.pushState({}, "", "/b");
  await flush();
  const secondObserver = FakeIntersectionObserver.instances.at(-1);
  assert.notEqual(secondObserver, firstObserver);
  assert.equal(firstObserver.disconnected, true);
  assert.ok(secondObserver?.observed.has(hero));
  secondObserver.emit(hero);
  rules.stop();
  await flush();

  const events = requestEvents(requests.calls).filter(
    (event) => event.type === "rule",
  );
  assert.deepEqual(
    events.map((event) => event.name),
    ["hero_a_seen", "hero_b_seen"],
  );
  assert.equal(browser.window.history.pushState, originalPushState);
});

test("discovers late view targets incrementally without rebuilding observers", async () => {
  const browser = installBrowser("https://app.example/");
  browser.document.queryResults.set(".late", []);
  const requests = captureRequests({
    rules: {
      rules: [
        {
          id: "rule_late",
          name: "late_card_seen",
          selector: ".late",
          type: "view",
        },
      ],
    },
  });
  const { trackRules } = await rulesModule;
  const rules = trackRules("site_test", { server: "https://api.example" });
  await flush();
  await flush();

  const mutationObserver = FakeMutationObserver.instances.at(-1);
  assert.ok(mutationObserver);
  assert.equal(
    mutationObserver.observedTarget,
    browser.document.documentElement,
  );
  assert.deepEqual(mutationObserver.options, {
    childList: true,
    subtree: true,
  });

  const observer = FakeIntersectionObserver.instances.at(-1);
  const firstCard = new FakeElement("article", ["late"]);
  mutationObserver.emit([{ addedNodes: [firstCard], removedNodes: [] }]);
  await flush(220);
  assert.ok(observer?.observed.has(firstCard));
  observer.emit(firstCard, 2);

  const secondCard = new FakeElement("article", ["late"]);
  mutationObserver.emit([{ addedNodes: [secondCard], removedNodes: [] }]);
  await flush(220);
  assert.equal(observer.observed.has(firstCard), false);
  assert.equal(observer.observed.has(secondCard), true);
  observer.emit(secondCard);
  assert.equal(FakeIntersectionObserver.instances.length, 1);
  const observerCountBeforeStop = FakeIntersectionObserver.instances.length;
  mutationObserver.emit([
    { addedNodes: [new FakeElement("div")], removedNodes: [] },
  ]);
  rules.stop();
  await flush(220);

  const events = requestEvents(requests.calls).filter(
    (event) => event.type === "rule",
  );
  assert.deepEqual(
    events.map((event) => event.name),
    ["late_card_seen", "late_card_seen"],
  );
  assert.equal(mutationObserver.disconnected, true);
  assert.equal(observer.disconnected, true);
  assert.equal(
    FakeIntersectionObserver.instances.length,
    observerCountBeforeStop,
    "stop must clear the pending mutation debounce",
  );
});

test("tracks common native interactions on stable attributes without capturing field contents", async () => {
  const browser = installBrowser("https://app.example/");
  const events = ["change", "focus", "blur", "keydown", "play", "ended"];
  const selector = '[data-owleye-track="plan"]';
  const requests = captureRequests({
    rules: {
      rules: events.map((event) => ({
        id: `rule_${event}`,
        name: `plan_${event}`,
        selector,
        type: "click",
        trigger_config: { dom_event: event, selector_type: "css" },
      })),
    },
  });
  const { trackRules } = await rulesModule;
  const tracker = trackRules("site_test", { server: "https://api.example" });
  await flush();
  await flush();
  const field = new FakeElement("input", ["generated-build-a"]);
  field.value = "private-field-value";
  field.closest = (candidate) => (candidate === selector ? field : null);
  for (const event of events)
    browser.document.emit(event, { target: field, type: event });
  field.className = "generated-build-b";
  browser.document.emit("change", { target: field, type: "change" });
  tracker.stop();
  await flush();
  const tracked = requestEvents(requests.calls).filter(
    (event) => event.type === "rule",
  );
  assert.deepEqual(
    tracked.map((event) => event.name),
    [...events.map((event) => `plan_${event}`), "plan_change"],
  );
  assert.equal(JSON.stringify(tracked).includes("private-field-value"), false);
  browser.document.emit("change", { target: field, type: "change" });
  await flush();
  assert.equal(
    requestEvents(requests.calls).filter((event) => event.type === "rule")
      .length,
    7,
  );
});

test("runs server-defined keyboard rules with page scope and bounded DOM fields", async () => {
  const browser = installBrowser("https://app.example/checkout");
  const plan = new FakeElement("input");
  plan.value = "founder";
  browser.document.queryResults.set("#plan", [plan]);
  const requests = captureRequests({
    rules: {
      rules: [
        {
          id: "rule_coupon",
          metadata: {
            custom_props: [
              { key: "plan", selector: "#plan", selector_type: "css" },
              {
                key: "missing",
                selector: ".does-not-exist",
                selector_type: "css",
              },
            ],
            event_name: "coupon_submitted",
          },
          name: "Coupon field",
          selector: "coupon-input",
          trigger_config: {
            dom_event: "keyup",
            key_condition: "enter",
            page_path: "/checkout",
            selector_type: "class",
          },
          type: "click",
        },
      ],
    },
  });
  const { trackRules } = await rulesModule;
  const rules = trackRules("site_test", { server: "https://api.example" });
  await flush();
  await flush();

  const coupon = new FakeElement("input");
  coupon.className = "field coupon-input";
  browser.document.emit("keyup", {
    key: "a",
    target: coupon,
    type: "keyup",
  });
  browser.document.emit("keyup", {
    key: "Enter",
    target: coupon,
    type: "keyup",
  });
  browser.location.href = "/somewhere-else";
  browser.document.emit("keyup", {
    key: "Enter",
    target: coupon,
    type: "keyup",
  });
  rules.stop();
  await flush();

  const events = requestEvents(requests.calls).filter(
    (event) => event.type === "rule",
  );
  assert.equal(events.length, 1);
  assert.equal(events[0].name, "coupon_submitted");
  assert.equal(events[0].data.rule_type, "keyup");
  assert.deepEqual(events[0].data.records, {
    plan: "founder",
  });
});

test("captures custom properties relative to the matched element", async () => {
  const browser = installBrowser("https://app.example/pricing");
  const firstPrice = new FakeElement("span");
  firstPrice.textContent = "$9";
  const secondPrice = new FakeElement("span");
  secondPrice.textContent = "$49";
  browser.document.queryResults.set(".price", [firstPrice]);

  const secondCard = new FakeElement("button");
  secondCard.querySelector = (selector) =>
    selector === ".price" ? secondPrice : null;
  secondCard.querySelectorAll = () => [];
  secondCard.getElementsByClassName = () => [];

  const requests = captureRequests({
    rules: {
      rules: [
        {
          id: "rule_plan",
          metadata: {
            custom_props: [{ key: "price", selector: ".price" }],
          },
          name: "plan_selected",
          selector: "button",
          type: "click",
        },
      ],
    },
  });
  const { trackRules } = await rulesModule;
  const rules = trackRules("site_test", { server: "https://api.example" });
  await flush();
  await flush();

  browser.document.emit("click", { target: secondCard, type: "click" });
  rules.stop();
  await flush();

  const event = requestEvents(requests.calls).find(
    (candidate) => candidate.name === "plan_selected",
  );
  assert.deepEqual(event.data.records, { price: "$49" });
});

test("keeps persisted legacy rule configs compatible with canonical SDK behavior", async () => {
  const browser = installBrowser("https://app.example/search");
  const query = new FakeElement("input");
  query.value = "privacy analytics";
  browser.document.queryResults.set("#query", [query]);
  const requests = captureRequests({
    rules: {
      rules: [
        {
          id: "rule_legacy_search",
          metadata: { event_name: "search_submitted" },
          name: "Legacy search",
          selector: "search-input",
          trigger_config: {
            condition: { key: "Enter", kind: "key" },
            custom_props: [
              { key: "query", selector: "#query", source: "value" },
            ],
            event: "key_up",
            page: "/search",
            selector_type: "id",
          },
          type: "click",
        },
      ],
    },
  });
  const { trackRules } = await rulesModule;
  const rules = trackRules("site_test", { server: "https://api.example" });
  await flush();
  await flush();

  const input = new FakeElement("input");
  input.id = "search-input";
  browser.document.emit("keyup", {
    key: "Enter",
    target: input,
    type: "keyup",
  });
  rules.stop();
  await flush();

  const events = requestEvents(requests.calls).filter(
    (event) => event.type === "rule",
  );
  assert.equal(events.length, 1);
  assert.equal(events[0].name, "search_submitted");
  assert.equal(events[0].data.rule_type, "keyup");
  assert.deepEqual(events[0].data.records, { query: "privacy analytics" });
});

test("invalid SDK calls return safe controllers, log bounded diagnostics, and leave valid tracking usable", async () => {
  installBrowser("https://app.example/");
  const requests = captureRequests();
  const warnings = [];
  const previousWarn = console.warn;
  console.warn = (...args) => warnings.push(args);
  cleanups.push(() => {
    console.warn = previousWarn;
  });
  const { useAnalytics } = await analyticsModule;
  const { trackPerf, trackWebVitals } = await performanceModule;
  const { trackRules } = await rulesModule;
  for (let i = 0; i < 20; i++) {
    const disabled = useAnalytics("invalid private email@example.com");
    assert.doesNotThrow(() => {
      disabled.start();
      disabled.pageview();
      disabled.track("event");
      disabled.setGlobalRecords({ invalid: {} });
      disabled.stop();
      trackPerf(null).start("span")();
      trackPerf(null).observeVitals().stop();
      trackWebVitals(undefined).stop();
      trackRules("").stop();
    });
  }
  assert.equal(warnings.length, 10);
  assert.ok(
    warnings.every(([message]) => !message.includes("email@example.com")),
  );
  assert.equal(requests.calls.length, 0);
  const analytics = useAnalytics("good");
  analytics.track("bad", { nested: {} });
  analytics.setGlobalRecords({ nested: {} });
  const end = trackPerf("good").start("span");
  assert.doesNotThrow(() => end({ nested: {} }));
  analytics.track("still_working");
  await flush();
  assert.deepEqual(
    requestEvents(requests.calls).map((e) => e.name),
    ["page_view", "still_working"],
  );
  analytics.stop();
});

test("SDK failures cannot escape lifecycle, observer, or History callbacks", async () => {
  const browser = installBrowser("https://app.example/");
  captureRequests({
    rules: {
      rules: [{ id: "view", name: "view", selector: ".tile", type: "view" }],
    },
  });
  const { useAnalytics } = await analyticsModule;
  const { trackRules } = await rulesModule;
  const { trackWebVitals } = await performanceModule;
  const analytics = useAnalytics("callbacks");
  const rules = trackRules("callbacks");
  const vitals = trackWebVitals("callbacks");
  await flush();
  await flush();
  Object.defineProperty(browser.document, "title", {
    configurable: true,
    get() {
      throw new Error("host getter failed");
    },
  });
  assert.doesNotThrow(() => browser.window.history.pushState({}, "", "/next"));
  assert.equal(browser.location.pathname, "/next");
  for (const event of ["pagehide", "pageshow"])
    assert.doesNotThrow(() => browser.window.emit(event));
  browser.document.visibilityState = "hidden";
  assert.doesNotThrow(() => browser.document.emit("visibilitychange"));
  assert.doesNotThrow(() =>
    FakePerformanceObserver.forType("paint").callback({
      getEntries() {
        throw new Error("broken entries");
      },
    }),
  );
  assert.doesNotThrow(() =>
    FakeMutationObserver.instances[0].emit([
      {
        get removedNodes() {
          throw new Error("broken record");
        },
      },
    ]),
  );
  const originalDispatch = browser.window.dispatchEvent;
  browser.window.dispatchEvent = () => {
    throw new Error("instrumentation failed");
  };
  assert.doesNotThrow(() =>
    browser.window.history.replaceState({}, "", "/last"),
  );
  assert.equal(browser.location.pathname, "/last");
  browser.window.dispatchEvent = originalDispatch;
  analytics.stop();
  rules.stop();
  vitals.stop();
});

test("partial setup rolls back and one broken cleanup cannot prevent other cleanup", async () => {
  const browser = installBrowser("https://app.example/");
  captureRequests();
  const { useAnalytics } = await analyticsModule;
  const { trackWebVitals } = await performanceModule;
  const originalHistory = browser.window.history.pushState;
  const originalAdd = browser.window.addEventListener;
  browser.window.addEventListener = function (type, ...args) {
    if (type === "pagehide") throw new Error("listener setup failed");
    return originalAdd.call(this, type, ...args);
  };
  assert.doesNotThrow(() => useAnalytics("partial"));
  assert.equal(browser.window.history.pushState, originalHistory);
  for (const listeners of browser.window.listeners.values())
    assert.equal(listeners.size, 0);
  browser.window.addEventListener = originalAdd;
  const analytics = useAnalytics("partial");
  assert.equal(browser.window.listeners.get("pagehide").size, 1);
  const vitals = trackWebVitals("partial");
  FakePerformanceObserver.instances[0].disconnect = () => {
    throw new Error("disconnect failed");
  };
  assert.doesNotThrow(() => vitals.stop());
  assert.ok(
    FakePerformanceObserver.instances
      .slice(1)
      .every((observer) => observer.disconnected),
  );
  analytics.stop();
  for (const listeners of browser.window.listeners.values())
    assert.equal(listeners.size, 0);
});

test("throwing console methods and rejected async enrichment cannot break tracking", async () => {
  const browser = installBrowser("https://app.example/");
  const requests = captureRequests({
    rules: {
      rules: [
        { id: "click", name: "click", type: "click", selector: "button" },
      ],
    },
  });
  const previousWarn = console.warn;
  const previousInfo = console.info;
  console.warn = console.info = () => {
    throw new Error("broken logger");
  };
  cleanups.push(() => {
    console.warn = previousWarn;
    console.info = previousInfo;
  });
  const { useAnalytics } = await analyticsModule;
  const { trackRules } = await rulesModule;
  assert.doesNotThrow(() => useAnalytics(null).track("ignored"));
  const analytics = useAnalytics("logging", { debug: true });
  const rules = trackRules("logging", {
    enrichRule: async () => {
      throw new Error("async hook failure");
    },
  });
  await flush();
  await flush();
  assert.doesNotThrow(() =>
    browser.document.emit("click", {
      type: "click",
      target: new FakeElement("button"),
    }),
  );
  analytics.track("works");
  await flush();
  await flush();
  assert.ok(
    requestEvents(requests.calls).some((event) => event.type === "rule"),
  );
  assert.ok(
    requestEvents(requests.calls).some((event) => event.name === "works"),
  );
  analytics.stop();
  rules.stop();
});

test("CDN initialization tolerates invalid attributes and failing script getters", () => {
  const browser = installBrowser("https://app.example/");
  captureRequests();
  for (const entry of ["analytics", "rules", "performance"]) {
    const source = readFileSync(
      new URL(`../dist/owleye.${entry}.iife.js`, import.meta.url),
      "utf8",
    );
    browser.document.currentScript = { dataset: { owleyeId: "invalid id" } };
    assert.doesNotThrow(() => runInThisContext(source));
    browser.document.currentScript = {
      get dataset() {
        throw new Error("blocked dataset");
      },
    };
    assert.doesNotThrow(() => runInThisContext(source));
  }
  assert.doesNotThrow(() => browser.window.OwlEyeAnalytics.track("ignored"));
  assert.doesNotThrow(() => browser.window.OwlEyeRules.stop());
  assert.doesNotThrow(() =>
    browser.window.OwlEyePerformance.start("ignored")(),
  );
});

test("controller methods keep their context when passed as framework callbacks", async () => {
  const browser = installBrowser("https://app.example/");
  const requests = captureRequests();
  const { useAnalytics } = await analyticsModule;
  const { trackPerf } = await performanceModule;
  const { trackRules } = await rulesModule;
  const { start, stop, track, pageview, setGlobalRecords } = useAnalytics(
    "callbacks_detached",
    { autoStart: false },
  );
  assert.doesNotThrow(() => {
    start();
    setGlobalRecords({ plan: "free" });
    track("detached");
    pageview();
  });
  const { start: startSpan, observeVitals } = trackPerf("callbacks_detached");
  const end = startSpan("span");
  assert.doesNotThrow(() => end());
  const vitals = observeVitals();
  const { stop: stopRules } = trackRules("callbacks_detached");
  await flush();
  await flush();
  assert.doesNotThrow(() => {
    stop();
    stopRules();
    vitals.stop();
  });
  assert.ok(
    requestEvents(requests.calls).some((event) => event.name === "detached"),
  );
  for (const listeners of browser.window.listeners.values())
    assert.equal(listeners.size, 0);
});

function captureRequests({
  eventError,
  eventStatus = 202,
  rules = { rules: [] },
} = {}) {
  const calls = [];
  const beacons = [];
  const previousFetch = globalThis.fetch;
  const previousBeacon = globalThis.navigator.sendBeacon;

  globalThis.fetch = async (url, options = {}) => {
    calls.push({ options, url: String(url) });
    if (String(url).includes("/v1/rules")) {
      return {
        json: async () => rules,
        ok: true,
        status: 200,
      };
    }
    if (eventError) throw eventError;
    return {
      json: async () => ({}),
      ok: eventStatus >= 200 && eventStatus < 300,
      status: eventStatus,
    };
  };
  globalThis.navigator.sendBeacon = (url, body) => {
    beacons.push({ body, url: String(url) });
    return true;
  };

  cleanups.push(() => {
    globalThis.fetch = previousFetch;
    globalThis.navigator.sendBeacon = previousBeacon;
  });
  return { beacons, calls };
}

function requestEvents(calls) {
  return calls
    .filter(isEventRequest)
    .map((request) => JSON.parse(request.options.body).event);
}

function isEventRequest(request) {
  return request.url.endsWith("/v1/events");
}

function installBrowser(
  url,
  referrer = "",
  doNotTrack = "0",
  globalPrivacyControl = false,
) {
  FakeIntersectionObserver.instances.length = 0;
  FakeMutationObserver.instances.length = 0;
  FakePerformanceObserver.instances.length = 0;
  const location = new FakeLocation(url);
  const window = new FakeWindow(location);
  const document = new FakeDocument(referrer);
  const navigator = {
    doNotTrack,
    globalPrivacyControl,
    language: "en-US",
    sendBeacon: () => false,
    userAgent: "Mozilla/5.0 Chrome/124.0.0.0",
  };
  const globals = {
    document,
    Element: FakeElement,
    HTMLElement: FakeElement,
    IntersectionObserver: FakeIntersectionObserver,
    MutationObserver: FakeMutationObserver,
    navigator,
    performance: {
      getEntriesByType: (type) =>
        type === "navigation" ? [{ responseStart: 640 }] : [],
      now: () => 1_000,
    },
    PerformanceObserver: FakePerformanceObserver,
    window,
    XPathResult: { ORDERED_NODE_SNAPSHOT_TYPE: 7 },
  };
  const descriptors = new Map();

  for (const [key, value] of Object.entries(globals)) {
    descriptors.set(key, Object.getOwnPropertyDescriptor(globalThis, key));
    Object.defineProperty(globalThis, key, {
      configurable: true,
      value,
      writable: true,
    });
  }

  const storageAccess = [];
  const guarded = [];
  for (const [target, keys] of [
    [document, ["cookie"]],
    [globalThis.navigator, ["storage", "serviceWorker"]],
    [window, ["name"]],
    [
      window,
      ["localStorage", "sessionStorage", "indexedDB", "cookieStore", "caches"],
    ],
    [
      globalThis,
      ["localStorage", "sessionStorage", "indexedDB", "cookieStore", "caches"],
    ],
  ]) {
    for (const key of keys) {
      guarded.push([target, key, Object.getOwnPropertyDescriptor(target, key)]);
      const fail = () => {
        storageAccess.push(key);
        throw new Error(`SDK accessed ${key}`);
      };
      Object.defineProperty(target, key, {
        configurable: true,
        get: fail,
        set: fail,
      });
    }
  }
  cleanups.push(() => {
    for (const [target, key, descriptor] of guarded) {
      if (descriptor) Object.defineProperty(target, key, descriptor);
      else delete target[key];
    }
    assert.deepEqual(
      storageAccess,
      [],
      "SDK must never read or write cookies or persistent storage",
    );
  });

  cleanups.push(() => {
    for (const [key, descriptor] of descriptors) {
      if (descriptor) Object.defineProperty(globalThis, key, descriptor);
      else delete globalThis[key];
    }
  });
  return { document, location, window };
}

class FakeEventTarget {
  listeners = new Map();

  addEventListener(type, listener) {
    const listeners = this.listeners.get(type) ?? new Set();
    listeners.add(listener);
    this.listeners.set(type, listeners);
  }

  removeEventListener(type, listener) {
    this.listeners.get(type)?.delete(listener);
  }

  dispatchEvent(event) {
    this.emit(event.type, event);
    return true;
  }

  emit(type, event = { type }) {
    for (const listener of this.listeners.get(type) ?? []) {
      if (typeof listener === "function") listener.call(this, event);
      else listener.handleEvent(event);
    }
  }
}

class FakePerformanceObserver {
  static instances = [];

  constructor(callback) {
    this.callback = callback;
    FakePerformanceObserver.instances.push(this);
  }

  observe(options) {
    this.type = options.type;
  }

  disconnect() {
    this.disconnected = true;
  }

  emit(entries) {
    this.callback({ getEntries: () => entries });
  }

  static forType(type) {
    const observer = FakePerformanceObserver.instances.find(
      (candidate) => candidate.type === type,
    );
    assert.ok(observer, `expected a ${type} PerformanceObserver`);
    return observer;
  }
}

class FakeDocument extends FakeEventTarget {
  constructor(referrer) {
    super();
    this.referrer = referrer;
  }

  referrer;
  documentElement = new FakeElement("html");
  queryCounts = new Map();
  queryResults = new Map();
  xpathResults = new Map();
  title = "OwlEye test";
  visibilityState = "visible";

  querySelectorAll(selector) {
    this.queryCounts.set(selector, (this.queryCounts.get(selector) ?? 0) + 1);
    return this.queryResults.get(selector) ?? [];
  }

  evaluate(expression) {
    const elements = this.xpathResults.get(expression) ?? [];
    return {
      snapshotItem: (index) => elements[index] ?? null,
      snapshotLength: elements.length,
    };
  }
}

class FakeIntersectionObserver {
  static instances = [];

  constructor(callback) {
    this.callback = callback;
    FakeIntersectionObserver.instances.push(this);
  }

  callback;
  disconnected = false;
  observed = new Set();

  disconnect() {
    this.disconnected = true;
    this.observed.clear();
  }

  emit(target, repeats = 1) {
    this.callback(
      Array.from({ length: repeats }, () => ({ isIntersecting: true, target })),
      this,
    );
  }

  observe(target) {
    this.observed.add(target);
  }

  unobserve(target) {
    this.observed.delete(target);
  }
}

class FakeMutationObserver {
  static instances = [];

  constructor(callback) {
    this.callback = callback;
    FakeMutationObserver.instances.push(this);
  }

  callback;
  disconnected = false;
  observedTarget;
  options;

  disconnect() {
    this.disconnected = true;
  }

  emit(records = []) {
    this.callback(records, this);
  }

  observe(target, options) {
    this.observedTarget = target;
    this.options = options;
  }
}

class FakeElement {
  constructor(tagName, classes = []) {
    this.tagName = tagName.toUpperCase();
    this.className = classes.join(" ");
  }

  className = "";
  dataset = {};
  id = "";
  innerText = "";
  isConnected = true;
  parentElement = null;
  tagName;
  textContent = null;
  value = undefined;

  closest(selector) {
    return selector.toLowerCase() === this.tagName.toLowerCase() ? this : null;
  }

  contains(element) {
    return element === this;
  }

  getElementsByClassName() {
    return [];
  }

  matches(selector) {
    if (selector.startsWith(".")) {
      return this.className.split(/\s+/).includes(selector.slice(1));
    }
    if (selector.startsWith("#")) return this.id === selector.slice(1);
    return selector.toLowerCase() === this.tagName.toLowerCase();
  }

  querySelector() {
    return null;
  }

  querySelectorAll() {
    return [];
  }
}

class FakeLocation {
  constructor(url) {
    this.url = new URL(url);
  }

  url;

  get hash() {
    return this.url.hash;
  }

  set hash(value) {
    this.url.hash = value;
  }

  get host() {
    return this.url.host;
  }

  get href() {
    return this.url.href;
  }

  set href(value) {
    this.url = new URL(value, this.url);
  }

  get origin() {
    return this.url.origin;
  }

  get pathname() {
    return this.url.pathname;
  }

  get search() {
    return this.url.search;
  }
}

class FakeHistory {
  constructor(location) {
    this.location = location;
  }

  location;

  pushState(_data, _unused, url) {
    if (url !== undefined && url !== null) this.location.href = String(url);
  }

  replaceState(_data, _unused, url) {
    if (url !== undefined && url !== null) this.location.href = String(url);
  }
}

class FakeWindow extends FakeEventTarget {
  constructor(location) {
    super();
    this.location = location;
    this.history = new FakeHistory(location);
  }

  devicePixelRatio = 1;
  history;
  innerHeight = 720;
  innerWidth = 1280;
  IntersectionObserver = FakeIntersectionObserver;
  location;
  screen = { height: 1080, width: 1920 };

  matchMedia() {
    return { matches: false };
  }
}

function flush(delay = 0) {
  return new Promise((resolve) => setTimeout(resolve, delay));
}

test("CDN globals accept ten primitive properties and reject eleven or unsupported values", async () => {
  const browser = installBrowser(
    "https://app.example/?utm_source=newsletter&utm_medium=email&utm_campaign=launch",
  );
  const requests = captureRequests();
  browser.document.currentScript = {
    dataset: {
      owleyeId: "cdn_properties",
      owleyeServer: "https://api.example",
      owleyeCaptureCampaigns: "true",
    },
  };
  for (const entry of ["analytics", "performance"]) {
    runInThisContext(
      readFileSync(
        new URL(`../dist/owleye.${entry}.iife.js`, import.meta.url),
        "utf8",
      ),
    );
  }
  const ten = Object.fromEntries(
    Array.from({ length: 10 }, (_, i) => [
      `field${i}`,
      i % 3 === 0 ? true : i % 3 === 1 ? "value" : i,
    ]),
  );
  const analytics = browser.window.OwlEyeAnalytics;
  analytics.track("accepted_ten", ten);
  analytics.track("rejected_eleven", { ...ten, extra: 1 });
  for (const value of [
    null,
    undefined,
    [],
    {},
    new Date(),
    1n,
    () => 1,
    NaN,
    Infinity,
    -Infinity,
  ]) {
    analytics.track("rejected_type", { value });
  }
  browser.window.OwlEyePerformance.start("ten_span", ten)(ten);
  browser.window.OwlEyePerformance.start("invalid_start", {
    ...ten,
    extra: true,
  })();
  browser.window.OwlEyePerformance.start("invalid_end")({
    ...ten,
    extra: true,
  });
  analytics.stop();
  browser.window.OwlEyePerformance.observeVitals().stop();
  await flush();
  const events = requestEvents(requests.calls);
  assert.deepEqual(
    events.find((e) => e.name === "accepted_ten").data.records,
    ten,
  );
  assert.equal(
    events.some(
      (e) => e.name.startsWith("rejected_") || e.name.startsWith("invalid_"),
    ),
    false,
  );
  const span = events.find((e) => e.name === "ten_span");
  assert.deepEqual(span.data.start, ten);
  assert.deepEqual(span.data.end, ten);
  assert.match(
    events.find((e) => e.name === "accepted_ten").page.search,
    /utm_campaign=launch/,
  );
});

test("rule DOM captures keep ten fields, omit missing values, and preserve configured values over enrichment", async () => {
  const browser = installBrowser("https://app.example/");
  const fields = Array.from({ length: 10 }, (_, i) => ({
    key: `field${i}`,
    selector: `#field${i}`,
  }));
  for (let i = 0; i < 10; i++) {
    const el = new FakeElement("input");
    el.value = `value${i}`;
    browser.document.queryResults.set(`#field${i}`, [el]);
  }
  const requests = captureRequests({
    rules: {
      rules: [
        {
          id: "ten_rules",
          name: "ten_rules",
          selector: "button",
          type: "click",
          metadata: { custom_props: fields },
        },
      ],
    },
  });
  const { trackRules } = await rulesModule;
  const tracker = trackRules("ten_rule_fields", {
    enrichRule: () => ({ field0: "must_not_replace", extra: true }),
  });
  await flush();
  await flush();
  browser.document.emit("click", {
    target: new FakeElement("button"),
    type: "click",
  });
  tracker.stop();
  await flush();
  const data = requestEvents(requests.calls).find((e) => e.type === "rule").data
    .records;
  assert.equal(Object.keys(data).length, 10);
  assert.equal(data.field0, "value0");
  assert.equal(data.field9, "value9");
  assert.equal(data.extra, undefined);
});
