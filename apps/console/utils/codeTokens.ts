export type CodeLanguage = "auto" | "javascript" | "html" | "shell" | "text";
export type CodeToken = { kind: string; text: string };

/** Small display-only lexer. Tokens are always text; nothing becomes HTML. */
export function codeTokens(
  code: string,
  language: CodeLanguage = "auto",
): CodeToken[] {
  if (!code || language === "text" || code.length > 60_000)
    return [{ kind: "plain", text: code }];
  const selected =
    language === "auto"
      ? /^\s*</.test(code)
        ? "html"
        : /^\s*(?:curl|(?:p?npm|yarn|bun)\s)/.test(code)
          ? "shell"
          : "javascript"
      : language;
  const pattern =
    selected === "html"
      ? /(?<comment><!--[\s\S]*?-->)|(?<string>"[^"\n]*"|'[^'\n]*')|(?<tag><\/?[\w:-]+|\/?>)|(?<attribute>[\w:-]+(?=\s*=))|(?<punctuation>=)/g
      : selected === "shell"
        ? /(?<comment>#[^\n]*)|(?<string>"(?:\\.|[^"\\])*"|'[^']*')|(?<keyword>\b(?:curl|pnpm|npm|yarn|bun|add|install)\b)|(?<attribute>--?[\w-]+)|(?<punctuation>\\\n|[{}[\]=])/g
        : /(?<comment>\/\/[^\n]*|\/\*[\s\S]*?\*\/)|(?<string>"(?:\\.|[^"\\])*"|'(?:\\.|[^'\\])*'|`(?:\\.|[^`\\])*`)|(?<keyword>\b(?:import|from|export|const|let|var|function|return|async|await|new|if|else|true|false|null|undefined|type|interface)\b)|(?<number>\b\d+(?:\.\d+)?\b)|(?<function>\b[A-Za-z_$][\w$]*(?=\s*\())|(?<punctuation>[{}[\]();,.=:])/g;
  const tokens: CodeToken[] = [];
  let offset = 0;
  for (const match of code.matchAll(pattern)) {
    const start = match.index!;
    if (start > offset)
      tokens.push({ kind: "plain", text: code.slice(offset, start) });
    const kind =
      Object.keys(match.groups ?? {}).find(
        (key) => match.groups![key] !== undefined,
      ) ?? "plain";
    tokens.push({ kind, text: match[0] });
    offset = start + match[0].length;
  }
  if (offset < code.length)
    tokens.push({ kind: "plain", text: code.slice(offset) });
  return tokens;
}
