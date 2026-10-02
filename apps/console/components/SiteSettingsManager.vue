<script setup lang="ts">
import { apiErrorMessage as requestError } from "~/utils/apiError";
import { routes } from "~/utils/routes";

type SiteSettingsResponse = {
  capabilities: {
    ai_toggle: boolean;
    contact_support: boolean;
    delete_site: boolean;
    pause_tracking: boolean;
    privacy_controls: boolean;
    rename_site: boolean;
  };
  site: {
    ai_enabled: boolean;
    data_retention_days: number | null;
    domain: string;
    honor_privacy_signals: boolean;
    id: string;
    name: string;
    privacy_contact_email: string | null;
    timezone: string;
    tracking_id: string;
    tracking_paused: boolean;
  };
};

const props = defineProps<{
  apiBase: string;
  demo?: boolean;
  siteId?: string;
}>();
const emit = defineEmits<{
  changed: [];
}>();
const { api } = useApi();

const settings = ref<SiteSettingsResponse | null>(null);
const name = ref("");
const trackingPaused = ref(false);
const privacyContactEmail = ref("");
const deleteConfirmation = ref("");
const loading = ref(false);
const mutation = ref<"delete" | "save" | "">("");
const error = ref("");
const notice = ref("");
let loadController: AbortController | undefined;
let loadGeneration = 0;

watch(
  () => props.siteId,
  () => void loadSettings(),
  { immediate: true },
);

onBeforeUnmount(() => loadController?.abort());

async function loadSettings() {
  loadController?.abort();
  const siteId = props.siteId;
  const generation = ++loadGeneration;
  settings.value = null;
  mutation.value = "";
  notice.value = "";
  error.value = "";
  if (!siteId) return;
  const controller = new AbortController();
  loadController = controller;
  loading.value = true;
  try {
    const response = await api<SiteSettingsResponse>(
      routes.sites.settings(siteId),
      {
        signal: controller.signal,
      },
    );
    if (controller.signal.aborted || generation !== loadGeneration) return;
    hydrateSettings(response);
    deleteConfirmation.value = "";
  } catch (cause) {
    if (controller.signal.aborted || generation !== loadGeneration) return;
    settings.value = null;
    error.value = requestError(cause, "App settings could not be loaded.");
  } finally {
    if (generation === loadGeneration) loading.value = false;
  }
}

async function saveSettings() {
  const siteId = props.siteId;
  if (!siteId || props.demo || mutation.value) return;
  mutation.value = "save";
  error.value = "";
  notice.value = "";
  try {
    const response = await api<SiteSettingsResponse>(
      routes.sites.settings(siteId),
      {
        body: {
          ...(settings.value?.capabilities.privacy_controls
            ? {
                privacy_contact_email: privacyContactEmail.value.trim(),
              }
            : {}),
          name: name.value,
          tracking_paused: trackingPaused.value,
        },
        method: "PUT",
      },
    );
    if (siteId !== props.siteId) return;
    hydrateSettings(response);
    notice.value = "App and privacy settings saved.";
    emit("changed");
  } catch (cause) {
    if (siteId !== props.siteId) return;
    error.value = requestError(cause, "App settings could not be saved.");
  } finally {
    if (siteId === props.siteId) mutation.value = "";
  }
}

function hydrateSettings(response: SiteSettingsResponse) {
  settings.value = response;
  name.value = response.site.name;
  trackingPaused.value = response.site.tracking_paused;
  privacyContactEmail.value = response.site.privacy_contact_email ?? "";
}

async function deleteSite() {
  const siteId = props.siteId;
  if (
    !siteId ||
    props.demo ||
    mutation.value ||
    !settings.value ||
    deleteConfirmation.value !== settings.value.site.name
  ) {
    return;
  }
  mutation.value = "delete";
  error.value = "";
  try {
    await api(routes.sites.one(siteId), {
      method: "DELETE",
    });
    if (siteId !== props.siteId) return;
    emit("changed");
    await navigateTo("/");
  } catch (cause) {
    if (siteId !== props.siteId) return;
    error.value = requestError(cause, "The app could not be deleted.");
  } finally {
    if (siteId === props.siteId) mutation.value = "";
  }
}
</script>

<template>
  <div v-if="loading" class="management-loading" role="status">
    <span class="state-orbit" aria-hidden="true"></span>
    <span>Loading owner settings…</span>
  </div>

  <section v-else-if="settings" class="settings-grid">
    <form
      class="management-card settings-primary"
      @submit.prevent="saveSettings"
    >
      <header class="management-card-header">
        <div>
          <p class="panel-kicker">Identity and ingestion</p>
          <h2>App controls</h2>
        </div>
        <span
          class="status-badge"
          :class="trackingPaused ? 'warning' : 'success'"
        >
          {{ trackingPaused ? "Tracking paused" : "Tracking live" }}
        </span>
      </header>

      <div class="settings-control-grid">
        <div class="settings-identity">
          <div class="settings-fields">
            <label>
              <span>App name</span>
              <input
                v-model="name"
                :disabled="demo || !settings.capabilities.rename_site"
                maxlength="120"
                required
              />
            </label>
            <label>
              <span>Tracking ID</span>
              <input
                :value="settings.site.tracking_id"
                readonly
                spellcheck="false"
              />
            </label>
            <label>
              <span>Primary domain</span>
              <input :value="settings.site.domain" readonly />
            </label>
            <label>
              <span>Timezone</span>
              <input :value="settings.site.timezone" readonly />
            </label>
          </div>

          <div class="settings-switches">
            <label class="setting-switch">
              <span>
                <strong>Pause event tracking</strong>
                <small
                  >The API rejects new ingestion while historical analytics stay
                  readable.</small
                >
              </span>
              <input
                v-model="trackingPaused"
                :disabled="demo || !settings.capabilities.pause_tracking"
                type="checkbox"
                role="switch"
              />
            </label>
          </div>
        </div>
        <section
          v-if="settings.capabilities.privacy_controls"
          class="settings-privacy"
          aria-labelledby="site-privacy-heading"
        >
          <header class="settings-section-heading">
            <div>
              <p class="panel-kicker">Data minimisation</p>
              <h3 id="site-privacy-heading">Privacy baseline</h3>
            </div>
            <span class="status-badge success">API enforced</span>
          </header>

          <div class="settings-fields">
            <div class="settings-field-stack">
              <span id="analytics-retention-label" class="settings-field-label">
                Analytics retention
              </span>
              <div class="management-empty compact">
                <strong>Unlimited retention</strong>
                <p>
                  Analytics are retained until you explicitly delete the app. Monitor server storage and keep backups.
                </p>
              </div>
            </div>

            <label>
              <span>Privacy contact email</span>
              <input
                v-model="privacyContactEmail"
                autocomplete="email"
                :disabled="demo"
                inputmode="email"
                maxlength="254"
                placeholder="privacy@example.com"
                type="email"
              />
              <small class="settings-field-help">
                A monitored address for privacy questions and rights requests.
                Leave blank only if your published notice names another route.
              </small>
            </label>
          </div>

          <div class="setting-switch privacy-signal-status">
            <span>
              <strong>Browser privacy signals</strong>
              <small>
                The SDK stops tracking for Global Privacy Control by default.
                Do Not Track is no longer used. The API does not filter events
                based on either header.
              </small>
            </span>
            <span class="status-badge">Handled by SDK</span>
          </div>
        </section>
      </div>

      <button
        class="button primary settings-save"
        :disabled="demo || Boolean(mutation)"
        type="submit"
      >
        {{ mutation === "save" ? "Saving…" : "Save settings" }}
      </button>
      <p v-if="demo" class="field-help">
        Settings are API-served and read-only in this workspace.
      </p>
    </form>



    <div class="settings-app-grid">
      <article class="management-card danger-zone">
        <header class="management-card-header">
          <div>
            <p class="panel-kicker">Danger zone</p>
            <h2>Delete this app</h2>
          </div>
        </header>
        <p>
          This permanently removes the app, stops future collection, and submits
          its analytics for asynchronous deletion. Type
          <strong>{{ settings.site.name }}</strong> to confirm. This cannot be
          undone.
        </p>
        <label>
          <span>Confirmation</span>
          <input
            v-model="deleteConfirmation"
            :disabled="demo"
            autocomplete="off"
          />
        </label>
        <button
          class="button danger"
          :disabled="
            demo ||
            Boolean(mutation) ||
            deleteConfirmation !== settings.site.name
          "
          type="button"
          @click="deleteSite"
        >
          {{ mutation === "delete" ? "Deleting…" : "Delete app" }}
        </button>
      </article>
      <slot name="app-details" />
    </div>

    <p v-if="notice" class="inline-notice settings-message" role="status">
      {{ notice }}
    </p>
    <p v-if="error" class="inline-error settings-message" role="alert">
      {{ error }}
    </p>
  </section>
  <p v-else-if="error" class="page-alert" role="alert">{{ error }}</p>
</template>

<style scoped>
.settings-app-grid {
  display: grid;
  grid-template-columns: repeat(2, minmax(0, 1fr));
  gap: var(--space-4);
}
/* Let the app details and full-width domains card share this grid. */
.settings-app-grid :deep(.management-grid) {
  display: contents;
}
.settings-app-grid :deep(.management-card-wide) {
  grid-column: 1 / -1;
}
@media (max-width: 1100px) {
  .settings-app-grid {
    grid-template-columns: minmax(0, 1fr);
  }
}

.settings-primary {
  container-type: inline-size;
}
.settings-control-grid,
.settings-identity {
  display: grid;
  align-content: start;
  gap: var(--space-4);
  min-width: 0;
}
.settings-control-grid {
  grid-template-columns: minmax(0, 1fr);
}
.settings-save {
  justify-self: start;
  min-width: 180px;
}
@container (min-width: 760px) {
  .settings-control-grid {
    grid-template-columns: repeat(2, minmax(0, 1fr));
    gap: var(--space-5);
  }
  .settings-privacy {
    border-top: 0;
    border-left: 1px solid var(--border-subtle);
    padding-top: 0;
    padding-left: var(--space-5);
  }
  .settings-privacy .settings-fields {
    grid-template-columns: minmax(0, 1fr);
  }
}
@container (max-width: 480px) {
  .settings-fields {
    grid-template-columns: minmax(0, 1fr);
  }
  .settings-save {
    width: 100%;
    min-width: 0;
  }
  .privacy-signal-status {
    flex-wrap: wrap;
  }
}
</style>
