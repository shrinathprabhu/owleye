import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { stripTypeScriptTypes } from "node:module";
import { runInNewContext } from "node:vm";
import test, { type TestContext } from "node:test";
import { computed, effectScope, reactive, ref, watch } from "vue";
import { useConsoleWorkspace } from "../composables/useConsoleWorkspace.ts";
import type { WorkspaceSession, WorkspaceSite } from "../types/workspace.ts";
import { AUTH_SESSION_INVALIDATED_EVENT } from "../utils/authSchedule.ts";
import { API_UNAUTHORIZED_EVENT } from "../composables/useApi.ts";

const session = (id = "owner"): WorkspaceSession => ({
  authenticated: true,
  access_expires_at: new Date(Date.now() + 3600000).toISOString(),
  onboarding: {
    has_site: true,
    next_step: "complete",
    two_factor_prompt_handled: true,
  },
  user: { id, email: `${id}@example.test`, two_factor_enabled: true },
});
const site = (id = "one"): WorkspaceSite => ({
  id,
  tracking_id: `owl_${id}`,
  name: id,
  domain: `${id}.test`,
  role: "owner",
  timezone: "UTC",
  created_at: "2026-09-01T00:00:00Z",
  updated_at: "2026-09-01T00:00:00Z",
});
function deferred<T>() {
  let resolve!: (value: T) => void;
  let reject!: (reason: unknown) => void;
  const promise = new Promise<T>((yes, no) => {
    resolve = yes;
    reject = no;
  });
  return { promise, resolve, reject };
}
function harness(t: TestContext) {
  const states = new Map<string, ReturnType<typeof ref>>();
  const app = {};
  const events = new EventTarget();
  const route = reactive({
    path: "/settings",
    query: {} as Record<string, string>,
    meta: {} as Record<string, unknown>,
  });
  let mounts: Array<() => void> = [];
  let unmounts: Array<() => void> = [];
  let sessionCalls = 0;
  let siteCalls = 0;
  let nextSession = async (): Promise<WorkspaceSession> => session();
  let nextSites = async (): Promise<WorkspaceSite[]> => [site(), site("two")];
  const original = new Map<string, PropertyDescriptor | undefined>();
  const mocks = {
    ref,
    computed,
    watch,
    useNuxtApp: () => app,
    useRoute: () => route,
    useRouter: () => ({
      replace: async ({ query }: { query: Record<string, string> }) => {
        route.query = query;
      },
    }),
    useState: (key: string, create: () => unknown) => {
      if (!states.has(key)) states.set(key, ref(create()));
      return states.get(key);
    },
    useApi: () => ({
      apiBase: ref("http://localhost:6174"),
      api: async (path: string) => {
        if (path === "/v1/sites") {
          siteCalls++;
          return nextSites();
        }
        if (path === "/v1/auth/logout") return {};
        throw new Error(`Unexpected API path ${path}`);
      },
      publicApi: async () => ({ status: "ready" }),
    }),
    useAuthSessionRefresh: () => {},
    fetchSessionWithRefresh: async () => {
      sessionCalls++;
      return nextSession();
    },
    onMounted: (fn: () => void) => mounts.push(fn),
    onBeforeUnmount: (fn: () => void) => unmounts.push(fn),
    navigateTo: async (path: string) => {
      route.path = path;
    },
    window: events,
    localStorage: { setItem: () => {} },
  };
  for (const [key, value] of Object.entries(mocks)) {
    original.set(key, Object.getOwnPropertyDescriptor(globalThis, key));
    Object.defineProperty(globalThis, key, {
      value,
      configurable: true,
      writable: true,
    });
  }
  const cleanups: Array<() => void> = [];
  t.after(() => {
    cleanups.forEach((fn) => fn());
    for (const [key, descriptor] of original) {
      if (descriptor) Object.defineProperty(globalThis, key, descriptor);
      else Reflect.deleteProperty(globalThis, key);
    }
  });
  function mount() {
    mounts = [];
    unmounts = [];
    const scope = effectScope();
    const workspace = scope.run(() => useConsoleWorkspace())!;
    const mounted = mounts;
    const beforeUnmount = unmounts;
    mounted.forEach((fn) => fn());
    let stopped = false;
    const stop = () => {
      if (stopped) return;
      stopped = true;
      beforeUnmount.forEach((fn) => fn());
      scope.stop();
    };
    cleanups.push(stop);
    return { workspace, stop };
  }
  return {
    mount,
    states,
    route,
    events,
    calls: () => ({ sessionCalls, siteCalls }),
    setSession: (fn: typeof nextSession) => {
      nextSession = fn;
    },
    setSites: (fn: typeof nextSites) => {
      nextSites = fn;
    },
  };
}

test("route navigation preserves the shell and skips redundant workspace requests", async (t) => {
  const h = harness(t);
  const first = h.mount();
  await first.workspace.initialize();
  const originalSite = first.workspace.selectedSite.value;
  first.stop();
  h.route.path = "/events";
  const next = h.mount();
  next.workspace.prepareForNavigation();
  assert.equal(next.workspace.loading.value, false);
  assert.equal(next.workspace.selectedSite.value, originalSite);
  await next.workspace.initialize({ force: false });
  assert.deepEqual(h.calls(), { sessionCalls: 1, siteCalls: 1 });
  h.route.query = { site: "owl_two" };
  next.workspace.prepareForNavigation();
  assert.equal(next.workspace.selectedSite.value?.id, "two");
});

test("background refresh keeps content and stable site references; concurrent callers share requests", async (t) => {
  const h = harness(t);
  const { workspace } = h.mount();
  await workspace.initialize();
  const originalSite = workspace.selectedSite.value;
  const pending = deferred<WorkspaceSession>();
  h.setSession(() => pending.promise);
  const first = workspace.initialize();
  const second = workspace.initialize();
  assert.equal(workspace.loading.value, false);
  assert.equal(workspace.selectedSite.value, originalSite);
  assert.equal(h.calls().sessionCalls, 2);
  pending.resolve(session());
  await Promise.all([first, second]);
  assert.equal(workspace.selectedSite.value, originalSite);
  assert.equal(h.calls().siteCalls, 2);
  h.setSites(async () => {
    throw new Error("offline");
  });
  await workspace.initialize();
  assert.equal(workspace.selectedSite.value, originalSite);
  assert.match(workspace.error.value, /refreshed/);
});

test("identity changes clear the old catalog before a different account's catalog arrives", async (t) => {
  const h = harness(t);
  const { workspace } = h.mount();
  await workspace.initialize();
  const catalog = deferred<WorkspaceSite[]>();
  h.setSession(async () => session("other"));
  h.setSites(() => catalog.promise);
  const pending = workspace.initialize();
  await new Promise(setImmediate);
  assert.equal(workspace.user.value?.id, "other");
  assert.equal(workspace.selectedSite.value, null);
  assert.equal(workspace.loading.value, true);
  catalog.resolve([site("private-other")]);
  await pending;
  assert.equal(workspace.selectedSiteId.value, "owl_private-other");
});

test("logout and rejected sessions discard cached identity and ignore late responses", async (t) => {
  const h = harness(t);
  const { workspace } = h.mount();
  await workspace.initialize();
  const pending = deferred<WorkspaceSession>();
  h.setSession(() => pending.promise);
  const refresh = workspace.initialize();
  await workspace.signOut();
  pending.resolve(session());
  await refresh;
  assert.equal(workspace.isAuthenticated.value, false);
  assert.equal(workspace.sites.value.length, 0);
  h.setSession(async () => session());
  await workspace.initialize();
  h.events.dispatchEvent(new Event(AUTH_SESSION_INVALIDATED_EVENT));
  assert.equal(workspace.isAuthenticated.value, false);
  assert.equal(workspace.sites.value.length, 0);
});

test("removed memberships replace the catalog even during a background refresh", async (t) => {
  const h = harness(t);
  const { workspace } = h.mount();
  await workspace.initialize();
  h.setSites(async () => []);
  await workspace.initialize();
  assert.equal(workspace.selectedSite.value, null);
  assert.equal(workspace.permissions.value.settings_manage, false);
  assert.equal(workspace.loading.value, false);
});

test("cached sessions still enforce onboarding navigation", async (t) => {
  const h = harness(t);
  h.route.path = "/";
  h.setSession(
    async () =>
      ({
        ...session(),
        onboarding: {
          has_site: false,
          next_step: "create_site",
          two_factor_prompt_handled: true,
        },
      }) as WorkspaceSession,
  );
  const first = h.mount();
  await first.workspace.initialize();
  first.stop();
  h.route.path = "/settings";
  await h.mount().workspace.initialize({ force: false });
  assert.equal(h.route.path, "/");
  assert.equal(h.calls().sessionCalls, 1);
});

test("explicitly rejected authentication clears cached content", async (t) => {
  const h = harness(t);
  const { workspace } = h.mount();
  await workspace.initialize();
  h.setSession(async () => {
    throw { status: 403 };
  });
  await workspace.initialize();
  assert.equal(workspace.isAuthenticated.value, false);
  assert.equal(workspace.sites.value.length, 0);
});

test("first load honors the requested project and keeps public health after sign-out", async (t) => {
  const h = harness(t);
  h.route.query = { site: "owl_two" };
  const { workspace } = h.mount();
  await workspace.initialize();
  assert.equal(workspace.selectedSite.value?.id, "two");
  const health = workspace.health.value;
  await workspace.signOut();
  assert.equal(workspace.health.value, health);
});

test("a rejected catalog request does not recursively revalidate", async (t) => {
  const h = harness(t);
  const { workspace } = h.mount();
  await workspace.initialize();
  h.setSites(async () => {
    h.events.dispatchEvent(new Event(API_UNAUTHORIZED_EVENT));
    throw { status: 401 };
  });
  await workspace.initialize();
  assert.equal(workspace.isAuthenticated.value, false);
  assert.equal(workspace.sites.value.length, 0);
  assert.equal(workspace.loading.value, false);
  assert.deepEqual(h.calls(), { sessionCalls: 2, siteCalls: 2 });
});

test("creating an app refreshes the cached catalog before opening its settings", async (t) => {
  const h = harness(t);
  const previous = h.mount();
  await previous.workspace.initialize();
  await previous.workspace.selectSite("owl_two");
  previous.stop();

  h.route.path = "/app/create";
  h.route.query = {};
  h.route.meta = { siteScoped: false };
  const creation = h.mount();
  const catalog = deferred<WorkspaceSite[]>();
  h.setSites(() => catalog.promise);

  // Exercise the actual page handler against the real workspace composable.
  const source = readFileSync(
    new URL("../pages/app/create.vue", import.meta.url),
    "utf8",
  ).match(/<script setup lang="ts">([\s\S]*?)<\/script>/)?.[1];
  assert.ok(source);
  const changed = runInNewContext(`${stripTypeScriptTypes(source)}; changed;`, {
    definePageMeta: () => {},
    useHead: () => {},
    useConsoleWorkspace: () => creation.workspace,
    navigateTo: async ({
      path,
      query,
    }: {
      path: string;
      query: Record<string, string>;
    }) => {
      h.route.path = path;
      h.route.query = query;
      h.route.meta = {};
    },
  }) as (trackingId?: string) => Promise<void>;

  await changed();
  assert.equal(h.route.path, "/app/create");
  const completion = changed("owl_new");
  await new Promise(setImmediate);
  assert.equal(h.route.path, "/app/create", "wait for the refreshed catalog");
  assert.equal(creation.workspace.selectedSite.value?.id, "two");
  catalog.resolve([site(), site("two"), site("new")]);
  await completion;
  assert.equal(h.route.path, "/settings");
  assert.equal(h.route.query.site, "owl_new");
  creation.stop();

  const settings = h.mount();
  settings.workspace.prepareForNavigation();
  await settings.workspace.initialize({ force: false });
  assert.equal(settings.workspace.selectedSite.value?.id, "new");
  assert.equal(settings.workspace.permissions.value.settings_manage, true);
  assert.deepEqual(h.states.get("console-last-site-selection")?.value, {
    userId: "owner",
    siteId: "owl_new",
  });
  assert.deepEqual(h.calls(), { sessionCalls: 2, siteCalls: 2 });

  settings.stop();
  h.route.path = "/events";
  h.route.query = {};
  const events = h.mount();
  events.workspace.prepareForNavigation();
  assert.equal(events.workspace.selectedSite.value?.id, "new");
});
