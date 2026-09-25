import type { RuleEventType, TrackingRule } from "./rule-matching";

const MAX_RULES_PER_RESPONSE = 5_000;
const RULE_TYPES = new Set<RuleEventType>([
  "change",
  "focus",
  "blur",
  "keydown",
  "play",
  "ended",
  "click",
  "dblclick",
  "keypress",
  "keyup",
  "mousedown",
  "mouseup",
  "submit",
  "view",
]);

export interface RuleValidationIssue {
  index: number;
  message: string;
}

export interface ParsedRulesResponse {
  issues: RuleValidationIssue[];
  rules: TrackingRule[];
}

/**
 * Keep the browser resilient to an invalid control-plane response without
 * duplicating the server's full rule schema validator in every visitor bundle.
 */
export function parseRulesResponse(value: unknown): ParsedRulesResponse {
  if (!isRecord(value) || !Array.isArray(value.rules)) {
    throw new TypeError("Rules response must contain a rules array.");
  }

  const issues: RuleValidationIssue[] = [];
  const rules: TrackingRule[] = [];
  const ids = new Set<string>();
  for (const [index, candidate] of value.rules
    .slice(0, MAX_RULES_PER_RESPONSE)
    .entries()) {
    if (isBrowserRule(candidate)) {
      if (!ids.has(candidate.id)) rules.push(candidate);
      ids.add(candidate.id);
    } else issues.push({ index, message: "Rule is missing required fields." });
  }

  if (value.rules.length > MAX_RULES_PER_RESPONSE) {
    issues.push({
      index: MAX_RULES_PER_RESPONSE,
      message: "Rules response exceeds the browser safety limit.",
    });
  }
  return { issues, rules };
}

function isBrowserRule(value: unknown): value is TrackingRule {
  return (
    isRecord(value) &&
    isNonEmptyString(value.id) &&
    isNonEmptyString(value.name) &&
    isNonEmptyString(value.selector) &&
    typeof value.type === "string" &&
    RULE_TYPES.has(value.type as RuleEventType)
  );
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}

function isNonEmptyString(value: unknown): value is string {
  return typeof value === "string" && value.trim().length > 0;
}
