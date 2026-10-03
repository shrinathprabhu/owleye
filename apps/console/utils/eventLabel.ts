export function eventLabel(type: string, name: string) {
  if (type === "pageview") return "Page View";
  if (type === "page_session") return "Session";
  return `${name || "Unnamed event"} (${type})`;
}
