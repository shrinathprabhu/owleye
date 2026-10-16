export type StatsTotals = {
  avg_duration_ms: number;
  events: number;
  external_events: number;
  pageviews: number;
  pageview_visitors: number;
  performance_events: number;
  rule_events: number;
  sessions: number;
  visitors: number;
};

export type TrafficPoint = {
  date: string;
  events: number;
  pageviews: number;
  visitors: number;
};

export type RegionTimeseriesPoint = {
  count: number;
  date: string;
  name: string;
};

export type DimensionStat = {
  count: number;
  visitors?: number;
  name: string;
};

export type TopPageStat = {
  path: string;
  title: string;
  views: number;
};

export type EventNameStat = {
  count: number;
  event_name: string;
  event_type: string;
};

export type CohortSummary = {
  status:
    | "observed_estimate"
    | "no_eligible_cohorts"
    | "unavailable_in_demo"
    | "query_failed";
  observed_pageview_cohorts: number;
  eligible_cohorts: number;
  excluded_open_or_boundary_cohorts: number;
  single_page_cohorts: number;
  estimated_bounce_rate: number | null;
  observed_span_samples: number;
  avg_observed_span_ms: number | null;
  median_observed_span_ms: number | null;
  entry_pages_withheld: boolean;
  last_pages_withheld: boolean;
};

export type CohortPoint = Pick<
  CohortSummary,
  | "eligible_cohorts"
  | "single_page_cohorts"
  | "estimated_bounce_rate"
  | "observed_span_samples"
  | "avg_observed_span_ms"
> & { date: string };

export type MeasurementContext = {
  definition_version: string;
  channel_version: string;
  timezone: "UTC";
  as_of: string;
  cohort_assignment: "fixed_30_minute_utc_bucket_start";
  delivery_grace_seconds: number;
  retention_context: "active_rows_only_historical_completeness_unknown";
  late_data_may_revise_results: true;
  acquisition_values: string;
  acquisition_status: "ready" | "query_failed" | "unavailable_in_demo";
  legacy_totals_policy?: "legacy_all_agents_v1";
  new_metrics_policy?: "known_crawlers_excluded_v1";
  unknown_agents?: "included_not_verified_human";
  classification_context?: "stored_ingestion_ua_classification_not_recomputed";
  demo: boolean;
};

export type StatsOverviewResponse = {
  browsers: DimensionStat[];
  countries: DimensionStat[];
  days: number;
  start_date: string;
  end_date: string;
  interval: "hour" | "day" | "week" | "month";
  devices: DimensionStat[];
  event_names: EventNameStat[];
  operating_systems: DimensionStat[];
  referrers: DimensionStat[];
  region_timeseries: RegionTimeseriesPoint[];
  regions: DimensionStat[];
  site_id: string;
  timeseries: TrafficPoint[];
  top_pages: TopPageStat[];
  totals: StatsTotals;
  utm_campaigns: DimensionStat[];
  // Additive contracts: old deployments or older snapshots can omit them.
  measurement?: MeasurementContext;
  cohorts?: CohortSummary;
  cohort_timeseries?: CohortPoint[];
  channels?: DimensionStat[];
  utm_sources?: DimensionStat[];
  utm_mediums?: DimensionStat[];
  entry_pages?: { path: string; cohorts: number }[];
  last_observed_pages?: { path: string; cohorts: number }[];
};
