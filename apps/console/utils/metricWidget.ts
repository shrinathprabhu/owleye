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
