import assert from "node:assert/strict";
import test from "node:test";
import { readFileSync } from "node:fs";
import { geoArea, geoContains, geoOrthographic, geoPath } from "d3-geo";
import {
  countryColor,
  countryLabel,
  type CountryGeometry,
} from "../utils/countryGeo.ts";
const geometry = JSON.parse(
  readFileSync(
    new URL("../public/maps/world-countries.json", import.meta.url),
    "utf8",
  ),
) as CountryGeometry;
test("shared map geometry has country keys, no Antarctica, and valid globe winding", () => {
  assert(geometry.features.length > 200);
  const codes = geometry.features.map((feature) => feature.properties.name);
  assert(!codes.includes("AQ"));
  assert.equal(new Set(codes).size, codes.length);
  for (const feature of geometry.features) {
    assert(geoArea(feature) < 2 * Math.PI, feature.properties.name);
    assert(feature.properties.label);
  }
  const india = geometry.features.find(
    (feature) => feature.properties.name === "IN",
  )!;
  assert(geoContains(india, [77.59, 12.97]));
  assert(!geoContains(india, [-74, 40]));
  assert(geoPath(geoOrthographic().rotate([-78, -20]))(india)?.includes("M"));
});
test("country shading is proportional and consistent at endpoints", () => {
  assert.equal(countryColor(0, 100), "rgb(184,200,255)");
  assert.equal(countryColor(100, 100), "rgb(24,45,163)");
  assert.equal(countryColor(50, 100), countryColor(500, 1000));
  assert.equal(countryColor(200, 100), countryColor(100, 100));
  assert.equal(countryLabel("IN"), "India");
});
