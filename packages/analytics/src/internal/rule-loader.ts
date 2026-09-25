import { guarded, cleanup } from "./safety";
import type { NormalizedConfig } from "./config";
import type { TrackingRule } from "./rule-matching";
import { parseRulesResponse, type RuleValidationIssue } from "./rule-guard";
import { buildUrl } from "./transport";

export interface LoadedRules {
  issues: RuleValidationIssue[];
  rules: TrackingRule[];
}

export async function fetchTrackingRules(
  siteId: string,
  config: NormalizedConfig,
  signal: AbortSignal,
): Promise<LoadedRules> {
  if (typeof fetch !== "function") {
    throw new Error("This browser does not support Fetch.");
  }

  const url = `${buildUrl(config, "/v1/rules")}?site_id=${encodeURIComponent(siteId)}`;
  const controller = new AbortController();
  const abort = guarded("rule request cancellation", () => controller.abort());
  signal.addEventListener("abort", abort, { once: true });
  if (signal.aborted) abort();
  const timeout = setTimeout(abort, 10_000);
  try {
    const response = await fetch(url, {
      cache: "no-store",
      credentials: "omit",
      headers: { accept: "application/json" },
      method: "GET",
      mode: "cors",
      referrerPolicy: "strict-origin",
      signal: controller.signal,
    });

    if (!response.ok) {
      throw new Error(`Rules request failed with HTTP ${response.status}.`);
    }

    return parseRulesResponse(await response.json());
  } finally {
    cleanup(
      () => clearTimeout(timeout),
      () => signal.removeEventListener("abort", abort),
    );
  }
}
