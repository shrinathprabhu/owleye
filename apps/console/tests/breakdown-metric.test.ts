import assert from "node:assert/strict";
import test from "node:test";
import { breakdownItems, breakdownLabel } from "../utils/breakdownMetric.ts";
test("chart toggles use API visitor counts, never infer them from page views", () => {
  const rows = [
    { name: "Chrome", count: 260000, visitors: 115000 },
    { name: "Safari", count: 10000, visitors: 9000 },
  ];
  assert.deepEqual(
    breakdownItems(rows, "pageviews").map((row) => row.count),
    [260000, 10000],
  );
  assert.deepEqual(
    breakdownItems(rows, "visitors").map((row) => row.count),
    [115000, 9000],
  );
  assert.equal(rows[0]!.count, 260000);
  assert.equal(
    breakdownItems([{ name: "Unknown", count: 500 }], "visitors")[0]!.count,
    0,
  );
  assert.equal(breakdownLabel("visitors"), "Unique visitors");
});
