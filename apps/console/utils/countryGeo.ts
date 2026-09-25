import type { FeatureCollection, MultiPolygon, Polygon } from "geojson";
export type CountryGeometry = FeatureCollection<
  Polygon | MultiPolygon,
  { name: string; label: string; id: string }
>;
export const COUNTRY_SCALE = [
  "#b8c8ff",
  "#7e99f5",
  "#506de2",
  "#304dcc",
  "#182da3",
];
export function countryColor(value: number, maximum: number) {
  const position =
    Math.min(1, Math.max(0, value / Math.max(1, maximum))) *
    (COUNTRY_SCALE.length - 1);
  const index = Math.min(COUNTRY_SCALE.length - 2, Math.floor(position));
  const fraction = position - index;
  const rgb = (hex: string) =>
    [1, 3, 5].map((offset) => parseInt(hex.slice(offset, offset + 2), 16));
  const a = rgb(COUNTRY_SCALE[index]!);
  const b = rgb(COUNTRY_SCALE[index + 1]!);
  return `rgb(${a.map((channel, i) => Math.round(channel + (b[i]! - channel) * fraction)).join(",")})`;
}
export function countryLabel(code: string) {
  try {
    return new Intl.DisplayNames(["en"], { type: "region" }).of(code) ?? code;
  } catch {
    return code === "UNKNOWN" ? "Unknown" : code;
  }
}
