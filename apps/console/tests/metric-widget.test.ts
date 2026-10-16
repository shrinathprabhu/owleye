import assert from "node:assert/strict";
import test from "node:test";
import { metricWidgetSvg } from "../utils/metricWidget.ts";
test("widget contains only escaped snapshot data and no live access", () => {
  const svg = metricWidgetSvg({
    label: "Estimated visitors",
    value: 0,
    period: "14 October (UTC)",
    site: '<script>alert("x")</script>',
  });
  assert.ok(svg.includes("Estimated visitors"));
  assert.ok(svg.includes("&lt;script&gt;"));
  assert.ok(svg.includes("14 October (UTC)"));
  assert.ok(!/<script|<image|<foreignObject|href=/.test(svg));
  assert.ok(svg.includes("Static export"));
  assert.throws(() =>
    metricWidgetSvg({ label: "Visitors", value: NaN, period: "", site: "" }),
  );
});
