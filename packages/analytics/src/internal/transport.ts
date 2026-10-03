import { guarded, reportFailure, debugLog } from "./safety";
import { instances } from "./instances";
import type { OwlConfig, OwlEvent, OwlEventEnvelope } from "../types";
import { assertSiteId, normalizeConfig, type NormalizedConfig } from "./config";

export interface Emitter {
  readonly config: NormalizedConfig;
  readonly enabled: boolean;
  readonly siteId: string;
  emit(event: OwlEvent): void;
  diagnose(reason: string, counts?: Record<string, number>): void;
}

export function createEmitter(siteId: string, config?: OwlConfig): Emitter {
  const normalizedSiteId = assertSiteId(siteId);
  const normalized = normalizeConfig(config);
  const enabled =
    !normalized.respectGlobalPrivacyControl || !hasGlobalPrivacyControlSignal();

  if (!enabled && normalized.debug) {
    debugLog("suppressed: Global Privacy Control is enabled");
  }

  return {
    config: normalized,
    enabled,
    siteId: normalizedSiteId,
    diagnose(reason, counts) {
      if (normalized.debug) debugLog(reason, counts ?? {});
    },
    emit(event) {
      if (!enabled) {
        if (normalized.debug)
          debugLog("suppressed: Global Privacy Control is enabled");
        return;
      }

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
    const diagnose = (reason: string, counts?: Record<string, number>) => {
      if (config.debug) debugLog(reason, counts ?? {});
    };
    if (config.mock || typeof window === "undefined") {
      diagnose(
        config.mock ? "not sent: mock mode" : "not sent: browser unavailable",
      );
      return;
    }
    if (typeof fetch !== "function" || typeof AbortController !== "function") {
      diagnose("dropped: fetch or AbortController unavailable");
      return;
    }

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
    if (budget.pending >= 8 || budget.tokens < 1) {
      diagnose(
        budget.pending >= 8
          ? "dropped: pending request limit"
          : "dropped: event rate budget",
      );
      return;
    }
    const body = JSON.stringify(envelope);
    const bytes = new TextEncoder().encode(body).byteLength;
    if (budget.bytes + bytes > 60_000) {
      diagnose("dropped: keepalive byte budget");
      return;
    }
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
        diagnose("delivery unconfirmed: request timed out");
        reportFailure("delivery timeout");
        controller.abort();
      }),
      5_000,
    );
    // No queue, persistence, or retry storm: drop excess work instead of slowing
    // the host application. The shared byte budget stays below keepalive limits.
    try {
      diagnose("request submitted; acceptance not yet confirmed");
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
        .then(async (response) => {
          if (!response.ok) {
            diagnose("rejected: HTTP response", { status: response.status });
            reportFailure("HTTP delivery");
            return response.body?.cancel();
          }
          if (!config.debug) return response.body?.cancel();
          const counts = await readAcknowledgement(response);
          if (counts)
            diagnose(
              counts.accepted === 0
                ? "not accepted: API acknowledged zero events"
                : "API acknowledged events; warehouse storage not independently verified",
              counts,
            );
          else
            diagnose("HTTP acknowledged; accepted count unavailable", {
              status: response.status,
            });
        })
        .catch(() => {
          diagnose("delivery unconfirmed: network failure or aborted request");
          reportFailure("network delivery");
        })
        .finally(finish);
    } catch {
      finish();
      diagnose("delivery unconfirmed: network request failed");
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

// Debug reads only a small acknowledgement, never logs response text or events.
// The existing request timeout also bounds this read; no retries or persistence.
async function readAcknowledgement(
  response: Response,
): Promise<Record<string, number> | undefined> {
  if (!response.body?.getReader) return undefined;
  const reader = response.body.getReader();
  try {
    let length = 0;
    const chunks: Uint8Array[] = [];
    while (true) {
      const { done, value } = await reader.read();
      if (done) break;
      length += value.byteLength;
      if (length > 4096) return undefined;
      chunks.push(value);
    }
    const bytes = new Uint8Array(length);
    let offset = 0;
    for (const chunk of chunks) {
      bytes.set(chunk, offset);
      offset += chunk.byteLength;
    }
    const result: unknown = JSON.parse(new TextDecoder().decode(bytes));
    if (!result || typeof result !== "object") return undefined;
    const data = result as Record<string, unknown>;
    if (
      typeof data.accepted !== "number" ||
      !Number.isSafeInteger(data.accepted) ||
      data.accepted < 0
    )
      return undefined;
    const counts: Record<string, number> = { accepted: data.accepted };
    if (
      typeof data.dropped === "number" &&
      Number.isSafeInteger(data.dropped) &&
      data.dropped >= 0
    )
      counts.dropped = data.dropped;
    return counts;
  } catch {
    return undefined;
  } finally {
    try {
      await reader.cancel();
    } catch {
      /* Diagnostics are optional. */
    }
    reader.releaseLock();
  }
}
