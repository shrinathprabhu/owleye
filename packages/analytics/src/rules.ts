import {
  safely,
  guarded,
  cleanup,
  noop,
  reportFailure,
  debugLog,
} from "./internal/safety";
import type { OwlConfig, RulesController } from "./types.js";
import {
  instances,
  instanceKey,
  rememberConfig,
  checkConfig,
} from "./internal/instances";
import { createRuleCatalog, type RuleCatalog } from "./internal/rule-catalog";
import { RuleConditionController } from "./internal/rule-conditions";
import { RuleEventDispatcher } from "./internal/rule-dispatcher";
import type { RuleEnricher } from "./internal/rule-enrichment.js";
import { fetchTrackingRules } from "./internal/rule-loader";
import {
  closestRuleElement,
  ruleAppliesToPage,
  type RuleDomEvent,
  type TrackingRule,
} from "./internal/rule-matching.js";
import { createEmitter, type Emitter } from "./internal/transport";
import { RuleViewController } from "./internal/rule-views";

export type {
  RuleEnricher,
  RuleEnrichmentContext,
} from "./internal/rule-enrichment.js";
export type {
  RuleCustomProperty,
  RuleDomEvent,
  RuleEventType,
  RuleKeyCondition,
  RuleMetadata,
  RuleSelectorType,
  RuleTriggerConfig,
  TrackingRule,
} from "./internal/rule-matching.js";

export interface RulesResponse {
  rules?: TrackingRule[];
}

export interface OwlRulesConfig extends OwlConfig {
  /**
   * Synchronously add up to ten primitive fields to a matched rule event.
   * Throwing or returning invalid data never prevents the base rule event.
   */
  enrichRule?: RuleEnricher;
}

export type { OwlConfig, RulesController } from "./types.js";

/**
 * Load server-defined interaction rules and begin matching them in the browser.
 * Import this optional entrypoint only when the site uses rule tracking.
 */
export function trackRules(
  siteId: string,
  config?: OwlRulesConfig,
): RulesController {
  return safely(
    "rule initialization",
    () => {
      const key = instanceKey(siteId, config);
      const controllers = instances<RulesController>("rules");
      const existing = controllers.get(key);
      if (existing) {
        checkConfig(existing, config);
        return existing;
      }
      const tracker = new RuleTracker(siteId, config, () => {
        if (controllers.get(key) === tracker) controllers.delete(key);
      });
      controllers.set(key, rememberConfig(tracker, config));
      return tracker.start();
    },
    { stop: noop },
  );
}

/** Coordinates rule subsystem lifecycle; specialized modules own all rule work. */
class RuleTracker implements RulesController {
  private readonly abortController = new AbortController();
  private readonly conditions: RuleConditionController;
  private readonly dispatcher: RuleEventDispatcher;
  private readonly domListeners = new Map<RuleDomEvent, EventListener>();
  private readonly emitter: Emitter;
  private readonly siteId: string;
  private stopped = false;
  private readonly views: RuleViewController;

  constructor(
    siteId: string,
    config: OwlRulesConfig | undefined,
    private readonly release: () => void,
  ) {
    this.emitter = createEmitter(siteId, config);
    this.siteId = this.emitter.siteId;
    this.dispatcher = new RuleEventDispatcher(this.emitter, config?.enrichRule);
    this.conditions = new RuleConditionController((rule, element, event) => {
      this.dispatcher.emit(rule, element, event);
    });
    this.views = new RuleViewController(
      (rule, element) => this.dispatcher.emit(rule, element),
      (rule, message, error) => this.dispatcher.warn(rule, message, error),
    );
  }

  start(): RulesController {
    if (!this.canStart()) {
      this.stop();
      return this;
    }

    void this.loadRules().catch(() => {
      reportFailure("rule loading");
      this.stop();
    });
    return this;
  }

  stop = (): void => {
    if (this.stopped) return;
    this.stopped = true;
    cleanup(
      this.release,
      () => this.abortController.abort(),
      () => this.conditions.stop(),
      () => this.views.stop(),
      () => this.removeDomListeners(),
    );
  };

  private canStart(): boolean {
    return (
      this.emitter.enabled &&
      typeof window !== "undefined" &&
      typeof document !== "undefined"
    );
  }

  private async loadRules(): Promise<void> {
    const config = this.emitter.config;
    if (config.mock) {
      if (config.debug) {
        debugLog("mock rules mode: no remote rules loaded");
      }
      return;
    }

    try {
      const loaded = await fetchTrackingRules(
        this.siteId,
        config,
        this.abortController.signal,
      );
      if (this.stopped) return;
      this.reportValidationIssues(loaded.issues);
      this.registerCatalog(createRuleCatalog(loaded.rules));
    } catch (error: unknown) {
      if (!this.stopped) reportFailure("rule loading");
      this.stop();
    }
  }

  private reportValidationIssues(
    issues: ReadonlyArray<{ index: number; message: string }>,
  ): void {
    if (issues.length) reportFailure("rule validation");
  }

  private registerCatalog(catalog: RuleCatalog): void {
    if (this.stopped) return;
    this.registerDomListeners(catalog.byDomEvent);
    this.views.start(catalog.viewRules);
  }

  private registerDomListeners(
    rulesByEvent: ReadonlyMap<RuleDomEvent, readonly TrackingRule[]>,
  ): void {
    for (const [eventName, rules] of rulesByEvent) {
      const listener: EventListener = guarded(
        "rule interaction",
        (event: Event) => {
          const target = eventTargetElement(event);
          if (target) this.matchRules(rules, target, event);
        },
      );
      this.domListeners.set(eventName, listener);
      document.addEventListener(eventName, listener, true);
    }
  }

  private matchRules(
    rules: readonly TrackingRule[],
    target: Element,
    event: Event,
  ): void {
    const matches = new Map<string, Element | null>();
    for (const rule of rules) {
      try {
        if (!ruleAppliesToPage(rule, window.location.pathname)) continue;
        const key = `${rule.trigger_config?.selector_type ?? "css"}\u0000${rule.selector}`;
        if (!matches.has(key))
          matches.set(key, this.closestRuleElement(rule, target));
        const element = matches.get(key);
        if (element) this.conditions.handle(rule, element, event);
      } catch (error) {
        this.dispatcher.warn(rule, "could not be matched", error);
      }
    }
  }

  private closestRuleElement(
    rule: TrackingRule,
    target: Element,
  ): Element | null {
    try {
      return closestRuleElement(rule, target, document);
    } catch (error: unknown) {
      this.dispatcher.warn(rule, "has an invalid selector", error);
      return null;
    }
  }

  private removeDomListeners(): void {
    if (typeof document !== "undefined") {
      for (const [eventName, listener] of this.domListeners) {
        safely("rule listener cleanup", () =>
          document.removeEventListener(eventName, listener, true),
        );
      }
    }
    this.domListeners.clear();
  }
}

function eventTargetElement(event: Event): Element | null {
  return typeof Element === "function" && event.target instanceof Element
    ? event.target
    : null;
}
