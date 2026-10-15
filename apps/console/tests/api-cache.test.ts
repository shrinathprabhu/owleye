import assert from "node:assert/strict";
import test from "node:test";
import { computed, effectScope, ref, watch } from "vue";
import { useApi } from "../composables/useApi.ts";

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
