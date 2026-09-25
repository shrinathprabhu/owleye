import type { OwlConfig } from "../types";
import { assertSiteId, normalizeConfig } from "./config";

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
export function firstPageStart(key: string): boolean {
  const pages = instances<string>("pages");
  const url = window.location.href;
  const changed = pages.get(key) !== url;
  pages.delete(key);
  pages.set(key, url);
  if (pages.size > 100) pages.delete(pages.keys().next().value!);
  return changed;
}
