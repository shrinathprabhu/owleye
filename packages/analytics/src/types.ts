export interface OwlConfig {
  /**
   * Start automatic page analytics as soon as `useAnalytics()` is called.
   * Set this to `false` when tracking must wait for a consent or lawful-basis
   * decision, then call the returned controller's `start()` method.
   * This option only affects the automatic analytics entrypoint.
   *
   * @default true
   */
  autoStart?: boolean;
  /** Automatically count the initial page and browser URL navigation. Disable for manual/memory routers. @default true */
  autoTrackPageviews?: boolean;
  /** Count query-only navigation. Does not enable query capture. @default false */
  trackQueryChanges?: boolean;
  /** Count hash-only navigation. Does not enable hash capture. @default false */
  trackHashChanges?: boolean;
  /**
   * Print delivery decisions and numeric acknowledgements, without event payloads.
   * Failures always produce bounded warnings, even when debug is false.
   *
   * @default false
   */
  debug?: boolean;
  /**
   * Do not send network requests. Useful with debug mode in development.
   *
   * @default false
   */
  mock?: boolean;
  /**
   * Include the current URL query string in page and referrer fields.
   * Query strings are excluded by default because they commonly contain
   * identifiers and other sensitive values.
   *
   * @default false
   */
  captureQuery?: boolean;
  /**
   * Include only `utm_source`, `utm_medium`, and `utm_campaign` from the
   * current URL. This keeps arbitrary query parameters excluded while making
   * explicitly opted-in campaign attribution available to analytics.
   * `captureQuery` takes precedence when both options are enabled.
   *
   * @default false
   */
  captureCampaigns?: boolean;
  /**
   * Include the current URL hash in page and referrer fields.
   *
   * @default false
   */
  captureHash?: boolean;
  /**
   * Disable tracking when the browser enables Global Privacy Control.
   * This is a browser-side check; the ingestion API does not filter requests
   * based on DNT or Sec-GPC headers.
   *
   * @default true
   */
  respectGlobalPrivacyControl?: boolean;
  /**
   * OwlEye API base URL. Override for self-hosting, a configured proxy, or testing.
   *
   * @default "https://api.owleye.dev"
   */
  server?: string | URL;
}

export interface AnalyticsController {
  /** Send an explicit page view for routers that do not update browser history. */
  pageview(page?: Partial<OwlPage>): void;
  /** Replace fields merged into future custom events. Pass no value to clear them. */
  setGlobalRecords(records?: OwlRecord): void;
  /** Start automatic page views, page sessions, and SPA navigation tracking. Idempotent. */
  start(): void;
  /**
   * Stop tracking, discard the active page segment without sending it, and
   * remove this controller's browser listeners. Idempotent.
   */
  stop(): void;
  /**
   * Send one named product event.
   *
   * Pass one plain object in normal use. Up to ten primitive fields are
   * accepted across all record objects; queryable nested data is intentionally
   * not accepted.
   */
  track(eventName: string, ...records: OwlTrackRecords): void;
}

export interface RulesController {
  /** Abort rule loading, disconnect observers, and remove interaction listeners. Idempotent. */
  stop(): void;
}

export interface PerfController {
  /**
   * Start one named measurement and return its idempotent end function.
   * Up to ten primitive fields can be supplied across the start records.
   */
  start(perfId: string, ...records: OwlPerfRecords): PerfEnd;
  /** Start automatic LCP, INP, CLS, FCP, and TTFB collection. Idempotent. */
  observeVitals(): WebVitalsController;
}

export interface WebVitalsController {
  /** Discard pending measurements and remove every observer/listener. Already-started requests cannot be retracted. */
  stop(): void;
}

/** A custom field value accepted by OwlEye ingestion. */
export type OwlRecordValue = string | number | boolean;

/** A flat collection of custom fields; nested values are not sent. */
export type OwlRecord = Record<string, OwlRecordValue>;

/** Up to ten record objects; their combined fields are checked at runtime. */
export type OwlTrackRecords =
  | []
  | [OwlRecord]
  | [OwlRecord, OwlRecord]
  | [OwlRecord, OwlRecord, OwlRecord]
  | [OwlRecord, OwlRecord, OwlRecord, OwlRecord]
  | [OwlRecord, OwlRecord, OwlRecord, OwlRecord, OwlRecord]
  | [OwlRecord, OwlRecord, OwlRecord, OwlRecord, OwlRecord, OwlRecord]
  | [
      OwlRecord,
      OwlRecord,
      OwlRecord,
      OwlRecord,
      OwlRecord,
      OwlRecord,
      OwlRecord,
    ]
  | [
      OwlRecord,
      OwlRecord,
      OwlRecord,
      OwlRecord,
      OwlRecord,
      OwlRecord,
      OwlRecord,
      OwlRecord,
    ]
  | [
      OwlRecord,
      OwlRecord,
      OwlRecord,
      OwlRecord,
      OwlRecord,
      OwlRecord,
      OwlRecord,
      OwlRecord,
      OwlRecord,
    ]
  | [
      OwlRecord,
      OwlRecord,
      OwlRecord,
      OwlRecord,
      OwlRecord,
      OwlRecord,
      OwlRecord,
      OwlRecord,
      OwlRecord,
      OwlRecord,
    ];
export type OwlPerfRecords = OwlTrackRecords;

/** End one performance measurement. Extra calls are ignored. */
export type PerfEnd = (...records: OwlPerfRecords) => void;

export type OwlEventType =
  "pageview" | "page_session" | "external" | "rule" | "performance";

export interface OwlEventEnvelope {
  site_id: string;
  event: OwlEvent;
}

export interface OwlEvent {
  sdk?: { name: string; version: string };
  data?: Record<string, unknown>;
  environment: OwlEnvironment;
  name: string;
  page: OwlPage;
  timestamp: string;
  type: OwlEventType;
}

export interface OwlEnvironment {
  browser?: string;
  browser_version?: string;
  color_scheme?: "dark" | "light";
  device_pixel_ratio?: number;
  language?: string;
  screen?: {
    height: number;
    width: number;
  };
  timezone?: string;
  viewport?: {
    height: number;
    width: number;
  };
}

export interface OwlPage {
  hash?: string;
  host?: string;
  path: string;
  referrer?: string;
  referrer_host?: string;
  search?: string;
  title?: string;
  url?: string;
}
