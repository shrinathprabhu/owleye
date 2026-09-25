import { guarded, safely } from "./safety";
import { onNavigation } from "./navigation";
import {
  elementsForViewRule,
  elementsForViewRuleInSubtree,
  ruleAppliesToPage,
  type TrackingRule,
} from "./rule-matching";

const MUTATION_DEBOUNCE_MS = 200;
const MAX_MUTATION_ROOTS_PER_TICK = 250;

type RuleMatchHandler = (rule: TrackingRule, element: Element) => void;
type RuleFailureHandler = (
  rule: TrackingRule,
  message: string,
  error: unknown,
) => void;

/** Owns discovery and one-shot visibility tracking for view rules. */
export class RuleViewController {
  private handledRules = new WeakMap<Element, Set<string>>();
  private intersectionObserver?: IntersectionObserver;
  private mutationObserver?: MutationObserver;
  private removeNavigationListener?: () => void;
  private rules: readonly TrackingRule[] = [];
  private stopped = false;
  private viewRules = new WeakMap<Element, TrackingRule[]>();
  private refreshTimeout?: ReturnType<typeof setTimeout>;
  private fullRefreshPending = false;
  private pendingAddedRoots = new Set<Element>();

  constructor(
    private readonly onMatch: RuleMatchHandler,
    private readonly onFailure: RuleFailureHandler,
  ) {}

  start(rules: readonly TrackingRule[]): void {
    if (this.stopped || rules.length === 0) return;
    this.rules = rules;
    this.refresh();
    this.observeMutations();
    this.removeNavigationListener = onNavigation(() => {
      this.handledRules = new WeakMap();
      this.scheduleRefresh();
    });
  }

  stop(): void {
    if (this.stopped) return;
    this.stopped = true;
    safely("view navigation cleanup", () => this.removeNavigationListener?.());
    this.removeNavigationListener = undefined;
    safely("view observer cleanup", () =>
      this.intersectionObserver?.disconnect(),
    );
    this.intersectionObserver = undefined;
    safely("mutation observer cleanup", () =>
      this.mutationObserver?.disconnect(),
    );
    this.mutationObserver = undefined;
    if (this.refreshTimeout !== undefined)
      safely("view timer cleanup", () => clearTimeout(this.refreshTimeout));
    this.refreshTimeout = undefined;
    this.handledRules = new WeakMap();
    this.rules = [];
    this.viewRules = new WeakMap();
    this.pendingAddedRoots.clear();
  }

  private refresh(): void {
    this.pendingAddedRoots.clear();
    safely("view observer cleanup", () =>
      this.intersectionObserver?.disconnect(),
    );
    this.intersectionObserver = undefined;
    this.viewRules = new WeakMap();
    if (typeof IntersectionObserver !== "function") return;

    this.intersectionObserver = new IntersectionObserver(
      guarded(
        "view observation",
        (
          entries: IntersectionObserverEntry[],
          observer: IntersectionObserver,
        ) => {
          for (const entry of entries) this.handleIntersection(entry, observer);
        },
      ),
    );

    for (const selectorRules of groupRulesBySelector(this.rules).values()) {
      this.observeSelectorRules(selectorRules);
    }
  }

  private observeSelectorRules(rules: readonly TrackingRule[]): void {
    const activeRules = rules.filter(isRuleActiveOnCurrentPage);
    const selectorRule = activeRules[0];
    if (!selectorRule || !this.intersectionObserver) return;

    try {
      for (const element of elementsForViewRule(selectorRule, document)) {
        this.observeElement(element, activeRules);
      }
    } catch (error: unknown) {
      for (const rule of activeRules) {
        this.onFailure(rule, "has an invalid selector", error);
      }
    }
  }

  private observeSelectorRulesInSubtree(
    rules: readonly TrackingRule[],
    root: Element,
  ): void {
    const activeRules = rules.filter(isRuleActiveOnCurrentPage);
    const selectorRule = activeRules[0];
    if (!selectorRule || !this.intersectionObserver) return;

    try {
      for (const element of elementsForViewRuleInSubtree(
        selectorRule,
        root,
        document,
      )) {
        this.observeElement(element, activeRules);
      }
    } catch (error: unknown) {
      for (const rule of activeRules) {
        this.onFailure(rule, "has an invalid selector", error);
      }
    }
  }

  private observeElement(
    element: Element,
    activeRules: readonly TrackingRule[],
  ): void {
    const unhandledRules = this.unhandledRules(element, activeRules);
    if (unhandledRules.length === 0) return;
    const matchedRules = this.viewRules.get(element) ?? [];
    const knownRuleIds = new Set(matchedRules.map((rule) => rule.id));
    matchedRules.push(
      ...unhandledRules.filter((rule) => !knownRuleIds.has(rule.id)),
    );
    this.viewRules.set(element, matchedRules);
    this.intersectionObserver?.observe(element);
  }

  private unhandledRules(
    element: Element,
    rules: readonly TrackingRule[],
  ): TrackingRule[] {
    const handled = this.handledRules.get(element);
    return handled ? rules.filter((rule) => !handled.has(rule.id)) : [...rules];
  }

  private handleIntersection(
    entry: IntersectionObserverEntry,
    observer: IntersectionObserver,
  ): void {
    if (this.stopped || !entry.isIntersecting) return;
    const handled = this.handledRules.get(entry.target) ?? new Set<string>();

    for (const rule of this.viewRules.get(entry.target) ?? []) {
      if (!isRuleActiveOnCurrentPage(rule) || handled.has(rule.id)) continue;
      handled.add(rule.id);
      this.onMatch(rule, entry.target);
    }

    this.handledRules.set(entry.target, handled);
    observer.unobserve(entry.target);
  }

  private observeMutations(): void {
    if (
      typeof MutationObserver !== "function" ||
      typeof IntersectionObserver !== "function" ||
      !document.documentElement
    ) {
      return;
    }

    this.mutationObserver = new MutationObserver(
      guarded("DOM observation", (records: MutationRecord[]) => {
        for (const record of records) {
          for (const node of record.removedNodes)
            this.unobserveRemovedNode(node);
          for (const node of record.addedNodes) this.queueAddedNode(node);
        }
        this.scheduleMutationWork();
      }),
    );
    this.mutationObserver.observe(document.documentElement, {
      childList: true,
      subtree: true,
    });
  }

  private queueAddedNode(node: Node): void {
    if (this.fullRefreshPending) return;
    const root = node instanceof Element ? node : node.parentElement;
    if (!root) return;
    if (this.pendingAddedRoots.size >= MAX_MUTATION_ROOTS_PER_TICK) {
      this.pendingAddedRoots.clear();
      this.scheduleRefresh(MUTATION_DEBOUNCE_MS);
    } else {
      this.pendingAddedRoots.add(root);
    }
  }

  private unobserveRemovedNode(node: Node): void {
    if (!(node instanceof Element)) return;
    this.intersectionObserver?.unobserve(node);
    this.viewRules.delete(node);
    for (const descendant of node.querySelectorAll("*")) {
      this.intersectionObserver?.unobserve(descendant);
      this.viewRules.delete(descendant);
    }
  }

  private scheduleMutationWork(): void {
    if (
      this.stopped ||
      this.fullRefreshPending ||
      this.pendingAddedRoots.size === 0
    )
      return;
    if (this.refreshTimeout !== undefined) return;

    const timeout = setTimeout(
      guarded("view refresh", () => {
        if (this.refreshTimeout === timeout) this.refreshTimeout = undefined;
        this.processAddedRoots();
      }),
      MUTATION_DEBOUNCE_MS,
    );
    this.refreshTimeout = timeout;
  }

  private processAddedRoots(): void {
    if (this.stopped) return;
    const roots = Array.from(this.pendingAddedRoots).slice(
      0,
      MAX_MUTATION_ROOTS_PER_TICK,
    );
    for (const root of roots) this.pendingAddedRoots.delete(root);

    const groupedRules = groupRulesBySelector(this.rules);
    for (const root of roots) {
      if (!root.isConnected) continue;
      for (const selectorRules of groupedRules.values()) {
        this.observeSelectorRulesInSubtree(selectorRules, root);
      }
    }

    if (this.pendingAddedRoots.size > 0) this.scheduleMutationWork();
  }

  private scheduleRefresh(delayMs = 0): void {
    if (this.stopped) return;
    this.fullRefreshPending = true;
    if (this.refreshTimeout !== undefined)
      safely("view timer cleanup", () => clearTimeout(this.refreshTimeout));

    const timeout = setTimeout(
      guarded("view refresh", () => {
        if (this.refreshTimeout === timeout) this.refreshTimeout = undefined;
        this.fullRefreshPending = false;
        if (!this.stopped) this.refresh();
      }),
      delayMs,
    );
    this.refreshTimeout = timeout;
  }
}

function isRuleActiveOnCurrentPage(rule: TrackingRule): boolean {
  return (
    typeof window !== "undefined" &&
    ruleAppliesToPage(rule, window.location.pathname)
  );
}

function groupRulesBySelector(
  rules: readonly TrackingRule[],
): Map<string, TrackingRule[]> {
  const grouped = new Map<string, TrackingRule[]>();

  for (const rule of rules) {
    const key = `${rule.trigger_config?.selector_type ?? "css"}\u0000${rule.selector}`;
    const selectorRules = grouped.get(key) ?? [];
    selectorRules.push(rule);
    grouped.set(key, selectorRules);
  }

  return grouped;
}
