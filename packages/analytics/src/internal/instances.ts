import type { OwlConfig } from "../types";
import { assertSiteId, normalizeConfig } from "./config";
import { navigationKey } from "./navigation";
import { reportWarning } from "./safety";

// Shared by ESM and CDN copies, scoped to this document (never server requests).
const REGISTRY = Symbol.for("owleye.instances.v1");
type Registry = Map<string, Map<string, unknown>>;

export function instances<T>(kind: string): Map<string, T> {
  if (typeof window === "undefined") return new Map();
  const host = window as unknown as { [REGISTRY]?: Registry };
  const registry = (host[REGISTRY] ??= new Map());
  let entries = registry.get(kind);
  if (!entries) registry.set(kind, (entries = new Map()));
  return entries as Map<string, T>;
}

export function instanceKey(siteId: string, config?: OwlConfig): string {
  return JSON.stringify([assertSiteId(siteId), normalizeConfig(config).server]);
}

// Retain only a bounded URL marker across stop/start, including framework effect
// cleanup/remount. Real navigation and explicit pageview() still count normally.
export function firstPageStart(key: string, config: OwlConfig): boolean {
  const pages = instances<string>("pages");
  const url = navigationKey(window.location.href, config);
  const changed = pages.get(key) !== url;
  pages.delete(key);
  pages.set(key, url);
  if (pages.size > 100) pages.delete(pages.keys().next().value!);
  return changed;
}

// Weak keys let stopped controllers be collected; share metadata across bundles.
function configurations(): WeakMap<
  object,
  ReturnType<typeof normalizeConfig> & { enrichRule?: unknown }
> {
  const registry =
    instances<
      WeakMap<
        object,
        ReturnType<typeof normalizeConfig> & { enrichRule?: unknown }
      >
    >("configurations");
  let configs = registry.get("controllers");
  if (!configs) registry.set("controllers", (configs = new WeakMap()));
  return configs;
}

export function rememberConfig<T extends object>(
  controller: T,
  config?: OwlConfig & { enrichRule?: unknown },
): T {
  configurations().set(controller, {
    ...normalizeConfig(config),
    enrichRule: config?.enrichRule,
  });
  return controller;
}

export function checkConfig(
  controller: object,
  config?: OwlConfig & { enrichRule?: unknown },
): void {
  if (!config) return;
  const previous = configurations().get(controller);
  if (!previous) return;
  const next = { ...normalizeConfig(config), enrichRule: config.enrichRule };
  const conflicts = (Object.keys(next) as Array<keyof typeof next>).filter(
    (key) => config[key] !== undefined && previous[key] !== next[key],
  );
  if (conflicts.length)
    reportWarning(
      `Existing controller kept its configuration; ignored conflicting options: ${conflicts.join(", ")}. Call stop(), then initialize again to change options. Repeated initialization never starts a paused controller.`,
    );
}
