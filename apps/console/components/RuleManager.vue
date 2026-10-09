<script setup lang="ts">
import {
  ruleDraftFromApi,
  rulePayload,
  type RuleDraft,
  type TrackingRule,
} from "~/types/rules";
import { apiErrorMessage as requestError } from "~/utils/apiError";
import { routes } from "~/utils/routes";

const props = withDefaults(
  defineProps<{
    readOnly?: boolean;
    siteId?: string;
    siteName?: string;
  }>(),
  {
    readOnly: false,
    siteId: undefined,
    siteName: undefined,
  },
);
const { api } = useApi();

const rules = ref<TrackingRule[]>([]);
const loading = ref(false);
const mutationPending = ref(false);
const errorMessage = ref("");
const notice = ref("");
const editingRule = shallowRef<TrackingRule | null>(null);
const confirmDeleteId = ref("");
let requestController: AbortController | undefined;
let siteContext = 0;
let mutationOperation = 0;

watch(
  () => props.siteId,
  () => {
    siteContext += 1;
    mutationOperation += 1;
    requestController?.abort();
    rules.value = [];
    errorMessage.value = "";
    notice.value = "";
    editingRule.value = null;
    confirmDeleteId.value = "";
    loading.value = false;
    mutationPending.value = false;
    void loadRules();
  },
  { immediate: true },
);

onBeforeUnmount(() => requestController?.abort());

async function loadRules() {
  if (!props.siteId) {
    requestController?.abort();
    requestController = undefined;
    rules.value = [];
    loading.value = false;
    errorMessage.value = "";
    return;
  }
  requestController?.abort();
  const controller = new AbortController();
  const siteId = props.siteId;
  const context = siteContext;
  requestController = controller;
  loading.value = true;
  errorMessage.value = "";

  try {
    const response = await api<TrackingRule[]>(routes.sites.rules(siteId), {
      signal: controller.signal,
    });
    if (isCurrentSite(siteId, context)) rules.value = response;
  } catch (cause) {
    if (controller.signal.aborted || !isCurrentSite(siteId, context)) return;
    rules.value = [];
    errorMessage.value = requestError(
      cause,
      "Tracking rules could not be loaded.",
    );
  } finally {
    if (requestController === controller && isCurrentSite(siteId, context)) {
      requestController = undefined;
      loading.value = false;
    }
  }
}

async function createRule(draft: RuleDraft) {
  const siteId = props.siteId;
  if (!siteId || props.readOnly) return;
  await mutateRule("created", siteId, () =>
    api(routes.sites.rules(siteId), {
      body: rulePayload(draft),
      method: "POST",
    }),
  );
}

async function updateRule(draft: RuleDraft) {
  const siteId = props.siteId;
  if (!siteId || !editingRule.value || props.readOnly) return;
  const ruleId = editingRule.value.id;
  const saved = await mutateRule("saved", siteId, () =>
    api(routes.sites.rule(siteId, ruleId), {
      body: rulePayload(draft),
      method: "PUT",
    }),
  );
  if (saved) editingRule.value = null;
}

async function toggleRule(rule: TrackingRule) {
  const siteId = props.siteId;
  if (!siteId || props.readOnly) return;
  const action = rule.enabled ? "paused" : "enabled";
  await mutateRule(action, siteId, () =>
    api(routes.sites.rule(siteId, rule.id), {
      body: { enabled: !rule.enabled },
      method: "PUT",
    }),
  );
}

async function deleteRule(rule: TrackingRule) {
  const siteId = props.siteId;
  if (!siteId || props.readOnly) return;
  const removed = await mutateRule("deleted", siteId, () =>
    api(routes.sites.rule(siteId, rule.id), {
      method: "DELETE",
    }),
  );
  if (removed) confirmDeleteId.value = "";
}

async function mutateRule(
  action: string,
  siteId: string,
  request: () => Promise<unknown>,
) {
  const context = siteContext;
  const operation = ++mutationOperation;
  mutationPending.value = true;
  errorMessage.value = "";
  notice.value = "";
  try {
    await request();
    if (!isCurrentSite(siteId, context)) return false;
    notice.value = `Tracking rule ${action}.`;
    await loadRules();
    return true;
  } catch (cause) {
    if (!isCurrentSite(siteId, context)) return false;
    errorMessage.value = requestError(
      cause,
      `The tracking rule could not be ${action}.`,
    );
    return false;
  } finally {
    if (operation === mutationOperation && isCurrentSite(siteId, context)) {
      mutationPending.value = false;
    }
  }
}

function isCurrentSite(siteId: string, context: number) {
  return context === siteContext && props.siteId === siteId;
}

function domEvent(rule: TrackingRule) {
  return rule.type === "view"
    ? "view"
    : (rule.trigger_config.dom_event ?? rule.type);
}

function eventLabel(rule: TrackingRule) {
  const event = domEvent(rule);
  const labels: Record<string, string> = {
    change: "Field changed",
    focus: "Focus entered",
    blur: "Focus left",
    keydown: "Key down",
    play: "Media play",
    ended: "Media completed",
    click: "Click",
    dblclick: "Double click",
    keypress: "Key press",
    keyup: "Key up",
    mousedown: "Mouse down",
    mouseup: "Mouse up",
    submit: "Form submit",
    view: "Scroll to section / element",
  };
  return labels[event] ?? event;
}

function selectorLabel(rule: TrackingRule) {
  const draft = ruleDraftFromApi(rule);
  return `${draft.selectorType === "tracking" ? "TRACKING ATTRIBUTE" : draft.selectorType.toUpperCase()} · ${draft.selector}`;
}
</script>

<template>
  <section class="rules-workspace" aria-labelledby="rules-heading">
    <header class="rules-intro">
      <div>
        <p class="panel-kicker">Server-defined browser signals</p>
        <h2 id="rules-heading">Track intent without redeploying the app.</h2>
        <p>
          Rules are stored by the API and downloaded by the optional SDK rules
          module. Nothing is inferred, and every custom field is deliberately
          bounded. Rules start collecting after they are enabled; they do not
          recover activity from before publication.
        </p>
      </div>
      <span v-if="readOnly" class="status-badge neutral">Read-only app</span>
    </header>

    <RuleEditor
      v-if="!editingRule"
      :busy="mutationPending"
      :read-only="readOnly"
      @submit="createRule"
    />
    <RuleEditor
      v-else
      :key="editingRule.id"
      :busy="mutationPending"
      :initial-draft="ruleDraftFromApi(editingRule)"
      mode="edit"
      :read-only="readOnly"
      @cancel="editingRule = null"
      @submit="updateRule"
    />

    <article class="management-card management-card-wide rules-list-card">
      <header class="management-card-header">
        <div>
          <p class="panel-kicker">Lifetime signals</p>
          <h2>{{ siteName ? `${siteName} rules` : "Saved rules" }}</h2>
        </div>
        <button
          class="text-button"
          :disabled="loading"
          type="button"
          @click="loadRules"
        >
          Refresh
        </button>
      </header>

      <p v-if="notice" class="inline-notice" role="status">{{ notice }}</p>
      <p v-if="errorMessage" class="inline-error" role="alert">
        {{ errorMessage }}
      </p>

      <div v-if="loading" class="management-loading" role="status">
        <span class="state-orbit" aria-hidden="true"></span>
        <span>Loading tracking rules…</span>
      </div>
      <div v-else-if="rules.length" class="rule-card-list">
        <article v-for="rule in rules" :key="rule.id" class="rule-card">
          <div class="rule-card-main">
            <div class="resource-title-line">
              <strong>{{ rule.name }}</strong>
              <span
                :class="['status-badge', rule.enabled ? 'success' : 'neutral']"
              >
                {{ rule.enabled ? "Enabled" : "Paused" }}
              </span>
            </div>
            <p v-if="rule.description">{{ rule.description }}</p>
            <code>{{ selectorLabel(rule) }}</code>
          </div>
          <dl class="rule-card-metrics">
            <div>
              <dt>Emits</dt>
              <dd>{{ rule.metadata.event_name || rule.name }}</dd>
            </div>
            <div>
              <dt>On</dt>
              <dd>{{ eventLabel(rule) }}</dd>
            </div>
            <div>
              <dt>Page</dt>
              <dd>{{ rule.trigger_config.page_path || "Everywhere" }}</dd>
            </div>
            <div class="rule-lifetime-count">
              <dt>Events tracked · all time</dt>
              <dd>{{ rule.lifetime_event_count.toLocaleString() }}</dd>
            </div>
          </dl>
          <div
            v-if="confirmDeleteId === rule.id"
            aria-live="assertive"
            class="confirm-panel"
            role="alertdialog"
          >
            <p>
              Delete <strong>{{ rule.name }}</strong
              >? Historical events stay in analytics; future SDK deliveries stop
              including this rule.
            </p>
            <div class="confirm-actions">
              <button
                autofocus
                class="button danger compact"
                :disabled="mutationPending"
                type="button"
                @click="deleteRule(rule)"
              >
                Delete rule
              </button>
              <button
                class="button secondary compact"
                :disabled="mutationPending"
                type="button"
                @click="confirmDeleteId = ''"
              >
                Cancel
              </button>
            </div>
          </div>
          <div v-else class="management-actions">
            <button
              class="button secondary compact rule-icon-action"
              :disabled="mutationPending || readOnly"
              type="button"
              :aria-label="`${rule.enabled ? 'Pause' : 'Enable'} ${rule.name}`"
              :title="rule.enabled ? 'Pause' : 'Enable'"
              @click="toggleRule(rule)"
            >
              <svg aria-hidden="true" viewBox="0 0 24 24" fill="currentColor">
                <path v-if="rule.enabled" d="M6 4h4v16H6zM14 4h4v16h-4z" />
                <path v-else d="m7 4 14 8-14 8z" />
              </svg>
            </button>
            <button
              class="button secondary compact rule-icon-action"
              :disabled="readOnly"
              type="button"
              :aria-label="`Edit ${rule.name}`"
              title="Edit"
              @click="editingRule = rule"
            >
              <svg
                aria-hidden="true"
                viewBox="0 0 24 24"
                fill="none"
                stroke="currentColor"
                stroke-width="2"
              >
                <path d="m16 3 5 5-12 12-6 1 1-6zM14 5l5 5" />
              </svg>
            </button>
            <button
              class="button danger-quiet compact rule-icon-action"
              :disabled="readOnly"
              type="button"
              :aria-label="`Delete ${rule.name}`"
              title="Delete"
              @click="confirmDeleteId = rule.id"
            >
              <svg
                aria-hidden="true"
                viewBox="0 0 24 24"
                fill="none"
                stroke="currentColor"
                stroke-width="2"
              >
                <path d="M3 6h18M9 6V3h6v3M6 6l1 15h10l1-15M10 10v7M14 10v7" />
              </svg>
            </button>
          </div>
        </article>
      </div>
      <div v-else class="management-empty">
        <strong>No tracking rules yet</strong>
        <p>
          Create one above. The SDK will not guess interactions on your behalf.
        </p>
      </div>
    </article>
  </section>
</template>

<style scoped>
.management-actions {
  flex-direction: row;
  align-items: center;
  flex-wrap: wrap;
}

.rule-icon-action {
  width: 40px;
  height: 40px;
  padding: 9px;
  flex: 0 0 40px;
}
.rule-icon-action svg {
  width: 20px;
  height: 20px;
}
</style>
