import assert from "node:assert/strict";
import test from "node:test";
import {
  orderedTrafficMetrics,
  type TrafficMetric,
} from "../utils/trafficMetrics.ts";

test("public traffic preserves every sharing selection without adding withheld metrics", () => {
  const all: TrafficMetric[] = ["pageviews", "visitors", "events", "sessions"];
  for (let mask = 0; mask < 16; mask++) {
    const allowed = all.filter((_, index) => mask & (1 << index));
    assert.deepEqual(orderedTrafficMetrics([...allowed].reverse()), allowed);
  }
  assert.deepEqual(orderedTrafficMetrics(["sessions"]), ["sessions"]);
});

test("public traffic ignores unknown keys and duplicate selections", () => {
  assert.deepEqual(
    orderedTrafficMetrics([
      "events",
      "private_metric",
      "pageviews",
      "events",
    ] as TrafficMetric[]),
    ["pageviews", "events"],
  );
});
