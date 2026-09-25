<script setup lang="ts">
import {
  publicMetricLabels,
  publicBreakdownLabels,
  type PublicSharingSettings,
  type PublicShareConfig,
} from "~/types/publicDashboard";
import { apiErrorMessage } from "~/utils/apiError";
const props = defineProps<{ siteId?: string }>();
const { api } = useApi();
const settings = ref<PublicSharingSettings | null>(null);
const draft = ref<PublicShareConfig | null>(null);
const pending = ref(false);
const error = ref("");
const notice = ref("");
const base = ref(import.meta.client ? window.location.origin : "");
let controller: AbortController | undefined;
const endpoint = () =>
  `/v1/sites/${encodeURIComponent(props.siteId!)}/public-overview`;
const links = computed(() =>
  settings.value?.config.enabled && settings.value.eligible
    ? [
        {
          label: "App ID link",
          url: `${base.value}/p?${new URLSearchParams({ site_id: settings.value.site_id })}`,
        },
        ...settings.value.config.urls.map((url) => ({
          label: url,
          url: `${base.value}/p?${new URLSearchParams({ site: url })}`,
        })),
      ]
    : [],
);
onMounted(() => {
  if (location.hostname === "localhost") base.value = location.origin;
});
watch(() => props.siteId, load, { immediate: true });
onBeforeUnmount(() => controller?.abort());
async function load() {
  controller?.abort();
  controller = new AbortController();
  const request = controller;
  settings.value = null;
  draft.value = null;
  error.value = "";
  notice.value = "";
  pending.value = false;
  if (!props.siteId) return;
  try {
    const result = await api<PublicSharingSettings>(endpoint(), {
      signal: request.signal,
    });
    if (request.signal.aborted) return;
    settings.value = result;
    draft.value = structuredClone(result.config);
    draft.value.urls = draft.value.urls.filter((url) =>
      result.available_urls.includes(url),
    );
  } catch (cause) {
    if (!request.signal.aborted)
      error.value = apiErrorMessage(
        cause,
        "Sharing settings could not be loaded.",
      );
  }
}
async function save(disable = false) {
  if (!draft.value || !settings.value || pending.value) return;
  const request = controller!;
  pending.value = true;
  error.value = "";
  notice.value = "";
  const config = structuredClone(
    toRaw(disable ? settings.value.config : draft.value),
  );
  if (disable) {
    config.enabled = false;
    config.urls = [];
  }
  try {
    const result = await api<{ config: PublicShareConfig }>(endpoint(), {
      method: "PUT",
      body: config,
      signal: request.signal,
    });
    if (request.signal.aborted) return;
    settings.value.config = result.config;
    draft.value = structuredClone(result.config);
    notice.value = result.config.enabled
      ? "Your selected Overview statistics are now public."
      : "Public sharing is off. Both link formats are unavailable.";
  } catch (cause) {
    if (!request.signal.aborted)
      error.value = apiErrorMessage(
        cause,
        "Sharing settings could not be saved.",
      );
  } finally {
    if (!request.signal.aborted) pending.value = false;
  }
}
async function copy(url: string) {
  try {
    await navigator.clipboard.writeText(url);
    notice.value = "Public link copied.";
  } catch {
    notice.value = "Copy the link from the field below.";
  }
}
</script>
<template>
  <section
    class="management-card public-sharing"
    aria-labelledby="public-sharing-title"
  >
    <header>
      <p class="eyebrow">Public Overview</p>
      <h2 id="public-sharing-title">Share only what you choose.</h2>
      <p>
        Publish a read-only Overview that anyone with its link can view without
        signing in.
      </p>
    </header>
    <p v-if="error" role="alert" class="inline-error">{{ error }}</p>
    <button
      v-if="!settings && error"
      type="button"
      class="button secondary compact"
      @click="load"
    >
      Retry sharing settings
    </button>
    <p v-else-if="!settings" role="status">Loading sharing settings…</p>
    <template v-if="settings && draft">
      <p v-if="!settings.eligible" class="empty-state">
        Public sharing is currently unavailable.
      </p>
      <form @submit.prevent="save()">
        <fieldset
          :disabled="pending || !settings.eligible"
          class="sharing-fields"
        >
          <label class="sharing-choice"
            ><input v-model="draft.enabled" type="checkbox" /> Enable public
            sharing</label
          >
          <p class="sharing-note">
            Your app name and the selections below will be public. Turning this
            off revokes both App ID and URL links.
          </p>
          <fieldset>
            <legend>Metrics</legend>
            <div class="sharing-options">
              <label
                v-for="(label, key) in publicMetricLabels"
                :key="key"
                class="sharing-choice"
                ><input
                  v-model="draft.metrics"
                  type="checkbox"
                  :value="key"
                />{{ label }}</label
              >
            </div>
          </fieldset>
          <label class="sharing-choice"
            ><input v-model="draft.traffic" type="checkbox" />Traffic chart for
            the selected metrics</label
          >
          <fieldset>
            <legend>Breakdowns · page-view counts</legend>
            <p class="sharing-note">
              Only groups with at least 5 visitors appear. Raw events, page
              paths, referrers, campaign names, AI, and Uptime stay private.
            </p>
            <div class="sharing-options">
              <label
                v-for="(label, key) in publicBreakdownLabels"
                :key="key"
                class="sharing-choice"
                ><input
                  v-model="draft.breakdowns"
                  type="checkbox"
                  :value="key"
                />{{ label }}</label
              >
            </div>
          </fieldset>
          <label class="sharing-range"
            >Maximum history<select
              aria-label="Maximum history"
              v-model.number="draft.max_days"
            >
              <option :value="7">Last 7 days</option>
              <option :value="30">Last 30 days</option>
              <option :value="90">Last 90 days</option>
            </select></label
          >
          <fieldset>
            <legend>URL links · optional</legend>
            <p class="sharing-note">
              The App ID link works whenever sharing is enabled. Select any
              configured app URL to also enable its URL link.
            </p>
            <label
              v-for="url in settings.available_urls"
              :key="url"
              class="sharing-choice"
              ><input
                v-model="draft.urls"
                type="checkbox"
                :value="url"
              /><span>{{ url }}</span></label
            >
            <p v-if="!settings.available_urls.length">
              Add an exact app domain to enable URL links.
            </p>
          </fieldset>
          <button
            type="submit"
            class="button primary"
            :disabled="!draft.metrics.length"
          >
            {{
              pending
                ? "Saving…"
                : draft.enabled
                  ? "Publish selected statistics"
                  : "Save sharing settings"
            }}
          </button>
        </fieldset>
      </form>
      <button
        v-if="settings.config.enabled"
        class="button secondary compact"
        type="button"
        :disabled="pending"
        @click="save(true)"
      >
        Turn off public sharing
      </button>
      <p v-if="notice" role="status">{{ notice }}</p>
      <div v-if="links.length" class="sharing-links">
        <h3>Published links</h3>
        <div v-for="link in links" :key="link.url" class="sharing-link">
          <label
            >{{ link.label
            }}<input
              :value="link.url"
              readonly
              @focus="($event.target as HTMLInputElement).select()" /></label
          ><a
            class="button secondary compact"
            :href="link.url"
            target="_blank"
            rel="noopener noreferrer"
            >View</a
          ><button
            type="button"
            class="button secondary compact"
            @click="copy(link.url)"
          >
            Copy
          </button>
        </div>
      </div>
    </template>
  </section>
</template>
<style scoped>
.public-sharing {
  display: grid;
  gap: 18px;
  min-width: 0;
}
.public-sharing header p {
  margin-block: 0 10px;
}
.sharing-fields {
  display: grid;
  gap: 22px;
  border: 0;
  padding: 0;
  min-width: 0;
}
.sharing-fields fieldset {
  border: 1px solid var(--border-default, #d5d0c3);
  border-radius: 12px;
  padding: 16px;
  min-width: 0;
}
.sharing-fields legend {
  font-weight: 750;
  padding: 0 6px;
}
.sharing-choice {
  display: flex;
  align-items: center;
  gap: 10px;
  font-size: 0.9rem;
  overflow-wrap: anywhere;
}
.sharing-choice input {
  width: 17px;
  height: 17px;
  flex-shrink: 0;
  accent-color: #4056ed;
}
.sharing-options {
  display: flex;
  flex-wrap: wrap;
  gap: 16px 24px;
}
.sharing-note {
  font-size: 0.85rem;
  color: var(--text-secondary);
  line-height: 1.6;
  margin: 0 0 12px;
}
.sharing-range {
  display: grid;
  gap: 8px;
  max-width: 260px;
}
.sharing-range select,
.sharing-link input {
  min-height: 42px;
  padding: 9px 12px;
  border: 1px solid #bdb7a9;
  border-radius: 10px;
  color: var(--text-primary);
  background: #fffefb;
  font: inherit;
}
.sharing-range select:focus-visible,
.sharing-link input:focus-visible {
  outline: 2px solid #4255ed;
  outline-offset: 2px;
}
.sharing-links {
  border-top: 1px solid #d5d0c3;
  padding-top: 16px;
  display: grid;
  gap: 16px;
  min-width: 0;
}
.sharing-link {
  display: flex;
  align-items: end;
  gap: 8px;
  flex-wrap: wrap;
  min-width: 0;
}
.sharing-link label {
  flex: 1 1 240px;
  display: grid;
  gap: 6px;
  min-width: 0;
  font-size: 0.8rem;
  overflow-wrap: anywhere;
}
.sharing-link input {
  width: 100%;
  font-size: 0.8rem;
}
.sharing-fields:disabled {
  opacity: 0.65;
}
.public-sharing > .button {
  justify-self: start;
}
</style>
