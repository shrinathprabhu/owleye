export type AnalyticsInterval = "day" | "week" | "month";
export type AnalyticsRange = {
  days: number;
  dates?: { start_date: string; end_date: string };
};

export function analyticsInterval(days: number): AnalyticsInterval {
  return days <= 30 ? "day" : days <= 180 ? "week" : "month";
}

/** Inclusive 365-day window, including today. */
export function analyticsDateBounds(today: string) {
  return {
    min: new Date(Date.parse(`${today}T00:00:00Z`) - 364 * 86_400_000)
      .toISOString()
      .slice(0, 10),
    max: today,
  };
}

export function analyticsQuery(
  siteId: string,
  days: number,
  dates?: AnalyticsRange["dates"],
) {
  return { site_id: siteId, days, ...dates, group_by: analyticsInterval(days) };
}

export function customAnalyticsRange(
  start: string,
  end: string,
  today: string,
): AnalyticsRange | null {
  const validDate = (value: string) => {
    if (!/^\d{4}-\d{2}-\d{2}$/.test(value)) return false;
    const timestamp = Date.parse(`${value}T00:00:00Z`);
    return (
      Number.isFinite(timestamp) &&
      new Date(timestamp).toISOString().slice(0, 10) === value
    );
  };
  if (
    !validDate(start) ||
    !validDate(end) ||
    end > today ||
    start < analyticsDateBounds(today).min
  )
    return null;
  const days = (Date.parse(end) - Date.parse(start)) / 86_400_000 + 1;
  if (days < 1 || days > 365) return null;
  return { days, dates: { start_date: start, end_date: end } };
}

export function trafficPeriodLabel(
  date: string,
  interval: AnalyticsInterval,
  compact = false,
) {
  const parsed = new Date(`${date}T00:00:00Z`);
  if (!Number.isFinite(parsed.getTime())) return date;
  const label = new Intl.DateTimeFormat("en", {
    timeZone: "UTC",
    month: "short",
    ...(interval === "month"
      ? { year: "numeric" as const }
      : {
          day: "numeric" as const,
          ...(!compact ? { year: "numeric" as const } : {}),
        }),
  }).format(parsed);
  return interval === "week" && !compact ? `Week of ${label}` : label;
}
