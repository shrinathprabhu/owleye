export type ReportPopulation = {
  policy?: "legacy_all_agents_v1" | "known_crawlers_excluded_v1";
  unknown_agents?: "included_not_verified_human";
  classification_context?: "stored_ingestion_ua_classification_not_recomputed";
};

export function populationDescription(policy?: ReportPopulation["policy"]) {
  if (policy === "known_crawlers_excluded_v1")
    return "Known crawlers are excluded using the stored classification. Unknown agents remain included; this does not verify human traffic.";
  if (policy === "legacy_all_agents_v1")
    return "Legacy population includes all stored agent classifications, including known crawlers. It is not a verified human audience.";
  return "This response does not expose a crawler population policy; do not assume it represents verified human traffic.";
}
