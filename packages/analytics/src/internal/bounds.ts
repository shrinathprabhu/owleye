export const EVENT_DATA_MAX_BYTES = 16 * 1024;
export const EVENT_NAME_MAX_BYTES = 128;
export const SITE_ID_MAX_BYTES = 128;

const CONTROL_CHARACTERS = /[\u0000-\u001f\u007f]/u;

export function normalizeEventName(value: string): string {
  if (typeof value !== "string") {
    throw new TypeError("OwlEye event name must be a string.");
  }

  const name = value.trim();
  if (!name) throw new TypeError("OwlEye event name is required.");
  if (CONTROL_CHARACTERS.test(name)) {
    throw new TypeError("OwlEye event name cannot contain control characters.");
  }
  if (utf8ByteLength(name) > EVENT_NAME_MAX_BYTES) {
    throw new TypeError(
      `OwlEye event name must be at most ${EVENT_NAME_MAX_BYTES} UTF-8 bytes.`,
    );
  }
  return name;
}

export function assertEventDataSize(data: unknown): void {
  if (data === undefined) return;

  let serialized: string;
  try {
    serialized = JSON.stringify(data);
  } catch {
    throw new TypeError("OwlEye event data must be JSON serializable.");
  }

  if (utf8ByteLength(serialized) > EVENT_DATA_MAX_BYTES) {
    throw new TypeError(
      `OwlEye event data must be at most ${EVENT_DATA_MAX_BYTES} UTF-8 bytes.`,
    );
  }
}

export function truncateUtf8(value: string, maxBytes: number): string {
  if (utf8ByteLength(value) <= maxBytes) return value;

  let bytes = 0;
  let result = "";
  for (const character of value) {
    const characterBytes = codePointByteLength(
      character.codePointAt(0) ?? 0xfffd,
    );
    if (bytes + characterBytes > maxBytes) break;
    result += character;
    bytes += characterBytes;
  }
  return result;
}

export function utf8ByteLength(value: string): number {
  let bytes = 0;
  for (const character of value) {
    bytes += codePointByteLength(character.codePointAt(0) ?? 0xfffd);
  }
  return bytes;
}

function codePointByteLength(codePoint: number): number {
  if (codePoint <= 0x7f) return 1;
  if (codePoint <= 0x7ff) return 2;
  if (codePoint <= 0xffff) return 3;
  return 4;
}
