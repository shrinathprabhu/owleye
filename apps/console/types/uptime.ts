export interface UptimeMonitor {
  id: string;
  name: string;
  url: string;
  enabled: boolean;
  state: "up" | "down" | "unknown" | "paused" | "disabled";
  last_checked_at: number | null;
  status_code: number | null;
  duration_ms: number | null;
  failure: string | null;
}
export interface UptimeSummary {
  monitors: UptimeMonitor[];
  limit: number;
  used: number;
  interval_seconds: number;
  history_days: number;
}
export interface UptimeHistory {
  history_available: boolean;
  checks: {
    checked_at: number;
    status_code: number | null;
    duration_ms: number;
    state: string;
    failure: string;
  }[];
  incidents: {
    id: string;
    started_at: number;
    ended_at: number | null;
    resolution: string | null;
    failure: string;
    status_code: number | null;
  }[];
}
