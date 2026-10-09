export const PRO_VIEW_CHART_TYPES = [
  "line",
  "bar",
  "donut",
  "pie",
  "scatter",
  "map",
] as const;

export const PRO_VIEW_COMPARISONS = [
  "none",
  "today_vs_yesterday",
  "last_7_vs_previous_7",
  "last_30_vs_previous_30",
] as const;

export type ProViewChartType = (typeof PRO_VIEW_CHART_TYPES)[number];
export type ProViewComparison = (typeof PRO_VIEW_COMPARISONS)[number];
export type ProViewSourceKind = "event" | "future_event" | "rule";
export type ProViewWidgetKind = "chart" | "funnel";
export type ProViewFunnelConditionKind = "event" | "page" | "rule";
export type ProViewFunnelWindow = "1d" | "1h" | "7d" | "30d" | "same_session";
export type ProViewFunnelOrderMode = "exact" | "ordered";
export type ProViewFunnelEntryMode = "closed" | "open";

export type ProViewPropertyFilter = {
  id: string;
  key: string;
  operator: "contains" | "equals" | "exists" | "not_equals";
  value?: string;
};

export type ProViewSource = {
  has_location: boolean;
  id: string;
  kind: ProViewSourceKind;
  name: string;
};

export type ProViewFunnelStep = {
  condition: {
    id: string;
    kind: ProViewFunnelConditionKind;
    name: string;
    operator?: "exact" | "starts_with";
    property_filters?: {
      filters: ProViewPropertyFilter[];
      logic: "and" | "or";
    };
  };
  id: string;
  name: string;
};

export const PRO_VIEW_BREAKDOWNS = {
  time: "Date / time",
  browser: "Browser",
  country: "Country",
  os: "Operating system",
  city: "City and country",
  device: "Device",
  campaign: "Campaign",
  referrer: "Referrer",
  source: "Campaign source",
  medium: "Campaign medium",
  page: "Page path",
  property: "Event property",
} as const;
export type ProViewBreakdown = keyof typeof PRO_VIEW_BREAKDOWNS;
export type ProViewDateRange = { days: number; compare_previous: boolean };

export type ProViewWidgetDefinition = {
  metric?: "events" | "visitors" | "sessions";
  breakdown_property?: string;
  property_filters?: { filters: ProViewPropertyFilter[]; logic: "and" | "or" };
  date_range?: ProViewDateRange;
  breakdown?: ProViewBreakdown;
  comparison: ProViewComparison;
  display: {
    order: number;
    size: "compact" | "wide";
  };
  filters: {
    country?: string;
    page?: string;
  };
  funnel?: {
    conversion_window: ProViewFunnelWindow;
    entry_mode: ProViewFunnelEntryMode;
    order_mode: ProViewFunnelOrderMode;
    steps: ProViewFunnelStep[];
  };
  id?: string;
  kind: ProViewWidgetKind;
  source?: ProViewSource;
  title: string;
  visualization: ProViewChartType | "funnel";
};

export type ProViewPoint = {
  period: string;
  value: number;
};

export type ProViewPreviewResponse = {
  current?: {
    label: string;
    points?: ProViewPoint[];
    steps?: ProViewFunnelPreviewStep[];
  };
  granularity: "day" | "hour";
  map?: Array<{
    country_code?: string;
    name: string;
    value: number;
  }>;
  previous?: {
    label: string;
    points?: ProViewPoint[];
    steps?: ProViewFunnelPreviewStep[];
  };
};

export type ProViewFunnelPreviewStep = {
  avg_time_to_next_ms?: number | null;
  count: number;
  dropoff_count: number;
  dropoff_rate: number;
  id: string;
  median_time_to_next_ms?: number | null;
  name: string;
  overall_conversion: number;
  step_conversion: number;
};

export type ProViewShareStatus =
  "accepted" | "dismissed" | "pending" | "revoked";

export type ProViewShare = {
  created_at: string;
  dashboard_id: string;
  display_name?: string | null;
  email: string;
  invited_by_user_id: string;
  responded_at?: string | null;
  status: ProViewShareStatus;
  updated_at: string;
  user_id: string;
};

export type ProViewShareCandidate = {
  email: string;
  id: string;
  is_current_user: boolean;
  name?: string | null;
  role: "admin" | "owner" | "read_only";
};

export type ProViewDashboard = {
  capabilities?: {
    can_delete: boolean;
    can_delete_all?: boolean;
    can_edit: boolean;
    can_remove_for_me?: boolean;
    can_revoke_for_others?: boolean;
    can_share: boolean;
  };
  created_at: string;
  created_by?: {
    email: string;
    id: string;
    name?: string | null;
  } | null;
  id: string;
  is_default: boolean;
  name: string;
  share_status?: "accepted" | "pending" | null;
  shares?: ProViewShare[];
  site_id: string;
  updated_at: string;
  widgets?: ProViewWidgetDefinition[];
};

export type ProViewEventOption = {
  hasLocation: boolean;
  id: string;
  kind: "event" | "rule";
  label: string;
  meta: string;
};

export type ProViewDeleteScope = "all" | "others" | "self";

export function comparisonLabel(comparison: ProViewComparison) {
  if (comparison === "today_vs_yesterday") {
    return "Today vs yesterday · hourly";
  }
  if (comparison === "last_7_vs_previous_7") {
    return "Last 7 days vs prior 7 · daily";
  }
  if (comparison === "last_30_vs_previous_30") {
    return "Last 30 days vs prior 30 · daily";
  }
  return "No comparison";
}

export function defaultProViewDefinition(
  kind: ProViewWidgetKind,
  source?: ProViewEventOption,
): ProViewWidgetDefinition {
  return {
    comparison: "none",
    metric: "events",
    date_range: { days: 30, compare_previous: false },
    breakdown: "time",
    display: {
      order: 0,
      size: "wide",
    },
    filters: {},
    funnel:
      kind === "funnel"
        ? {
            conversion_window: "7d",
            entry_mode: "closed",
            order_mode: "ordered",
            steps: [
              {
                condition: {
                  id: "",
                  kind: "page",
                  name: "",
                  operator: "exact",
                  property_filters: { filters: [], logic: "and" },
                },
                id: cryptoSafeId(),
                name: "Landed on a page",
              },
              {
                condition: {
                  id: source?.id ?? "",
                  kind: source?.kind === "rule" ? "rule" : "event",
                  name: source?.label ?? "",
                  property_filters: { filters: [], logic: "and" },
                },
                id: cryptoSafeId(),
                name: source?.label ?? "Completed an event",
              },
            ],
          }
        : undefined,
    kind,
    source:
      kind === "funnel"
        ? undefined
        : source
          ? {
              has_location: source.hasLocation,
              id: source.id,
              kind: source.kind,
              name: source.label,
            }
          : {
              has_location: false,
              id: "",
              kind: "event",
              name: "",
            },
    title: kind === "funnel" ? "Untitled funnel" : "Untitled chart",
    visualization: kind === "funnel" ? "funnel" : "line",
  };
}

function cryptoSafeId() {
  if (typeof crypto !== "undefined" && "randomUUID" in crypto) {
    return crypto.randomUUID();
  }
  return `draft-${Date.now()}-${Math.random().toString(36).slice(2)}`;
}

export function widgetDateRange(
  definition: ProViewWidgetDefinition,
): ProViewDateRange {
  return (
    definition.date_range ?? {
      days:
        definition.comparison === "today_vs_yesterday"
          ? 1
          : definition.comparison === "last_7_vs_previous_7"
            ? 7
            : 30,
      compare_previous: definition.comparison !== "none",
    }
  );
}
export function widgetRangeLabel(definition: ProViewWidgetDefinition) {
  if (!definition.date_range)
    return definition.comparison === "none"
      ? "Last 30 days"
      : comparisonLabel(definition.comparison);
  const { days, compare_previous } = definition.date_range;
  return `${days === 1 ? "Last 24 hours" : `Last ${days} days`}${compare_previous ? " vs previous period" : ""}`;
}
export function breakdownLabel(definition: ProViewWidgetDefinition) {
  if (definition.breakdown === "property" && definition.visualization !== "map")
    return `Property: ${definition.breakdown_property ?? ""}`;
  return PRO_VIEW_BREAKDOWNS[
    definition.visualization === "map"
      ? "country"
      : (definition.breakdown ?? "time")
  ];
}

/** Send only editable fields; saved widgets also carry server-owned metadata. */
export function widgetRequestBody(definition: ProViewWidgetDefinition) {
  return {
    comparison: definition.comparison,
    metric: definition.metric,
    breakdown_property: definition.breakdown_property,
    property_filters: definition.property_filters,
    date_range: definition.date_range,
    breakdown: definition.breakdown,
    display: definition.display,
    filters: definition.filters,
    funnel: definition.funnel,
    kind: definition.kind,
    source: definition.source,
    title: definition.title,
    visualization: definition.visualization,
  };
}

export function widgetMetricLabel(definition?: ProViewWidgetDefinition | null) {
  return {
    events: "Occurrences",
    visitors: "Unique visitors",
    sessions: "Sessions",
  }[definition?.metric ?? "events"];
}
