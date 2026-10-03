import { instances } from "./instances";

export const noop = (): void => undefined;
const fallbackReports = { count: 0, since: 0 };

/** Diagnostics must never throw, leak payloads, or flood a host console. */
export function reportFailure(operation: string): void {
  reportWarning(`${operation} failed; analytics work was skipped.`);
}

export function reportWarning(message: string): void {
  try {
    let reports = fallbackReports;
    try {
      if (typeof window !== "undefined") {
        const registry = instances<typeof reports>("diagnostics");
        reports = registry.get("errors") ?? { count: 0, since: 0 };
        registry.set("errors", reports);
      }
    } catch {
      /* A locked global registry must not break diagnostics. */
    }
    const now = Date.now();
    if (now - reports.since >= 60_000) {
      reports.count = 0;
      reports.since = now;
    }
    if (reports.count++ >= 10) return;
    console.warn(`[OwlEye] ${message}`);
  } catch {
    /* Even a replaced or unavailable console is optional. */
  }
}

export function safely<T>(operation: string, work: () => T, fallback: T): T;
export function safely(operation: string, work: () => void): void;
export function safely<T>(
  operation: string,
  work: () => T,
  fallback?: T,
): T | undefined {
  try {
    return work();
  } catch {
    reportFailure(operation);
    return fallback;
  }
}

export function guarded<Args extends unknown[]>(
  operation: string,
  callback: (...args: Args) => void,
): (...args: Args) => void {
  return (...args) => safely(operation, () => callback(...args));
}

export function cleanup(...tasks: Array<() => void>): void {
  for (const task of tasks) safely("cleanup", task);
}

export function debugLog(...args: unknown[]): void {
  safely("debug logging", () => {
    const registry = instances<{ count: number; since: number }>("diagnostics");
    const now = Date.now();
    let budget = registry.get("debug");
    if (!budget || now - budget.since >= 60_000) {
      budget = { count: 0, since: now };
      registry.set("debug", budget);
    }
    if (budget.count++ >= 100) return;
    console.info("[OwlEye]", ...args);
  });
}
