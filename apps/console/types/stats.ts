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

export type StatsOverviewResponse = {
  browsers: DimensionStat[];
  countries: DimensionStat[];
  days: number;
  start_date: string;
  end_date: string;
  interval: "day" | "week" | "month";
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
};
