import type { OwlConfig } from "../types";
import { SITE_ID_MAX_BYTES, utf8ByteLength } from "./bounds";

export const DEFAULT_SERVER = "https://api.owleye.dev";

export function assertSiteId(siteId: string): string {
  if (typeof siteId !== "string") {
    throw new TypeError("OwlEye site ID is required.");
  }

  const normalized = siteId.trim();
  if (!normalized) throw new TypeError("OwlEye site ID is required.");
  if (
    utf8ByteLength(normalized) > SITE_ID_MAX_BYTES ||
    !/^[A-Za-z0-9._-]+$/u.test(normalized)
  ) {
    throw new TypeError(
      "OwlEye site ID must use 1-128 ASCII letters, numbers, dots, underscores, or hyphens.",
    );
  }
  return normalized;
}

export interface NormalizedConfig {
  autoStart: boolean;
  captureCampaigns: boolean;
  captureHash: boolean;
  captureQuery: boolean;
  debug: boolean;
  mock: boolean;
  respectDoNotTrack: boolean;
  respectGlobalPrivacyControl: boolean;
  server: string;
}

export function normalizeConfig(config: OwlConfig = {}): NormalizedConfig {
  return {
    autoStart: config.autoStart !== false,
    captureCampaigns: Boolean(config.captureCampaigns),
    captureHash: Boolean(config.captureHash),
    captureQuery: Boolean(config.captureQuery),
    debug: Boolean(config.debug),
    mock: Boolean(config.mock),
    respectDoNotTrack: config.respectDoNotTrack !== false,
    respectGlobalPrivacyControl: config.respectGlobalPrivacyControl !== false,
    server: normalizeServer(config.server),
  };
}

export function normalizeServer(server: OwlConfig["server"]): string {
  if (!server) return DEFAULT_SERVER;

  const raw = (
    server instanceof URL ? server.toString() : String(server)
  ).trim();
  if (!raw) return DEFAULT_SERVER;

  if (raw.startsWith("/") && !raw.startsWith("//")) {
    if (raw.includes("?") || raw.includes("#")) {
      throw new TypeError(
        "OwlEye server paths cannot include a query string or fragment.",
      );
    }
    return stripTrailingSlashes(raw);
  }

  let parsed: URL;
  try {
    parsed = new URL(raw);
  } catch {
    throw new TypeError(
      "OwlEye server must be an absolute HTTP(S) URL or a same-origin path.",
    );
  }

  if (parsed.protocol !== "http:" && parsed.protocol !== "https:") {
    throw new TypeError("OwlEye server must use HTTP or HTTPS.");
  }
  if (parsed.username || parsed.password || parsed.search || parsed.hash) {
    throw new TypeError(
      "OwlEye server cannot include credentials, a query string, or a fragment.",
    );
  }
  return stripTrailingSlashes(parsed.toString());
}

function stripTrailingSlashes(value: string): string {
  return value.replace(/\/+$/u, "");
}

export function readScriptConfig(script: HTMLScriptElement | null): {
  config: OwlConfig;
  siteId?: string;
} {
  if (!script) return { config: {} };

  return {
    config: {
      autoStart: parseBoolean(
        readDataset(script, "owleyeAutoStart", "owlEyeAutoStart"),
      ),
      captureCampaigns: parseBoolean(
        readDataset(script, "owleyeCaptureCampaigns", "owlEyeCaptureCampaigns"),
      ),
      captureHash: parseBoolean(
        readDataset(script, "owleyeCaptureHash", "owlEyeCaptureHash"),
      ),
      captureQuery: parseBoolean(
        readDataset(script, "owleyeCaptureQuery", "owlEyeCaptureQuery"),
      ),
      debug: parseBoolean(readDataset(script, "owleyeDebug", "owlEyeDebug")),
      mock: parseBoolean(readDataset(script, "owleyeMock", "owlEyeMock")),
      respectDoNotTrack: parseBoolean(
        readDataset(
          script,
          "owleyeRespectDoNotTrack",
          "owlEyeRespectDoNotTrack",
        ),
      ),
      respectGlobalPrivacyControl: parseBoolean(
        readDataset(
          script,
          "owleyeRespectGlobalPrivacyControl",
          "owlEyeRespectGlobalPrivacyControl",
        ),
      ),
      server: readDataset(script, "owleyeServer", "owlEyeServer"),
    },
    siteId: readDataset(script, "owleyeId", "owlEyeId"),
  };
}

function readDataset(
  script: HTMLScriptElement,
  canonicalKey: string,
  legacyKey: string,
): string | undefined {
  return script.dataset[canonicalKey] ?? script.dataset[legacyKey];
}

function parseBoolean(value: string | undefined): boolean | undefined {
  if (value === undefined) return undefined;
  if (value === "" || value === "true" || value === "1") return true;
  if (value === "false" || value === "0") return false;
  return undefined;
}
