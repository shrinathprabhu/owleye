import type { AiEvidence, AiMetric, AiChart } from "../types/ai.ts";

export const metricLabels: Record<AiMetric, string> = {
  events: "Events",
  pageviews: "Pageviews",
  visitors: "Visitors",
  sessions: "Sessions",
};
export function reportLabel(label: string, report: string) {
  if (report !== "country" || !/^[A-Z]{2}$/.test(label)) return label;
  return `${new Intl.DisplayNames(["en"], { type: "region" }).of(label) ?? label} (${label})`;
}
function filterValues(value: string | string[] | null | undefined): string[] {
  return Array.isArray(value) ? value : value ? [value] : [];
}
export function reportScope(evidence: AiEvidence) {
  return (
    [
      filterValues(evidence.country)
        .map((v) => reportLabel(v, "country"))
        .join(" or "),
      filterValues(evidence.browser).join(" or "),
      filterValues(evidence.os).join(" or "),
      filterValues(evidence.device).join(" or "),
      evidence.details?.cities.join(" or "),
      evidence.details?.events.join(" or "),
      evidence.details?.campaigns.map((c) => `Campaign: ${c}`).join(" or "),
      evidence.details?.properties.join(" and "),
    ]
      .filter(Boolean)
      .join(" · ") || "All traffic"
  );
}
export function reportNotes(evidence: AiEvidence) {
  const notes = [
    "Visitors and sessions are approximate distinct estimates. Do not sum bucket distinct counts into a period total.",
    "Only active analytics is included; performance and session lifecycle events are excluded.",
  ];
  if (evidence.groups_suppressed_below_visitors)
    notes.push(
      "Groups below 5 visitors are omitted. Missing groups do not prove zero traffic.",
    );
  if (
    [
      "country",
      "city",
      "event",
      "campaign",
      "browser",
      "device",
      "os",
    ].includes(evidence.report)
  )
    notes.push("Only the top 20 groups by event count are included.");
  if (evidence.report === "weekly")
    notes.push(
      "Weeks start Monday in UTC. The first and last weeks may be partial.",
    );
  notes.push(
    ...(evidence.details?.steps ?? []),
    ...(evidence.details?.notes ?? []),
  );
  return notes;
}
export const chartLabels: Record<string, string> = {
  line: "Line",
  bar: "Bar",
  pie: "Pie",
  donut: "Donut",
};
export function chartOptions(
  evidence: AiEvidence,
  metric: AiMetric,
): AiChart[] {
  // Presentation is opt-in. A single total/category is not a useful chart.
  if (
    !evidence.chart ||
    evidence.chart === "none" ||
    evidence.chart === "auto" ||
    evidence.report === "totals"
  )
    return [];
  if (evidence.rows.length < 2) return [];
  if (["daily", "weekly"].includes(evidence.report)) return ["line", "bar"];
  if (evidence.rows.filter((row) => row[metric] > 0).length < 2) return [];
  return ["bar", "pie", "donut"];
}
export function defaultChart(evidence: AiEvidence): AiChart {
  const options = chartOptions(evidence, evidence.metric ?? "visitors");
  return options.includes(evidence.chart!)
    ? evidence.chart!
    : (options[0] ?? "none");
}
export function chartPoints(evidence: AiEvidence, metric: AiMetric) {
  if (!["daily", "weekly"].includes(evidence.report))
    return evidence.rows.map((row) => ({
      label: reportLabel(row.label, evidence.report),
      value: row[metric],
    }));
  const values = new Map(evidence.rows.map((row) => [row.label, row[metric]]));
  const cursor = new Date(`${evidence.start_date}T00:00:00Z`);
  const end = new Date(`${evidence.end_date}T00:00:00Z`);
  const weekly = evidence.report === "weekly";
  if (weekly)
    cursor.setUTCDate(cursor.getUTCDate() - ((cursor.getUTCDay() + 6) % 7));
  const points: { label: string; value: number | null }[] = [];
  while (cursor <= end && points.length < 366) {
    const label = cursor.toISOString().slice(0, 10);
    points.push({
      label,
      value:
        values.get(label) ??
        (evidence.groups_suppressed_below_visitors ? null : 0),
    });
    cursor.setUTCDate(cursor.getUTCDate() + (weekly ? 7 : 1));
  }
  return points;
}
