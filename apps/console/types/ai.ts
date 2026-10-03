export type AiAllocationMode = "custom" | "equal" | "fluid";

export type AiMemberAllowance = {
  ai_access: boolean;
  allocated_prompts: number | null;
  email: string;
  is_current_user: boolean;
  name?: string | null;
  prompts_remaining: number;
  prompts_used: number;
  role: "admin" | "owner" | "read_only";
  user_id: string;
};

export type AiModeResponse = {
  provider_available: boolean;
  allocation_mode: AiAllocationMode;
  app_prompts_remaining: number;
  app_prompts_used: number;
  can_manage: boolean;
  can_use: boolean;
  enabled: boolean;
  member_prompts_remaining: number;
  member_prompts_used: number;
  members?: AiMemberAllowance[];
  site_id: string;
  total_prompts: number;
};

export type AiPromptResponse = {
  accepted: boolean;
  answer: string;
  request_id: string;
  evidence: AiEvidence;
  app_prompts_remaining: number;
  member_prompts_remaining: number;
};

export type AiMetric = "events" | "pageviews" | "visitors" | "sessions" | "value";
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
    value?: number;
    events: number;
    pageviews: number;
    visitors: number;
    sessions: number;
  }[];
};
