import type {
  OwlEvent,
  OwlEventType,
  OwlRecord,
  OwlRecordValue,
} from "../types";
import { assertEventDataSize, normalizeEventName } from "./bounds";
import {
  getEnvironment,
  getPage,
  type PageCaptureOptions,
} from "./environment";

const TRACK_RECORD_LIMIT = 10;
declare const __OWLEYE_SDK_VERSION__: string;
const PERF_RECORD_LIMIT = 10;
const RULE_RECORD_LIMIT = 10;

interface CreateEventOptions {
  page?: OwlEvent["page"];
  pageCapture?: PageCaptureOptions;
}

export function createEvent(
  type: OwlEventType,
  name: string,
  data?: Record<string, unknown>,
  options: CreateEventOptions = {},
): OwlEvent {
  const normalizedName = normalizeEventName(name);
  assertEventDataSize(data);

  return {
    sdk: { name: "owleye-js", version: __OWLEYE_SDK_VERSION__ },
    data,
    environment: getEnvironment(),
    name: normalizedName,
    page: options.page ?? getPage(options.pageCapture),
    timestamp: new Date().toISOString(),
    type,
  };
}

export function mergeTrackRecords(
  records: Array<Record<string, unknown>>,
): OwlRecord | undefined {
  return mergeRecords(records, TRACK_RECORD_LIMIT, "event");
}

export function mergePerfRecords(
  records: Array<Record<string, unknown>>,
): OwlRecord | undefined {
  return mergeRecords(records, PERF_RECORD_LIMIT, "performance");
}

export function mergeRuleRecords(
  records: Array<Record<string, unknown>>,
): OwlRecord | undefined {
  return mergeRecords(records, RULE_RECORD_LIMIT, "rule");
}

function mergeRecords(
  records: Array<Record<string, unknown>>,
  limit: number,
  context: "event" | "performance" | "rule",
): OwlRecord | undefined {
  if (records.length === 0) return undefined;
  if (records.length > limit) {
    throw new TypeError(
      `OwlEye ${context} data accepts at most ${limit} record objects.`,
    );
  }

  const merged: OwlRecord = {};
  let fieldCount = 0;

  for (const [index, record] of records.entries()) {
    if (!isPlainRecord(record)) {
      throw new TypeError(
        `OwlEye ${context} record ${index + 1} must be a plain JSON object.`,
      );
    }

    for (const [key, value] of Object.entries(record)) {
      fieldCount += 1;

      if (fieldCount > limit) {
        throw new TypeError(
          `OwlEye ${context} data accepts at most ${limit} fields across all records.`,
        );
      }

      merged[key] = assertRecordValue(key, value);
    }
  }

  return fieldCount === 0 ? undefined : merged;
}

function isPlainRecord(value: unknown): value is Record<string, unknown> {
  if (typeof value !== "object" || value === null || Array.isArray(value))
    return false;
  const prototype = Object.getPrototypeOf(value);
  return prototype === Object.prototype || prototype === null;
}

function assertRecordValue(key: string, value: unknown): OwlRecordValue {
  if (typeof value === "string" || typeof value === "boolean") return value;
  if (typeof value === "number" && Number.isFinite(value)) return value;

  throw new TypeError(
    `OwlEye data value "${key}" must be a string, finite number, or boolean.`,
  );
}
