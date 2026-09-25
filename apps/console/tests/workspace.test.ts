import assert from "node:assert/strict";
import test from "node:test";

import {
  apiErrorCode,
  apiErrorMessage,
  apiErrorStatus,
  isAbortError,
} from "../utils/apiError.ts";
import {
  consoleRole,
  sitePermissions,
  type WorkspaceSite,
} from "../types/workspace.ts";

function site(overrides: Partial<WorkspaceSite> = {}): WorkspaceSite {
  return {
    created_at: "2026-08-01T00:00:00Z",
    domain: "example.test",
    id: "site_1",
    name: "Example",
    role: "read_only",
    timezone: "UTC",
    tracking_id: "owl_example",
    updated_at: "2026-08-01T00:00:00Z",
    ...overrides,
  };
}

test("workspace roles default to least privilege", () => {
  assert.equal(consoleRole(null), "viewer");
  assert.equal(consoleRole(site()), "viewer");
  assert.equal(
    consoleRole(site({ role: "Owner" as WorkspaceSite["role"] })),
    "owner",
  );
  assert.equal(consoleRole(site({ read_only: true, role: "owner" })), "viewer");
  assert.deepEqual(sitePermissions(site()), {
    ai_use: false,
    analytics_read: true,
    rules_write: false,
    settings_manage: false,
    users_manage: false,
  });
});

test("owner defaults are useful while explicit permissions remain authoritative", () => {
  assert.deepEqual(
    sitePermissions(
      site({
        permissions: { rules_write: false, users_manage: false },
        role: "owner",
      }),
    ),
    {
      ai_use: false,
      analytics_read: true,
      rules_write: false,
      settings_manage: true,
      users_manage: false,
    },
  );
});

test("API error helpers normalize fetch failures without leaking unknown values", () => {
  assert.equal(
    apiErrorMessage({ data: { error: "Readable server error" } }, "Fallback"),
    "Readable server error",
  );
  assert.equal(apiErrorMessage("not an object", "Fallback"), "Fallback");
  assert.equal(apiErrorStatus({ response: { status: 429 } }), 429);
  assert.equal(
    apiErrorCode({ data: { error: { code: "step_up_required" } } }),
    "step_up_required",
  );
  assert.equal(apiErrorCode({ data: { error: "legacy" } }), undefined);
  assert.equal(isAbortError({ name: "AbortError" }), true);
  assert.equal(isAbortError(new Error("network")), false);
});
