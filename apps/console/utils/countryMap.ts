export const COUNTRY_CODES_BY_NAME: Readonly<Record<string, string>> = {
  Australia: "AU",
  Brazil: "BR",
  Canada: "CA",
  France: "FR",
  Germany: "DE",
  India: "IN",
  Japan: "JP",
  Netherlands: "NL",
  "New Zealand": "NZ",
  Singapore: "SG",
  "United Kingdom": "GB",
  "United States": "US",
};

export type CountryMapDictionary = {
  codesByName: Record<string, string>;
  labelsByCode: Record<string, string>;
};

export function parseCountryMap(svg: string): CountryMapDictionary {
  const codesByName: Record<string, string> = { ...COUNTRY_CODES_BY_NAME };
  const labelsByCode: Record<string, string> = {};
  const pathNames = /name="([^"]+)"\s+data-name="([^"]+)"/g;

  for (const match of svg.matchAll(pathNames)) {
    const code = match[1];
    const encodedLabel = match[2];
    if (!code || !encodedLabel) continue;

    const label = decodeXmlEntities(encodedLabel);
    labelsByCode[code] = label;
    codesByName[label] = code;
  }

  return { codesByName, labelsByCode };
}

export function normalizeCountryCode(
  value: string,
  codesByName: Readonly<Record<string, string>>,
) {
  if (value === "Other") return "OTHER";
  if (value === "Unknown") return "UNKNOWN";
  if (/^[A-Z]{2}(?:-[A-Z]{2})?$/.test(value)) return value;
  return codesByName[value] || value.toLocaleUpperCase();
}

function decodeXmlEntities(value: string) {
  const named: Record<string, string> = {
    amp: "&",
    apos: "'",
    gt: ">",
    lt: "<",
    quot: '"',
  };

  return value.replace(
    /&(#x[\da-f]+|#\d+|amp|apos|gt|lt|quot);/gi,
    (entity, token: string) => {
      if (token[0] !== "#") return named[token.toLowerCase()] ?? entity;
      const hexadecimal = token[1]?.toLowerCase() === "x";
      const parsed = Number.parseInt(
        token.slice(hexadecimal ? 2 : 1),
        hexadecimal ? 16 : 10,
      );
      return Number.isInteger(parsed) && parsed >= 0 && parsed <= 0x10ffff
        ? String.fromCodePoint(parsed)
        : entity;
    },
  );
}
