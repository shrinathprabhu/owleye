import assert from "node:assert/strict";
import test from "node:test";
import { populationDescription } from "../utils/reportPopulation.ts";

test("population labels distinguish legacy, known-crawler exclusions and old responses", () => {
  assert.match(
    populationDescription("legacy_all_agents_v1"),
    /includes all.*known crawlers/,
  );
  assert.match(
    populationDescription("known_crawlers_excluded_v1"),
    /Unknown agents remain included/,
  );
  assert.match(populationDescription(), /does not expose.*do not assume/);
});
