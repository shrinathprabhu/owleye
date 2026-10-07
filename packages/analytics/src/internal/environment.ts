import type { OwlEnvironment, OwlPage } from "../types";
import { truncateUtf8 } from "./bounds";

const BROWSER_MAX_BYTES = 128;
const CAMPAIGN_VALUE_MAX_BYTES = 200;
const HOST_MAX_BYTES = 255;
const LANGUAGE_MAX_BYTES = 64;
const PAGE_PATH_MAX_BYTES = 2_048;
const PAGE_TITLE_MAX_BYTES = 512;
const PAGE_URL_MAX_BYTES = 4_096;
const TIMEZONE_MAX_BYTES = 128;

export interface PageCaptureOptions {
  captureCampaigns?: boolean;
  captureHash?: boolean;
  captureQuery?: boolean;
}

export function getPage(
  options: PageCaptureOptions = {},
  referrerOverride?: string,
  pageUrlOverride?: string,
): OwlPage {
  if (typeof window === "undefined") {
    return { path: "/" };
  }

  const { hash, host, href, pathname, search } = pageUrlOverride
    ? new URL(pageUrlOverride)
    : window.location;
  const candidate = referrerOverride || document.referrer || undefined;
  const referrer = externalReferrer(candidate, href);

  return {
    hash: options.captureHash
      ? boundedOptional(hash, PAGE_PATH_MAX_BYTES)
      : undefined,
    host: truncateUtf8(host, HOST_MAX_BYTES),
    path: truncateUtf8(pathname || "/", PAGE_PATH_MAX_BYTES),
    referrer: sanitizeUrl(referrer, options),
    referrer_host: parseHost(referrer),
    search: pageSearch(search, options),
    title:
      pageUrlOverride && pageUrlOverride !== window.location.href
        ? undefined
        : boundedOptional(document.title, PAGE_TITLE_MAX_BYTES),
    url: sanitizeUrl(href, options),
  };
}

export function getEnvironment(): OwlEnvironment {
  if (typeof window === "undefined" || typeof navigator === "undefined") {
    return {};
  }

  return {
    ...detectBrowser(),
    color_scheme: getColorScheme(),
    device_pixel_ratio: boundedRatio(window.devicePixelRatio),
    language: boundedOptional(navigator.language, LANGUAGE_MAX_BYTES),
    screen: window.screen
      ? {
          height: boundedDimension(window.screen.height),
          width: boundedDimension(window.screen.width),
        }
      : undefined,
    timezone: getTimezone(),
    viewport: {
      height: boundedDimension(window.innerHeight),
      width: boundedDimension(window.innerWidth),
    },
  };
}

// Internal navigation is not acquisition traffic. Compare complete origins so
// a different subdomain or port remains a legitimate external referrer.
function externalReferrer(
  value: string | undefined,
  pageUrl: string,
): string | undefined {
  if (!value) return undefined;
  try {
    const referrer = new URL(value, pageUrl);
    return referrer.origin === new URL(pageUrl).origin
      ? undefined
      : referrer.href;
  } catch {
    return undefined;
  }
}

function parseHost(value: string | undefined): string | undefined {
  if (!value) return undefined;

  try {
    return truncateUtf8(new URL(value).host, HOST_MAX_BYTES);
  } catch {
    return undefined;
  }
}

function sanitizeUrl(
  value: string | undefined,
  options: PageCaptureOptions,
): string | undefined {
  if (!value) return undefined;

  try {
    const url = new URL(value, window.location.href);
    if (!options.captureQuery) {
      url.search = options.captureCampaigns ? campaignSearch(url.search) : "";
    }
    if (!options.captureHash) url.hash = "";
    return truncateUtf8(url.toString(), PAGE_URL_MAX_BYTES);
  } catch {
    return undefined;
  }
}

function pageSearch(
  search: string,
  options: PageCaptureOptions,
): string | undefined {
  if (options.captureQuery) {
    return boundedOptional(search, PAGE_PATH_MAX_BYTES);
  }
  if (!options.captureCampaigns) return undefined;
  return campaignSearch(search) || undefined;
}

function campaignSearch(search: string): string {
  const captured = new URLSearchParams();
  const seen = new Set<string>();
  for (const [key, value] of new URLSearchParams(search)) {
    const normalizedKey = key.toLocaleLowerCase();
    if (
      normalizedKey === "utm_source" ||
      normalizedKey === "utm_medium" ||
      normalizedKey === "utm_campaign"
    ) {
      if (seen.has(normalizedKey)) continue;
      seen.add(normalizedKey);
      captured.append(
        normalizedKey,
        truncateUtf8(value, CAMPAIGN_VALUE_MAX_BYTES),
      );
      if (seen.size === 3) break;
    }
  }
  const value = captured.toString();
  return value ? `?${value}` : "";
}

let cachedTimezone: string | undefined;
let timezoneResolved = false;
function getTimezone(): string | undefined {
  if (timezoneResolved) return cachedTimezone;
  timezoneResolved = true;
  try {
    cachedTimezone = boundedOptional(
      Intl.DateTimeFormat().resolvedOptions().timeZone,
      TIMEZONE_MAX_BYTES,
    );
    return cachedTimezone;
  } catch {
    return undefined;
  }
}

function getColorScheme(): "dark" | "light" | undefined {
  if (!window.matchMedia) return undefined;
  return window.matchMedia("(prefers-color-scheme: dark)").matches
    ? "dark"
    : "light";
}

// Shared aliases for explicit UA tokens and low-entropy client-hint brands.
const browserNames: Record<string, string> = {
  arc: "Arc",
  arcmobile2: "Arc",
  "arc search": "Arc",
  comet: "Comet",
  "perplexity comet": "Comet",
  dia: "Dia",
  zen: "Zen",
  floorp: "Floorp",
  brave: "Brave",
  vivaldi: "Vivaldi",
  samsung: "Samsung Internet",
  "samsung internet": "Samsung Internet",
  headlesschrome: "Headless Chrome",
  headlessedge: "Headless Edge",
  uc: "UC Browser",
  ucbrowser: "UC Browser",
  ucweb: "UC Browser",
  edge: "Edge",
  "microsoft edge": "Edge",
  opera: "Opera",
};

function browserName(value: string): string | undefined {
  const key = value.toLowerCase().replace(/ browser$/, "");
  const name = browserNames[key];
  return typeof name === "string" ? name : undefined;
}

function detectBrowser(): Pick<OwlEnvironment, "browser" | "browser_version"> {
  const nav = navigator as Navigator & {
    brave?: { isBrave?: () => Promise<boolean> };
    userAgentData?: { brands?: Array<{ brand: string; version: string }> };
  };
  const userAgent = nav.userAgent;
  // Explicit fork/headless tokens must survive generic engine hints.
  const product = userAgent.match(
    /(?:^|[\s;(])(HeadlessChrome|HeadlessEdge|ArcMobile2|Arc|Comet|Dia|Zen|Floorp|Brave|Vivaldi|UCBrowser|UCWEB)\/([\d.]+)/i,
  );
  if (product) return boundedBrowser(browserName(product[1]!)!, product[2]);

  const brands = nav.userAgentData?.brands ?? [];
  for (const { brand, version } of brands) {
    const name = browserName(brand);
    if (name) return boundedBrowser(name, version);
  }
  if (typeof nav.brave?.isBrave === "function") {
    return boundedBrowser("Brave", undefined);
  }
  const versionless = userAgent.match(
    /(?:^|[\s;(])(ArcMobile2|Brave)(?:[;\s)]|$)/i,
  );
  if (versionless)
    return boundedBrowser(browserName(versionless[1]!)!, undefined);

  const matchers: Array<[string, RegExp]> = [
    ["Edge", /(?:EdgA|EdgiOS|Edg|Edge)\/([\d.]+)/],
    ["Opera", /(?:OPR|OPiOS|Opera)\/([\d.]+)/],
    ["Samsung Internet", /SamsungBrowser\/([\d.]+)/],
    ["Waterfox", /Waterfox\/([\d.]+)/],
    ["Firefox", /(?:Firefox|FxiOS)\/([\d.]+)/],
    ["Chrome", /(?:Chrome|CriOS|Chromium)\/([\d.]+)/],
    ["Safari", /Version\/([\d.]+).*Safari/],
  ];
  for (const [name, pattern] of matchers) {
    const match = userAgent.match(pattern);
    if (match?.[1]) return boundedBrowser(name, match[1]);
  }
  for (const { brand, version } of brands) {
    if (/^(Google Chrome|Chromium)$/i.test(brand))
      return boundedBrowser("Chrome", version);
    if (/^Firefox$/i.test(brand)) return boundedBrowser("Firefox", version);
  }
  return {};
}

function boundedBrowser(
  browser: string,
  version: string | undefined,
): Pick<OwlEnvironment, "browser" | "browser_version"> {
  return {
    browser: truncateUtf8(browser, BROWSER_MAX_BYTES),
    browser_version: boundedOptional(version, BROWSER_MAX_BYTES),
  };
}

function boundedDimension(value: number): number {
  if (!Number.isFinite(value)) return 0;
  return Math.min(100_000, Math.max(0, Math.round(value)));
}

function boundedOptional(
  value: string | undefined,
  maxBytes: number,
): string | undefined {
  if (!value) return undefined;
  return truncateUtf8(value, maxBytes) || undefined;
}

function boundedRatio(value: number): number | undefined {
  if (!Number.isFinite(value) || value <= 0) return undefined;
  return Math.min(16, Math.max(0.1, value));
}
