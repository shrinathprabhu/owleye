export type DeveloperScope = "ai:prompt" | "events:read" | "stats:read";

export type DeveloperSettings = {
  allowed_scopes: DeveloperScope[];
  api_access_enabled: boolean;
  developer_mode: boolean;
  endpoints: {
    control_plane_backup: string;
    events: string;
    events_export: string;
    stats: string;
    prompt: string;
  };
  site_id: string;
};

export type DeveloperKey = {
  created_at: string;
  expires_at: string | null;
  id: string;
  key_prefix: string;
  last_used_at: string | null;
  name: string;
  revoked_at: string | null;
  scopes: DeveloperScope[];
};

export type CreatedDeveloperKey = {
  api_key: DeveloperKey;
  secret: string;
};
