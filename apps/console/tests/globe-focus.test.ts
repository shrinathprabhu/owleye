import assert from "node:assert/strict";
import test from "node:test";
import { shortestRotation } from "../utils/globeFocus.ts";

test("country focus takes a bounded shortest arc across the date line", () => {
  assert.equal(shortestRotation(170, -170), 190);
  assert.equal(shortestRotation(-170, 170), -190);
  assert.equal(shortestRotation(740, 20), 740);
  assert.equal(shortestRotation(-740, -20), -740);
  for (const current of [-2000, -180, 0, 180, 2000])
    for (const target of [-179, 0, 179])
      assert(Math.abs(shortestRotation(current, target) - current) <= 180);
});
