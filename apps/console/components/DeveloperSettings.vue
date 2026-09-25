<script setup lang="ts">
import type { ConsoleSelectOption } from "~/types/console";
import type {
  CreatedDeveloperKey,
  DeveloperKey,
  DeveloperScope,
  DeveloperSettings,
} from "~/types/developer";
import { apiErrorMessage as requestError } from "~/utils/apiError";
import { downloadBrowserFile } from "~/utils/download";
import { routes } from "~/utils/routes";

const props = defineProps<{
  apiBase: string;
  demo?: boolean;
  siteId?: string;
}>();
const { api } = useApi();

const settings = ref<DeveloperSettings | null>(null);
const keys = ref<DeveloperKey[]>([]);
const developerMode = ref(false);
const apiAccessEnabled = ref(false);
const keyName = ref("Analytics integration");
const lifetime = ref<string>("24h");
const selectedScopes = ref<DeveloperScope[]>(["stats:read"]);
const createdSecret = ref("");
const createdKeyName = ref("");
const exportDays = ref<string>("30");
const loading = ref(false);
const mutation = ref<"backup" | "create" | "export" | "revoke" | "save" | "">(
  "",
);
const error = ref("");
const notice = ref("");
let loadController: AbortController | undefined;
let downloadController: AbortController | undefined;
let loadGeneration = 0;

const lifetimeOptions: ConsoleSelectOption[] = [
  {
    description: "Good for one focused task.",
    label: "1 hour",
    value: "1h",
  },
  {
    description: "A sensible temporary default.",
    label: "24 hours",
    value: "24h",
  },
  {
    description: "For a short integration sprint.",
    label: "7 days",
    value: "7d",
  },
  {
    badge: "Rotate manually",
    description: "No expiry. Revocation is still instant.",
    label: "Permanent",
    value: "permanent",
  },
];
const exportRangeOptions: ConsoleSelectOption[] = [
  { description: "A quick recent slice.", label: "Last 7 days", value: "7" },
  { description: "The useful default.", label: "Last 30 days", value: "30" },
  { description: "Maximum export window.", label: "Last 90 days", value: "90" },
];

const activeKeys = computed(() => keys.value.filter((key) => !key.revoked_at));
const statsCurl = computed(() => {
  if (!settings.value) return "";
  return `curl --get '${props.apiBase}${settings.value.endpoints.stats}' \\\n  --header 'Authorization: Bearer YOUR_KEY' \\\n  --data-urlencode 'days=7'`;
});
const eventsCurl = computed(() => {
  if (!settings.value) return "";
  return `curl --get '${props.apiBase}${settings.value.endpoints.events}' \\\n  --header 'Authorization: Bearer YOUR_KEY' \\\n  --data-urlencode 'days=7' \\\n  --data-urlencode 'limit=500'`;
});
const promptCurl = computed(() => {
  if (!settings.value) return "";
  return `curl '${props.apiBase}${settings.value.endpoints.prompt}' \\\n  --header 'Authorization: Bearer YOUR_KEY' \\\n  --header 'Content-Type: application/json' \\\n  --data '{"prompt":"Show daily traffic over the last seven days"}'`;
});
watch(
  () => props.siteId,
  () => {
    downloadController?.abort();
    downloadController = undefined;
    void load();
  },
  { immediate: true },
);

onBeforeUnmount(() => {
  loadController?.abort();
  downloadController?.abort();
});

async function load() {
  loadController?.abort();
  const siteId = props.siteId;
  const generation = ++loadGeneration;
  settings.value = null;
  keys.value = [];
  createdSecret.value = "";
  mutation.value = "";
  error.value = "";
  notice.value = "";
  if (!siteId) return;

  const controller = new AbortController();
  loadController = controller;
  loading.value = true;
  try {
    const [settingsResponse, keyResponse] = await Promise.all([
      api<DeveloperSettings>(routes.sites.developer(siteId), {
        signal: controller.signal,
      }),
      api<DeveloperKey[]>(routes.sites.developerKeys(siteId), {
        signal: controller.signal,
      }),
    ]);
    if (controller.signal.aborted || generation !== loadGeneration) return;
    settings.value = settingsResponse;
    selectedScopes.value = selectedScopes.value.filter((scope) =>
      settingsResponse.allowed_scopes.includes(scope),
    );
    developerMode.value = settingsResponse.developer_mode;
    apiAccessEnabled.value = settingsResponse.api_access_enabled;
    keys.value = keyResponse;
  } catch (cause) {
    if (controller.signal.aborted || generation !== loadGeneration) return;
    error.value = requestError(
      cause,
      "Analytics API settings could not be loaded.",
    );
  } finally {
    if (generation === loadGeneration) loading.value = false;
  }
}

function toggleDeveloperMode() {
  if (!developerMode.value) apiAccessEnabled.value = false;
}

async function saveSettings() {
  const siteId = props.siteId;
  if (!siteId || props.demo || mutation.value) return;
  mutation.value = "save";
  error.value = "";
  notice.value = "";
  try {
    const response = await api<DeveloperSettings>(
      routes.sites.developer(siteId),
      {
        body: {
          api_access_enabled: apiAccessEnabled.value,
          developer_mode: developerMode.value,
        },
        method: "PUT",
      },
    );
    if (siteId !== props.siteId) return;
    settings.value = response;
    developerMode.value = response.developer_mode;
    apiAccessEnabled.value = response.api_access_enabled;
    createdSecret.value = "";
    notice.value = response.developer_mode
      ? "API settings saved."
      : "Developer tools are off. Analytics API keys are disabled; ingestion keys are unaffected.";
  } catch (cause) {
    if (siteId !== props.siteId) return;
    error.value = requestError(
      cause,
      "Analytics API settings could not be saved.",
    );
  } finally {
    if (siteId === props.siteId) mutation.value = "";
  }
}

function toggleScope(scope: DeveloperScope) {
  selectedScopes.value = selectedScopes.value.includes(scope)
    ? selectedScopes.value.filter((value) => value !== scope)
    : [...selectedScopes.value, scope];
}

async function createKey() {
  const siteId = props.siteId;
  if (
    !siteId ||
    props.demo ||
    mutation.value ||
    !settings.value?.api_access_enabled ||
    !selectedScopes.value.length
  ) {
    return;
  }
  mutation.value = "create";
  error.value = "";
  notice.value = "";
  createdSecret.value = "";
  try {
    const response = await api<CreatedDeveloperKey>(
      routes.sites.developerKeys(siteId),
      {
        body: {
          lifetime: lifetime.value,
          name: keyName.value,
          scopes: selectedScopes.value,
        },
        method: "POST",
      },
    );
    if (siteId !== props.siteId) return;
    createdSecret.value = response.secret;
    createdKeyName.value = response.api_key.name;
    keys.value = [response.api_key, ...keys.value];
    keyName.value = "Analytics integration";
    notice.value = "Key created. Copy it now; OwlEye only stores the hash.";
  } catch (cause) {
    if (siteId !== props.siteId) return;
    error.value = requestError(
      cause,
      "The analytics API key could not be created.",
    );
  } finally {
    if (siteId === props.siteId) mutation.value = "";
  }
}

async function revokeKey(key: DeveloperKey) {
  const siteId = props.siteId;
  if (!siteId || props.demo || key.revoked_at || mutation.value) return;
  mutation.value = "revoke";
  error.value = "";
  try {
    await api(routes.sites.developerKey(siteId, key.id), {
      method: "DELETE",
    });
    if (siteId !== props.siteId) return;
    keys.value = keys.value.map((item) =>
      item.id === key.id
        ? { ...item, revoked_at: new Date().toISOString() }
        : item,
    );
    notice.value = `${key.name} revoked.`;
  } catch (cause) {
    if (siteId !== props.siteId) return;
    error.value = requestError(
      cause,
      "The analytics API key could not be revoked.",
    );
  } finally {
    if (siteId === props.siteId) mutation.value = "";
  }
}

async function downloadBackup() {
  const siteId = props.siteId;
  if (
    !siteId ||
    props.demo ||
    !settings.value?.developer_mode ||
    mutation.value
  )
    return;
  mutation.value = "backup";
  error.value = "";
  try {
    await downloadFile(
      `${props.apiBase}${settings.value.endpoints.control_plane_backup}`,
      `owleye-${settings.value.site_id}-control-plane.json`,
      siteId,
    );
    if (siteId !== props.siteId) return;
    notice.value = "Control-plane backup downloaded.";
  } catch (cause) {
    if (siteId !== props.siteId) return;
    error.value = requestError(
      cause,
      "The control-plane backup could not be downloaded.",
    );
  } finally {
    if (siteId === props.siteId) mutation.value = "";
  }
}

async function downloadEventExport() {
  const siteId = props.siteId;
  if (
    !siteId ||
    props.demo ||
    !settings.value?.developer_mode ||
    mutation.value
  )
    return;
  mutation.value = "export";
  error.value = "";
  try {
    const days = Number.parseInt(exportDays.value, 10);
    const query = new URLSearchParams({ days: String(days), limit: "10000" });
    await downloadFile(
      `${props.apiBase}${settings.value.endpoints.events_export}?${query}`,
      `owleye-${settings.value.site_id}-events-${days}d.ndjson`,
      siteId,
    );
    if (siteId !== props.siteId) return;
    notice.value = "Bounded analytics export downloaded.";
  } catch (cause) {
    if (siteId !== props.siteId) return;
    error.value = requestError(
      cause,
      "The analytics export could not be downloaded.",
    );
  } finally {
    if (siteId === props.siteId) mutation.value = "";
  }
}

async function downloadFile(url: string, fileName: string, siteId: string) {
  downloadController?.abort();
  const controller = new AbortController();
  downloadController = controller;
  try {
    await downloadBrowserFile(url, fileName, {
      shouldDownload: () => props.siteId === siteId,
      signal: controller.signal,
    });
    if (controller.signal.aborted || props.siteId !== siteId) return;
  } finally {
    if (downloadController === controller) downloadController = undefined;
  }
}

async function copy(value: string) {
  try {
    await navigator.clipboard.writeText(value);
    notice.value = "Copied.";
  } catch {
    notice.value =
      "Select and copy it manually—your browser blocked clipboard access.";
  }
}

function formatDate(value: string | null) {
  if (!value) return "Never";
  const date = new Date(value);
  if (Number.isNaN(date.valueOf())) return value;
  return new Intl.DateTimeFormat(undefined, {
    dateStyle: "medium",
    timeStyle: "short",
  }).format(date);
}
</script>

<template>
  <div v-if="loading" class="management-loading" role="status">
    <span class="state-orbit" aria-hidden="true"></span>
    <span>Loading analytics API controls…</span>
  </div>

  <section v-else-if="settings" class="developer-settings">
    <article class="management-card developer-mode-card">
      <header class="management-card-header">
        <div>
          <p class="panel-kicker">Owner controls · analytics and exports</p>
          <h2>Analytics API and exports</h2>
        </div>
        <span
          class="status-badge"
          :class="settings.api_access_enabled ? 'success' : 'neutral'"
        >
          {{
            settings.api_access_enabled
              ? "Analytics API enabled"
              : "Analytics API disabled"
          }}
        </span>
      </header>

      <p class="developer-lede">
        Analytics API keys give integrations access to this app’s stats and
        privacy-safe events through selected scopes. AI access must be enabled for the app. They cannot send events, edit data, delete data, or
        manage the app. Event ingestion keys are managed separately under API
        Keys.
      </p>

      <div class="settings-switches developer-switches">
        <label class="setting-switch">
          <span>
            <strong>Developer tools</strong>
            <small
              >Enable configuration backups, event exports, and analytics API
              controls.</small
            >
          </span>
          <input
            v-model="developerMode"
            :disabled="demo"
            role="switch"
            type="checkbox"
            @change="toggleDeveloperMode"
          />
        </label>
        <label class="setting-switch" :class="{ muted: !developerMode }">
          <span>
            <strong>Analytics API access</strong>
            <small
              >Turning this off stops analytics API keys. Event ingestion keys
              keep working.</small
            >
          </span>
          <input
            v-model="apiAccessEnabled"
            :disabled="
              demo || !developerMode || !settings.allowed_scopes.length
            "
            role="switch"
            type="checkbox"
          />
        </label>
      </div>

      <div class="developer-actions">
        <button
          class="button primary"
          :disabled="demo || Boolean(mutation)"
          type="button"
          @click="saveSettings"
        >
          {{ mutation === "save" ? "Saving…" : "Save API settings" }}
        </button>
      </div>

      <div class="developer-download-grid">
        <article>
          <div>
            <p class="panel-kicker">Configuration backup</p>
            <strong>Control plane · JSON</strong>
            <p>
              App metadata, domains, rules, and dashboards. Credentials,
              sessions, hashes, and analytics facts stay out.
            </p>
          </div>
          <button
            class="button secondary"
            :disabled="demo || !settings.developer_mode || Boolean(mutation)"
            type="button"
            @click="downloadBackup"
          >
            {{ mutation === "backup" ? "Preparing…" : "Download backup" }}
          </button>
        </article>

        <article>
          <div>
            <p class="panel-kicker">Analytics data export</p>
            <strong>Privacy-safe events · NDJSON</strong>
            <p>
              Latest events only, capped at 10,000 rows and 90 days. No raw IP,
              query string, fragment, or full custom payload.
            </p>
          </div>
          <div class="developer-export-action">
            <ConsoleSelect
              v-model="exportDays"
              aria-label="Analytics export date range"
              :disabled="demo || !settings.developer_mode"
              :options="exportRangeOptions"
            />
            <button
              class="button secondary"
              :disabled="demo || !settings.developer_mode || Boolean(mutation)"
              type="button"
              @click="downloadEventExport"
            >
              {{ mutation === "export" ? "Exporting…" : "Download events" }}
            </button>
          </div>
        </article>
      </div>
    </article>

    <article
      v-if="settings.api_access_enabled"
      class="management-card developer-key-card"
    >
      <header class="management-card-header">
        <div>
          <p class="panel-kicker">Choose analytics and AI permissions</p>
          <h2>Create an analytics API key</h2>
          <p>
            All analytics and AI APIs are available. Keys remain scoped to this app and the permissions you select.
          </p>
        </div>
        <span class="status-badge neutral">{{ activeKeys.length }} active</span>
      </header>

      <form class="developer-key-form" @submit.prevent="createKey">
        <label class="developer-field">
          <span>Key name</span>
          <input v-model="keyName" maxlength="120" required />
        </label>
        <label class="developer-field">
          <span>Lifetime</span>
          <ConsoleSelect
            v-model="lifetime"
            aria-label="Analytics key lifetime"
            :options="lifetimeOptions"
          />
        </label>

        <fieldset class="developer-scope-fieldset">
          <legend>Exact access</legend>
          <label>
            <input
              :checked="selectedScopes.includes('events:read')"
              :disabled="!settings.allowed_scopes.includes('events:read')"
              type="checkbox"
              @change="toggleScope('events:read')"
            />
            <span>
              <code>events:read</code>
              <small
                >Privacy-safe events, max 90 days / 5,000
                rows per call.</small
              >
            </span>
          </label>
          <label>
            <input
              :checked="selectedScopes.includes('stats:read')"
              :disabled="!settings.allowed_scopes.includes('stats:read')"
              type="checkbox"
              @change="toggleScope('stats:read')"
            />
            <span
              ><code>stats:read</code
              ><small
                >Read aggregate totals and chart series, up to 90 days.</small
              ></span
            >
          </label>
          <label>
            <input
              :disabled="!settings.allowed_scopes.includes('ai:prompt')"
              :checked="selectedScopes.includes('ai:prompt')"
              type="checkbox"
              @change="toggleScope('ai:prompt')"
            />
            <span>
              <code>ai:prompt</code>
              <small
                >AI requires enabled app access and a configured provider.</small
              >
            </span>
          </label>
        </fieldset>

        <button
          class="button primary"
          :disabled="demo || !selectedScopes.length || Boolean(mutation)"
          type="submit"
        >
          {{ mutation === "create" ? "Generating…" : "Create analytics key" }}
        </button>
      </form>

      <div v-if="createdSecret" class="developer-secret" role="status">
        <div>
          <p class="panel-kicker">Shown once · {{ createdKeyName }}</p>
          <strong>Save this secret now. It will not be shown again.</strong>
        </div>
        <code>{{ createdSecret }}</code>
        <button
          class="button secondary"
          type="button"
          @click="copy(createdSecret)"
        >
          Copy secret
        </button>
      </div>
    </article>

    <article
      v-if="settings.api_access_enabled"
      class="management-card developer-endpoints-card"
    >
      <header class="management-card-header">
        <div>
          <p class="panel-kicker">Use your analytics API key</p>
          <h2>Allowed endpoints</h2>
        </div>
      </header>
      <div class="developer-endpoint-list">
        <section>
          <div class="developer-endpoint-title">
            <div>
              <code>GET {{ settings.endpoints.stats }}</code>
              <span class="status-badge neutral">stats:read</span>
            </div>
            <button class="text-button" type="button" @click="copy(statsCurl)">
              Copy curl
            </button>
          </div>
          <pre><code>{{ statsCurl }}</code></pre>
          <p>Read aggregate totals and chart series for this app.</p>
        </section>
        <section>
          <div class="developer-endpoint-title">
            <div>
              <code>GET {{ settings.endpoints.events }}</code>
              <span class="status-badge neutral">events:read</span>
            </div>
            <button class="text-button" type="button" @click="copy(eventsCurl)">
              Copy curl
            </button>
          </div>
          <pre><code>{{ eventsCurl }}</code></pre>
          <p>
            Use <code>pagination.next_before</code> and
            <code>pagination.next_event_id</code> as <code>before</code> and
            <code>before_event_id</code> for the next bounded page.
          </p>
        </section>
        <section>
          <div class="developer-endpoint-title">
            <div>
              <code>POST {{ settings.endpoints.prompt }}</code>
              <span class="status-badge neutral">ai:prompt</span>
            </div>
            <button class="text-button" type="button" @click="copy(promptCurl)">
              Copy curl
            </button>
          </div>
          <pre><code>{{ promptCurl }}</code></pre>
          <p>
            Consumes one available app/member prompt. The prompt body is never
            stored.
          </p>
        </section>
      </div>
    </article>

    <article
      v-if="keys.length || settings.api_access_enabled"
      class="management-card developer-key-list-card"
    >
      <header class="management-card-header">
        <div>
          <p class="panel-kicker">Inventory</p>
          <h2>Analytics API keys</h2>
        </div>
      </header>
      <div v-if="keys.length" class="developer-key-list">
        <article v-for="key in keys" :key="key.id" class="developer-key-row">
          <div>
            <strong>{{ key.name }}</strong>
            <code>{{ key.key_prefix }}…</code>
          </div>
          <div class="developer-scope-list">
            <span v-for="scope in key.scopes" :key="scope">{{ scope }}</span>
          </div>
          <dl>
            <div>
              <dt>Expires</dt>
              <dd>
                {{ key.expires_at ? formatDate(key.expires_at) : "Permanent" }}
              </dd>
            </div>
            <div>
              <dt>Last used</dt>
              <dd>{{ formatDate(key.last_used_at) }}</dd>
            </div>
          </dl>
          <span v-if="key.revoked_at" class="status-badge warning"
            >Revoked</span
          >
          <button
            v-else
            class="button danger small"
            :disabled="demo || Boolean(mutation)"
            type="button"
            @click="revokeKey(key)"
          >
            Revoke
          </button>
        </article>
      </div>
      <div v-else class="management-empty">
        <strong>No analytics API keys yet.</strong>
        <p>
          Create a key above for a reporting integration or AI client. Choose
          only the permissions it needs.
        </p>
      </div>
    </article>

    <p v-if="error" class="inline-error developer-message" role="alert">
      {{ error }}
    </p>
    <p v-else-if="notice" class="inline-notice developer-message" role="status">
      {{ notice }}
    </p>
    <p v-if="demo" class="field-help">
      Credential creation and data exports are disabled in this workspace.
    </p>
  </section>
</template>
