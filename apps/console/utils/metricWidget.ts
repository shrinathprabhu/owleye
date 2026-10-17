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

/** Only tiny raster PNGs generated from bundled source icons may enter a static widget. */
export function isSnapshotPngDataUrl(value: string): boolean {
  if (value.length > 16384 || !value.startsWith("data:image/png;base64,"))
    return false;
  const encoded = value.slice("data:image/png;base64,".length);
  if (
    !/^(?:[A-Za-z0-9+/]{4})*(?:[A-Za-z0-9+/]{2}==|[A-Za-z0-9+/]{3}=)?$/.test(
      encoded,
    )
  )
    return false;
  try {
    const bytes = atob(encoded);
    if (
      bytes.length < 45 ||
      bytes.slice(0, 8) !== "\x89PNG\r\n\x1a\n" ||
      bytes.slice(8, 16) !== "\0\0\0\rIHDR"
    )
      return false;
    const uint32 = (offset: number) =>
      bytes.charCodeAt(offset) * 16777216 +
      bytes.charCodeAt(offset + 1) * 65536 +
      bytes.charCodeAt(offset + 2) * 256 +
      bytes.charCodeAt(offset + 3);
    const width = uint32(16),
      height = uint32(20);
    return (
      width > 0 &&
      height > 0 &&
      width <= 32 &&
      height <= 32 &&
      bytes.slice(-12, -4) === "\0\0\0\0IEND"
    );
  } catch {
    return false;
  }
}

function isSafeSnapshotImage(tag: string): boolean {
  let attributes = /^<image\b([\s\S]*?)\/?>$/i.exec(tag)?.[1];
  if (attributes === undefined) return false;
  const seen = new Set<string>();
  let imageSource = false;
  while (attributes.trim()) {
    const attribute =
      /^\s+([a-z_][\w:.-]*)\s*=\s*(?:"([^"]*)"|'([^']*)')/i.exec(attributes);
    if (!attribute) return false;
    const key = attribute[1]!.toLowerCase(),
      value = attribute[2] ?? attribute[3] ?? "";
    if (seen.has(key)) return false;
    seen.add(key);
    if (key === "href" || key === "xlink:href") {
      if (imageSource || !isSnapshotPngDataUrl(value)) return false;
      imageSource = true;
    } else if (["width", "height", "x", "y", "opacity"].includes(key)) {
      if (!/^[+-]?(?:\d+\.?\d*|\.\d+)(?:e[+-]?\d+)?$/i.test(value))
        return false;
      const number = Number(value);
      if (
        !Number.isFinite(number) ||
        Math.abs(number) > 8192 ||
        (["width", "height"].includes(key) && (number <= 0 || number > 32)) ||
        (key === "opacity" && (number < 0 || number > 1))
      )
        return false;
    } else if (key === "transform") {
      const numbers = /^matrix\(([\d+.,eE\s-]+)\)$/
        .exec(value)?.[1]
        ?.split(/[\s,]+/);
      if (
        numbers?.length !== 6 ||
        numbers.some(
          (number) =>
            !number ||
            !Number.isFinite(Number(number)) ||
            Math.abs(Number(number)) > 8192,
        )
      )
        return false;
    } else if (key === "preserveaspectratio") {
      if (
        !/^(?:none|x(?:Min|Mid|Max)Y(?:Min|Mid|Max)(?: (?:meet|slice))?)$/.test(
          value,
        )
      )
        return false;
    } else if (key === "clip-path") {
      if (!/^url\(#[\w-]+\)$/.test(value)) return false;
    } else if (
      ["ecmeta_series_index", "ecmeta_data_index", "ecmeta_ssr_type"].includes(
        key,
      )
    ) {
      if (!/^[\w-]{1,40}$/.test(value)) return false;
    } else return false;
    attributes = attributes.slice(attribute[0].length);
  }
  return imageSource;
}

/** ECharts uses these local, bounded primitives for the traffic line glow. */
function isSafeSnapshotShadow(tag: string): boolean {
  const match = /^<(filter|feDropShadow)\b([\s\S]*?)\/?>$/i.exec(tag);
  if (!match) return false;
  const filter = match[1]!.toLowerCase() === "filter";
  let attributes = match[2]!;
  const seen = new Set<string>();
  while (attributes.trim()) {
    const attribute = /^\s+([a-z-]+)\s*=\s*(?:"([^"]*)"|'([^']*)')/i.exec(
      attributes,
    );
    if (!attribute) return false;
    const key = attribute[1]!.toLowerCase();
    const value = attribute[2] ?? attribute[3] ?? "";
    if (seen.has(key)) return false;
    seen.add(key);
    if (filter) {
      if (key === "id") {
        if (!/^[a-z_][\w.-]{0,79}$/i.test(value)) return false;
      } else if (["x", "y", "width", "height"].includes(key)) {
        if (!/^-?\d+(?:\.\d+)?%$/.test(value)) return false;
        const number = Number(value.slice(0, -1));
        if (
          Math.abs(number) > 300 ||
          (["width", "height"].includes(key) && number <= 0)
        )
          return false;
      } else return false;
    } else if (["dx", "dy", "stddeviation", "flood-opacity"].includes(key)) {
      const numbers = value.split(/\s+/);
      if (
        numbers.length > (key === "stddeviation" ? 2 : 1) ||
        numbers.some(
          (number) =>
            !/^-?\d+(?:\.\d+)?$/.test(number) ||
            !Number.isFinite(Number(number)) ||
            Math.abs(Number(number)) > (key === "flood-opacity" ? 1 : 32) ||
            ((key === "stddeviation" || key === "flood-opacity") &&
              Number(number) < 0),
        )
      )
        return false;
    } else if (key === "flood-color") {
      if (!/^#(?:[a-f\d]{3}|[a-f\d]{4}|[a-f\d]{6}|[a-f\d]{8})$/i.test(value))
        return false;
    } else return false;
    attributes = attributes.slice(attribute[0].length);
  }
  return filter ? seen.has("id") : seen.has("stddeviation");
}

function isSelfContainedChartSvg(svg: string): boolean {
  if (!/^<svg\b[^>]*>[\s\S]*<\/svg>\s*$/.test(svg) || svg.length > 2097152)
    return false;
  const allowedTags = new Set([
    "svg",
    "g",
    "path",
    "rect",
    "circle",
    "ellipse",
    "line",
    "polyline",
    "polygon",
    "text",
    "tspan",
    "title",
    "desc",
    "defs",
    "clippath",
    "lineargradient",
    "radialgradient",
    "stop",
    "pattern",
    "image",
    "filter",
    "fedropshadow",
  ]);
  if (
    /<!|<\?/.test(svg) ||
    [...svg.matchAll(/<\/?([a-z][\w:.-]*)\b/gi)].some(
      (tag) => !allowedTags.has(tag[1]!.toLowerCase()),
    )
  )
    return false;
  const images = [...svg.matchAll(/<image\b[^>]*>/gi)];
  if (images.length > 32 || images.some(([tag]) => !isSafeSnapshotImage(tag)))
    return false;
  const shadows = [...svg.matchAll(/<(?:filter|feDropShadow)\b[^>]*>/gi)];
  if (
    shadows.length > 32 ||
    shadows.some(([tag]) => !isSafeSnapshotShadow(tag))
  )
    return false;
  const withoutImages = svg
    .replace(/<image\b[^>]*>/gi, "")
    .replace(/<\/image\s*>/gi, "");
  if (
    [...withoutImages.matchAll(/url\s*\([^)]*\)/gi)].some(
      ([url]) => !/^url\(\s*["']?#[\w-]+["']?\s*\)$/i.test(url),
    )
  )
    return false;
  return !/<image\b|\son[a-z]+\s*=|(?:href|src)\s*=|@import|@font-face|\sstyle\s*=\s*["'][^"']*\\/i.test(
    withoutImages,
  );
}

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
  if (!isSelfContainedChartSvg(chart.svg))
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
  const svg = `<svg xmlns="http://www.w3.org/2000/svg" xmlns:xlink="http://www.w3.org/1999/xlink" width="${width}" height="${height}" viewBox="0 0 ${width} ${height}" role="img" aria-label="${xml(description)}">
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
