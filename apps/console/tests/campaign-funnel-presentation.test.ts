import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";
import ts from "typescript";
import * as Vue from "vue";
import { compileScript, compileTemplate, parse } from "vue/compiler-sfc";
import { renderToString } from "vue/server-renderer";
import { funnelBarWidth, funnelPercent } from "../utils/funnelPresentation.ts";
import { apiErrorMessage, apiErrorStatus } from "../utils/apiError.ts";
import { routes } from "../utils/routes.ts";
import { ResponseCache } from "../utils/responseCache.ts";
import type { ProViewFunnelPreviewStep } from "../types/pro-view.ts";
import type { CampaignsResponse } from "../types/insights.ts";

function component(name: string) {
  const filename = new URL(`../components/${name}.vue`, import.meta.url);
  return { filename, ...parse(readFileSync(filename, "utf8")) };
}
function setup(
  name: string,
  props: object,
  extra: Record<string, unknown> = {},
) {
  const { descriptor } = component(name);
  const source = ts
    .transpileModule(compileScript(descriptor, { id: name }).content, {
      compilerOptions: {
        module: ts.ModuleKind.ESNext,
        target: ts.ScriptTarget.ESNext,
      },
    })
    .outputText.replace(/^import .+$/gm, "")
    .replace("export default", "return");
  const environment = {
    _defineComponent: Vue.defineComponent,
    ref: Vue.ref,
    shallowRef: Vue.shallowRef,
    reactive: Vue.reactive,
    computed: Vue.computed,
    watch: () => {},
    onBeforeUnmount: () => {},
    funnelBarWidth,
    funnelPercent,
    requestError: apiErrorMessage,
    apiErrorStatus,
    routes,
    ...extra,
  };
  const compiled = new Function(...Object.keys(environment), source)(
    ...Object.values(environment),
  );
  return compiled.setup(props, { expose: () => {} });
}
function renderFunction(name: string) {
  const { filename, descriptor } = component(name);
  const compiled = compileTemplate({
    source: descriptor.template!.content,
    filename: filename.pathname,
    id: name,
  });
  assert.deepEqual(compiled.errors, []);
  const source = ts
    .transpileModule(compiled.code, {
      compilerOptions: {
        module: ts.ModuleKind.ESNext,
        target: ts.ScriptTarget.ESNext,
      },
    })
    .outputText.replace(
      /import \{([^}]+)\} from "vue"/,
      (_all, names: string) =>
        `const {${names.replace(/\bas\b/g, ":")}} = Vue;`,
    )
    .replace("export function render", "return function render");
  return new Function("Vue", source)(Vue);
}
async function render(name: string, bindings: object) {
  const app = Vue.createSSRApp({
    render: renderFunction(name),
    setup: () => bindings,
  });
  app.component("ConsoleDropdown", { render: () => null });
  return renderToString(app);
}
function findButton(node: Vue.VNode, label: string): Vue.VNode | undefined {
  if (
    node.type === "button" &&
    typeof node.children === "string" &&
    node.children.trim() === label
  )
    return node;
  if (Array.isArray(node.children)) {
    for (const child of node.children) {
      if (Vue.isVNode(child)) {
        const match = findButton(child, label);
        if (match) return match;
      }
    }
  }
}
function steps(length: number): ProViewFunnelPreviewStep[] {
  return Array.from({ length }, (_, index) => ({
    id: `step-${index}`,
    name:
      index === 0
        ? "Landing → pricing · 文"
        : `A deliberately long funnel stage ${index + 1} with clear meaning`,
    count: index === length - 1 ? 0 : 100 - index * 9,
    dropoff_count: index ? 9 : 0,
    dropoff_rate: index ? 9 : 0,
    overall_conversion: index === length - 1 ? 0 : 100 - index * 9,
    step_conversion: index ? 91 : 100,
    median_time_to_next_ms: 2000,
    avg_time_to_next_ms: 3000,
  }));
}

test("funnel view defaults to Detailed, switches through its actual controls, and preserves server counts and timings", async () => {
  const props = {
    label: "Current period",
    steps: steps(3),
    sameSession: false,
  };
  const original = structuredClone(props.steps);
  const state = setup("FunnelResults", props);
  const bindings = { ...props, ...state };
  const before = await render("FunnelResults", bindings);
  assert.match(before, /class="result-steps"/);
  assert.match(before, /2s median · 3s average/);
  const tree = renderFunction("FunnelResults")(Vue.proxyRefs(bindings), []);
  const compact = findButton(tree, "Compact");
  assert.ok(compact);
  compact.props!.onClick();
  const after = await render("FunnelResults", bindings);
  assert.match(after, /class="compact-steps"/);
  assert.doesNotMatch(after, /class="result-steps"/);
  assert.match(after, /width:0%/);
  assert.match(after, /View accessible results table/);
  assert.deepEqual(props.steps, original);
  const detailed = findButton(
    renderFunction("FunnelResults")(Vue.proxyRefs(bindings), []),
    "Detailed",
  );
  assert.ok(detailed);
  detailed.props!.onClick();
  assert.equal(state.view.value, "detailed");
});

test("compact funnels retain all two-to-ten steps, long labels, actual zeroes and comparison independence", async () => {
  for (let length = 2; length <= 10; length++) {
    const props = {
      label: "Previous period",
      steps: steps(length),
      sameSession: true,
    };
    const state = setup("FunnelResults", props);
    state.view.value = "compact";
    const html = await render("FunnelResults", { ...props, ...state });
    assert.equal(
      (html.match(/class="compact-step-body"/g) ?? []).length,
      length,
    );
    assert.match(html, /estimated sessions/);
    assert.match(html, /width:0%/);
    assert.match(html, /Landing → pricing · 文/);
    assert.doesNotMatch(html, /NaN|Infinity/);
  }
  const current = setup("FunnelResults", {
    label: "Current",
    steps: steps(2),
    sameSession: false,
  });
  const previous = setup("FunnelResults", {
    label: "Previous",
    steps: steps(2),
    sameSession: false,
  });
  current.view.value = "compact";
  assert.equal(previous.view.value, "detailed");
  assert.equal(funnelBarWidth(0), 0);
  assert.equal(funnelBarWidth(150), 100);
  assert.equal(funnelBarWidth(Number.NaN), 0);
  assert.equal(funnelPercent(Number.NaN), "Unavailable");
});

test("empty or suppressed funnel stages stay unavailable instead of manufacturing a compact chart", async () => {
  const props = { label: "Withheld", steps: [], sameSession: false };
  const state = setup("FunnelResults", props);
  state.view.value = "compact";
  const html = await render("FunnelResults", { ...props, ...state });
  assert.match(html, /No reportable funnel stages/);
  assert.match(html, /do not establish zero activity/);
  assert.doesNotMatch(
    html,
    /class="compact-track"|role="group"|View accessible results table/,
  );
});

function campaignResponse(): CampaignsResponse {
  return {
    site_id: "site-a",
    days: 30,
    totals: { campaigns: 1, views: 120, visitors: 42, conversions: 7 },
    items: [
      {
        id: "campaign-a",
        name: "Launch",
        campaign_key: "launch",
        source: "newsletter",
        medium: "email",
        term: "analytics",
        content: "hero",
        destination_url: "https://example.com/",
        tracking_url: "https://example.com/?utm_campaign=launch",
        created_at: null,
        updated_at: null,
        views: 120,
        visitors: 42,
        conversions: 7,
        variants_withheld: true,
        variants: [
          {
            source: "newsletter",
            medium: "email",
            term: "analytics",
            content: "hero",
            views: 80,
            visitors: 30,
            conversions: 4,
          },
          {
            source: "newsletter",
            medium: "email",
            term: "",
            content: "",
            views: 40,
            visitors: 30,
            conversions: 3,
          },
        ],
      },
    ],
  };
}
function campaignHarness() {
  const calls: Array<{
    url: string;
    options: { method?: string; body?: Record<string, string> };
  }> = [];
  const props = { siteId: "site-a", canWrite: true, demo: false };
  const state = setup("CampaignManager", props, {
    useApi: () => ({
      responseCache: new ResponseCache(),
      api: async (
        url: string,
        options: { method?: string; body?: Record<string, string> },
      ) => {
        calls.push({
          url,
          options: {
            ...options,
            body: options.body ? { ...options.body } : undefined,
          },
        });
        return campaignResponse();
      },
    }),
  });
  return { state, props, calls };
}

test("campaign builder sends both optional creative codes once and resets them after creation", async () => {
  const { state, props, calls } = campaignHarness();
  state.createOpen.value = true;
  Object.assign(state.draft, {
    name: "Launch",
    campaign_key: "launch",
    destination_url: "https://example.com/",
    term: "analytics",
    content: "hero",
  });
  const html = await render("CampaignManager", { ...props, ...state });
  assert.match(html, /utm_term/);
  assert.match(html, /utm_content/);
  assert.doesNotMatch(html, /captureCampaigns/);
  await state.createCampaign();
  const write = calls.find((call) => call.options.method === "POST")!;
  assert.equal(write.options.body!.term, "analytics");
  assert.equal(write.options.body!.content, "hero");
  assert.equal(
    calls.filter((call) => call.options.method === "POST").length,
    1,
  );
  assert.equal(state.draft.term, "");
  assert.equal(state.draft.content, "");
});

test("campaign variants retain independent totals, unknown legacy fields and privacy notes", async () => {
  const { state, props, calls } = campaignHarness();
  state.response.value = campaignResponse();
  const html = await render("CampaignManager", { ...props, ...state });
  assert.match(html, /visitors/);
  assert.match(html, /at least five distinct visitors/);
  assert.match(html, /do not sum them as people/);
  assert.match(html, /Missing combinations are not zero/);
  assert.match(html, /Not captured/);
  assert.equal(state.response.value.totals.visitors, 42);
  assert.equal(
    state.response.value.items[0].variants.reduce(
      (total: number, variant: { visitors: number }) =>
        total + variant.visitors,
      0,
    ),
    60,
  );
  assert.equal(
    calls.length,
    0,
    "expanding a variant table requires no additional API request",
  );
  const legacy = campaignResponse();
  delete legacy.items[0]!.variants;
  delete legacy.items[0]!.variants_withheld;
  state.response.value = legacy;
  const oldHtml = await render("CampaignManager", { ...props, ...state });
  assert.doesNotMatch(oldHtml, /Source &amp; creative variants/);
});
