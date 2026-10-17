import type { AnalyticsInterval } from "./analyticsRange.ts";

/** Evaluate completeness against the report timestamp, not the viewer's clock. */
export function incompleteTrafficPeriod(
  date: string,
  interval: AnalyticsInterval,
  asOf: string,
) {
  if (!validDay(interval === "hour" ? date.slice(0, 10) : date)) return false;
  const start = Date.parse(interval === "hour" ? date : date + "T00:00:00Z");
  const at = Date.parse(asOf);
  if (!Number.isFinite(start) || !Number.isFinite(at)) return false;
  const end = new Date(start);
  if (interval === "month") end.setUTCMonth(end.getUTCMonth() + 1, 1);
  else
    end.setTime(
      start + { hour: 3_600_000, day: 86_400_000, week: 604_800_000 }[interval],
    );
  return start <= at && at < end.getTime();
}

/** Only the connection into an unfinished bucket is dashed; gaps stay gaps. */
export function trafficLineSegments<T>(
  dates: string[],
  values: (T | null)[],
  interval: AnalyticsInterval,
  asOf: string,
) {
  const partial = dates.map((date) =>
    incompleteTrafficPeriod(date, interval, asOf),
  );
  return {
    complete: values.map((value, index) => (partial[index] ? null : value)),
    partial: values.map((value, index) =>
      partial[index] || partial[index + 1] ? value : null,
    ),
    hasPartial: partial.some(Boolean),
  };
}

export type TrafficSpikeSource = {
  label: string;
  sourceDomain: string | null;
  publicUrl: string | null;
  observed: number;
  share: number;
  excess: number;
  evidence:
    | "owner_registered_campaign"
    | "owner_supplied_campaign_link"
    | "observed_known_referrer";
};

/** One spike day can have up to three independently significant associated sources. */
export type TrafficSpike = {
  date: string;
  observed: number;
  expected: number;
  sources: TrafficSpikeSource[];
};

function publicHost(value: unknown): string | null {
  if (typeof value !== "string" || value.length > 253) return null;
  const host = value.toLowerCase().replace(/^www\./, "");
  const labels = host.split(".");
  if (
    labels.length < 2 ||
    !/^[a-z]{2,63}$/.test(labels.at(-1)!) ||
    labels.some(
      (label) => !/^[a-z0-9](?:[a-z0-9-]{0,61}[a-z0-9])?$/.test(label),
    ) ||
    /\.(?:local|internal|test|localhost|invalid|example|home|lan)$/.test(host)
  )
    return null;
  return host;
}

function validDay(value: unknown): value is string {
  if (typeof value !== "string" || !/^\d{4}-\d{2}-\d{2}$/.test(value))
    return false;
  const timestamp = Date.parse(value + "T00:00:00Z");
  return (
    Number.isFinite(timestamp) &&
    new Date(timestamp).toISOString().slice(0, 10) === value
  );
}

function finiteCount(value: unknown): value is number {
  return typeof value === "number" && Number.isFinite(value) && value >= 0;
}

const unsafeLabel =
  /[<>@\u0000-\u001f\u007f\u200b-\u200f\u202a-\u202e\u2060-\u206f]/;
const uninformativeLabel =
  /^(?:\(?direct(?:\s*(?:\/|or)\s*(?:unknown|none))?\)?|unknown|no campaign|none|other)$/i;

/** Hacker News permalinks use a single numeric item ID; other query data stays private. */
function hackerNewsPost(url: URL, raw: string) {
  if (url.hostname !== "news.ycombinator.com" || url.pathname !== "/item")
    return false;
  if (!/^https:\/\/news\.ycombinator\.com\/item\?id=[1-9]\d{0,19}$/.test(raw))
    return false;
  return BigInt(url.search.slice(4)) <= 18_446_744_073_709_551_615n;
}

/** Links are optional owner-supplied context, never inferred from a social origin. */
export function spikePublicUrl(value: unknown): string | null {
  if (
    typeof value !== "string" ||
    value.length > 2000 ||
    /[\u0000-\u0020\u007f\u200b-\u200f\u202a-\u202e\u2060-\u206f]/.test(value)
  )
    return null;
  try {
    const url = new URL(value);
    if (
      url.protocol !== "https:" ||
      url.username ||
      url.password ||
      url.port ||
      url.hash ||
      (url.search && !hackerNewsPost(url, value)) ||
      !publicHost(url.hostname) ||
      /[\u0000-\u001f\u007f\u200b-\u200f\u202a-\u202e\u2060-\u206f]/.test(
        decodeURIComponent(url.pathname),
      )
    )
      return null;
    return url.href;
  } catch {
    return null;
  }
}

/** Bound chart annotations and fail closed for unavailable, minor or Direct-led traffic. */
export function trafficSpikes(
  rows: unknown,
  dates: readonly string[],
): TrafficSpike[] {
  if (!Array.isArray(rows)) return [];
  const allowed = new Set(dates);
  const candidates: (TrafficSpike & { group: string })[] = [];
  for (const row of rows.slice(0, 90)) {
    if (
      !row ||
      row.status !== "candidate_spike" ||
      row.major_source_status !== "ready" ||
      !allowed.has(row.date) ||
      !validDay(row.date)
    )
      continue;
    const observed = row.observed_pageviews,
      expected = row.expected_pageviews,
      identifiable = row.identifiable_source_pageviews;
    if (
      !finiteCount(observed) ||
      !finiteCount(expected) ||
      !finiteCount(identifiable) ||
      observed <= 0 ||
      observed <= expected * 2.5 ||
      identifiable > observed ||
      identifiable <= observed * 0.5 ||
      !Array.isArray(row.contributors)
    )
      continue;
    const sources: TrafficSpikeSource[] = [];
    const identities = new Set<string>();
    for (const item of row.contributors.slice(0, 3)) {
      if (
        !item ||
        typeof item.label !== "string" ||
        item.label.trim().length === 0 ||
        item.label.length > 120 ||
        unsafeLabel.test(item.label) ||
        uninformativeLabel.test(item.label.trim()) ||
        ![
          "owner_registered_campaign",
          "owner_supplied_campaign_link",
          "observed_known_referrer",
        ].includes(String(item.evidence_type)) ||
        !finiteCount(item.positive_excess) ||
        item.positive_excess <= 0 ||
        !finiteCount(item.observed_pageviews) ||
        item.observed_pageviews < observed * 0.2 ||
        item.observed_pageviews > identifiable
      )
        continue;
      const evidence = item.evidence_type as TrafficSpikeSource["evidence"];
      const label = item.label.trim();
      const sourceDomain = publicHost(item.source_domain);
      const publicUrl =
        evidence === "owner_supplied_campaign_link"
          ? spikePublicUrl(item.public_url)
          : null;
      const identity = publicUrl ?? sourceDomain ?? label.toLowerCase();
      if (identities.has(identity)) continue;
      identities.add(identity);
      sources.push({
        label,
        sourceDomain,
        publicUrl,
        observed: item.observed_pageviews,
        share: (item.observed_pageviews / observed) * 100,
        excess: item.positive_excess,
        evidence,
      });
    }
    // Source partitions must remain disjoint and fit within the identified population.
    if (
      !sources.length ||
      sources.reduce((total, source) => total + source.observed, 0) >
        identifiable
    )
      continue;
    sources.sort(
      (a, b) => b.observed - a.observed || a.label.localeCompare(b.label),
    );
    candidates.push({
      date: row.date,
      observed,
      expected,
      sources,
      group:
        typeof row.annotation_group === "string"
          ? row.annotation_group
          : row.date,
    });
  }
  const groups = new Set<string>(),
    annotatedDates = new Set<string>();
  const selected: TrafficSpike[] = [];
  for (const { group, ...candidate } of candidates.sort(
    (a, b) =>
      b.observed - b.expected - (a.observed - a.expected) ||
      a.date.localeCompare(b.date),
  )) {
    if (groups.has(group) || annotatedDates.has(candidate.date)) continue;
    groups.add(group);
    annotatedDates.add(candidate.date);
    selected.push(candidate);
    if (selected.length === 4) break;
  }
  return selected.sort((a, b) => a.date.localeCompare(b.date));
}

export function chartText(value: unknown) {
  return String(value).replace(
    /[&<>"']/g,
    (c) =>
      ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;", "'": "&#39;" })[
        c
      ]!,
  );
}
