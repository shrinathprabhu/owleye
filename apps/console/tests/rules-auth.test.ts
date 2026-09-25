import assert from "node:assert/strict";
import test from "node:test";

import {
  defaultRuleDraft,
  draftRuleSelector,
  storedRuleSelector,
  ruleDraftFromApi,
  rulePayload,
  type TrackingRule,
} from "../types/rules.ts";
import { refreshAtForExpiry } from "../utils/authSchedule.ts";

test("access refresh uses bounded skew and never schedules in the past", () => {
  const now = 1_000_000;
  assert.equal(refreshAtForExpiry(now - 1, now), now);
  assert.equal(refreshAtForExpiry(now + 60_000, now), now + 30_000);
  assert.equal(refreshAtForExpiry(now + 20 * 60_000, now), now + 17 * 60_000);
  assert.equal(
    refreshAtForExpiry(now + 24 * 60 * 60_000, now),
    now + 24 * 60 * 60_000 - 10 * 60_000,
  );
});

test("rule draft conversion preserves canonical wire event precedence", () => {
  const rule: TrackingRule = {
    capture_text: true,
    created_at: "2026-08-29T00:00:00Z",
    description: "Submit checkout with Enter",
    enabled: true,
    id: "rule_1",
    lifetime_event_count: 12,
    metadata: {
      custom_props: [{ key: "plan", selector: ".plan" }],
      event_name: "checkout_submitted",
    },
    name: "Checkout",
    sample_rate: 0.5,
    selector: "#checkout",
    site_id: "site_1",
    trigger_config: {
      condition_value: 1,
      dom_event: "keyup",
      key_condition: "enter",
      page_path: "/checkout",
      selector_type: "css",
    },
    type: "click",
    updated_at: "2026-08-29T00:00:00Z",
  };

  const payload = rulePayload(ruleDraftFromApi(rule));
  assert.equal(payload.type, "click");
  assert.equal(payload.trigger_config.dom_event, "keyup");
  assert.equal(payload.trigger_config.key_condition, "enter");
  assert.equal(payload.metadata.event_name, "checkout_submitted");
  assert.deepEqual(payload.metadata.custom_props, [
    { key: "plan", selector: ".plan", selector_type: "css" },
  ]);
});

test("stable tracking attributes round-trip through existing CSS wire format", () => {
  const draft = {
    ...defaultRuleDraft(),
    selector: "pricing",
    domEvent: "view" as const,
  };
  const payload = rulePayload(draft);
  assert.equal(payload.type, "view");
  assert.equal(payload.trigger_config.dom_event, undefined);
  assert.equal(payload.selector, '[data-owleye-track="pricing"]');
  assert.equal(payload.trigger_config.selector_type, "css");
  assert.deepEqual(draftRuleSelector(payload.selector, "css"), {
    selector: "pricing",
    selectorType: "tracking",
  });
  assert.deepEqual(draftRuleSelector(".legacy > button", "css"), {
    selector: ".legacy > button",
    selectorType: "css",
  });
  assert.throws(() => storedRuleSelector('x"] , input', "tracking"));
  draft.customProperties = [
    { key: "plan", selector: "plan-select", selectorType: "tracking" },
  ];
  assert.deepEqual(rulePayload(draft).metadata.custom_props, [
    {
      key: "plan",
      selector: '[data-owleye-track="plan-select"]',
      selector_type: "css",
    },
  ]);
});

test("native interactions serialize as canonical DOM events with keyboard conditions only on keys", () => {
  for (const event of [
    "change",
    "focus",
    "blur",
    "play",
    "ended",
    "keydown",
  ] as const) {
    const payload = rulePayload({
      ...defaultRuleDraft(),
      selector: "target",
      domEvent: event,
      keyCondition: "enter",
    });
    assert.equal(payload.type, "click");
    assert.equal(payload.trigger_config.dom_event, event);
    assert.equal(
      payload.trigger_config.key_condition,
      event === "keydown" ? "enter" : undefined,
    );
    assert.equal(payload.capture_text, false);
  }
});

test("rule payload and editor round-trip all ten custom properties", () => {
  const draft = defaultRuleDraft();
  draft.name = "example";
  draft.selector = "button";
  draft.selectorType = "css";
  draft.customProperties = Array.from({ length: 10 }, (_, i) => ({
    key: `field${i}`,
    selector: `#field${i}`,
    selectorType: "css" as const,
  }));
  const payload = rulePayload(draft);
  assert.equal(payload.metadata.custom_props?.length, 10);
  const stored = {
    ...payload,
    id: "test",
    created_at: "2026-09-25",
    updated_at: "2026-09-25",
  } as TrackingRule;
  assert.equal(ruleDraftFromApi(stored).customProperties.length, 10);
});
