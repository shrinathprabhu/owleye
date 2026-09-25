export type StoredRuleSelectorType = "class" | "css" | "id" | "text" | "xpath";
export type RuleSelectorType = StoredRuleSelectorType | "tracking";
export const TRACKING_VALUE_PATTERN = "[A-Za-z0-9][A-Za-z0-9:_\\x2d]{0,119}";

export function storedRuleSelector(selector: string, type: RuleSelectorType) {
  const value = selector.trim();
  if (type !== "tracking") return { selector: value, selector_type: type };
  if (!new RegExp(`^${TRACKING_VALUE_PATTERN}$`).test(value)) {
    throw new Error(
      "Use a tracking name with letters, numbers, colons, underscores, or hyphens (up to 120 characters).",
    );
  }
  return {
    selector: `[data-owleye-track="${value}"]`,
    selector_type: "css" as const,
  };
}

export function draftRuleSelector(
  selector: string,
  type: StoredRuleSelectorType = "css",
) {
  const match =
    type === "css" &&
    selector.match(
      /^\[data-owleye-track="([A-Za-z0-9][A-Za-z0-9:_-]{0,119})"\]$/,
    );
  return match
    ? { selector: match[1]!, selectorType: "tracking" as const }
    : { selector, selectorType: type };
}

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
  | "submit"
  | "view";

export type RuleKeyCondition =
  "characters" | "debounce" | "enter" | "escape" | "none" | "period" | "space";

export type RuleCustomPropertyDraft = {
  key: string;
  selector: string;
  selectorType: RuleSelectorType;
};

export type RuleDraft = {
  captureText: boolean;
  customProperties: RuleCustomPropertyDraft[];
  description: string;
  domEvent: RuleDomEvent;
  emittedEvent: string;
  enabled: boolean;
  keyCondition: RuleKeyCondition;
  keyValue: number;
  name: string;
  pagePath: string;
  samplePercent: number;
  selector: string;
  selectorType: RuleSelectorType;
};

export type TrackingRule = {
  capture_text: boolean;
  created_at: string;
  description: string | null;
  enabled: boolean;
  id: string;
  lifetime_event_count: number;
  metadata: {
    custom_props?: Array<{
      key: string;
      selector: string;
      selector_type?: StoredRuleSelectorType;
    }>;
    event_name?: string;
  };
  name: string;
  sample_rate: number;
  selector: string;
  site_id: string;
  trigger_config: {
    condition_value?: number;
    dom_event?: Exclude<RuleDomEvent, "view">;
    key_condition?: Exclude<RuleKeyCondition, "none">;
    page_path?: string;
    selector_type?: StoredRuleSelectorType;
  };
  type: "click" | "submit" | "view";
  updated_at: string;
};

export type RulePayload = {
  capture_text: boolean;
  description: string;
  enabled: boolean;
  metadata: TrackingRule["metadata"];
  name: string;
  sample_rate: number;
  selector: string;
  trigger_config: TrackingRule["trigger_config"];
  type: TrackingRule["type"];
};

export function defaultRuleDraft(): RuleDraft {
  return {
    captureText: false,
    customProperties: [],
    description: "",
    domEvent: "click",
    emittedEvent: "",
    enabled: true,
    keyCondition: "none",
    keyValue: 1,
    name: "",
    pagePath: "",
    samplePercent: 100,
    selector: "",
    selectorType: "tracking",
  };
}

export function ruleDraftFromApi(rule: TrackingRule): RuleDraft {
  return {
    captureText: rule.capture_text,
    customProperties: (rule.metadata.custom_props ?? [])
      .slice(0, 10)
      .map((property) => ({
        key: property.key,
        ...draftRuleSelector(property.selector, property.selector_type),
      })),
    description: rule.description ?? "",
    domEvent:
      rule.type === "view"
        ? "view"
        : (rule.trigger_config.dom_event ?? rule.type),
    emittedEvent: rule.metadata.event_name ?? rule.name,
    enabled: rule.enabled,
    keyCondition: rule.trigger_config.key_condition ?? "none",
    keyValue: rule.trigger_config.condition_value ?? 1,
    name: rule.name,
    pagePath: rule.trigger_config.page_path ?? "",
    samplePercent: Math.round(rule.sample_rate * 100),
    ...draftRuleSelector(rule.selector, rule.trigger_config.selector_type),
  };
}

export function rulePayload(draft: RuleDraft): RulePayload {
  const isKeyboard =
    draft.domEvent === "keydown" ||
    draft.domEvent === "keyup" ||
    draft.domEvent === "keypress";
  const keyCondition =
    draft.keyCondition === "none" ? undefined : draft.keyCondition;
  const condition = isKeyboard && keyCondition;
  const customProps = draft.customProperties
    .map((property) => ({
      key: property.key.trim(),
      ...storedRuleSelector(property.selector, property.selectorType),
    }))
    .filter((property) => property.key && property.selector)
    .slice(0, 10);

  return {
    capture_text: draft.captureText,
    description: draft.description.trim(),
    enabled: draft.enabled,
    metadata: {
      ...(customProps.length ? { custom_props: customProps } : {}),
      event_name: draft.emittedEvent.trim() || draft.name.trim(),
    },
    name: draft.name.trim(),
    sample_rate: Math.min(100, Math.max(0, Number(draft.samplePercent))) / 100,
    selector: storedRuleSelector(draft.selector, draft.selectorType).selector,
    trigger_config: {
      ...(condition
        ? {
            condition_value: Math.max(1, Number(draft.keyValue) || 1),
            key_condition: keyCondition,
          }
        : {}),
      ...(draft.domEvent === "view" ? {} : { dom_event: draft.domEvent }),
      ...(draft.pagePath.trim() ? { page_path: draft.pagePath.trim() } : {}),
      selector_type: storedRuleSelector(draft.selector, draft.selectorType)
        .selector_type,
    },
    type:
      draft.domEvent === "view"
        ? "view"
        : draft.domEvent === "submit"
          ? "submit"
          : "click",
  };
}
