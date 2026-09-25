import {
  domEventForRule,
  type RuleDomEvent,
  type TrackingRule,
} from "./rule-matching";

export interface RuleCatalog {
  byDomEvent: ReadonlyMap<RuleDomEvent, readonly TrackingRule[]>;
  viewRules: readonly TrackingRule[];
}

export function createRuleCatalog(rules: readonly TrackingRule[]): RuleCatalog {
  const byDomEvent = new Map<RuleDomEvent, TrackingRule[]>();
  const viewRules: TrackingRule[] = [];

  for (const rule of rules) {
    if (rule.type === "view") viewRules.push(rule);
    const eventName = domEventForRule(rule);
    if (!eventName) continue;
    const eventRules = byDomEvent.get(eventName) ?? [];
    eventRules.push(rule);
    byDomEvent.set(eventName, eventRules);
  }

  return { byDomEvent, viewRules };
}
