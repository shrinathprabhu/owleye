import type { VNode } from "vue";
import type { ConsoleSelectOption } from "../types/console";

// Called during render so conditional options and v-for fragments stay reactive.
export function optionsFromNodes(nodes: VNode[]): ConsoleSelectOption[] {
  const result: ConsoleSelectOption[] = [];
  function text(node: unknown): string {
    if (typeof node === "string" || typeof node === "number")
      return String(node);
    if (Array.isArray(node)) return node.map(text).join("");
    if (node && typeof node === "object" && "children" in node)
      return text(node.children);
    return "";
  }
  for (const node of nodes) {
    if (node.type === "option") {
      const label = text(node.children).trim();
      result.push({
        label,
        value: node.props?.value ?? label,
        disabled: node.props?.disabled === "" || node.props?.disabled === true,
      });
    } else if (Array.isArray(node.children)) {
      result.push(...optionsFromNodes(node.children as VNode[]));
    }
  }
  return result;
}
