export function eventLabel(type: string, name: string) {
  if (type === "pageview") return "Page View";
  if (type === "page_session") return "Visible page segment";
  return `${name || "Unnamed event"} (${type})`;
}
