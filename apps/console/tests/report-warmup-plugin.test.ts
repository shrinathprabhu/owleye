import assert from "node:assert/strict";
import test from "node:test";
import { effectScope, nextTick, reactive, ref, watch } from "vue";

test("client warming requires mounted authorized analytics context and respects public routes/data saving/cleanup", async () => {
  const scope = effectScope();
  const route = reactive({ path: "/" });
  const states = new Map([
    [
      "console-workspace-session",
      ref<any>({
        authenticated: true,
        user: { id: "owner" },
        onboarding: { next_step: "complete" },
      }),
    ],
    [
      "console-workspace-sites",
      ref<any>([
        {
          id: "site",
          tracking_id: "owl_a",
          permissions: { analytics_read: true },
        },
      ]),
    ],
    ["console-workspace-selected-site", ref<any>("owl_a")],
    ["console-workspace-validated-at", ref<any>(1)],
  ]);
  const connection = { saveData: false, effectiveType: "4g" };
  const documentState = { visibilityState: "visible" };
  const listeners = new Map<string, () => void>();
  const warmed: string[] = [];
  let cleared = 0;
  let mounted!: () => void;
  let unmount!: () => void;
  const mocks = {
    defineNuxtPlugin: (plugin: Function) => plugin,
    useRoute: () => route,
    useState: (key: string) => states.get(key),
    watch,
    useApi: () => ({
      warmReport: (report: { key: string }) => warmed.push(report.key),
      reportWarmupQueue: { clear: () => cleared++ },
    }),
    navigator: { connection, onLine: true },
    document: {
      get visibilityState() {
        return documentState.visibilityState;
      },
      addEventListener: (name: string, handler: () => void) =>
        listeners.set(name, handler),
      removeEventListener: (name: string) => listeners.delete(name),
    },
  };
  const previous = new Map(
    Object.keys(mocks).map((key) => [
      key,
      Object.getOwnPropertyDescriptor(globalThis, key),
    ]),
  );
  for (const [key, value] of Object.entries(mocks))
    Object.defineProperty(globalThis, key, {
      configurable: true,
      writable: true,
      value,
    });
  try {
    const plugin = (await import("../plugins/reportWarmup.client.ts")).default;
    scope.run(() =>
      plugin({
        hook: (_: string, callback: () => void) => {
          mounted = callback;
        },
        vueApp: {
          onUnmount: (callback: () => void) => {
            unmount = callback;
          },
        },
      } as any),
    );
    assert.equal(warmed.length, 0);
    mounted();
    assert.equal(warmed.length, 3);
    assert.ok(
      warmed.every(
        (key) => key.includes("owl_a") && !key.startsWith("overview:"),
      ),
    );
    route.path = "/share";
    await nextTick();
    assert.equal(warmed.length, 3);
    assert.equal(cleared, 1);
    connection.saveData = true;
    route.path = "/performance";
    await nextTick();
    assert.equal(warmed.length, 3);
    connection.saveData = false;
    states.get("console-workspace-sites")!.value[0].permissions.analytics_read =
      false;
    await nextTick();
    assert.equal(warmed.length, 3);
    states.get("console-workspace-sites")!.value = [
      {
        id: "other",
        tracking_id: "owl_b",
        permissions: { analytics_read: true },
      },
    ];
    states.get("console-workspace-selected-site")!.value = "owl_b";
    await nextTick();
    assert.equal(warmed.length, 6);
    assert.ok(warmed.slice(3).every((key) => key.includes("owl_b")));
    documentState.visibilityState = "hidden";
    listeners.get("visibilitychange")!();
    assert.equal(cleared, 2);
    states.get("console-workspace-session")!.value = { authenticated: false };
    documentState.visibilityState = "visible";
    listeners.get("visibilitychange")!();
    await nextTick();
    assert.equal(warmed.length, 6);
    unmount();
    assert.equal(listeners.size, 0);
  } finally {
    scope.stop();
    for (const [key, descriptor] of previous) {
      if (descriptor) Object.defineProperty(globalThis, key, descriptor);
      else Reflect.deleteProperty(globalThis, key);
    }
  }
});
