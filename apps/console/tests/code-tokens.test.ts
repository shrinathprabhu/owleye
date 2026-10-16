import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";
import * as Vue from "vue";
import { compileTemplate, parse } from "vue/compiler-sfc";
import { renderToString } from "vue/server-renderer";
import { codeTokens } from "../utils/codeTokens.ts";
import {
  optionalTrackingSetupPrompt,
  optionalTrackingSnippet,
} from "../utils/optionalTrackingSnippet.ts";

test("syntax tokens preserve clipboard source, escapes and non-ASCII across languages", () => {
  for (const [code, language] of [
    [
      'import { trackRules } from "@owleye/analytics/rules";\n// 日本語\nconst rules = trackRules("owl_test");',
      "javascript",
    ],
    [
      '<script data-id="a&amp;b" src="https://example.test/sdk.js"></script>',
      "html",
    ],
    [
      "curl 'https://example.test' \\\n  -H 'Authorization: Bearer YOUR_KEY'",
      "shell",
    ],
    ['const incomplete = "unfinished\n<script>alert(1)</script>', "auto"],
  ] as const) {
    const tokens = codeTokens(code, language);
    assert.equal(tokens.map((token) => token.text).join(""), code);
    assert.ok(tokens.some((token) => token.kind !== "plain"));
  }
  const long = '"'.repeat(70_000);
  assert.deepEqual(codeTokens(long), [{ kind: "plain", text: long }]);
});

test("the actual highlighting template escapes hostile text rather than creating markup", async () => {
  const path = new URL("../components/SyntaxCode.vue", import.meta.url);
  const { descriptor } = parse(readFileSync(path, "utf8"));
  const compiled = compileTemplate({
    source: descriptor.template!.content,
    filename: path.pathname,
    id: "highlight-test",
  });
  assert.deepEqual(compiled.errors, []);
  const source = compiled.code
    .replace(
      /import \{([^}]+)\} from "vue"/,
      (_, imports: string) =>
        `const {${imports.replace(/\bas\b/g, ":")}} = Vue;`,
    )
    .replace("export function render", "return function render");
  const render = new Function("Vue", source)(Vue);
  const code = 'const label = "</script><img src=x onerror=alert(1)> & 雪";';
  const html = await renderToString(
    Vue.createSSRApp({ render, setup: () => ({ tokens: codeTokens(code) }) }),
  );
  assert.ok(html.includes("&lt;/script&gt;&lt;img"));
  assert.ok(html.includes("syntax-keyword"));
  assert.ok(!html.includes("<img"));
  assert.ok(!html.includes("<script"));
  const renderedText = html
    .replace(/<!--[\s\S]*?-->/g, "")
    .replace(/<\/?span\b[^>]*>/g, "")
    .replace(/&lt;/g, "<")
    .replace(/&gt;/g, ">")
    .replace(/&quot;/g, '"')
    .replace(/&#39;/g, "'")
    .replace(/&amp;/g, "&");
  assert.equal(renderedText, code);
});

test("optional assistant prompts use the current collector setup and preserve installation boundaries", () => {
  for (const kind of ["rules", "performance"] as const) {
    const snippets = optionalTrackingSnippet(
      kind,
      'owl_"test',
      "https://api.example.test/",
    );
    const prompt = optionalTrackingSetupPrompt(
      kind,
      'owl_"test',
      "https://api.example.test/",
    );
    assert.ok(prompt.includes(snippets.npm));
    assert.ok(prompt.includes(snippets.cdn));
    assert.ok(prompt.includes("@owleye/analytics@1.0.3"));
    assert.ok(prompt.includes("do not initialize a second collector"));
    assert.ok(prompt.includes("Global Privacy Control"));
    assert.ok(prompt.includes("could not be verified"));
    assert.ok(
      prompt.includes(
        kind === "rules" ? "capture_text disabled" : "Missing INP is not zero",
      ),
    );
  }
});
