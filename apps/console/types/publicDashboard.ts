export const publicMetricLabels = {
  visitors: "Unique visitors",
  pageviews: "Page views",
  events: "Events",
  sessions: "Sessions",
} as const;
export const publicBreakdownLabels = {
  countries: "Countries",
  browsers: "Browsers",
  devices: "Devices",
  operating_systems: "Operating systems",
} as const;
export type PublicMetric = keyof typeof publicMetricLabels;
export type PublicBreakdown = keyof typeof publicBreakdownLabels;
export type PublicShareConfig = {
  enabled: boolean;
  metrics: PublicMetric[];
  traffic: boolean;
  breakdowns: PublicBreakdown[];
  max_days: number;
  urls: string[];
};
export type PublicSharingSettings = {
  eligible: boolean;
  config: PublicShareConfig;
  available_urls: string[];
  site_id: string;
};
export type PublicOverview = {
  name: string;
  max_days: number;
  metrics: PublicMetric[];
  data: {
    days: number;
    start_date: string;
    end_date: string;
    totals: Partial<Record<PublicMetric, number>>;
    audience_totals?: { pageviews: number; visitors: number };
    traffic?: Array<{ date: string } & Partial<Record<PublicMetric, number>>>;
  } & Partial<
    Record<
      PublicBreakdown,
      Array<{ name: string; count: number; visitors: number }>
    >
  >;
};
