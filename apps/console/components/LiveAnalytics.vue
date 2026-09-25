<script setup lang="ts">
import { createLivePoller } from "~/utils/livePolling";
import { apiErrorMessage, apiErrorStatus } from "~/utils/apiError";
import { routes } from "~/utils/routes";
import type { LiveResponse } from "~/types/live";
const props = withDefaults(defineProps<{ siteId?: string; demo?: boolean }>(), {
  demo: false,
  siteId: "",
});
const { api } = useApi();
const data = shallowRef<LiveResponse | null>(null);
const error = ref("");
const pending = ref(false);
const paused = ref(false);
const hidden = ref(false);
const offline = ref(false);
const blocked = ref(false);
const eventDraft = ref("");
const selectedEvent = ref("");
const metric = ref<
  "events" | "pageviews" | "errors" | "custom_events" | "selected_events"
>("events");
const metricLabels = {
  events: "Events",
  pageviews: "Pageviews",
  errors: "Errors",
  custom_events: "Custom and rule events",
  selected_events: "Selected event",
};
const breakdownLabels = {
  events: "Recent event names",
  pages: "Pages",
  countries: "Countries",
  campaigns: "Campaigns",
  referrers: "Referrers",
};
const status = computed(() =>
  offline.value
    ? "Offline"
    : paused.value
      ? "Paused"
      : hidden.value
        ? "Paused while hidden"
        : blocked.value
          ? "Access unavailable"
          : error.value
            ? "Retrying"
            : data.value
              ? "Live"
              : "Connecting",
);
const peak = computed(() =>
  Math.max(
    1,
    ...(data.value?.minutes ?? []).map((point) => point[metric.value]),
  ),
);
const latestTime = computed(() =>
  data.value
    ? new Date(data.value.as_of).toLocaleTimeString()
    : "Not updated yet",
);
let mounted = false;
const poller = createLivePoller<LiveResponse>({
  load: (signal) =>
    api(routes.sites.live(props.siteId), {
      signal,
      timeout: 15000,
      query: selectedEvent.value ? { event: selectedEvent.value } : undefined,
      retry: 0,
    }),
  onData(value) {
    data.value = value;
    error.value = "";
  },
  onError(reason) {
    error.value = apiErrorMessage(
      reason,
      "Recent activity could not be updated. We’ll retry automatically.",
    );
    if ([401, 403].includes(apiErrorStatus(reason) ?? 0)) {
      blocked.value = true;
      data.value = null;
      poller.stop();
    }
  },
  onPending(value) {
    pending.value = value;
  },
});
function sync() {
  if (!mounted) return;
  hidden.value = document.hidden;
  offline.value = !navigator.onLine;
  if (
    props.siteId &&
    !props.demo &&
    !paused.value &&
    !hidden.value &&
    !offline.value &&
    !blocked.value
  )
    poller.start();
  else poller.stop();
}
function restart() {
  poller.stop();
  data.value = null;
  error.value = "";
  blocked.value = false;
  sync();
}
function applyEvent(name = eventDraft.value) {
  eventDraft.value = name;
  selectedEvent.value = name.trim();
}
function time(value: string) {
  return new Date(value).toLocaleTimeString([], {
    hour: "2-digit",
    minute: "2-digit",
  });
}
function countryName(value: string) {
  if (!/^[A-Z]{2}$/.test(value)) return value;
  try {
    return (
      new Intl.DisplayNames(undefined, { type: "region" }).of(value) ?? value
    );
  } catch {
    return value;
  }
}
watch(
  () => [props.siteId, props.demo],
  () => {
    eventDraft.value = "";
    selectedEvent.value = "";
    restart();
  },
);
watch(selectedEvent, () => {
  if (!selectedEvent.value && metric.value === "selected_events")
    metric.value = "events";
  restart();
});
watch(paused, sync);
onMounted(() => {
  mounted = true;
  sync();
  document.addEventListener("visibilitychange", sync);
  window.addEventListener("online", sync);
  window.addEventListener("offline", sync);
});
onBeforeUnmount(() => {
  mounted = false;
  poller.stop();
  document.removeEventListener("visibilitychange", sync);
  window.removeEventListener("online", sync);
  window.removeEventListener("offline", sync);
});
</script>

<template>
  <section v-if="demo" class="live-card">
    <h2>Live activity needs a connected app</h2>
    <p>
      The demo contains sample history. Connect your app to see incoming
      activity here.
    </p>
  </section>
  <div v-else class="live-stack">
    <section class="live-card live-toolbar">
      <div>
        <span class="live-status" :class="{ running: status === 'Live' }"
          >● {{ status }}</span
        >
        <h2>Last 30 minutes</h2>
        <p>Updates about every 10 seconds while this page is visible.</p>
        <p class="live-note">
          Last updated: {{ latestTime }}<span v-if="pending"> · Updating…</span>
        </p>
      </div>
      <button
        class="button secondary compact"
        type="button"
        @click="paused = !paused"
      >
        {{ paused ? "Resume live updates" : "Pause live updates" }}
      </button>
    </section>
    <div v-if="error" class="live-card live-error" role="alert">
      <strong>{{ error }}</strong>
      <p>
        {{
          data
            ? "Showing the last successful snapshot; these counts may be out of date."
            : "Activity is unavailable. This is not a zero-traffic result."
        }}
      </p>
      <button
        v-if="blocked"
        class="button secondary compact"
        type="button"
        @click="
          blocked = false;
          error = '';
          sync();
        "
      >
        Retry access
      </button>
    </div>
    <form class="live-card event-filter" @submit.prevent="applyEvent()">
      <label for="live-event"
        >Follow a specific event <small>optional</small>
        <input
          id="live-event"
          v-model="eventDraft"
          maxlength="120"
          placeholder="signup_completed"
          autocomplete="off"
        />
      </label>
      <button type="submit" class="button secondary compact">
        Apply event
      </button>
      <button
        v-if="selectedEvent"
        type="button"
        class="button secondary compact"
        @click="applyEvent('')"
      >
        Clear
      </button>
      <p>
        Exact event name. This adds its count and chart without filtering the
        traffic totals.
      </p>
    </form>
    <p v-if="!data && !error" role="status">
      {{
        paused
          ? "Resume updates to load recent activity."
          : offline
            ? "Reconnect to load recent activity."
            : "Loading recent activity…"
      }}
    </p>
    <template v-if="data">
      <section class="live-totals" aria-label="Recent activity totals">
        <article class="live-card">
          <span>Active visitors · last 5 min</span
          ><strong>{{ data.totals.active_visitors.toLocaleString() }}</strong
          ><small>Estimated from recent events</small>
        </article>
        <article class="live-card">
          <span>Pageviews</span
          ><strong>{{ data.totals.pageviews.toLocaleString() }}</strong
          ><small>Last 30 minutes</small>
        </article>
        <article class="live-card">
          <span>Custom and rule events</span
          ><strong>{{ data.totals.custom_events.toLocaleString() }}</strong
          ><small>Includes tracked conversions</small>
        </article>
        <article class="live-card">
          <span>Errors</span
          ><strong>{{ data.totals.errors.toLocaleString() }}</strong
          ><small>Recorded error events</small>
        </article>
        <article v-if="data.selected_event" class="live-card">
          <span>{{ data.selected_event }}</span
          ><strong>{{ data.totals.selected_events.toLocaleString() }}</strong
          ><small>Exact matches · last 30 minutes</small>
        </article>
      </section>
      <section class="live-card">
        <header class="live-toolbar">
          <h2>Activity by minute</h2>
          <label
            >Metric
            <select v-model="metric">
              <option
                v-for="(label, key) in metricLabels"
                :key="key"
                :value="key"
                :disabled="key === 'selected_events' && !data.selected_event"
              >
                {{ label }}
              </option>
            </select></label
          >
        </header>
        <p v-if="!data.totals.events" class="live-empty">
          No events in the last 30 minutes. New activity will appear here as it
          arrives. If you expected traffic, check the app’s SDK setup.
        </p>
        <template v-else>
          <svg
            viewBox="0 0 600 150"
            role="img"
            :aria-label="`${metricLabels[metric]} in the last 30 minutes: ${data.totals[metric].toLocaleString()}. One bar per minute.`"
            class="live-chart"
          >
            <line
              x1="0"
              y1="140"
              x2="600"
              y2="140"
              stroke="currentColor"
              opacity="0.25"
            />
            <rect
              v-for="(point, index) in data.minutes"
              :key="point.at"
              :x="index * 20 + 2"
              :y="140 - (point[metric] / peak) * 125"
              width="16"
              :height="(point[metric] / peak) * 125"
              rx="2"
            >
              <title>
                {{ time(point.at) }} · {{ point[metric] }}
                {{ metricLabels[metric].toLowerCase() }}
              </title>
            </rect>
          </svg>
          <div class="chart-axis">
            <span>{{ time(data.start_at) }}</span
            ><span>{{ metricLabels[metric] }} · peak {{ peak }}</span
            ><span>{{ time(data.as_of) }}</span>
          </div>
          <details class="live-note">
            <summary>View minute counts</summary>
            <table>
              <thead>
                <tr>
                  <th>Minute starting</th>
                  <th>{{ metricLabels[metric] }}</th>
                </tr>
              </thead>
              <tbody>
                <tr v-for="point in data.minutes" :key="point.at">
                  <td>{{ time(point.at) }}</td>
                  <td>{{ point[metric] }}</td>
                </tr>
              </tbody>
            </table>
          </details>
        </template>
      </section>
      <div class="live-breakdowns">
        <section
          v-for="(label, key) in breakdownLabels"
          :key="key"
          class="live-card"
          :class="{ 'event-list': key === 'events' }"
        >
          <h2>{{ label }}</h2>
          <p class="live-note">Top 10 · event counts in the last 30 minutes</p>
          <ul v-if="data.breakdowns[key]?.length">
            <li v-for="item in data.breakdowns[key]" :key="item.name">
              <button
                v-if="key === 'events'"
                class="event-name"
                type="button"
                :aria-label="`Follow event ${item.name}`"
                @click="applyEvent(item.name)"
              >
                {{ item.name }}
              </button>
              <span v-else>{{
                key === "countries" ? countryName(item.name) : item.name
              }}</span
              ><strong>{{ item.count.toLocaleString() }}</strong>
            </li>
          </ul>
          <p v-else class="live-empty">No recent activity.</p>
        </section>
      </div>
      <p class="live-note">
        Active visitors means visitors with a recorded event in the last five
        minutes, not everyone with a tab open. Counts can arrive late. “Direct”
        means no referrer was recorded; “No campaign” means no campaign tag was
        recorded. No visitor identifiers or event payloads are shown.
      </p>
    </template>
  </div>
</template>

<style scoped>
.live-stack {
  display: grid;
  gap: 18px;
}
.live-card {
  min-width: 0;
  padding: 24px;
  border: 1px solid var(--border, #d8d2c4);
  border-radius: 18px;
  background: var(--surface, #fffef9);
}
.live-card h2 {
  margin: 8px 0 12px;
  font-size: 1.15rem;
}
.live-card p {
  margin: 8px 0;
  line-height: 1.5;
}
.live-toolbar {
  display: flex;
  align-items: center;
  justify-content: space-between;
  flex-wrap: wrap;
  gap: 16px;
}
.live-status {
  font-size: 0.82rem;
  font-weight: 700;
  color: var(--text-secondary);
}
.live-status.running {
  color: #187643;
}
.live-note,
small {
  color: var(--text-secondary);
  font-size: 0.83rem;
  line-height: 1.6;
}
.live-error {
  border-color: #c23e35;
  background: #fff0ed;
}
.event-filter {
  display: flex;
  flex-wrap: wrap;
  align-items: end;
  gap: 12px;
}
.event-filter label {
  flex: 1 1 240px;
}
.event-filter p {
  flex-basis: 100%;
  font-size: 0.82rem;
  color: var(--text-secondary);
}
input,
select {
  display: block;
  max-width: 100%;
  width: 100%;
  padding: 12px;
  margin-top: 8px;
  border: 1px solid #b9b3a7;
  border-radius: 10px;
  background: var(--surface, #fffef9);
  color: inherit;
  font: inherit;
}
.live-totals {
  display: grid;
  grid-template-columns: repeat(auto-fit, minmax(180px, 1fr));
  gap: 14px;
}
.live-totals article {
  display: grid;
  gap: 10px;
}
.live-totals span {
  font-size: 0.88rem;
  overflow-wrap: anywhere;
}
.live-totals strong {
  font-size: 2.4rem;
  line-height: 1.2;
}
.live-chart {
  display: block;
  width: 100%;
  height: auto;
  min-height: 140px;
  color: var(--text-secondary);
}
.live-chart rect {
  fill: #3d5afe;
}
.chart-axis {
  display: flex;
  justify-content: space-between;
  gap: 8px;
  font-size: 0.78rem;
  color: var(--text-secondary);
}
.live-breakdowns {
  display: grid;
  grid-template-columns: repeat(2, minmax(0, 1fr));
  gap: 18px;
}
.event-list {
  grid-column: 1/-1;
}
ul {
  list-style: none;
  padding: 0;
  margin: 16px 0 0;
}
li {
  display: flex;
  justify-content: space-between;
  align-items: baseline;
  gap: 18px;
  padding: 12px 0;
  border-bottom: 1px solid #e9e5dc;
}
li:last-child {
  border: 0;
}
li span,
.event-name {
  overflow-wrap: anywhere;
  min-width: 0;
}
.event-name {
  padding: 0;
  border: 0;
  background: none;
  font: inherit;
  color: #3148d9;
  text-align: left;
  text-decoration: underline;
  cursor: pointer;
}
table {
  width: 100%;
  text-align: left;
  margin-top: 12px;
}
td,
th {
  padding: 8px;
}
summary {
  cursor: pointer;
  margin-top: 16px;
}
.live-empty {
  padding: 18px 0;
  color: var(--text-secondary);
}
@media (max-width: 640px) {
  .live-card {
    padding: 18px;
  }
  .live-breakdowns {
    grid-template-columns: minmax(0, 1fr);
  }
  .live-totals {
    grid-template-columns: repeat(2, minmax(0, 1fr));
  }
  .live-totals .live-card {
    padding: 14px;
  }
  .live-totals strong {
    font-size: 2rem;
  }
  .event-filter .button {
    flex: 1;
  }
  .live-toolbar > .button {
    width: 100%;
  }
}
</style>
