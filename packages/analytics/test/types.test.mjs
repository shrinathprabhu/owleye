import assert from "node:assert/strict";
import { execFileSync } from "node:child_process";
import { cpSync, mkdirSync, mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { createRequire } from "node:module";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { test } from "node:test";

const require = createRequire(import.meta.url);
const compiler = require.resolve("typescript/bin/tsc");

const consumer = `
import { useAnalytics, type OwlConfig, type OwlRecord, type AnalyticsController, type OwlPage } from "@owleye/analytics";
import { trackRules, type OwlRulesConfig, type RuleEnricher, type RulesController } from "@owleye/analytics/rules";
import { trackPerf, trackWebVitals, type PerfController, type PerfEnd, type WebVitalsController } from "@owleye/analytics/performance";

const config = { captureCampaigns: true, autoStart: false, autoTrackPageviews: false, trackQueryChanges: true, trackHashChanges: true } satisfies OwlConfig;
const fields = { plan: "business", seats: 3 } satisfies OwlRecord;
const analytics: AnalyticsController = useAnalytics("site", config);
analytics.start();
analytics.track("signup_completed", fields);
analytics.track("ten", {field0:0}, {field1:1}, {field2:2}, {field3:3}, {field4:4}, {field5:5}, {field6:6}, {field7:7}, {field8:8}, {field9:9});
// @ts-expect-error Eleven record arguments exceed the supported maximum.
analytics.track("eleven", {}, {}, {}, {}, {}, {}, {}, {}, {}, {}, {});
const page: Partial<OwlPage> = { path: "/virtual", title: "Virtual screen" };
analytics.pageview(page);
analytics.setGlobalRecords(fields);
analytics.setGlobalRecords();
analytics.stop();

const enrichRule: RuleEnricher = (rule, context) => {
  const target: Element = context.element;
  const event: Event | undefined = context.event;
  return { rule: rule.name, interaction: context.type, tag: target.tagName };
};
const rulesConfig = { ...config, enrichRule } satisfies OwlRulesConfig;
const rules: RulesController = trackRules("site", rulesConfig);
rules.stop();
const perf: PerfController = trackPerf("site", config);
const end: PerfEnd = perf.start("report", { format: "csv" });
end({ status: "ok" });
const vitals: WebVitalsController = trackWebVitals("site", config);
vitals.stop();
perf.observeVitals().stop();

declare global {
  interface Window {
    OwlEyeAnalytics?: AnalyticsController;
    OwlEyeRules?: RulesController;
    OwlEyePerformance?: PerfController;
  }
}
window.OwlEyeAnalytics?.track("cdn_event", fields);
window.OwlEyeRules?.stop();
window.OwlEyePerformance?.start("cdn_operation")();

// @ts-expect-error Unknown options are not supported.
useAnalytics("site", { performance: true });
// @ts-expect-error Nested event values are not supported.
analytics.track("test", { nested: { value: 1 } });
// @ts-expect-error Async enrichment cannot return a primitive record synchronously.
trackRules("site", { enrichRule: async () => ({ value: 1 }) });
// @ts-expect-error Performance timers have no global stop method.
perf.stop();
`;

for (const resolution of ["Bundler", "NodeNext"]) {
  test(`published declarations support strict ${resolution} consumers`, () => {
    const directory = mkdtempSync(join(tmpdir(), "owleye-types-"));
    try {
      // Resolve the public package exports using only files included in a release.
      const installed = join(directory, "node_modules/@owleye/analytics");
      mkdirSync(installed, { recursive: true });
      cpSync(new URL("../dist", import.meta.url), join(installed, "dist"), {
        recursive: true,
      });
      cpSync(
        new URL("../package.json", import.meta.url),
        join(installed, "package.json"),
      );
      writeFileSync(join(directory, "consumer.mts"), consumer);
      writeFileSync(
        join(directory, "tsconfig.json"),
        JSON.stringify({
          compilerOptions: {
            strict: true,
            noEmit: true,
            skipLibCheck: false,
            types: [],
            lib: ["ES2020", "DOM"],
            target: "ES2020",
            module: resolution === "Bundler" ? "ESNext" : "NodeNext",
            moduleResolution: resolution,
          },
          files: ["consumer.mts"],
        }),
      );
      let output;
      try {
        output = execFileSync(
          process.execPath,
          [compiler, "-p", join(directory, "tsconfig.json")],
          {
            encoding: "utf8",
            timeout: 30_000,
          },
        );
      } catch (error) {
        assert.fail(error.stdout || error.message);
      }
      assert.equal(output, "");
    } finally {
      rmSync(directory, { recursive: true, force: true });
    }
  });
}
