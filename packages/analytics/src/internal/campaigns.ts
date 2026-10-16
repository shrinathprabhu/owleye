import { truncateUtf8, utf8ByteLength } from "./bounds";

const KEYS = [
  "utm_source",
  "utm_medium",
  "utm_campaign",
  "utm_term",
  "utm_content",
];

// Capture bounded campaign labels, omitting recognizable identifiers and
// credentials. Check ambiguous first occurrences before truncating values.
function campaignValue(raw: string): string | undefined {
  const value = raw.trim().normalize("NFC");
  if (
    !value ||
    /[\u0000-\u001f\u007f\u200b-\u200f\u202a-\u202e\u2066-\u2069\ufeff@<>%]/u.test(
      value,
    ) ||
    /[a-z][a-z\d+.-]*:\/\/|www\.|(?:\d{1,3}\.){3}\d{1,3}|(?:[a-f\d]{0,4}:){2,}[a-f\d]{0,4}/i.test(
      value,
    ) ||
    /[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}|\d{7}|[a-z\d]{32}|[a-z\d_-]{8,}\.[a-z\d_-]{8,}\.[a-z\d_-]{8,}/i.test(
      value,
    ) ||
    /(?:^|[\s_-])(?:user|customer|recipient|contact|subscriber|member|account)[_-]?id(?:[\s_:=.-]|\d|$)|(?:^|[\s_-])(?:user|customer|recipient|contact|subscriber|member|account)[_-]?\d{3,}|(?:^|[\s_-])(?:token|secret|password|session|auth|api[_-]?key)[\s_:=.-]|(?:^|[\s_-])(?:sk|pk)_(?:live|test)_[a-z\d]+|\bbearer\s+[a-z\d._-]+/i.test(
      value,
    )
  )
    return undefined;
  return truncateUtf8(value, 200);
}

export function campaignSearch(search: string): string {
  if (search.length > 16_384) return "";
  const first = new Map<string, string>();
  for (const [key, value] of new URLSearchParams(search)) {
    const normalized = key.toLowerCase();
    if (KEYS.includes(normalized) && !first.has(normalized))
      first.set(normalized, value);
  }
  const captured = new URLSearchParams();
  for (const key of KEYS) {
    const value = first.has(key) ? campaignValue(first.get(key)!) : undefined;
    if (value) {
      captured.set(key, value);
      // Keep only complete parameters within the server's page-search bound.
      if (utf8ByteLength(`?${captured}`) > 2_048) captured.delete(key);
    }
  }
  const value = captured.toString();
  return value ? `?${value}` : "";
}
