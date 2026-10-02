export function browserTimezone(): string {
  try {
    const zone = Intl.DateTimeFormat().resolvedOptions().timeZone;
    return zone === "Asia/Calcutta" ? "Asia/Kolkata" : zone || "UTC";
  } catch {
    return "UTC";
  }
}

export function timezoneOptions(current: string, date = new Date()) {
  const zones = new Set(["UTC", "Asia/Kolkata", current]);
  // Older browsers can still choose UTC, their own zone, or the saved zone.
  const supported =
    typeof Intl.supportedValuesOf === "function"
      ? Intl.supportedValuesOf("timeZone")
      : [browserTimezone()];
  for (const zone of supported) {
    if (zone === "Asia/Calcutta" && current !== zone) continue;
    zones.add(zone);
  }
  return [...zones]
    .filter(Boolean)
    .sort()
    .map((value) => {
      let offset = "";
      try {
        offset =
          new Intl.DateTimeFormat("en", {
            timeZone: value,
            timeZoneName: "longOffset",
          })
            .formatToParts(date)
            .find((part) => part.type === "timeZoneName")
            ?.value.replace("GMT", "UTC") || "";
      } catch {
        // Preserve legacy saved values even if this browser cannot format them.
      }
      const city =
        value === "Asia/Calcutta"
          ? "Asia/Calcutta · Kolkata"
          : value.replaceAll("_", " ");
      return { value, label: offset ? `${city} (${offset})` : city };
    });
}
