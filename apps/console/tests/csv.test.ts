import assert from "node:assert/strict";
import test from "node:test";
import { csvText } from "../utils/csv.ts";

test("CSV preserves Unicode, commas, quotes, and multiline values", () => {
  assert.equal(
    csvText([["Mumbai, IN", 'a"b', "two\nlines", "日本", 42, null]]),
    '"Mumbai, IN","a""b","two\nlines","日本","42",""\r\n',
  );
});
test("CSV neutralizes formula labels while retaining numeric negatives", () => {
  assert.equal(
    csvText([["=1+1", " @SUM(A1)", "\t-2+3", "+cmd", -2]]),
    '"\'=1+1","\' @SUM(A1)","\'\t-2+3","\'+cmd","-2"\r\n',
  );
});
