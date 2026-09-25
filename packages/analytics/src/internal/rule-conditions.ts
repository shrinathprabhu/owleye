import { guarded, safely } from "./safety";
import {
  elementInputLength,
  keyboardEventKey,
  keyConditionForRule,
  keyConditionValue,
  ruleAppliesToPage,
  type RuleKeyCondition,
  type TrackingRule,
} from "./rule-matching";

type RuleMatchHandler = (
  rule: TrackingRule,
  element: Element,
  event?: Event,
) => void;

const EXPECTED_KEYS: Partial<Record<RuleKeyCondition, readonly string[]>> = {
  enter: ["Enter"],
  escape: ["Escape"],
  period: ["."],
  space: [" ", "Spacebar"],
};

/** Owns stateful keyboard thresholds and debounce timers for matched rules. */
export class RuleConditionController {
  private readonly activeTimeouts = new Set<ReturnType<typeof setTimeout>>();
  private readonly characterThresholds = new Map<string, WeakSet<Element>>();
  private readonly debounceTimers = new Map<
    string,
    WeakMap<Element, ReturnType<typeof setTimeout>>
  >();
  private stopped = false;

  constructor(private readonly onMatch: RuleMatchHandler) {}

  handle(rule: TrackingRule, element: Element, event: Event): void {
    if (this.stopped) return;
    const condition = keyConditionForRule(rule);
    if (!condition) {
      this.onMatch(rule, element, event);
      return;
    }
    if (condition === "characters") {
      this.handleCharacterThreshold(rule, element, event);
      return;
    }
    if (condition === "debounce") {
      this.scheduleDebounce(rule, element);
      return;
    }

    const expectedKeys = EXPECTED_KEYS[condition];
    if (expectedKeys?.includes(keyboardEventKey(event) ?? "")) {
      this.onMatch(rule, element, event);
    }
  }

  stop(): void {
    if (this.stopped) return;
    this.stopped = true;
    for (const timeout of this.activeTimeouts)
      safely("rule timer cleanup", () => clearTimeout(timeout));
    this.activeTimeouts.clear();
    this.characterThresholds.clear();
    this.debounceTimers.clear();
  }

  private handleCharacterThreshold(
    rule: TrackingRule,
    element: Element,
    event: Event,
  ): void {
    const threshold = Math.max(1, Math.round(keyConditionValue(rule)));
    const matchedElements = getOrCreate(
      this.characterThresholds,
      rule.id,
      () => new WeakSet<Element>(),
    );

    if (elementInputLength(element) < threshold) {
      matchedElements.delete(element);
      return;
    }
    if (matchedElements.has(element)) return;
    matchedElements.add(element);
    this.onMatch(rule, element, event);
  }

  private scheduleDebounce(rule: TrackingRule, element: Element): void {
    const timers = getOrCreate(
      this.debounceTimers,
      rule.id,
      () => new WeakMap<Element, ReturnType<typeof setTimeout>>(),
    );
    this.clearTimer(timers.get(element));
    if (this.activeTimeouts.size >= 250) return;

    const seconds = Math.min(60, Math.max(0.1, keyConditionValue(rule) || 1));
    const timeout = setTimeout(
      guarded("rule timer", () => {
        timers.delete(element);
        this.activeTimeouts.delete(timeout);
        if (
          !this.stopped &&
          typeof window !== "undefined" &&
          ruleAppliesToPage(rule, window.location.pathname)
        ) {
          this.onMatch(rule, element);
        }
      }),
      seconds * 1_000,
    );

    timers.set(element, timeout);
    this.activeTimeouts.add(timeout);
  }

  private clearTimer(timeout: ReturnType<typeof setTimeout> | undefined): void {
    if (timeout === undefined) return;
    clearTimeout(timeout);
    this.activeTimeouts.delete(timeout);
  }
}

function getOrCreate<Key, Value>(
  map: Map<Key, Value>,
  key: Key,
  create: () => Value,
): Value {
  const existing = map.get(key);
  if (existing !== undefined) return existing;
  const value = create();
  map.set(key, value);
  return value;
}
