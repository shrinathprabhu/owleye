<script setup lang="ts">
import type {
  UptimeHistory,
  UptimeMonitor,
  UptimeSummary,
} from "~/types/uptime";
import { apiErrorMessage } from "~/utils/apiError";
import { routes } from "~/utils/routes";

const props = defineProps<{
  siteId?: string;
  demo?: boolean;
  canManage?: boolean;
}>();
const { api } = useApi();
const summary = ref<UptimeSummary | null>(null);
const history = ref<UptimeHistory | null>(null);
const selectedId = ref("");
const name = ref("");
const url = ref("");
const loading = ref(false);
const historyLoading = ref(false);
const pending = ref(false);
const error = ref("");
const historyError = ref("");
const notice = ref("");
const confirmDelete = ref("");
const selected = computed(() =>
  summary.value?.monitors.find((m) => m.id === selectedId.value),
);
let context = 0;
let controller = new AbortController();
let historyRequest = 0;
let timer: ReturnType<typeof setInterval> | undefined;
const labels: Record<string, string> = {
  up: "Up",
  down: "Down",
  unknown: "Awaiting check",
  paused: "Paused",
  disabled: "Disabled",
};
const reasons: Record<string, string> = {
  http_status: "Unexpected HTTP status",
  dns: "DNS lookup failed",
  timeout: "Request timed out",
  connection_or_tls: "Connection or TLS failed",
  blocked_destination: "Destination blocked: public URLs only",
  checker_unavailable: "Checker unavailable",
  recovered: "Recovered",
  paused: "Monitoring paused",
  monitoring_suspended: "Monitoring suspended",
};
const formatTime = (value: number | null) =>
  value === null ? "Not checked yet" : new Date(value * 1000).toLocaleString();
const reason = (value: string | null) =>
  value ? reasons[value] || value : "HTTP 200";

watch(
  () => [props.siteId, props.demo],
  () => {
    context++;
    controller.abort();
    controller = new AbortController();
    historyRequest++;
    summary.value = null;
    history.value = null;
    selectedId.value = "";
    confirmDelete.value = "";
    error.value = "";
    historyError.value = "";
    notice.value = "";
    name.value = "";
    url.value = "";
    pending.value = false;
    loading.value = false;
    historyLoading.value = false;
    void load();
  },
  { immediate: true },
);
onMounted(() => {
  timer = setInterval(() => {
    if (!document.hidden && !pending.value && !loading.value) void load(true);
  }, 30_000);
});
onBeforeUnmount(() => {
  context++;
  controller.abort();
  clearInterval(timer);
});

async function load(quiet = false) {
  if (!props.siteId || props.demo) return;
  const current = context;
  const signal = controller.signal;
  if (!quiet) loading.value = true;
  try {
    const result = await api<UptimeSummary>(routes.sites.uptime(props.siteId), {
      signal,
    });
    if (current !== context) return;
    summary.value = result;
    error.value = "";
    if (
      selectedId.value &&
      !result.monitors.some((m) => m.id === selectedId.value)
    ) {
      selectedId.value = "";
      history.value = null;
    }
    if (selectedId.value) void showHistory(selectedId.value, true);
  } catch (cause) {
    if (current === context && !signal.aborted)
      error.value = apiErrorMessage(
        cause,
        "Uptime monitors could not be loaded. Displayed results may be out of date.",
      );
  } finally {
    if (current === context) loading.value = false;
  }
}
async function showHistory(id: string, quiet = false) {
  if (!props.siteId) return;
  const current = context;
  const request = ++historyRequest;
  const signal = controller.signal;
  if (selectedId.value !== id) {
    history.value = null;
    historyError.value = "";
  }
  selectedId.value = id;
  if (!quiet) historyLoading.value = true;
  try {
    const result = await api<UptimeHistory>(
      routes.sites.uptimeHistory(props.siteId, id),
      { signal },
    );
    if (current === context && request === historyRequest) {
      history.value = result;
      historyError.value = "";
    }
  } catch (cause) {
    if (current === context && request === historyRequest && !signal.aborted)
      historyError.value = apiErrorMessage(
        cause,
        "Check history could not be loaded.",
      );
  } finally {
    if (current === context && request === historyRequest)
      historyLoading.value = false;
  }
}
async function mutate(
  action: "create" | "update" | "delete",
  monitor?: UptimeMonitor,
  body?: object,
) {
  if (!props.siteId || !props.canManage || props.demo || pending.value) return;
  const current = context;
  const signal = controller.signal;
  const siteId = props.siteId;
  pending.value = true;
  error.value = "";
  notice.value = "";
  try {
    await api(
      action === "create"
        ? routes.sites.uptime(siteId)
        : routes.sites.uptimeMonitor(siteId, monitor!.id),
      {
        method:
          action === "create" ? "POST" : action === "delete" ? "DELETE" : "PUT",
        body:
          action === "create"
            ? {
                name: name.value,
                url: url.value,
              }
            : body,
        signal,
      },
    );
    if (current !== context) return;
    if (action === "create") {
      name.value = "";
      url.value = "";
    }
    confirmDelete.value = "";
    notice.value =
      action === "create"
        ? "Monitor enabled. The first check will run shortly."
        : action === "delete"
          ? "Monitor deleted."
          : "Monitor updated.";
    await load(true);
  } catch (cause) {
    if (current === context && !signal.aborted)
      error.value = apiErrorMessage(cause, "The monitor could not be saved.");
  } finally {
    if (current === context) pending.value = false;
  }
}
</script>

<template>
  <section class="uptime-panel" aria-label="Uptime monitors">
    <p v-if="demo" class="uptime-note">
      Uptime monitoring is unavailable in a preview.
    </p>
    <template v-else>
      <div class="uptime-toolbar">
        <div>
          <h2>Availability monitors</h2>
          <p>HEAD requests · HTTP 200 required · every 5 minutes</p>
        </div>
        <button
          class="button secondary compact"
          :disabled="loading || pending"
          @click="load()"
        >
          {{ loading ? "Refreshing…" : "Refresh" }}
        </button>
      </div>
      <p v-if="error" class="page-alert" role="alert">{{ error }}</p>
      <p v-if="notice" class="uptime-note" role="status">{{ notice }}</p>
      <p v-if="loading && !summary" role="status">Loading monitors…</p>
      <template v-if="summary">
        <p class="uptime-meta">{{ summary.used }} monitors · {{ summary.history_days }}-day history</p>
        <form
          v-if="canManage && summary.limit"
          class="uptime-form"
          @submit.prevent="mutate('create')"
        >
          <label
            >Monitor name<input
              v-model="name"
              required
              maxlength="100"
              placeholder="Production API"
              :disabled="pending"
          /></label>
          <label class="uptime-url-input"
            >Public URL<input
              v-model="url"
              type="url"
              required
              maxlength="2048"
              placeholder="https://api.example.com/health"
              :disabled="pending"
          /></label>

          <button
            class="button primary"
            :disabled="pending || summary.used >= summary.limit"
          >
            {{ pending ? "Saving…" : "Add monitor" }}
          </button>
          <p class="uptime-form-help">
            Public HTTP/HTTPS on ports 80 or 443. No credentials, query strings,
            or redirects. Use an endpoint that supports HEAD.
          </p>
        </form>
        <p v-if="!summary.monitors.length" class="uptime-empty">
          No monitors for this app yet. Add a website or health endpoint to
          start checking its availability.
        </p>
        <ul v-else class="uptime-list">
          <li
            v-for="monitor in summary.monitors"
            :key="monitor.id"
            class="uptime-monitor"
          >
            <div class="uptime-monitor-heading">
              <h3>{{ monitor.name }}</h3>
              <span class="uptime-status" :data-state="monitor.state">{{
                labels[monitor.state]
              }}</span>
            </div>
            <p class="uptime-url">{{ monitor.url }}</p>
            <p class="uptime-meta">
              Last check: {{ formatTime(monitor.last_checked_at)
              }}<template v-if="monitor.last_checked_at">
                ·
                {{
                  monitor.status_code
                    ? `HTTP ${monitor.status_code}`
                    : reason(monitor.failure)
                }}
                · {{ monitor.duration_ms }} ms</template
              >
            </p>
            <p
              v-if="monitor.state === 'down' && monitor.failure"
              class="uptime-failure"
            >
              {{ reason(monitor.failure)
              }}<template v-if="monitor.status_code === 405"
                >. This endpoint does not support HEAD.</template
              >
            </p>
            <div class="uptime-actions">
              <button
                class="button secondary compact"
                :aria-expanded="selectedId === monitor.id"
                @click="showHistory(monitor.id)"
              >
                Checks &amp; incidents
              </button>
              <template v-if="canManage">
                <button
                  class="button secondary compact"
                  :disabled="pending || (!monitor.enabled && !summary.limit)"
                  @click="
                    mutate('update', monitor, { enabled: !monitor.enabled })
                  "
                >
                  {{ monitor.enabled ? "Pause" : "Enable" }}
                </button>

                <button
                  v-if="confirmDelete !== monitor.id"
                  class="button secondary compact"
                  :disabled="pending"
                  @click="confirmDelete = monitor.id"
                >
                  Delete
                </button>
                <template v-else
                  ><span>Delete this monitor and its incidents?</span
                  ><button
                    class="button secondary compact"
                    :disabled="pending"
                    @click="mutate('delete', monitor)"
                  >
                    Confirm delete</button
                  ><button
                    class="button secondary compact"
                    :disabled="pending"
                    @click="confirmDelete = ''"
                  >
                    Cancel
                  </button></template
                >
              </template>
            </div>
          </li>
        </ul>
        <section v-if="selected" class="uptime-history" aria-live="polite">
          <div class="uptime-toolbar">
            <h2>{{ selected.name }} · history</h2>
            <button
              class="button secondary compact"
              @click="
                selectedId = '';
                historyRequest++;
              "
            >
              Close
            </button>
          </div>
          <p v-if="historyLoading">Loading history…</p>
          <p v-if="historyError" role="alert">{{ historyError }}</p>
          <template v-if="history">
            <p v-if="!history.history_available" role="status">
              Check history is temporarily unavailable. Current status and
              recorded incidents remain visible.
            </p>
            <template v-else>
              <h3>Recent checks</h3>
              <p class="uptime-meta">
                Showing the latest
                {{ Math.min(history.checks.length, 72) }} checks.
                {{ history.checks.length }} recorded in the last
                {{ summary.history_days }} days (up to
                {{ summary.history_days * 288 }}). Gaps are unknown. Five-minute
                checks can miss shorter outages.
              </p>
              <UptimeCheckStrip
                v-if="history.checks.length"
                :checks="history.checks"
              />
              <p v-else>No checks recorded yet.</p>
              <details v-if="history.checks.length">
                <summary>View check details</summary>
                <div
                  class="uptime-table-wrap"
                  tabindex="0"
                  role="region"
                  aria-label="Recent uptime checks"
                >
                  <table>
                    <thead>
                      <tr>
                        <th>Time</th>
                        <th>Result</th>
                        <th>HTTP</th>
                        <th>Response</th>
                      </tr>
                    </thead>
                    <tbody>
                      <tr v-for="(check, index) in history.checks" :key="index">
                        <td>{{ formatTime(check.checked_at) }}</td>
                        <td>
                          {{
                            check.state === "up" ? "Up" : reason(check.failure)
                          }}
                        </td>
                        <td>{{ check.status_code ?? "—" }}</td>
                        <td>{{ check.duration_ms }} ms</td>
                      </tr>
                    </tbody>
                  </table>
                </div>
              </details>
            </template>
            <h3>Incidents</h3>
            <p v-if="!history.incidents.length">
              No incidents recorded in the last {{ summary.history_days }} days.
            </p>
            <ol v-else class="uptime-incidents">
              <li v-for="incident in history.incidents" :key="incident.id">
                <strong
                  >{{ reason(incident.failure)
                  }}{{
                    incident.status_code ? ` (${incident.status_code})` : ""
                  }}</strong
                >
                <p>
                  {{ formatTime(incident.started_at) }} →
                  {{
                    incident.ended_at
                      ? formatTime(incident.ended_at)
                      : "Ongoing"
                  }}
                </p>
                <p v-if="incident.resolution">
                  {{ reason(incident.resolution) }}
                </p>
              </li>
            </ol>
          </template>
        </section>
        <p class="uptime-footnote">
          Checks run from OwlEye’s monitoring server. A failed check is retried
          after 10 seconds before an incident opens. This checks the endpoint’s
          HTTP response; it does not test page rendering or user journeys.
        </p>
      </template>
    </template>
  </section>
</template>

<style scoped>
.uptime-panel {
  min-width: 0;
  display: grid;
  gap: 20px;
}
.uptime-panel h2,
.uptime-panel h3,
.uptime-panel p {
  margin: 0;
}
.uptime-toolbar,
.uptime-monitor-heading,
.uptime-actions {
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  justify-content: space-between;
  gap: 12px;
}
.uptime-toolbar > div {
  display: grid;
  gap: 8px;
}
.uptime-form {
  display: grid;
  grid-template-columns: minmax(150px, 1fr) minmax(200px, 2fr);
  gap: 16px;
  padding: 24px;
  border: 1px solid var(--line, #d5d2c8);
  border-radius: 14px;
}
.uptime-form label {
  display: grid;
  gap: 8px;
  font-weight: 600;
  min-width: 0;
}
.uptime-form input:not([type="checkbox"]) {
  width: 100%;
  min-width: 0;
  padding: 12px;
  border: 1px solid var(--line, #d5d2c8);
  border-radius: 8px;
  background: var(--surface, #fff);
  color: inherit;
  font: inherit;
  box-sizing: border-box;
}
.uptime-form .uptime-checkbox {
  display: flex;
  align-items: center;
  font-weight: 400;
}
.uptime-checkbox input {
  width: 18px;
  height: 18px;
  flex-shrink: 0;
}
.uptime-form-help {
  grid-column: 1 / -1;
}
.uptime-meta,
.uptime-form-help,
.uptime-footnote {
  font-size: 0.82rem;
  line-height: 1.6;
  color: var(--muted, #626760);
}
.uptime-note,
.uptime-empty {
  padding: 20px;
  background: var(--surface-muted, #f1f3eb);
  border: 1px solid var(--line, #d5d2c8);
  border-radius: 12px;
  line-height: 1.6;
}
.uptime-list {
  list-style: none;
  padding: 0;
  margin: 0;
  display: grid;
  gap: 16px;
}
.uptime-monitor {
  display: grid;
  gap: 12px;
  min-width: 0;
  padding: 22px;
  border: 1px solid var(--line, #d5d2c8);
  border-radius: 14px;
}
.uptime-monitor h3,
.uptime-history h2 {
  overflow-wrap: anywhere;
}
.uptime-url {
  overflow-wrap: anywhere;
  font-family: monospace;
  font-size: 0.85rem;
}
.uptime-status {
  border-radius: 100px;
  padding: 5px 12px;
  font-size: 0.75rem;
  font-weight: 700;
  background: #eceee9;
  color: #42483e;
}
.uptime-status[data-state="up"] {
  background: #e1efce;
  color: #315519;
}
.uptime-status[data-state="down"] {
  background: #fde4df;
  color: #8b231d;
}
.uptime-failure {
  color: #8b231d;
  font-size: 0.85rem;
}
.uptime-actions {
  justify-content: flex-start;
}
.uptime-history {
  min-width: 0;
  display: grid;
  gap: 16px;
  padding: 24px;
  border: 1px solid var(--line, #d5d2c8);
  border-radius: 14px;
}
.uptime-table-wrap {
  max-width: 100%;
  overflow: auto;
  max-height: 380px;
  margin-top: 16px;
}
table {
  width: 100%;
  border-collapse: collapse;
  font-size: 0.8rem;
  text-align: left;
}
th,
td {
  padding: 12px;
  border-bottom: 1px solid var(--line, #d5d2c8);
  white-space: nowrap;
}
.uptime-incidents {
  margin: 0;
  padding-left: 20px;
  display: grid;
  gap: 18px;
  font-size: 0.85rem;
  line-height: 1.7;
}
summary {
  cursor: pointer;
}
@media (max-width: 650px) {
  .uptime-form {
    grid-template-columns: minmax(0, 1fr);
    padding: 16px;
  }
  .uptime-monitor,
  .uptime-history {
    padding: 16px;
  }
}
</style>
