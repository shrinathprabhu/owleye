export type AiModeResponse={provider_available:boolean;can_manage:boolean;can_use:boolean;enabled:boolean;site_id:string;unlimited:boolean};
export type AiPromptResponse = {
  accepted: boolean;
  answer: string;
  request_id: string;
  evidence: AiEvidence;
};

export type AiMetric = "events" | "pageviews" | "visitors" | "sessions";
export type AiChart = "auto" | "none" | "line" | "bar" | "pie" | "donut";
export type AiEvidence = {
  report: string;
  start_date: string;
  end_date: string;
  timezone: string;
  period_label?: string;
  groups_suppressed_below_visitors: number;
  country?: string | string[] | null;
  browser?: string | string[] | null;
  device?: string | string[] | null;
  os?: string | string[] | null;
  chart?: AiChart;
  metric?: AiMetric;
  output?: "text" | "pdf";
  details?: {
    cities: string[];
    events: string[];
    campaigns: string[];
    properties: string[];
    steps: string[];
    notes: string[];
  };
  rows: {
    label: string;
    events: number;
    pageviews: number;
    visitors: number;
    sessions: number;
  }[];
};
