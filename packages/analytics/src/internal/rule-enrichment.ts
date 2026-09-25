import type { OwlRecord } from "../types.js";
import type { RuleEventType, TrackingRule } from "./rule-matching.js";

export interface RuleEnrichmentContext {
  element: Element;
  event?: Event;
  type: RuleEventType;
}

export type RuleEnricher = (
  rule: Readonly<TrackingRule>,
  context: Readonly<RuleEnrichmentContext>,
) => OwlRecord | void;
