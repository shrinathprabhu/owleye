<script setup lang="ts">
import type { CampaignItem, CampaignsResponse } from "~/types/insights";
import { apiErrorMessage as requestError } from "~/utils/apiError";
import { routes } from "~/utils/routes";

const props = withDefaults(
  defineProps<{ canWrite?: boolean; demo?: boolean; siteId?: string }>(),
  { canWrite: false, demo: false, siteId: "" },
);
const { api } = useApi();
const response = shallowRef<CampaignsResponse | null>(null);
const days = ref(30);
const loading = ref(false);
const mutating = ref(false);
const error = ref("");
const notice = ref("");
const createOpen = ref(false);
const confirmDelete = ref("");
const draft = reactive({
  campaign_key: "",
  destination_url: "",
  medium: "email",
  name: "",
  source: "newsletter",
});
let controller: AbortController | undefined;

watch(
  () => props.siteId,
  () => void load(),
  { immediate: true },
);
onBeforeUnmount(() => controller?.abort());

async function load() {
  controller?.abort();
  response.value = null;
  error.value = "";
  notice.value = "";
  if (!props.siteId) return;
  const current = new AbortController();
  controller = current;
  loading.value = true;
  try {
    response.value = await api<CampaignsResponse>(
      routes.sites.campaigns(props.siteId),
      {
        query: { days: days.value },
        signal: current.signal,
      },
    );
  } catch (cause) {
    if (!current.signal.aborted)
      error.value = requestError(cause, "Campaigns could not be loaded.");
  } finally {
    if (controller === current) {
      controller = undefined;
      loading.value = false;
    }
  }
}

async function createCampaign() {
  if (!props.siteId || mutating.value || !props.canWrite || props.demo) return;
  mutating.value = true;
  error.value = "";
  notice.value = "";
  try {
    await api(routes.sites.campaigns(props.siteId), {
      body: draft,
      method: "POST",
    });
    Object.assign(draft, {
      campaign_key: "",
      destination_url: "",
      medium: "email",
      name: "",
      source: "newsletter",
    });
    createOpen.value = false;
    await load();
    notice.value = "Campaign created. Its tagged link is ready to share.";
  } catch (cause) {
    error.value = requestError(cause, "The campaign could not be created.");
  } finally {
    mutating.value = false;
  }
}

async function deleteCampaign(item: CampaignItem) {
  if (
    !props.siteId ||
    !item.id ||
    mutating.value ||
    !props.canWrite ||
    props.demo
  )
    return;
  mutating.value = true;
  error.value = "";
  try {
    await api(routes.sites.campaign(props.siteId, item.id), {
      method: "DELETE",
    });
    confirmDelete.value = "";
    await load();
    notice.value =
      "Campaign definition removed. Historical attribution remains queryable.";
  } catch (cause) {
    error.value = requestError(cause, "The campaign could not be removed.");
  } finally {
    mutating.value = false;
  }
}

async function copyLink(item: CampaignItem) {
  if (!item.tracking_url || !navigator.clipboard) return;
  await navigator.clipboard.writeText(item.tracking_url);
  notice.value = `${item.name} link copied.`;
}

const conversionsPerVisitor = (item: CampaignItem) =>
  item.visitors ? `${(item.conversions / item.visitors).toFixed(2)}×` : "—";
</script>

<template>
  <div class="campaign-stack">
    <div class="campaign-toolbar">
      <div>
        <strong>UTM attribution</strong>
        <span>Landing session → page views → custom conversions</span>
      </div>
      <div class="campaign-toolbar-actions">
        <label>
          <span>Range</span>
          <select v-model.number="days" @change="load">
            <option :value="7">Last 7 days</option>
            <option :value="30">Last 30 days</option>
            <option :value="90">Last 90 days</option>
          </select>
        </label>
        <button
          class="primary-action"
          :disabled="!canWrite || demo"
          type="button"
          @click="createOpen = !createOpen"
        >
          {{ createOpen ? "Close builder" : "+ Create campaign" }}
        </button>
      </div>
    </div>

    <form
      v-if="createOpen"
      class="campaign-builder"
      @submit.prevent="createCampaign"
    >
      <header>
        <div>
          <p class="panel-kicker">Tagged link builder</p>
          <h2>Create a campaign</h2>
        </div>
        <span>Stored server-side</span>
      </header>
      <div class="campaign-fields">
        <label
          ><span>Name</span
          ><input
            v-model="draft.name"
            maxlength="120"
            placeholder="Launch week"
            required
        /></label>
        <label
          ><span>Campaign key <small>optional</small></span
          ><input
            v-model="draft.campaign_key"
            maxlength="128"
            placeholder="launch-week"
        /></label>
        <label class="wide"
          ><span>Destination URL</span
          ><input
            v-model="draft.destination_url"
            type="url"
            placeholder="https://example.com/pricing"
            required
        /></label>
        <label
          ><span>Source</span
          ><input
            v-model="draft.source"
            maxlength="80"
            placeholder="newsletter"
            required
        /></label>
        <label
          ><span>Medium</span
          ><input
            v-model="draft.medium"
            maxlength="80"
            placeholder="email"
            required
        /></label>
      </div>
      <button class="primary-action" :disabled="mutating" type="submit">
        {{ mutating ? "Creating…" : "Create tagged campaign" }}
      </button>
    </form>

    <p v-if="error" class="page-alert" role="alert">{{ error }}</p>
    <p v-if="notice" class="campaign-notice" role="status">{{ notice }}</p>

    <section class="campaign-totals" aria-label="Campaign totals">
      <article>
        <span>Campaigns</span
        ><strong>{{
          response?.totals.campaigns.toLocaleString() ?? "—"
        }}</strong
        ><small>defined or observed</small>
      </article>
      <article>
        <span>Visitors</span
        ><strong>{{ response?.totals.visitors.toLocaleString() ?? "—" }}</strong
        ><small>attributed people</small>
      </article>
      <article>
        <span>Views</span
        ><strong>{{ response?.totals.views.toLocaleString() ?? "—" }}</strong
        ><small>in attributed sessions</small>
      </article>
      <article>
        <span>Conversions</span
        ><strong>{{
          response?.totals.conversions.toLocaleString() ?? "—"
        }}</strong
        ><small>custom + rule events</small>
      </article>
    </section>

    <section class="campaign-table-card">
      <header>
        <div>
          <p class="panel-kicker">Campaign performance</p>
          <h2>Acquisition results</h2>
        </div>
        <span>Last {{ days }} days</span>
      </header>
      <div v-if="loading" class="campaign-state">
        <span class="state-orbit"></span><strong>Attributing sessions…</strong>
      </div>
      <div v-else-if="response?.items.length" class="campaign-table-wrap">
        <table>
          <thead>
            <tr>
              <th>Campaign</th>
              <th>Source / medium</th>
              <th>Visitors</th>
              <th>Views</th>
              <th>Conversions</th>
              <th>Events / visitor</th>
              <th><span class="sr-only">Actions</span></th>
            </tr>
          </thead>
          <tbody>
            <tr
              v-for="item in response.items"
              :key="item.id ?? item.campaign_key"
            >
              <th>
                <strong>{{ item.name }}</strong
                ><code>{{ item.campaign_key }}</code>
              </th>
              <td>{{ item.source || "—" }} / {{ item.medium || "—" }}</td>
              <td>{{ item.visitors.toLocaleString() }}</td>
              <td>{{ item.views.toLocaleString() }}</td>
              <td>{{ item.conversions.toLocaleString() }}</td>
              <td>{{ conversionsPerVisitor(item) }}</td>
              <td>
                <div class="row-actions">
                  <button
                    v-if="item.tracking_url"
                    type="button"
                    @click="copyLink(item)"
                  >
                    Copy link
                  </button>
                  <template v-if="item.id && canWrite && !demo">
                    <button
                      v-if="confirmDelete !== item.id"
                      type="button"
                      @click="confirmDelete = item.id"
                    >
                      Remove
                    </button>
                    <button
                      v-else
                      class="danger-action"
                      type="button"
                      @click="deleteCampaign(item)"
                    >
                      Confirm
                    </button>
                  </template>
                </div>
              </td>
            </tr>
          </tbody>
        </table>
      </div>
      <div v-else class="campaign-state">
        <span class="state-symbol">↗</span
        ><strong>No campaigns in this range</strong
        ><span
          >Create a tagged link or enable captureCampaigns in the SDK.</span
        >
      </div>
    </section>

    <aside class="campaign-help">
      <strong>SDK requirement</strong>
      <code
        >useAnalytics("{{ response?.site_id || siteId }}", { captureCampaigns:
        true })</code
      >
      <span
        >Only utm_source, utm_medium, and utm_campaign are retained; arbitrary
        query parameters stay excluded.</span
      >
    </aside>
  </div>
</template>

<style scoped>
.campaign-stack {
  display: grid;
  gap: 18px;
}
.campaign-toolbar,
.campaign-toolbar-actions,
.campaign-builder > header,
.campaign-table-card > header,
.campaign-help {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 16px;
}
.campaign-toolbar {
  border: 1px solid var(--border-subtle);
  border-radius: 14px;
  background: var(--bg-elevated);
  padding: 13px 16px;
}
.campaign-toolbar > div:first-child {
  display: grid;
  gap: 2px;
}
.campaign-toolbar strong,
h2 {
  color: var(--text-primary);
}
.campaign-toolbar span {
  color: var(--text-tertiary);
  font-size: 0.76rem;
}
.campaign-toolbar-actions label {
  display: flex;
  align-items: center;
  gap: 8px;
  font-size: 0.75rem;
  font-weight: 700;
}
select,
input {
  min-height: 42px;
  border: 1px solid var(--border-default);
  border-radius: 9px;
  color: var(--text-primary);
  background: var(--bg-elevated);
  padding: 0 11px;
}
.primary-action,
.row-actions button {
  min-height: 39px;
  border: 1.5px solid var(--text-primary);
  border-radius: 999px;
  color: var(--text-on-brand);
  background: var(--brand);
  padding: 0 13px;
  font-weight: 740;
  cursor: pointer;
}
.campaign-builder,
.campaign-table-card {
  min-width: 0;
  border: 1px solid var(--border-subtle);
  border-radius: 18px;
  background: var(--bg-elevated);
  padding: clamp(16px, 2.5vw, 23px);
}
.campaign-builder {
  border-width: 1.5px;
  box-shadow: 5px 5px 0 var(--text-primary);
}
.campaign-builder h2,
.campaign-table-card h2 {
  margin: 0;
  font-size: 1.05rem;
}
.campaign-builder header > span,
.campaign-table-card header > span {
  color: var(--text-tertiary);
  font-family: var(--font-mono);
  font-size: 0.67rem;
}
.campaign-fields {
  display: grid;
  grid-template-columns: repeat(2, minmax(0, 1fr));
  gap: 13px;
  margin: 18px 0;
}
.campaign-fields label {
  display: grid;
  gap: 6px;
  font-size: 0.78rem;
  font-weight: 700;
}
.campaign-fields label span small {
  color: var(--text-tertiary);
  font-weight: 500;
}
.campaign-fields .wide {
  grid-column: 1 / -1;
}
.campaign-notice {
  margin: 0;
  border: 1px solid var(--success-500);
  border-radius: 10px;
  color: var(--success-700);
  background: var(--success-50);
  padding: 10px 12px;
  font-size: 0.8rem;
}
.campaign-totals {
  display: grid;
  grid-template-columns: repeat(4, minmax(0, 1fr));
  gap: 10px;
}
.campaign-totals article {
  display: grid;
  gap: 3px;
  border: 1px solid var(--border-subtle);
  border-radius: 14px;
  background: var(--bg-elevated);
  padding: 16px;
}
.campaign-totals span,
.campaign-totals small {
  color: var(--text-tertiary);
  font-size: 0.7rem;
}
.campaign-totals strong {
  color: var(--text-primary);
  font-size: 1.65rem;
  letter-spacing: -0.04em;
}
.campaign-table-card > header {
  margin-bottom: 12px;
}
.campaign-table-wrap {
  overflow-x: auto;
}
table {
  width: 100%;
  min-width: 800px;
  border-collapse: collapse;
  font-size: 0.8rem;
  font-variant-numeric: tabular-nums;
}
th,
td {
  border-top: 1px solid var(--border-subtle);
  padding: 11px;
  text-align: right;
}
th:first-child,
td:first-child,
th:nth-child(2),
td:nth-child(2) {
  text-align: left;
}
thead th {
  border-top: 0;
  color: var(--text-tertiary);
  font-size: 0.65rem;
  text-transform: uppercase;
}
tbody th {
  display: grid;
  gap: 2px;
  color: var(--text-primary);
}
tbody th code {
  color: var(--text-tertiary);
  font-size: 0.65rem;
  font-weight: 500;
}
.row-actions {
  display: flex;
  justify-content: flex-end;
  gap: 6px;
}
.row-actions button {
  min-height: 31px;
  color: var(--text-primary);
  background: var(--bg-subtle);
  font-size: 0.68rem;
}
.row-actions .danger-action {
  color: #fff;
  background: var(--error-600);
}
.campaign-state {
  display: grid;
  min-height: 260px;
  place-items: center;
  align-content: center;
  gap: 8px;
  color: var(--text-tertiary);
  text-align: center;
}
.campaign-help {
  justify-content: flex-start;
  flex-wrap: wrap;
  border: 1px solid var(--border-subtle);
  border-radius: 13px;
  background: var(--bg-accent-subtle);
  padding: 13px;
  font-size: 0.75rem;
}
.campaign-help code {
  border-radius: 7px;
  color: #f7f7f2;
  background: #111214;
  padding: 8px 10px;
}
.campaign-help span {
  color: var(--text-tertiary);
}
@media (max-width: 820px) {
  .campaign-toolbar {
    align-items: stretch;
    flex-direction: column;
  }
  .campaign-toolbar-actions {
    justify-content: space-between;
  }
  .campaign-totals {
    grid-template-columns: repeat(2, 1fr);
  }
}
@media (max-width: 560px) {
  .campaign-toolbar-actions,
  .campaign-fields {
    display: grid;
    grid-template-columns: 1fr;
  }
  .campaign-fields .wide {
    grid-column: auto;
  }
  .campaign-totals {
    grid-template-columns: 1fr;
  }
}
</style>
