<script setup lang="ts">
import type { ConsoleSelectOption } from "~/types/console";
import { apiErrorMessage as requestError } from "~/utils/apiError";
import { routes } from "~/utils/routes";

type EventExplorerItem = {
  count: number;
  event_name: string;
  event_type: string;
  first_seen_at: string;
  key: string;
  last_seen_at: string;
};

type EventExplorerResponse = {
  available_groupings: string[];
  days: number;
  group_by: string;
  items: EventExplorerItem[];
  read_only: true;
  site_id: string;
};

const props = defineProps<{
  siteId?: string;
}>();
const { api } = useApi();

const days = ref(30);
const eventType = ref("all");
const groupBy = ref("event_name");
const order = ref("desc");
const search = ref("");
const sortBy = ref("count");
const response = ref<EventExplorerResponse | null>(null);
const loading = ref(false);
const error = ref("");
let requestController: AbortController | undefined;
let searchTimer: ReturnType<typeof setTimeout> | undefined;

const rangeOptions: ConsoleSelectOption[] = [
  { description: "A quick pulse check", label: "7 days", value: 7 },
  {
    description: "Enough context, little archaeology",
    label: "30 days",
    value: 30,
  },
  { description: "Quarter-ish behavior", label: "90 days", value: 90 },
  { description: "Maximum explorer window", label: "1 year", value: 365 },
];
const groupOptions: ConsoleSelectOption[] = [
  { label: "Event name", value: "event_name" },
  { label: "Event type", value: "event_type" },
  { label: "Date", value: "date" },
  { label: "Page", value: "page" },
  { label: "Country", value: "country" },
  { label: "Browser", value: "browser" },
  { label: "Operating system", value: "operating_system" },
  { label: "Device", value: "device" },
];
const typeOptions: ConsoleSelectOption[] = [
  { label: "All event types", value: "all" },
  { label: "Page views", value: "pageview" },
  { label: "Custom events", value: "external" },
  { label: "Tracking rules", value: "rule" },
  { label: "Timings & Web Vitals", value: "performance" },
];
const sortOptions: ConsoleSelectOption[] = [
  { label: "Most events", value: "count:desc" },
  { label: "Fewest events", value: "count:asc" },
  { label: "Recently seen", value: "last_seen_at:desc" },
  { label: "Oldest activity", value: "first_seen_at:asc" },
  { label: "Name A–Z", value: "key:asc" },
];
const sortValue = computed({
  get: () => `${sortBy.value}:${order.value}`,
  set: (value: number | string) => {
    const [nextSort = "count", nextOrder = "desc"] = String(value).split(":");
    sortBy.value = nextSort;
    order.value = nextOrder;
  },
});

watch(
  () => props.siteId,
  () => {
    if (searchTimer) clearTimeout(searchTimer);
    searchTimer = undefined;
    response.value = null;
    void loadEvents();
  },
  { immediate: true },
);
watch([days, eventType, groupBy, order, sortBy], () => void loadEvents());
watch(search, () => {
  if (searchTimer) clearTimeout(searchTimer);
  searchTimer = setTimeout(() => void loadEvents(), 260);
});

onBeforeUnmount(() => {
  requestController?.abort();
  if (searchTimer) clearTimeout(searchTimer);
});

async function loadEvents() {
  if (!props.siteId) {
    requestController?.abort();
    requestController = undefined;
    response.value = null;
    loading.value = false;
    error.value = "";
    return;
  }
  const siteId = props.siteId;
  requestController?.abort();
  const controller = new AbortController();
  requestController = controller;
  loading.value = true;
  error.value = "";

  try {
    const result = await api<EventExplorerResponse>(
      routes.sites.events(siteId),
      {
        query: {
          days: days.value,
          event_type: eventType.value === "all" ? undefined : eventType.value,
          group_by: groupBy.value,
          order: order.value,
          search: search.value.trim() || undefined,
          sort_by: sortBy.value,
        },
        signal: controller.signal,
      },
    );
    if (controller.signal.aborted || props.siteId !== siteId) return;
    response.value = result;
  } catch (cause) {
    if (controller.signal.aborted || props.siteId !== siteId) return;
    response.value = null;
    error.value = requestError(cause, "Events could not be loaded.");
  } finally {
    if (requestController === controller) {
      requestController = undefined;
      loading.value = false;
    }
  }
}

function formatDate(value: string) {
  const date = new Date(value);
  return Number.isNaN(date.valueOf())
    ? value
    : new Intl.DateTimeFormat(undefined, {
        dateStyle: "medium",
        timeStyle: "short",
      }).format(date);
}

function label(value: string) {
  if (value === "performance") return "Timings & Web Vitals";
  return value.replaceAll("_", " ");
}
</script>

<template>
  <section class="explorer-panel" aria-labelledby="event-explorer-heading">
    <header class="explorer-heading">
      <div>
        <p class="panel-kicker">Read-only event warehouse</p>
        <h2 id="event-explorer-heading">
          Ask questions. Leave no fingerprints.
        </h2>
      </div>
      <span class="status-badge neutral">No write operations</span>
    </header>

    <div class="explorer-controls">
      <label class="explorer-search">
        <span>Find an event</span>
        <input
          v-model="search"
          autocomplete="off"
          placeholder="signup, checkout, pricing…"
          type="search"
        />
      </label>
      <div class="explorer-select-field">
        <span id="event-type-label">Event type</span>
        <ConsoleSelect
          :labelledby="'event-type-label'"
          :model-value="eventType"
          :options="typeOptions"
          @change="eventType = String($event)"
        />
      </div>
      <div class="explorer-select-field">
        <span id="event-group-label">Group by</span>
        <ConsoleSelect
          :labelledby="'event-group-label'"
          :model-value="groupBy"
          :options="groupOptions"
          @change="groupBy = String($event)"
        />
      </div>
      <div class="explorer-select-field">
        <span id="event-range-label">Range</span>
        <ConsoleSelect
          :labelledby="'event-range-label'"
          :model-value="days"
          :options="rangeOptions"
          @change="days = Number($event)"
        />
      </div>
      <div class="explorer-select-field">
        <span id="event-sort-label">Sort</span>
        <ConsoleSelect
          v-model="sortValue"
          :labelledby="'event-sort-label'"
          :options="sortOptions"
        />
      </div>
    </div>

    <p v-if="eventType === 'performance'" class="timing-note">
      Custom timings and Web Vitals are stored as events. This view shows their
      names and counts; automatic browser measurements are charted in
      <NuxtLink :to="{ path: '/performance', query: { site: props.siteId } }"
        >Web Vitals</NuxtLink
      >.
    </p>

    <div v-if="error" class="page-alert" role="alert">
      <p>{{ error }}</p>
      <button class="text-button" type="button" @click="loadEvents">
        Try again
      </button>
    </div>
    <div
      v-else-if="loading && !response"
      class="management-loading"
      role="status"
    >
      <span class="state-orbit" aria-hidden="true"></span>
      <span>Grouping events without judging them…</span>
    </div>
    <div
      v-else-if="response?.items.length"
      class="event-table-wrap"
      :aria-busy="loading"
    >
      <table class="event-table">
        <caption class="sr-only">
          Event explorer grouped by
          {{
            label(response.group_by)
          }}
        </caption>
        <thead>
          <tr>
            <th scope="col">{{ label(response.group_by) }}</th>
            <th scope="col">Representative event</th>
            <th scope="col">Count</th>
            <th scope="col">First seen</th>
            <th scope="col">Last seen</th>
          </tr>
        </thead>
        <tbody>
          <tr
            v-for="item in response.items"
            :key="`${response.group_by}:${item.key}`"
          >
            <th scope="row" data-label="Group">{{ item.key || "Unnamed" }}</th>
            <td data-label="Event">
              <span class="event-type-chip">{{ label(item.event_type) }}</span>
              {{ item.event_name || "unnamed" }}
            </td>
            <td data-label="Count">
              <strong>{{ item.count.toLocaleString() }}</strong>
            </td>
            <td data-label="First seen">
              {{ formatDate(item.first_seen_at) }}
            </td>
            <td data-label="Last seen">{{ formatDate(item.last_seen_at) }}</td>
          </tr>
        </tbody>
      </table>
      <p v-if="loading" class="table-updating" role="status">
        Refreshing results…
      </p>
    </div>
    <div v-else-if="response && !loading" class="management-empty">
      <strong>No matching events</strong>
      <p>Try a wider range or clear the event search.</p>
    </div>
  </section>
</template>
