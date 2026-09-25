export type LiveCounts = {
  events: number;
  pageviews: number;
  errors: number;
  custom_events: number;
  selected_events: number;
  active_visitors: number;
};
export type LiveResponse = {
  as_of: string;
  start_at: string;
  selected_event: string | null;
  totals: LiveCounts;
  minutes: Array<LiveCounts & { at: string }>;
  breakdowns: Record<string, Array<{ name: string; count: number }>>;
};
