<script setup lang="ts">
import { apiErrorMessage } from "~/utils/apiError";
import { routes } from "~/utils/routes";

type ApiKeySummary = {
  created_at: string;
  expires_at: string | null;
  id: string;
  key_prefix: string;
  last_used_at: string | null;
  name: string;
  revoked_at: string | null;
  scopes: string[];
};

type CreateApiKeyResponse = {
  api_key: ApiKeySummary;
  secret: string;
};

const props = defineProps<{
  siteId?: string;
  siteName?: string;
}>();
const { api } = useApi();

const keys = ref<ApiKeySummary[]>([]);
const loading = ref(false);
const errorMessage = ref("");
const notice = ref("");
const mutationPending = ref(false);

const createName = ref("");
const createExpiry = ref("");
const createdSecret = ref("");
const createdKeyName = ref("");
const secretCopied = ref(false);

const editingKeyId = ref("");
const editName = ref("");
const editExpiry = ref("");
const confirmRevokeId = ref("");
let loadController: AbortController | undefined;
let siteContext = 0;
let mutationOperation = 0;

watch(
  () => props.siteId,
  (siteId) => {
    siteContext += 1;
    mutationOperation += 1;
    loadController?.abort();
    clearSecret();
    keys.value = [];
    errorMessage.value = "";
    notice.value = "";
    editingKeyId.value = "";
    confirmRevokeId.value = "";
    loading.value = false;
    mutationPending.value = false;
    if (siteId) void loadKeys();
  },
  { immediate: true },
);

onBeforeUnmount(() => {
  loadController?.abort();
  clearSecret();
});

async function loadKeys() {
  if (!props.siteId) return;
  const siteId = props.siteId;
  const context = siteContext;
  loadController?.abort();
  const controller = new AbortController();
  loadController = controller;

  errorMessage.value = "";
  loading.value = true;

  try {
    const response = await api<ApiKeySummary[]>(routes.sites.apiKeys(siteId), {
      signal: controller.signal,
    });
    if (isCurrentSite(siteId, context)) keys.value = response;
  } catch (error) {
    if (controller.signal.aborted || !isCurrentSite(siteId, context)) return;
    keys.value = [];
    errorMessage.value = requestError(error, "API keys could not be loaded.");
  } finally {
    if (loadController === controller) loadController = undefined;
    if (isCurrentSite(siteId, context)) loading.value = false;
  }
}

async function createKey() {
  const siteId = props.siteId;
  if (!siteId) return;
  const context = siteContext;
  const operation = ++mutationOperation;

  errorMessage.value = "";
  notice.value = "";
  mutationPending.value = true;
  clearSecret();

  try {
    const response = await api<CreateApiKeyResponse>(
      routes.sites.apiKeys(siteId),
      {
        body: {
          expires_at: toRfc3339(createExpiry.value),
          name: createName.value,
        },
        method: "POST",
      },
    );
    if (!isCurrentSite(siteId, context)) return;
    createdSecret.value = response.secret;
    createdKeyName.value = response.api_key.name;
    createName.value = "";
    createExpiry.value = "";
    notice.value = "API key created.";
    await loadKeys();
  } catch (error) {
    if (!isCurrentSite(siteId, context)) return;
    errorMessage.value = requestError(
      error,
      "The API key could not be created.",
    );
  } finally {
    if (operation === mutationOperation && isCurrentSite(siteId, context)) {
      mutationPending.value = false;
    }
  }
}

function startEditing(key: ApiKeySummary) {
  editingKeyId.value = key.id;
  editName.value = key.name;
  editExpiry.value = toLocalDateTime(key.expires_at);
  confirmRevokeId.value = "";
  errorMessage.value = "";
  notice.value = "";
}

function cancelEditing() {
  editingKeyId.value = "";
  editName.value = "";
  editExpiry.value = "";
}

async function updateKey(key: ApiKeySummary) {
  const siteId = props.siteId;
  if (!siteId) return;
  const context = siteContext;
  const operation = ++mutationOperation;

  errorMessage.value = "";
  notice.value = "";
  mutationPending.value = true;

  try {
    await api<ApiKeySummary>(routes.sites.apiKey(siteId, key.id), {
      body: {
        expires_at: toRfc3339(editExpiry.value),
        name: editName.value,
      },
      method: "PUT",
    });
    if (!isCurrentSite(siteId, context)) return;
    cancelEditing();
    notice.value = "API key settings were saved.";
    await loadKeys();
  } catch (error) {
    if (!isCurrentSite(siteId, context)) return;
    errorMessage.value = requestError(
      error,
      "The API key could not be updated.",
    );
  } finally {
    if (operation === mutationOperation && isCurrentSite(siteId, context)) {
      mutationPending.value = false;
    }
  }
}

async function revokeKey(key: ApiKeySummary) {
  const siteId = props.siteId;
  if (!siteId) return;
  const context = siteContext;
  const operation = ++mutationOperation;

  errorMessage.value = "";
  notice.value = "";
  mutationPending.value = true;

  try {
    await api<{ status: "revoked" }>(routes.sites.apiKey(siteId, key.id), {
      method: "DELETE",
    });
    if (!isCurrentSite(siteId, context)) return;
    confirmRevokeId.value = "";
    notice.value = `${key.name} was revoked.`;
    await loadKeys();
  } catch (error) {
    if (!isCurrentSite(siteId, context)) return;
    errorMessage.value = requestError(
      error,
      "The API key could not be revoked.",
    );
  } finally {
    if (operation === mutationOperation && isCurrentSite(siteId, context)) {
      mutationPending.value = false;
    }
  }
}

function isCurrentSite(siteId: string, context: number) {
  return context === siteContext && props.siteId === siteId;
}

async function copySecret() {
  if (!createdSecret.value) return;

  try {
    await navigator.clipboard.writeText(createdSecret.value);
    secretCopied.value = true;
  } catch {
    errorMessage.value =
      "Clipboard access was unavailable. Select and copy the secret manually.";
  }
}

function clearSecret() {
  createdSecret.value = "";
  createdKeyName.value = "";
  secretCopied.value = false;
}

function toRfc3339(value: string) {
  if (!value) return null;
  const date = new Date(value);
  if (Number.isNaN(date.valueOf())) {
    throw new Error("Invalid expiry");
  }
  return date.toISOString();
}

function toLocalDateTime(value: string | null) {
  if (!value) return "";
  const date = new Date(value);
  if (Number.isNaN(date.valueOf())) return "";
  const local = new Date(date.valueOf() - date.getTimezoneOffset() * 60_000);
  return local.toISOString().slice(0, 16);
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

function keyStatus(key: ApiKeySummary) {
  if (key.revoked_at) return "Revoked";
  if (key.expires_at) {
    const expiry = new Date(key.expires_at);
    if (!Number.isNaN(expiry.valueOf()) && expiry.valueOf() <= Date.now())
      return "Expired";
  }
  return "Active";
}

function keyStatusClass(key: ApiKeySummary) {
  const status = keyStatus(key);
  if (status === "Active") return "success";
  if (status === "Revoked") return "danger";
  return "neutral";
}

function requestError(error: unknown, fallback: string) {
  if (error instanceof Error && error.message === "Invalid expiry") {
    return "Choose a valid expiry date and time.";
  }
  return apiErrorMessage(error, fallback);
}
</script>

<template>
  <article class="management-card management-card-wide">
    <header class="management-card-header">
      <div>
        <p class="panel-kicker">Send events · events:write</p>
        <h3>Event ingestion keys</h3>
      </div>
      <button
        v-if="siteId"
        class="text-button"
        :disabled="loading"
        type="button"
        @click="loadKeys"
      >
        Refresh
      </button>
    </header>
    <p class="management-copy">
      Send events from a trusted server to
      <code>POST /v1/events/api</code> using <code>X-OwlEye-Api-Key</code>. Each
      key can only send events for this app; it cannot read analytics, run AI
      prompts, or manage the app.
    </p>

    <template v-if="siteId">
      <form
        class="management-form create-resource-form"
        @submit.prevent="createKey"
      >
        <div class="form-grid form-grid-two">
          <label>
            <span>Key name</span>
            <input
              v-model="createName"
              autocomplete="off"
              maxlength="120"
              placeholder="Production ingest"
              required
            />
          </label>
          <label>
            <span>Expiry <small>optional</small></span>
            <input v-model="createExpiry" type="datetime-local" />
          </label>
        </div>
        <p class="field-help">
          Times use your browser timezone. Leave expiry empty for a key that
          does not expire.
        </p>
        <button
          class="button primary"
          :disabled="mutationPending"
          type="submit"
        >
          {{ mutationPending ? "Creating…" : "Create ingestion key" }}
        </button>
      </form>

      <aside
        v-if="createdSecret"
        class="secret-callout"
        role="alert"
        aria-live="assertive"
      >
        <div>
          <p class="panel-kicker">Shown once</p>
          <h4>Copy the secret for {{ createdKeyName }} now</h4>
          <p>
            OWLEYE cannot show this value again after you dismiss or leave this
            site.
          </p>
        </div>
        <code tabindex="0">{{ createdSecret }}</code>
        <div class="management-actions">
          <button
            class="button primary compact"
            type="button"
            @click="copySecret"
          >
            {{ secretCopied ? "Copied" : "Copy secret" }}
          </button>
          <button
            class="button secondary compact"
            type="button"
            @click="clearSecret"
          >
            I have saved it
          </button>
        </div>
      </aside>

      <div v-if="loading" class="management-loading" role="status">
        <span class="state-orbit" aria-hidden="true"></span>
        <span>Loading ingestion keys…</span>
      </div>
      <div v-else-if="keys.length" class="resource-list">
        <article
          v-for="key in keys"
          :key="key.id"
          class="resource-row resource-row-stacked"
        >
          <form
            v-if="editingKeyId === key.id"
            class="management-form resource-edit-form"
            @submit.prevent="updateKey(key)"
          >
            <div class="form-grid form-grid-two">
              <label>
                <span>Key name</span>
                <input v-model="editName" maxlength="120" required />
              </label>
              <label>
                <span>Expiry <small>leave empty for none</small></span>
                <input v-model="editExpiry" type="datetime-local" />
              </label>
            </div>
            <p class="field-help">
              Clear the expiry to make this key never expire.
            </p>
            <div class="management-actions">
              <button
                class="button primary compact"
                :disabled="mutationPending"
                type="submit"
              >
                {{ mutationPending ? "Saving…" : "Save key" }}
              </button>
              <button
                class="button secondary compact"
                :disabled="mutationPending"
                type="button"
                @click="cancelEditing"
              >
                Cancel
              </button>
            </div>
          </form>

          <template v-else>
            <div class="resource-summary">
              <div class="resource-main">
                <div class="resource-title-line">
                  <strong>{{ key.name }}</strong>
                  <span :class="['status-badge', keyStatusClass(key)]">
                    {{ keyStatus(key) }}
                  </span>
                </div>
                <code>{{ key.key_prefix }}…</code>
              </div>
              <dl class="resource-meta">
                <div>
                  <dt>Scopes</dt>
                  <dd>
                    {{
                      key.scopes.length ? key.scopes.join(", ") : "Site default"
                    }}
                  </dd>
                </div>
                <div>
                  <dt>Last used</dt>
                  <dd>{{ formatDate(key.last_used_at) }}</dd>
                </div>
                <div>
                  <dt>Expires</dt>
                  <dd>{{ formatDate(key.expires_at) }}</dd>
                </div>
              </dl>
            </div>

            <div
              v-if="confirmRevokeId === key.id"
              aria-live="assertive"
              class="confirm-panel"
              role="alertdialog"
            >
              <p>
                Revoke <strong>{{ key.name }}</strong
                >? Existing integrations using it will stop working.
              </p>
              <div class="confirm-actions">
                <button
                  autofocus
                  class="button danger compact"
                  :disabled="mutationPending"
                  type="button"
                  @click="revokeKey(key)"
                >
                  {{ mutationPending ? "Revoking…" : "Revoke key" }}
                </button>
                <button
                  class="button secondary compact"
                  :disabled="mutationPending"
                  type="button"
                  @click="confirmRevokeId = ''"
                >
                  Cancel
                </button>
              </div>
            </div>
            <div v-else-if="!key.revoked_at" class="management-actions">
              <button
                class="button secondary compact"
                type="button"
                @click="startEditing(key)"
              >
                Edit
              </button>
              <button
                class="button danger-quiet compact"
                type="button"
                @click="confirmRevokeId = key.id"
              >
                Revoke
              </button>
            </div>
          </template>
        </article>
      </div>
      <div v-else class="management-empty">
        <strong>No ingestion keys</strong>
        <p>
          Create one when a trusted server needs to send data for this site.
        </p>
      </div>
    </template>
    <div v-else class="management-empty">
      <strong>No site selected</strong>
      <p>Create or select a site before managing API keys.</p>
    </div>

    <p v-if="notice" class="inline-notice" role="status">{{ notice }}</p>
    <p v-if="errorMessage" class="inline-error" role="alert">
      {{ errorMessage }}
    </p>
  </article>
</template>
