import assert from "node:assert/strict";
import test from "node:test";
import { computed, effectScope, ref, watch } from "vue";
import { useApi } from "../composables/useApi.ts";
import { reportWarmups } from "../utils/reportWarmup.ts";

test("API cache follows identity, permissions and writes; app instances stay isolated", () => {
  const scope = effectScope();
  let app = {};
  const states = new Map<string, ReturnType<typeof ref>>();
  const options: Array<{
    onRequest?: (event: { options: { method?: string } }) => void;
    onResponse?: (event: { options: { method?: string } }) => void;
    onResponseError?: (event: { response: { status: number } }) => void;
  }> = [];
  const mocks = {
    computed,
    watch,
    useNuxtApp: () => app,
    useRuntimeConfig: () => ({
      public: { apiBase: "https://api.example.test" },
    }),
    useState: (key: string, initial: () => unknown) => {
      if (!states.has(key)) states.set(key, ref(initial()));
      return states.get(key);
    },
    $fetch: {
      create: (value: (typeof options)[number]) => {
        options.push(value);
        return () => Promise.resolve({});
      },
    },
  };
  const previous = new Map(
    Object.keys(mocks).map((key) => [
      key,
      Object.getOwnPropertyDescriptor(globalThis, key),
    ]),
  );
  Object.assign(globalThis, mocks);
  try {
    const first = scope.run(() => useApi())!;
    const second = scope.run(() => useApi())!;
    first.responseCache.write("one", 1);
    assert.equal(second.responseCache.read("one")?.value, 1);
    states.get("console-workspace-session")!.value = {
      authenticated: true,
      user: { id: "one" },
    };
    assert.equal(first.responseCache.read("one"), undefined);
    first.responseCache.write("one", 1);
    states.get("console-workspace-sites")!.value = [
      { id: "site", permissions: { analytics_read: false } },
    ];
    assert.equal(first.responseCache.read("one"), undefined);
    for (const status of [401, 403]) {
      first.responseCache.write("one", 1);
      options[0]!.onResponseError!({ response: { status } });
      assert.equal(first.responseCache.read("one"), undefined);
    }
    first.responseCache.write("one", 1);
    options[0]!.onRequest!({ options: { method: "GET" } });
    assert.ok(first.responseCache.read("one"));
    options[0]!.onRequest!({ options: { method: "POST" } });
    assert.equal(first.responseCache.read("one"), undefined);
    first.responseCache.write("one", 1);
    options[0]!.onResponse!({ options: { method: "DELETE" } });
    assert.equal(first.responseCache.read("one"), undefined);
    first.responseCache.write("one", 1);
    app = {};
    const isolated = scope.run(() => useApi())!;
    assert.equal(isolated.responseCache.read("one"), undefined);
    states.get("console-workspace-session")!.value = { authenticated: false };
    assert.equal(first.responseCache.read("one"), undefined);
  } finally {
    scope.stop();
    for (const [key, descriptor] of previous) {
      if (descriptor) Object.defineProperty(globalThis, key, descriptor);
      else Reflect.deleteProperty(globalThis, key);
    }
  }
});

test("warm reads reuse one network call and invalidation prevents a canceled scope from refilling or joining", async (t) => {
  t.mock.timers.enable({ apis: ["setTimeout"] });
  const scope = effectScope();
  const states = new Map<string, ReturnType<typeof ref>>([
    [
      "console-workspace-session",
      ref({ authenticated: true, user: { id: "owner" } }),
    ],
    ["console-workspace-selected-site", ref("owl_site")],
    [
      "console-workspace-sites",
      ref([
        {
          id: "site",
          tracking_id: "owl_site",
          permissions: { analytics_read: true },
        },
      ]),
    ],
  ]);
  const requests: Array<{
    url: string;
    options: any;
    resolve: (value: unknown) => void;
    background: boolean;
  }> = [];
  const app = {};
  const mocks = {
    computed,
    watch,
    useNuxtApp: () => app,
    useRuntimeConfig: () => ({
      public: { apiBase: "https://api.example.test" },
    }),
    useState: (key: string, initial: () => unknown) => {
      if (!states.has(key)) states.set(key, ref(initial()));
      return states.get(key);
    },
    $fetch: {
      create:
        (hooks: any) =>
        (url: string, options: any = {}) =>
          new Promise((resolve) => {
            hooks.onRequest?.({ options });
            requests.push({
              url,
              options,
              background: hooks.retry === 0,
              resolve: (value) => {
                hooks.onResponse?.({ options });
                resolve(value);
              },
            });
          }),
    },
  };
  const previous = new Map(
    Object.keys(mocks).map((key) => [
      key,
      Object.getOwnPropertyDescriptor(globalThis, key),
    ]),
  );
  Object.assign(globalThis, mocks);
  try {
    const api = scope.run(() => useApi())!;
    const report = reportWarmups("owl_site", "/").find((item) =>
      item.key.startsWith("events:"),
    )!;
    api.warmReport({
      key: "secret",
      url: "/v1/sites/other/api-keys",
      query: {},
    });
    api.warmReport(report);
    t.mock.timers.tick(800);
    await Promise.resolve();
    assert.equal(requests.length, 1);
    assert.equal(requests[0]!.background, true);
    const joined = api.api(report.url, { query: report.query });
    assert.equal(requests.length, 1);
    requests[0]!.resolve({ items: ["safe"] });
    assert.deepEqual(await joined, { items: ["safe"] });
    await Promise.resolve();
    assert.deepEqual(api.responseCache.read(report.key)?.value, {
      items: ["safe"],
    });

    api.responseCache.clear();
    api.warmReport(report);
    t.mock.timers.tick(800);
    await Promise.resolve();
    assert.equal(requests.length, 2);
    states.get("console-workspace-sites")!.value = [
      {
        id: "site",
        tracking_id: "owl_site",
        permissions: { analytics_read: false },
      },
    ];
    assert.equal(requests[1]!.options.signal.aborted, true);
    // Even a transport that ignores cancellation cannot refill or be joined.
    const next = api.api(report.url, { query: report.query });
    assert.equal(requests.length, 3);
    assert.equal(requests[2]!.background, false);
    requests[1]!.resolve({ items: ["obsolete"] });
    await Promise.resolve();
    await Promise.resolve();
    assert.equal(api.responseCache.read(report.key), undefined);
    requests[2]!.resolve({ items: [] });
    await next;
    api.warmReport(report);
    t.mock.timers.tick(800);
    await Promise.resolve();
    assert.equal(requests.length, 3);
  } finally {
    scope.stop();
    for (const [key, descriptor] of previous) {
      if (descriptor) Object.defineProperty(globalThis, key, descriptor);
      else Reflect.deleteProperty(globalThis, key);
    }
  }
});
