import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";
import ts from "typescript";
import * as Vue from "vue";
import { compileScript, compileTemplate, parse } from "vue/compiler-sfc";
import { renderToString } from "vue/server-renderer";
import * as cohort from "../utils/cohortPresentation.ts";
import { toggleChartMetric } from "../utils/chartSelection.ts";
import { populationDescription } from "../utils/reportPopulation.ts";
import type { CohortPoint, CohortSummary } from "../types/stats.ts";

function source(name: string) {
  const filename = new URL(`../components/${name}.vue`, import.meta.url);
  return { filename, ...parse(readFileSync(filename, "utf8")) };
}
function harness(name: string, props: object) {
  const { descriptor } = source(name);
  const code = ts
    .transpileModule(compileScript(descriptor, { id: name }).content, {
      compilerOptions: {
        module: ts.ModuleKind.ESNext,
        target: ts.ScriptTarget.ESNext,
      },
    })
    .outputText.replace(/^import .+$/gm, "")
    .replace("export default", "return");
  const watchers: Array<() => void> = [];
  const events: unknown[][] = [];
  const env = {
    _defineComponent: Vue.defineComponent,
    ref: Vue.ref,
    computed: Vue.computed,
    useId: () => "cohort-test",
    watch: (_source: unknown, effect: () => void) => watchers.push(effect),
    toggleChartMetric,
    populationDescription,
    ...cohort,
  };
  const state = new Function(...Object.keys(env), code)(
    ...Object.values(env),
  ).setup(props, {
    expose: () => {},
    emit: (...args: unknown[]) => events.push(args),
  });
  return {
    state,
    events,
    changed: () => watchers.forEach((effect) => effect()),
  };
}
async function render(name: string, bindings: object) {
  const { filename, descriptor } = source(name);
  const compiled = compileTemplate({
    source: descriptor.template!.content,
    filename: filename.pathname,
    id: name,
  });
  assert.deepEqual(compiled.errors, []);
  const code = ts
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
  const app = Vue.createSSRApp({
    render: new Function("Vue", code)(Vue),
    setup: () => ({
      measurement: undefined,
      entryPages: undefined,
      lastPages: undefined,
      ...bindings,
    }),
  });
  app.component("ChartSeriesControls", { render: () => null });
  return renderToString(app);
}
const summary = {
  status: "observed_estimate",
  eligible_cohorts: 10,
  estimated_bounce_rate: 40,
  observed_span_samples: 2,
  avg_observed_span_ms: 1500,
} as CohortSummary;
const points = [
  { ...summary, date: "2026-10-14" },
  {
    ...summary,
    date: "2026-10-15",
    eligible_cohorts: 0,
    estimated_bounce_rate: null,
    observed_span_samples: 0,
    avg_observed_span_ms: null,
  },
  {
    ...summary,
    date: "2026-10-16",
    estimated_bounce_rate: 0,
    avg_observed_span_ms: 0,
  },
] as CohortPoint[];

test("metric cards contain no mini graphs or trend placeholders and retain totals, selection and stale captions", async () => {
  const props = {
    modelValue: ["pageviews", "visitors", "events"],
    totals: { pageviews: 240, pageview_visitors: 120, events: 350 },
    caption: "Last 30 days",
    error: "Refresh failed",
    loading: false,
  };
  const h = harness("OverviewMetricCards", props);
  const html = await render("OverviewMetricCards", { ...props, ...h.state });
  assert.doesNotMatch(html, /<svg|spark|Loading trend|No trend/);
  assert.match(html, /240/);
  assert.match(html, /Last loaded data · refresh failed/);
  assert.equal((html.match(/aria-pressed="true"/g) ?? []).length, 3);
  h.state.toggle("visitors");
  assert.deepEqual(h.events.at(-1), [
    "update:modelValue",
    ["pageviews", "events"],
  ]);
});

test("pointer selection uses the existing plotted geometry, clamps edges and handles a single bucket", () => {
  assert.equal(cohort.cohortPointerIndex(0, 240, 3), 0);
  assert.equal(cohort.cohortPointerIndex(120, 240, 3), 1);
  assert.equal(cohort.cohortPointerIndex(240, 240, 3), 2);
  assert.equal(cohort.cohortPointerIndex(-50, 240, 3), 0);
  assert.equal(cohort.cohortPointerIndex(500, 240, 3), 2);
  assert.equal(cohort.cohortPointerIndex(50, 240, 1), 0);
  assert.equal(cohort.cohortPointerIndex(20, 0, 3), undefined);
  assert.equal(cohort.cohortPointerIndex(20, 240, 0), undefined);
});

test("inspecting a gap never makes a zero duration or percentage and retains actual sample counts", () => {
  const gap = cohort.cohortInspection(points, "span", 1, 1500)!;
  assert.equal(gap.text, "Unavailable");
  assert.equal(gap.samples, "0");
  assert.equal(gap.sampleLabel, "span samples");
  assert.equal(gap.y, undefined);
  const zero = cohort.cohortInspection(points, "bounce", 2, 100)!;
  assert.equal(zero.text, "0%");
  assert.equal(zero.y, 52);
  assert.equal(cohort.cohortInspection(points, "cohorts", null, 10), undefined);
});

test("larger cohort charts support focus, keyboard, pointer/touch and synchronized crosshairs without changing measurements", async () => {
  const props = { cohorts: summary, points, loading: false, error: "" };
  const h = harness("CohortOverview", props);
  h.state.focusChart("span");
  assert.equal(h.state.inspectedIndex.value, 0);
  let prevented = false;
  h.state.inspectKey(
    {
      key: "ArrowRight",
      preventDefault() {
        prevented = true;
      },
    },
    "span",
  );
  assert.equal(prevented, true);
  assert.equal(h.state.inspected.value.span.text, "Unavailable");
  let html = await render("CohortOverview", { ...props, ...h.state });
  assert.equal(
    (html.match(/<svg/g) ?? []).length,
    3,
    "only the three large plots render SVGs",
  );
  assert.equal((html.match(/class="cohort-crosshair"/g) ?? []).length, 3);
  assert.match(html, /0 span samples/);
  assert.match(html, /tabindex="0"/);
  assert.match(html, /Unknown measurements are unavailable, not zero/);
  assert.equal((html.match(/aria-live="polite"/g) ?? []).length, 1);
  h.state.inspectPointer(
    {
      clientX: 250,
      currentTarget: {
        getBoundingClientRect: () => ({ left: 10, width: 240 }),
      },
    },
    "bounce",
  );
  assert.equal(h.state.inspectedIndex.value, 2);
  assert.equal(h.state.inspected.value.bounce.text, "0%");
  h.state.inspectKey({ key: "Home", preventDefault() {} }, "cohorts");
  assert.equal(h.state.inspectedIndex.value, 0);
  h.state.inspectKey({ key: "End", preventDefault() {} }, "cohorts");
  assert.equal(h.state.inspectedIndex.value, 2);
  h.state.inspectKey({ key: "Escape", preventDefault() {} }, "cohorts");
  assert.equal(h.state.inspectedIndex.value, null);
  h.state.focusChart("span");
  h.changed();
  assert.equal(
    h.state.inspectedIndex.value,
    null,
    "new data invalidates the previous inspection",
  );
  html = await render("CohortOverview", { ...props, ...h.state });
  assert.doesNotMatch(html, /class="cohort-crosshair"/);
  assert.deepEqual(points[1]!.avg_observed_span_ms, null);
});

test("all-unavailable plots remain focusable and query failure still withholds charts", async () => {
  const props = {
    cohorts: summary,
    points: [points[1]!],
    loading: false,
    error: "",
  };
  const h = harness("CohortOverview", props);
  h.state.selected.value = ["span"];
  h.state.focusChart("span");
  let html = await render("CohortOverview", { ...props, ...h.state });
  assert.match(html, /<svg/);
  assert.match(html, /Unavailable/);
  assert.doesNotMatch(html, /class="cohort-highlight"/);
  const failedProps = {
    ...props,
    cohorts: { ...summary, status: "query_failed" },
  };
  const failed = harness("CohortOverview", failedProps);
  html = await render("CohortOverview", { ...failedProps, ...failed.state });
  assert.match(html, /Cohort measurements could not be loaded/);
  assert.doesNotMatch(html, /<svg/);
});
