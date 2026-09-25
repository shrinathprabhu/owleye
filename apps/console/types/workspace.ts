import type { ConsoleRole } from "~/types/console";

export const PREVIEW_USER_ID = "a9be8cb0-3af3-4f4e-a33b-b748bc2c9137";

export type WorkspacePermission =
  | "ai_use"
  | "analytics_read"
  | "rules_write"
  | "settings_manage"
  | "users_manage";

export type WorkspacePermissions = Record<WorkspacePermission, boolean>;

export type WorkspaceSite = {
  entitlements?: {
    ai: boolean;
    uptime: boolean;
  };
  created_at: string;
  domain: string;
  id: string;
  name: string;
  organization_id?: string | null;
  permissions?: Partial<WorkspacePermissions>;
  public_key?: string | null;
  read_only?: boolean;
  role?: "admin" | "owner" | "read_only" | "viewer";
  timezone: string;
  tracking_id: string;
  updated_at: string;
};

export type WorkspaceUser = {
  is_admin?: boolean;
  avatar_url?: string | null;
  email: string;
  id: string;
  name?: string | null;
  two_factor_enabled: boolean;
};

export type WorkspaceSession =
  | { authenticated: false }
  | {
      authenticated: true;
      access_expires_at: string;
      onboarding: {
        has_site: boolean;
        next_step: "complete" | "create_site" | "two_factor";
        two_factor_prompt_handled: boolean;
      };
      user: WorkspaceUser;
    };

export type WorkspaceHealth = {
  service: string;
  status: string;
  version: string;
};

export function consoleRole(site?: WorkspaceSite | null): ConsoleRole {
  if (!site || site.read_only) return "viewer";
  const role = String(site.role ?? "").toLowerCase();
  if (role === "owner") return "owner";
  if (role === "admin") return "admin";
  return "viewer";
}

export function sitePermissions(
  site?: WorkspaceSite | null,
): WorkspacePermissions {
  const role = consoleRole(site);
  const admin = role === "owner" || role === "admin";
  const owner = role === "owner";

  return {
    ai_use: site?.permissions?.ai_use ?? false,
    analytics_read: site?.permissions?.analytics_read ?? Boolean(site),
    rules_write: site?.permissions?.rules_write ?? admin,
    settings_manage: site?.permissions?.settings_manage ?? owner,
    users_manage: site?.permissions?.users_manage ?? admin,
  };
}
