<script setup lang="ts">
import { apiErrorMessage as requestError } from "~/utils/apiError";
import { routes } from "~/utils/routes";
import { browserTimezone } from "~/utils/timezones";

type ManagedSite = {
  created_at: string;
  domain: string;
  id: string;
  name: string;
  organization_id?: string | null;
  public_key?: string | null;
  timezone: string;
  tracking_id: string;
  updated_at: string;
};

type SiteDomain = {
  created_at: string;
  domain: string;
  id: string;
  is_primary: boolean;
};

const props = withDefaults(
  defineProps<{
    selectedSite: ManagedSite | null;
    canManage?: boolean;
    showCreate?: boolean;
    showEdit?: boolean;
  }>(),
  { canManage: true, showCreate: true, showEdit: true },
);

const emit = defineEmits<{
  changed: [trackingId?: string];
}>();
const { api } = useApi();

const createName = ref("");
const createDomain = ref("");
const createTimezone = ref("UTC");
onMounted(() => {
  createTimezone.value = browserTimezone();
});
const createPending = ref(false);
const createError = ref("");
const createNotice = ref("");

const editDomain = ref("");
const editTimezone = ref("UTC");
const editPending = ref(false);
const editError = ref("");
const editNotice = ref("");

const domains = ref<SiteDomain[]>([]);
const domainsPending = ref(false);
const domainsError = ref("");
const domainsNotice = ref("");
const newDomain = ref("");
const domainMutationPending = ref(false);
const confirmDomainId = ref("");
let domainsController: AbortController | undefined;
let siteContext = 0;

watch(
  () => props.selectedSite,
  () => {
    siteContext += 1;
    domainsController?.abort();
    const site = props.selectedSite;
    editDomain.value = site?.domain ?? "";
    editTimezone.value = site?.timezone ?? "UTC";
    editError.value = "";
    editNotice.value = "";
    domains.value = [];
    domainsError.value = "";
    domainsNotice.value = "";
    confirmDomainId.value = "";
    domainsPending.value = false;
    domainMutationPending.value = false;
    editPending.value = false;
    if (site && props.canManage && props.showEdit) void loadDomains();
  },
  { immediate: true },
);

onBeforeUnmount(() => domainsController?.abort());

async function createSite() {
  if (createPending.value) return;
  createError.value = "";
  createNotice.value = "";
  createPending.value = true;

  try {
    const body: { domain?: string; name: string; timezone: string } = {
      name: createName.value,
      timezone: createTimezone.value,
    };
    if (createDomain.value.trim()) body.domain = createDomain.value;
    const site = await api<ManagedSite>(routes.sites.list, {
      body,
      method: "POST",
    });
    createName.value = "";
    createDomain.value = "";
    createNotice.value = `${site.name} was created and selected.`;
    emit("changed", site.tracking_id);
  } catch (error) {
    createError.value = requestError(
      error,
      "The site could not be created. Check the fields.",
    );
  } finally {
    createPending.value = false;
  }
}

async function updateSite() {
  const selectedSite = props.selectedSite;
  if (!selectedSite || editPending.value) return;
  const context = siteContext;

  editError.value = "";
  editNotice.value = "";
  editPending.value = true;

  try {
    const body: { domain?: string; timezone: string } = {
      timezone: editTimezone.value,
    };
    if (editDomain.value.trim() || selectedSite.domain.trim()) {
      body.domain = editDomain.value;
    }
    const site = await api<ManagedSite>(routes.sites.one(selectedSite.id), {
      body,
      method: "PUT",
    });
    if (!isCurrentSite(selectedSite.id, context)) return;
    editDomain.value = site.domain;
    editTimezone.value = site.timezone;
    editNotice.value = "App details were saved.";
    emit("changed", site.tracking_id);
    await loadDomains();
  } catch (error) {
    if (!isCurrentSite(selectedSite.id, context)) return;
    editError.value = requestError(
      error,
      "The site settings could not be saved.",
    );
  } finally {
    if (isCurrentSite(selectedSite.id, context)) editPending.value = false;
  }
}

async function loadDomains() {
  if (!props.selectedSite) return;
  const siteId = props.selectedSite.id;
  const context = siteContext;
  domainsController?.abort();
  const controller = new AbortController();
  domainsController = controller;

  domainsError.value = "";
  domainsPending.value = true;

  try {
    const response = await api<SiteDomain[]>(routes.sites.domains(siteId), {
      signal: controller.signal,
    });
    if (isCurrentSite(siteId, context)) domains.value = response;
  } catch (error) {
    if (controller.signal.aborted || !isCurrentSite(siteId, context)) return;
    domains.value = [];
    domainsError.value = requestError(
      error,
      "Allowed domains could not be loaded.",
    );
  } finally {
    if (domainsController === controller) domainsController = undefined;
    if (isCurrentSite(siteId, context)) domainsPending.value = false;
  }
}

async function addDomain() {
  const selectedSite = props.selectedSite;
  if (!selectedSite || domainMutationPending.value) return;
  const context = siteContext;

  domainsError.value = "";
  domainsNotice.value = "";
  domainMutationPending.value = true;

  try {
    await api(routes.sites.domains(selectedSite.id), {
      body: { domain: newDomain.value },
      method: "POST",
    });
    if (!isCurrentSite(selectedSite.id, context)) return;
    newDomain.value = "";
    domainsNotice.value = "Allowed domain added.";
    await loadDomains();
  } catch (error) {
    if (!isCurrentSite(selectedSite.id, context)) return;
    domainsError.value = requestError(
      error,
      "The allowed domain could not be added.",
    );
  } finally {
    if (isCurrentSite(selectedSite.id, context)) {
      domainMutationPending.value = false;
    }
  }
}

async function deleteDomain(domain: SiteDomain) {
  const selectedSite = props.selectedSite;
  if (!selectedSite || domain.is_primary || domainMutationPending.value) return;
  const context = siteContext;

  domainsError.value = "";
  domainsNotice.value = "";
  domainMutationPending.value = true;

  try {
    await api(routes.sites.domain(selectedSite.id, domain.id), {
      method: "DELETE",
    });
    if (!isCurrentSite(selectedSite.id, context)) return;
    confirmDomainId.value = "";
    domainsNotice.value = `${domain.domain} was removed.`;
    await loadDomains();
  } catch (error) {
    if (!isCurrentSite(selectedSite.id, context)) return;
    domainsError.value = requestError(
      error,
      "The allowed domain could not be removed.",
    );
  } finally {
    if (isCurrentSite(selectedSite.id, context)) {
      domainMutationPending.value = false;
    }
  }
}

function isCurrentSite(siteId: string, context: number) {
  return context === siteContext && props.selectedSite?.id === siteId;
}
</script>

<template>
  <div class="management-grid">
    <article
      v-if="showCreate"
      class="management-card"
      :class="{ 'management-card-wide': !showEdit }"
    >
      <header class="management-card-header">
        <div>
          <p class="panel-kicker">Workspace</p>
          <h3>Create an app</h3>
        </div>
      </header>
      <p class="management-copy">
        An app receives its own tracking ID, rules, and API keys. Add production
        domains now or whenever the app is ready.
      </p>
      <form class="management-form" @submit.prevent="createSite">
        <div class="form-grid">
          <label>
            <span>App name</span>
            <input
              v-model="createName"
              autocomplete="off"
              maxlength="120"
              placeholder="Marketing website"
              required
            />
          </label>
          <label>
            <span>Primary domain <small>optional</small></span>
            <input
              v-model="createDomain"
              autocomplete="off"
              inputmode="url"
              placeholder="example.com"
              spellcheck="false"
            />
          </label>
          <TimezoneSelect v-model="createTimezone" />
        </div>
        <button class="button primary" :disabled="createPending" type="submit">
          {{ createPending ? "Creating…" : "Create app" }}
        </button>
      </form>
      <p v-if="createNotice" class="inline-notice" role="status">
        {{ createNotice }}
      </p>
      <p v-if="createError" class="inline-error" role="alert">
        {{ createError }}
      </p>
    </article>

    <article v-if="canManage && showEdit" class="management-card">
      <header class="management-card-header">
        <div>
          <p class="panel-kicker">Selected app</p>
          <h3>Domain and timezone</h3>
        </div>
        <code v-if="selectedSite" class="resource-id">{{
          selectedSite.tracking_id
        }}</code>
      </header>

      <form
        v-if="selectedSite"
        class="management-form"
        @submit.prevent="updateSite"
      >
        <div class="form-grid">
          <label>
            <span>Primary domain</span>
            <input
              v-model="editDomain"
              inputmode="url"
              placeholder="Optional until ready"
              :required="Boolean(selectedSite.domain)"
              spellcheck="false"
            />
          </label>
          <TimezoneSelect v-model="editTimezone" />
        </div>
        <p class="field-help">
          Changing the primary domain keeps it in the allowed-domain list.
        </p>
        <button class="button primary" :disabled="editPending" type="submit">
          {{ editPending ? "Saving…" : "Save app details" }}
        </button>
      </form>
      <div v-else class="management-empty">
        <strong>No app selected</strong>
        <p>Create an app or select one above to edit its settings.</p>
      </div>
      <p v-if="editNotice" class="inline-notice" role="status">
        {{ editNotice }}
      </p>
      <p v-if="editError" class="inline-error" role="alert">{{ editError }}</p>
    </article>

    <article
      v-if="canManage && showEdit"
      class="management-card management-card-wide"
    >
      <header class="management-card-header">
        <div>
          <p class="panel-kicker">Browser origins</p>
          <h3>Allowed domains</h3>
        </div>
        <button
          v-if="selectedSite"
          class="text-button"
          :disabled="domainsPending"
          type="button"
          @click="loadDomains"
        >
          Refresh
        </button>
      </header>
      <p class="management-copy">
        Event tracking and rule delivery allow all browser origins when no
        domains are configured. Otherwise, an origin must match one of the
        configured domains. Wildcards such as
        <code>*.example.com</code> are accepted for additional domains.
      </p>

      <template v-if="selectedSite">
        <form class="inline-form" @submit.prevent="addDomain">
          <label>
            <span>Additional domain</span>
            <input
              v-model="newDomain"
              autocomplete="off"
              inputmode="url"
              placeholder="app.example.com"
              required
              spellcheck="false"
            />
          </label>
          <button
            class="button secondary"
            :disabled="domainMutationPending"
            type="submit"
          >
            {{ domainMutationPending ? "Adding…" : "Add domain" }}
          </button>
        </form>

        <div v-if="domainsPending" class="management-loading" role="status">
          <span class="state-orbit" aria-hidden="true"></span>
          <span>Loading allowed domains…</span>
        </div>
        <div v-else-if="domains.length" class="resource-list">
          <div v-for="domain in domains" :key="domain.id" class="resource-row">
            <div class="resource-main">
              <strong>{{ domain.domain }}</strong>
              <span>{{
                domain.is_primary ? "Primary domain" : "Allowed domain"
              }}</span>
            </div>
            <span v-if="domain.is_primary" class="status-badge success"
              >Primary</span
            >
            <div
              v-else-if="confirmDomainId === domain.id"
              aria-live="assertive"
              class="confirm-actions"
              role="alertdialog"
            >
              <span>Remove this domain?</span>
              <button
                autofocus
                class="button danger compact"
                :disabled="domainMutationPending"
                type="button"
                @click="deleteDomain(domain)"
              >
                Remove
              </button>
              <button
                class="button secondary compact"
                :disabled="domainMutationPending"
                type="button"
                @click="confirmDomainId = ''"
              >
                Cancel
              </button>
            </div>
            <button
              v-else
              class="button danger-quiet compact"
              :disabled="domainMutationPending"
              type="button"
              @click="confirmDomainId = domain.id"
            >
              Remove
            </button>
          </div>
        </div>
        <div v-else-if="domainsError" class="management-empty">
          <strong>Allowed domains unavailable</strong>
          <p>Refresh to check which browser origins are allowed.</p>
        </div>
        <div v-else-if="selectedSite.domain" class="management-empty">
          <strong>Primary domain allowed</strong>
          <p>{{ selectedSite.domain }} is allowed. Add more domains above.</p>
        </div>
        <div v-else class="management-empty">
          <strong>All browser origins allowed</strong>
          <p>Add domains above to restrict event tracking and rule delivery.</p>
        </div>
      </template>
      <div v-else class="management-empty">
        <strong>No app selected</strong>
        <p>Select an app to manage its browser origins.</p>
      </div>

      <p v-if="domainsNotice" class="inline-notice" role="status">
        {{ domainsNotice }}
      </p>
      <p v-if="domainsError" class="inline-error" role="alert">
        {{ domainsError }}
      </p>
    </article>
  </div>
</template>
