export type CampaignItem = {
  campaign_key: string;
  conversions: number;
  created_at: string | null;
  destination_url: string | null;
  id: string | null;
  medium: string;
  name: string;
  source: string;
  tracking_url: string | null;
  updated_at: string | null;
  views: number;
  visitors: number;
};

export type CampaignsResponse = {
  days: number;
  items: CampaignItem[];
  site_id: string;
  totals: {
    campaigns: number;
    conversions: number;
    views: number;
    visitors: number;
  };
};

export const PERFORMANCE_METRICS = [
  "LCP",
  "INP",
  "CLS",
  "FCP",
  "TTFB",
] as const;
export type PerformanceMetric = (typeof PERFORMANCE_METRICS)[number];
export type PerformanceMetricSummary = {
  metric: PerformanceMetric;
  p50: number;
  p75: number;
  p95: number;
  rating: "good" | "needs_improvement" | "poor";
  samples: number;
};
export type PerformancePoint = {
  date: string;
  p50: number;
  p75: number;
  p95: number;
  samples: number;
};
export type PerformanceResponse = {
  collecting: boolean;
  days: number;
  metric: PerformanceMetric;
  metrics: PerformanceMetricSummary[];
  pages: Array<{ p75: number; path: string; samples: number }>;
  series: PerformancePoint[];
  site_id: string;
};
