import { guarded, reportFailure, debugLog } from "./safety";
import { instances } from "./instances";
import type { OwlConfig, OwlEvent, OwlEventEnvelope } from "../types";
import { assertSiteId, normalizeConfig, type NormalizedConfig } from "./config";

export interface Emitter {
  readonly config: NormalizedConfig;
  readonly enabled: boolean;
  readonly siteId: string;
  emit(event: OwlEvent): void;
}

export function createEmitter(siteId: string, config?: OwlConfig): Emitter {
  const normalizedSiteId = assertSiteId(siteId);
  const normalized = normalizeConfig(config);
  const enabled =
    (!normalized.respectDoNotTrack || !hasDoNotTrackSignal()) &&
    (!normalized.respectGlobalPrivacyControl ||
      !hasGlobalPrivacyControlSignal());

  if (!enabled && normalized.debug) {
    debugLog("tracking disabled by a browser privacy signal");
  }

  return {
    config: normalized,
    enabled,
    siteId: normalizedSiteId,
    emit(event) {
      if (!enabled) return;

      const envelope: OwlEventEnvelope = {
        event,
        site_id: normalizedSiteId,
      };

      sendEnvelope(envelope, normalized);
    },
  };
}

export function buildUrl(config: NormalizedConfig, path: string): string {
  return `${config.server}${path.startsWith("/") ? path : `/${path}`}`;
}

function sendEnvelope(
  envelope: OwlEventEnvelope,
  config: NormalizedConfig,
): void {
  try {
    if (config.debug) debugLog(envelope);
    if (config.mock || typeof window === "undefined") return;
    if (typeof fetch !== "function" || typeof AbortController !== "function")
      return;

    const budgets = instances<DeliveryBudget>("transport");
    let budget = budgets.get("document");
    if (!budget) {
      budget = { pending: 0, bytes: 0, tokens: 200, updated: Date.now() };
      budgets.set("document", budget);
    }
    const now = Date.now();
    budget.tokens = Math.min(
      200,
      budget.tokens + Math.max(0, now - budget.updated) / 10,
    );
    budget.updated = now;
    if (budget.pending >= 8 || budget.tokens < 1) return;
    const body = JSON.stringify(envelope);
    const bytes = new TextEncoder().encode(body).byteLength;
    if (budget.bytes + bytes > 60_000) return;
    budget.pending += 1;
    budget.bytes += bytes;
    budget.tokens -= 1;

    const controller = new AbortController();
    let finished = false;
    const finish = guarded("delivery cleanup", () => {
      if (finished) return;
      finished = true;
      budget.pending -= 1;
      budget.bytes -= bytes;
      clearTimeout(timeout);
    });
    const timeout = setTimeout(
      guarded("delivery timeout", () => {
        finish();
        reportFailure("delivery timeout");
        controller.abort();
      }),
      5_000,
    );
    // No queue, persistence, or retry storm: drop excess work instead of slowing
    // the host application. The shared byte budget stays below keepalive limits.
    try {
      void fetch(buildUrl(config, "/v1/events"), {
        body,
        cache: "no-store",
        credentials: "omit",
        headers: { "content-type": "application/json" },
        keepalive: true,
        method: "POST",
        mode: "cors",
        referrerPolicy: "no-referrer",
        signal: controller.signal,
      })
        .then((response) => {
          if (!response.ok) reportFailure("HTTP delivery");
          return response.body?.cancel();
        })
        .catch((error: unknown) => {
          reportFailure("network delivery");
        })
        .finally(finish);
    } catch {
      finish();
      reportFailure("network delivery");
    }
  } catch {
    reportFailure("event delivery");
  }
}

interface DeliveryBudget {
  pending: number;
  bytes: number;
  tokens: number;
  updated: number;
}

function hasDoNotTrackSignal(): boolean {
  if (typeof navigator === "undefined") return false;

  const legacyNavigator = navigator as Navigator & {
    msDoNotTrack?: string;
  };
  const legacyWindow =
    typeof window === "undefined"
      ? undefined
      : (window as Window & { doNotTrack?: string });
  const values = [
    navigator.doNotTrack,
    legacyNavigator.msDoNotTrack,
    legacyWindow?.doNotTrack,
  ];

  return values.some(
    (value) => value === "1" || value?.toLowerCase() === "yes",
  );
}

function hasGlobalPrivacyControlSignal(): boolean {
  if (typeof navigator === "undefined") return false;

  return (
    (
      navigator as Navigator & {
        globalPrivacyControl?: boolean;
      }
    ).globalPrivacyControl === true
  );
}
