import assert from "node:assert/strict";
import test from "node:test";
import { h, Fragment, createCommentVNode } from "vue";
import { optionsFromNodes } from "../utils/controlOptions.ts";
import { optionalTrackingSnippet } from "../utils/optionalTrackingSnippet.ts";

test("dropdown options preserve typed values and conditional fragments", () => {
  const options = optionsFromNodes([
    h("option", { value: "", disabled: "" }, "Choose a source"),
    h(Fragment, {}, [
      h("option", { value: 7 }, "Seven days"),
      h("option", { value: "rule", disabled: false }, "Rule"),
    ]),
    createCommentVNode("hidden conditional option"),
  ]);
  assert.deepEqual(options, [
    { value: "", label: "Choose a source", disabled: true },
    { value: 7, label: "Seven days", disabled: false },
    { value: "rule", label: "Rule", disabled: false },
  ]);
});

test("optional collector snippets retain public site and custom endpoint without leaking HTML", () => {
  for (const kind of ["rules", "performance"] as const) {
    const result = optionalTrackingSnippet(
      kind,
      'owl_"<example>',
      'https://example.test/collect/?q="&x=<test>',
    );
    assert.ok(result.npm.includes(JSON.stringify('owl_"<example>')));
    assert.ok(result.npm.includes("server: "));
    assert.ok(
      result.cdn.includes('data-owleye-id="owl_&quot;&lt;example&gt;"'),
    );
    assert.ok(result.cdn.includes("owleye." + kind + ".iife.js"));
    assert.ok(result.cdn.includes("&amp;x=&lt;test&gt;"));
    assert.ok(!result.cdn.includes('"<example>'));
  }
  const rules = optionalTrackingSnippet("rules", "owl_test", " /analytics/ ");
  assert.ok(
    rules.npm.includes('trackRules("owl_test", { server: "/analytics" })'),
  );
  assert.ok(rules.npm.includes("rules.stop()"));
});
