export type RuleDomEvent =
  | "change"
  | "focus"
  | "blur"
  | "keydown"
  | "play"
  | "ended"
  | "click"
  | "dblclick"
  | "keypress"
  | "keyup"
  | "mousedown"
  | "mouseup"
  | "submit";

export type RuleEventType = RuleDomEvent | "view";

export type RuleSelectorType = "class" | "css" | "id" | "text" | "xpath";

export type RuleKeyCondition =
  "characters" | "debounce" | "enter" | "escape" | "period" | "space";

export interface RuleTriggerConfig {
  /** Legacy API alias retained so older persisted rules keep working. */
  condition?:
    | { kind: "characters"; count: number }
    | { kind: "debounce"; milliseconds: number }
    | { kind: "key"; key: "." | " " | "Enter" | "Escape" };
  condition_value?: number;
  /** Legacy API alias retained so older persisted rules keep working. */
  custom_props?: RuleCustomProperty[];
  debounce_seconds?: number;
  dom_event?: RuleDomEvent;
  /** Legacy API alias retained so older persisted rules keep working. */
  event?:
    | RuleDomEvent
    | "double_click"
    | "key_press"
    | "key_up"
    | "mouse_down"
    | "mouse_up"
    | "view";
  key_condition?: RuleKeyCondition;
  key_value?: number;
  on?: RuleDomEvent;
  page?: string;
  page_path?: string;
  selector_type?: RuleSelectorType;
}

export interface RuleCustomProperty {
  key: string;
  selector: string;
  selector_type?: RuleSelectorType;
  /** Optional hint accepted from the control plane; value-first fallback remains safe. */
  source?: "text_content" | "value";
}

export interface RuleMetadata {
  custom_props?: RuleCustomProperty[];
  event_name?: string;
}

export interface TrackingRule {
  capture_text?: boolean;
  id: string;
  metadata?: RuleMetadata;
  name: string;
  sample_rate?: number;
  selector: string;
  trigger_config?: RuleTriggerConfig;
  type: RuleEventType;
}

export type AutomaticRuleRecord = Record<string, boolean | number | string>;

const MAX_TEXT_SCAN_ELEMENTS = 2_000;
const MAX_XPATH_RESULTS = 500;
const MAX_CAPTURED_VALUE_LENGTH = 512;

export function domEventForRule(rule: TrackingRule): RuleDomEvent | undefined {
  if (rule.type === "view") return undefined;
  const configured =
    rule.trigger_config?.dom_event ??
    rule.trigger_config?.on ??
    legacyDomEvent(rule.trigger_config?.event);
  return configured ?? rule.type;
}

export function ruleAppliesToPage(
  rule: TrackingRule,
  pathname: string,
): boolean {
  const page = rule.trigger_config?.page_path ?? rule.trigger_config?.page;
  return !page || page === pathname;
}

export function closestRuleElement(
  rule: TrackingRule,
  target: Element,
  documentRef: Document,
): Element | null {
  const selectorType = rule.trigger_config?.selector_type ?? "css";

  if (selectorType === "xpath") {
    const matches = new Set(findXPathElements(documentRef, rule.selector));
    return closestMatchingAncestor(target, (element) => matches.has(element));
  }

  if (selectorType === "id") {
    return closestMatchingAncestor(
      target,
      (element) => element.id === rule.selector,
    );
  }

  if (selectorType === "class") {
    return closestMatchingAncestor(target, (element) =>
      elementClasses(element).includes(rule.selector),
    );
  }

  if (selectorType === "text") {
    const expected = normalizeText(rule.selector);
    return closestMatchingAncestor(target, (element) =>
      normalizeText(element.textContent ?? elementText(element)).includes(
        expected,
      ),
    );
  }

  return target.closest(rule.selector);
}

export function elementsForViewRule(
  rule: TrackingRule,
  documentRef: Document,
): Element[] {
  const selectorType = rule.trigger_config?.selector_type ?? "css";

  if (selectorType === "xpath") {
    return findXPathElements(documentRef, rule.selector);
  }
  if (selectorType === "id") {
    const element = documentRef.getElementById?.(rule.selector);
    return element ? [element] : [];
  }
  if (selectorType === "class") {
    return Array.from(
      documentRef.getElementsByClassName?.(rule.selector) ?? [],
    );
  }
  if (selectorType === "text") {
    const expected = normalizeText(rule.selector);
    return Array.from(documentRef.querySelectorAll("*"))
      .slice(0, MAX_TEXT_SCAN_ELEMENTS)
      .filter((element) =>
        normalizeText(element.textContent ?? elementText(element)).includes(
          expected,
        ),
      );
  }
  return Array.from(documentRef.querySelectorAll(rule.selector));
}

/** Finds rule matches only within a newly attached subtree. */
export function elementsForViewRuleInSubtree(
  rule: TrackingRule,
  root: Element,
  documentRef: Document,
): Element[] {
  const selectorType = rule.trigger_config?.selector_type ?? "css";

  if (selectorType === "xpath") {
    return findXPathElements(documentRef, rule.selector).filter(
      (element) => element === root || root.contains(element),
    );
  }

  const descendants = descendantsForSelector(rule, root);
  return elementMatchesRule(root, rule) ? [root, ...descendants] : descendants;
}

export function automaticRuleRecords(
  rule: TrackingRule,
  matchedElement: Element,
  documentRef: Document,
): AutomaticRuleRecord | undefined {
  const properties = (
    rule.metadata?.custom_props ??
    rule.trigger_config?.custom_props ??
    []
  ).slice(0, 10);
  if (!properties.length) return undefined;

  const records: AutomaticRuleRecord = {};
  for (const property of properties) {
    const key = property.key.trim();
    if (!key || key in records) continue;
    let element: Element | null = null;
    try {
      const scope =
        matchedElement.closest("[data-owleye-scope]") ?? matchedElement;
      element = firstConfiguredElementInScope(property, scope);
    } catch {
      // A stale or malformed remote selector must never break the host page's
      // event handler. Treat it like a configured property with no match.
    }
    if (!element) {
      try {
        element = firstConfiguredElement(property, documentRef);
      } catch {
        // The document fallback is equally defensive for stale selectors.
      }
    }
    const value = element ? readElementValue(element, property.source) : null;
    if (
      typeof value === "string" ||
      typeof value === "boolean" ||
      (typeof value === "number" && Number.isFinite(value))
    )
      records[key] = value;
  }

  return Object.keys(records).length ? records : undefined;
}

function firstConfiguredElementInScope(
  property: RuleCustomProperty,
  scope: Element,
): Element | null {
  const selectorType = property.selector_type ?? "css";
  if (selectorType === "css") return scope.querySelector(property.selector);
  if (selectorType === "class") {
    return scope.getElementsByClassName(property.selector)[0] ?? null;
  }
  if (selectorType === "text") {
    const expected = normalizeText(property.selector);
    return (
      Array.from(scope.querySelectorAll("*"))
        .slice(0, MAX_TEXT_SCAN_ELEMENTS)
        .find((element) =>
          normalizeText(element.textContent ?? elementText(element)).includes(
            expected,
          ),
        ) ?? null
    );
  }
  return null;
}

export function keyboardEventKey(event: Event): string | undefined {
  if (!("key" in event) || typeof event.key !== "string") return undefined;
  return event.key;
}

export function elementInputLength(element: Element): number {
  const value =
    "value" in element
      ? (element as Element & { value?: unknown }).value
      : undefined;
  if (typeof value === "string") return value.length;
  return (element.textContent ?? elementText(element)).length;
}

export function keyConditionValue(rule: TrackingRule): number {
  const legacy = rule.trigger_config?.condition;
  const legacyValue =
    legacy?.kind === "debounce"
      ? legacy.milliseconds / 1_000
      : legacy?.kind === "characters"
        ? legacy.count
        : 0;
  const value =
    rule.trigger_config?.condition_value ??
    rule.trigger_config?.debounce_seconds ??
    rule.trigger_config?.key_value ??
    legacyValue;
  return Number.isFinite(value) ? Math.max(0, value) : 0;
}

export function keyConditionForRule(
  rule: TrackingRule,
): RuleKeyCondition | undefined {
  const configured = rule.trigger_config?.key_condition;
  if (configured) return configured;

  const legacy = rule.trigger_config?.condition;
  if (!legacy) return undefined;
  if (legacy.kind === "characters" || legacy.kind === "debounce") {
    return legacy.kind;
  }
  if (legacy.key === "Enter") return "enter";
  if (legacy.key === "Escape") return "escape";
  if (legacy.key === ".") return "period";
  if (legacy.key === " ") return "space";
  return undefined;
}

function firstConfiguredElement(
  property: RuleCustomProperty,
  documentRef: Document,
): Element | null {
  const syntheticRule: TrackingRule = {
    id: "custom-property",
    name: property.key,
    selector: property.selector,
    trigger_config: { selector_type: property.selector_type ?? "css" },
    type: "click",
  };
  return elementsForViewRule(syntheticRule, documentRef)[0] ?? null;
}

function descendantsForSelector(rule: TrackingRule, root: Element): Element[] {
  const selectorType = rule.trigger_config?.selector_type ?? "css";
  if (selectorType === "id") {
    const escapedId =
      typeof CSS === "undefined" || typeof CSS.escape !== "function"
        ? rule.selector.replace(/["\\]/g, "\\$&")
        : CSS.escape(rule.selector);
    const element = root.querySelector(`[id="${escapedId}"]`);
    return element ? [element] : [];
  }
  if (selectorType === "class") {
    return Array.from(root.getElementsByClassName(rule.selector));
  }
  if (selectorType === "text") {
    const expected = normalizeText(rule.selector);
    return Array.from(root.querySelectorAll("*"))
      .slice(0, MAX_TEXT_SCAN_ELEMENTS)
      .filter((element) => elementMatchesText(element, expected));
  }
  return Array.from(root.querySelectorAll(rule.selector));
}

function elementMatchesRule(element: Element, rule: TrackingRule): boolean {
  const selectorType = rule.trigger_config?.selector_type ?? "css";
  if (selectorType === "id") return element.id === rule.selector;
  if (selectorType === "class")
    return elementClasses(element).includes(rule.selector);
  if (selectorType === "text") {
    return elementMatchesText(element, normalizeText(rule.selector));
  }
  if (selectorType === "xpath") return false;
  return element.matches(rule.selector);
}

function elementMatchesText(element: Element, expected: string): boolean {
  return normalizeText(element.textContent ?? elementText(element)).includes(
    expected,
  );
}

function readElementValue(
  element: Element,
  source?: RuleCustomProperty["source"],
): boolean | null | number | string {
  if (source === "text_content") {
    const text = element.textContent ?? elementText(element);
    return text == null
      ? null
      : text.trim().slice(0, MAX_CAPTURED_VALUE_LENGTH);
  }
  if ("value" in element) {
    const value = (element as Element & { value?: unknown }).value;
    if (typeof value === "boolean" || typeof value === "number") return value;
    if (typeof value === "string")
      return value.slice(0, MAX_CAPTURED_VALUE_LENGTH);
  }

  const text = element.textContent ?? elementText(element);
  return text == null ? null : text.trim().slice(0, MAX_CAPTURED_VALUE_LENGTH);
}

function legacyDomEvent(
  value: RuleTriggerConfig["event"],
): RuleDomEvent | undefined {
  if (!value || value === "view") return undefined;
  if (value === "double_click") return "dblclick";
  if (value === "key_press") return "keypress";
  if (value === "key_up") return "keyup";
  if (value === "mouse_down") return "mousedown";
  if (value === "mouse_up") return "mouseup";
  return value;
}

function closestMatchingAncestor(
  target: Element,
  matches: (element: Element) => boolean,
): Element | null {
  let element: Element | null = target;
  while (element) {
    if (matches(element)) return element;
    element = element.parentElement;
  }
  return null;
}

function elementClasses(element: Element): string[] {
  if (element.classList) return Array.from(element.classList);
  const className = "className" in element ? String(element.className) : "";
  return className.split(/\s+/).filter(Boolean);
}

function elementText(element: Element): string {
  return element instanceof HTMLElement ? (element.innerText ?? "") : "";
}

function normalizeText(value: string): string {
  return value.trim().replace(/\s+/g, " ").toLocaleLowerCase();
}

function findXPathElements(
  documentRef: Document,
  expression: string,
): Element[] {
  if (!documentRef.evaluate || typeof XPathResult === "undefined") return [];
  const result = documentRef.evaluate(
    expression,
    documentRef,
    null,
    XPathResult.ORDERED_NODE_SNAPSHOT_TYPE,
    null,
  );
  const elements: Element[] = [];
  const count = Math.min(result.snapshotLength, MAX_XPATH_RESULTS);
  for (let index = 0; index < count; index += 1) {
    const node = result.snapshotItem(index);
    if (node instanceof Element) elements.push(node);
  }
  return elements;
}
