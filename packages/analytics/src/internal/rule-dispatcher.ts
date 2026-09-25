import { reportFailure } from "./safety";
import type { OwlRecord } from "../types";
import { truncateUtf8 } from "./bounds";
import { createEvent, mergeRuleRecords } from "./events";
import type { RuleEnricher } from "./rule-enrichment";
import {
  automaticRuleRecords,
  domEventForRule,
  type AutomaticRuleRecord,
  type TrackingRule,
} from "./rule-matching";
import type { Emitter } from "./transport";

const RULE_FIELD_LIMIT = 10;

/** Builds and emits rule events while isolating host-provided enrichment. */
export class RuleEventDispatcher {
  constructor(
    private readonly emitter: Emitter,
    private readonly enrichRule?: RuleEnricher,
  ) {}

  emit(rule: TrackingRule, element: Element, event?: Event): void {
    if (!shouldSample(rule.sample_rate)) return;

    try {
      const records = this.ruleRecords(rule, element, event);
      this.emitter.emit(
        createEvent(
          "rule",
          rule.metadata?.event_name ?? rule.name,
          {
            element: describeElement(element, rule.capture_text === true),
            ...(records ? { records } : {}),
            rule_id: rule.id,
            rule_type: domEventForRule(rule) ?? rule.type,
          },
          { pageCapture: this.emitter.config },
        ),
      );
    } catch (error: unknown) {
      this.warn(rule, "could not be emitted", error);
    }
  }

  warn(rule: TrackingRule, message: string, error: unknown): void {
    reportFailure(`rule ${message}`);
  }

  private ruleRecords(
    rule: TrackingRule,
    element: Element,
    event?: Event,
  ): AutomaticRuleRecord | OwlRecord | undefined {
    const automatic = automaticRuleRecords(rule, element, document);
    const enriched = this.enrich(rule, element, event);
    if (!automatic) return enriched;
    if (!enriched) return automatic;

    const combined = [
      ...Object.entries(automatic),
      ...Object.entries(enriched).filter(
        ([key]) => !Object.prototype.hasOwnProperty.call(automatic, key),
      ),
    ];
    if (combined.length > RULE_FIELD_LIMIT) {
      this.warn(
        rule,
        `enrichment exceeded the ${RULE_FIELD_LIMIT}-field rule limit`,
        new TypeError(
          "Configured fields take precedence over hook enrichment.",
        ),
      );
    }
    return Object.fromEntries(
      combined.slice(0, RULE_FIELD_LIMIT),
    ) as AutomaticRuleRecord;
  }

  private enrich(
    rule: TrackingRule,
    element: Element,
    event?: Event,
  ): OwlRecord | undefined {
    if (!this.enrichRule) return undefined;

    try {
      const enrichment = this.enrichRule(rule, {
        element,
        event,
        type: domEventForRule(rule) ?? rule.type,
      });
      // JavaScript callers may accidentally provide an async hook. Observe its
      // rejection even though async enrichment is not part of this API.
      if (
        enrichment &&
        typeof (enrichment as { then?: unknown }).then === "function"
      ) {
        void Promise.resolve(enrichment).catch(() =>
          reportFailure("asynchronous rule enrichment"),
        );
        reportFailure("asynchronous rule enrichment");
        return undefined;
      }
      return enrichment === undefined
        ? undefined
        : mergeRuleRecords([enrichment]);
    } catch (error: unknown) {
      this.warn(rule, "enrichment failed", error);
      return undefined;
    }
  }
}

function shouldSample(sampleRate: number | undefined): boolean {
  if (sampleRate === undefined || sampleRate >= 1) return true;
  if (sampleRate <= 0) return false;
  return Math.random() < sampleRate;
}

function describeElement(
  element: Element,
  captureText: boolean,
): Record<string, unknown> {
  const htmlElement = element instanceof HTMLElement ? element : undefined;

  return {
    classes: htmlElement?.className
      ? truncateUtf8(String(htmlElement.className), 512)
      : undefined,
    id: element.id ? truncateUtf8(element.id, 512) : undefined,
    tag: truncateUtf8(element.tagName.toLowerCase(), 64),
    text: captureText
      ? truncateUtf8(htmlElement?.innerText?.trim() ?? "", 512) || undefined
      : undefined,
  };
}
