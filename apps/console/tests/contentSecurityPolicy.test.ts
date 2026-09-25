import assert from "node:assert/strict";
import { test } from "node:test";
import { consoleContentSecurityPolicy } from "../utils/contentSecurityPolicy.ts";

const nonce = "dGVzdC1ub25jZS1mb3ItdGVzdHM=";

test("console CSP trusts nonce scripts and only the configured API origin", () => {
  const policy = consoleContentSecurityPolicy("https://api.example/v1", nonce);
  assert.ok(
    policy.includes(`script-src 'self' 'nonce-${nonce}' 'strict-dynamic'`),
  );
  assert.ok(policy.includes("script-src-attr 'none'"));
  assert.ok(policy.includes("connect-src 'self' https://api.example;"));
  assert.ok(
    !policy
      .split(";")
      .find((directive) => directive.trim().startsWith("script-src "))
      ?.includes("unsafe-inline"),
  );
});

test("CSP configuration cannot inject directives or credentials", () => {
  assert.throws(() =>
    consoleContentSecurityPolicy("javascript:alert(1)", nonce),
  );
  assert.throws(() =>
    consoleContentSecurityPolicy("https://user:secret@api.example", nonce),
  );
  assert.throws(() =>
    consoleContentSecurityPolicy("https://api.example", "'; unsafe-inline"),
  );
  assert.ok(
    consoleContentSecurityPolicy("/api", nonce).includes("connect-src 'self';"),
  );
});
