import assert from "node:assert/strict";
import test from "node:test";
import { vitalValue, vitalPosition } from "../utils/vitalPresentation.ts";
test("vitals preserve zero, unitless CLS, unavailable values and bounded bands", () => {
  assert.equal(vitalValue("CLS", 0), "0.000");
  assert.equal(vitalValue("LCP", 0), "0 ms");
  assert.equal(vitalValue("LCP", NaN), "—");
  assert.equal(vitalValue("INP"), "—");
  assert.equal(vitalPosition("CLS"), null);
  assert.equal(vitalPosition("LCP", 2500), 100 / 3);
  assert.equal(vitalPosition("LCP", 4000), 200 / 3);
  assert.equal(vitalPosition("LCP", 1e9), 100);
});
