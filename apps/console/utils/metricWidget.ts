function xml(value: string) {
  return value.replace(
    /[&<>"']/g,
    (character) =>
      ({
        "&": "&amp;",
        "<": "&lt;",
        ">": "&gt;",
        '"': "&quot;",
        "'": "&apos;",
      })[character]!,
  );
}
/** A self-contained snapshot: no scripts, API keys, remote fonts, pixels or live requests. */
export function metricWidgetSvg(input: {
  label: string;
  value: number;
  period: string;
  site: string;
}) {
  if (!Number.isFinite(input.value) || input.value < 0)
    throw new Error("A recorded metric is required.");
  const text = (value: string, limit: number) =>
    xml(value.length > limit ? `${value.slice(0, limit - 1)}…` : value);
  return `<svg xmlns="http://www.w3.org/2000/svg" width="720" height="360" viewBox="0 0 720 360" role="img" aria-label="${xml(`${input.label}: ${input.value.toLocaleString()} · ${input.period}`)}">
<rect width="720" height="360" rx="28" fill="#111214"/>
<rect x="26" y="26" width="668" height="308" rx="18" fill="#191b20" stroke="#3c414b"/>
<text x="54" y="71" fill="#d8ff52" font-family="sans-serif" font-size="16" font-weight="700">OWLEYE · ANALYTICS SNAPSHOT</text>
<text x="54" y="113" fill="#afb5c2" font-family="sans-serif" font-size="18">${text(input.site, 48)}</text>
<text x="54" y="158" fill="#f7f7f2" font-family="sans-serif" font-size="24">${text(input.label, 40)}</text>
<text x="50" y="239" fill="#d8ff52" font-family="sans-serif" font-size="66" font-weight="700">${text(input.value.toLocaleString(), 17)}</text>
<text x="54" y="286" fill="#afb5c2" font-family="sans-serif" font-size="16">${text(input.period, 64)}</text>
<text x="54" y="312" fill="#afb5c2" font-family="sans-serif" font-size="12">Static export · observed analytics, not verified people</text>
</svg>`;
}

export type SnapshotMetric = {
  key: string;
  label: string;
  value: number;
  unit: string;
  color: string;
};

/** The same captured chart powers SVG, PNG and static embed code. Totals come from the API, never summed buckets. */
export function chartWidgetSvg(input: {
  chart: { svg: string; width: number; height: number };
  metrics: readonly SnapshotMetric[];
  period: string;
  site: string;
}) {
  const { chart, metrics } = input;
  if (
    !metrics.length ||
    metrics.length > 3 ||
    metrics.some((metric) => !Number.isFinite(metric.value) || metric.value < 0)
  )
    throw new Error("Recorded series totals are required.");
  if (
    !Number.isFinite(chart.width) ||
    !Number.isFinite(chart.height) ||
    chart.width < 1 ||
    chart.height < 1 ||
    chart.width > 8192 ||
    chart.height > 8192
  )
    throw new Error("Chart dimensions unavailable.");
  if (
    !/^<svg\b[^>]*>[\s\S]*<\/svg>\s*$/.test(chart.svg) ||
    /<(?:script|foreignObject|image|iframe)\b|\son[a-z]+\s*=|(?:href|src)\s*=/i.test(
      chart.svg,
    )
  )
    throw new Error("Only a self-contained chart SVG can be exported.");
  const width = chart.width + 48;
  const header = 126 + metrics.length * 32;
  const height = header + chart.height + 64;
  const text = (
    value: string,
    limit = Math.max(14, Math.floor((width - 48) / 8)),
  ) => xml(value.length > limit ? `${value.slice(0, limit - 1)}…` : value);
  const hasVisitors = metrics.some((metric) => metric.key === "visitors");
  const description = `${input.site} · ${input.period}. ${metrics.map((metric) => `${metric.label}: ${metric.value.toLocaleString()} ${metric.unit}`).join("; ")}. Header totals cover the full selected date range. Chart preserves the selected series, axis scales and current zoom. ${hasVisitors ? "Visitor totals are range-deduplicated estimates, not sums of bucket counts. " : ""}Static snapshot; no live API access.`;
  const content = chart.svg
    .replace(/^<svg\b[^>]*>/, "")
    .replace(/<\/svg>\s*$/, "");
  const svg = `<svg xmlns="http://www.w3.org/2000/svg" width="${width}" height="${height}" viewBox="0 0 ${width} ${height}" role="img" aria-label="${xml(description)}">
<title>${xml(`${input.site} · traffic snapshot`)}</title><desc>${xml(description)}</desc>
<defs><pattern id="owleye-snapshot-grid" width="28" height="28" patternUnits="userSpaceOnUse"><path d="M 28 0 L 0 0 0 28" fill="none" stroke="#ffffff" stroke-opacity=".035"/></pattern></defs>
<rect width="${width}" height="${height}" rx="22" fill="#111214"/>
<rect width="${width}" height="${height}" rx="22" fill="url(#owleye-snapshot-grid)"/>
<text x="24" y="30" fill="#d8ff52" font-family="sans-serif" font-size="12" font-weight="700">OWLEYE · TRAFFIC SNAPSHOT</text>
<text x="24" y="57" fill="#f7f7f2" font-family="sans-serif" font-size="19" font-weight="700">${text(input.site)}</text>
<text x="24" y="80" fill="#c4c5c0" font-family="sans-serif" font-size="12">${text(input.period)}</text>
<text x="24" y="102" fill="#c4c5c0" font-family="sans-serif" font-size="11">Full-range totals · chart zoom retained</text>
${metrics.map((metric, index) => `<text x="24" y="${128 + index * 32}" fill="${/^#[a-f\d]{6}$/i.test(metric.color) ? metric.color : "#d8ff52"}" font-family="sans-serif" font-size="14">${text(`${metric.label}: ${metric.value.toLocaleString()} ${metric.unit}`)}</text>`).join("\n")}
<g transform="translate(24,${header})">${content}</g>
<text x="24" y="${height - 35}" fill="#c4c5c0" font-family="sans-serif" font-size="11">Static snapshot · selected series and chart zoom</text>
<text x="24" y="${height - 17}" fill="#c4c5c0" font-family="sans-serif" font-size="11">${text(hasVisitors ? "Estimated visitors are deduplicated across the full range." : "Observed analytics · not verified people.")}</text>
</svg>`;
  return { svg, width, height };
}
