<script setup lang="ts">
import { toRaw } from "vue";

import {
  PRO_VIEW_CHART_TYPES,
  PRO_VIEW_BREAKDOWNS,
  widgetDateRange,
  defaultProViewDefinition,
  type ProViewChartType,
  type ProViewEventOption,
  type ProViewFunnelStep,
  type ProViewWidgetDefinition,
  type ProViewWidgetKind,
} from "~/types/pro-view";

const props = withDefaults(
  defineProps<{
    busy?: boolean;
    canSave?: boolean;
    eventOptions?: ProViewEventOption[];
    initialDefinition?: ProViewWidgetDefinition | null;
    kind?: ProViewWidgetKind;
    previewAvailable?: boolean;
  }>(),
  {
    busy: false,
    canSave: false,
    eventOptions: () => [],
    initialDefinition: null,
    kind: "chart",
    previewAvailable: true,
  },
);

const emit = defineEmits<{
  cancel: [];
  preview: [definition: ProViewWidgetDefinition];
  save: [definition: ProViewWidgetDefinition];
}>();

const chartLabels: Record<ProViewChartType, string> = {
  bar: "Bar",
  donut: "Donut",
  line: "Line",
  map: "Map",
  pie: "Pie",
  scatter: "Scatter",
};

const draft = ref(createDraft());
// createDraft normalizes persisted definitions once. These computed values are
// pure views, which keeps template reads from unexpectedly mutating the draft.
const chartSource = computed(() => draft.value.source!);
const dateRange = computed(() => draft.value.date_range!);
const funnelConfig = computed(() => draft.value.funnel!);
const filteredSources = computed(() =>
  props.eventOptions.filter((option) => option.kind === chartSource.value.kind),
);
const sourceKindLabel = computed(() =>
  chartSource.value.kind === "rule" ? "Tracking rule" : "Tracked event",
);
const isFunnel = computed(() => draft.value.kind === "funnel");
const canQuery = computed(() => {
  if (
    !draft.value.title.trim() ||
    !Number.isInteger(dateRange.value.days) ||
    dateRange.value.days < 1 ||
    dateRange.value.days > 365
  )
    return false;
  if (isFunnel.value) {
    return Boolean(
      draft.value.funnel?.entry_mode === "closed" &&
      (draft.value.funnel?.steps.length ?? 0) >= 2 &&
      draft.value.funnel.steps.every(
        (step) =>
          step.name.trim() &&
          step.condition.id.trim() &&
          stepPropertyFilters(step).filters.every(
            (filter) =>
              /^[A-Za-z0-9_.-]{1,64}$/.test(filter.key.trim()) &&
              (filter.operator === "exists" || Boolean(filter.value?.trim())),
          ),
      ),
    );
  }
  return Boolean(chartSource.value.id && chartSource.value.name.trim());
});

watch(
  () => [props.initialDefinition, props.kind] as const,
  () => {
    draft.value = createDraft();
  },
);

watch(
  () => draft.value.source?.kind,
  (kind, previousKind) => {
    const source = draft.value.source;
    if (!kind || !source || kind === previousKind) return;
    source.id = "";
    source.name = "";
    source.has_location = false;
  },
);

function createDraft() {
  const source = props.eventOptions[0];
  const definition = props.initialDefinition
    ? cloneDefinition(props.initialDefinition)
    : defaultProViewDefinition(props.kind, source);

  definition.date_range = { ...widgetDateRange(definition) };
  definition.breakdown =
    definition.visualization === "map"
      ? "country"
      : (definition.breakdown ?? "time");
  if (definition.kind === "funnel") {
    definition.source = undefined;
    definition.funnel ??= defaultProViewDefinition("funnel", source).funnel!;
  } else {
    definition.funnel = undefined;
    definition.source ??= defaultProViewDefinition("chart", source).source!;
  }
  for (const step of definition.funnel?.steps ?? []) {
    step.condition.property_filters ??= { filters: [], logic: "and" };
  }
  if (definition.funnel) definition.funnel.entry_mode = "closed";
  return definition;
}

function updateSelectedSource() {
  const source = props.eventOptions.find(
    (option) =>
      option.id === chartSource.value.id &&
      option.kind === chartSource.value.kind,
  );
  chartSource.value.name = source?.label ?? "";
  chartSource.value.has_location = source?.hasLocation ?? false;
}

function setChartType(type: ProViewChartType) {
  if (type === "map") draft.value.breakdown = "country";
  draft.value.visualization = type;
}

function updateBreakdown() {
  if (
    draft.value.breakdown !== "country" &&
    draft.value.visualization === "map"
  )
    draft.value.visualization = "bar";
  if (
    draft.value.breakdown !== "time" &&
    ["line", "scatter"].includes(draft.value.visualization)
  )
    draft.value.visualization = "bar";
}

function addFunnelStep() {
  if ((draft.value.funnel?.steps.length ?? 0) >= 10) return;
  draft.value.funnel ??= {
    conversion_window: "7d",
    entry_mode: "closed",
    order_mode: "ordered",
    steps: [],
  };
  draft.value.funnel.steps.push({
    condition: {
      id: "",
      kind: "event",
      name: "",
      property_filters: { filters: [], logic: "and" },
    },
    id: draftId(),
    name: `Step ${draft.value.funnel.steps.length + 1}`,
  });
}

function moveFunnelStep(index: number, direction: -1 | 1) {
  const target = index + direction;
  if (target < 0 || target >= funnelConfig.value.steps.length) return;
  const [step] = funnelConfig.value.steps.splice(index, 1);
  if (step) funnelConfig.value.steps.splice(target, 0, step);
}

function addPropertyFilter(index: number) {
  const condition = funnelConfig.value.steps[index]?.condition;
  if (!condition) return;
  condition.property_filters ??= { filters: [], logic: "and" };
  if (condition.property_filters.filters.length >= 3) return;
  condition.property_filters.filters.push({
    id: draftId(),
    key: "",
    operator: "equals",
    value: "",
  });
}

function stepPropertyFilters(step: ProViewFunnelStep) {
  step.condition.property_filters ??= { filters: [], logic: "and" };
  return step.condition.property_filters;
}

function removePropertyFilter(stepIndex: number, filterIndex: number) {
  funnelConfig.value.steps[
    stepIndex
  ]?.condition.property_filters?.filters.splice(filterIndex, 1);
}

function funnelSources(kind: "event" | "page" | "rule") {
  if (kind === "page") return [];
  return props.eventOptions.filter((option) => option.kind === kind);
}

function removeFunnelStep(index: number) {
  if ((draft.value.funnel?.steps.length ?? 0) <= 2) return;
  draft.value.funnel?.steps.splice(index, 1);
}

function updateFunnelStep(index: number) {
  const step = draft.value.funnel?.steps[index];
  if (!step) return;
  const source = props.eventOptions.find(
    (option) =>
      option.id === step.condition.id && option.kind === step.condition.kind,
  );
  if (source) {
    step.condition.name = source.label;
    if (!step.name.trim() || /^Step \d+$/.test(step.name)) {
      step.name = source.label;
    }
  }
}

function changeFunnelCondition(index: number) {
  const step = draft.value.funnel?.steps[index];
  if (!step) return;
  step.condition.id = "";
  step.condition.name = "";
  step.condition.operator =
    step.condition.kind === "page" ? "exact" : undefined;
  step.condition.property_filters ??= { filters: [], logic: "and" };
}

function submitPreview() {
  if (canQuery.value) emit("preview", normalizedDraft());
}

function submitSave() {
  if (canQuery.value && props.canSave) emit("save", normalizedDraft());
}

function normalizedDraft() {
  const definition = cloneDefinition(draft.value);
  definition.title = definition.title.trim();
  definition.filters.page = definition.filters.page?.trim() || undefined;
  definition.filters.country = definition.filters.country?.trim() || undefined;
  if (definition.kind === "funnel") {
    definition.visualization = "funnel";
    definition.source = undefined;
    for (const step of definition.funnel?.steps ?? []) {
      step.name = step.name.trim();
      step.condition.id = step.condition.id.trim();
      step.condition.name = step.condition.name.trim();
      const propertyFilters = step.condition.property_filters;
      if (!propertyFilters?.filters.length) {
        step.condition.property_filters = undefined;
        continue;
      }
      propertyFilters.filters = propertyFilters.filters.map((filter) => ({
        ...filter,
        key: filter.key.trim(),
        value: filter.operator === "exists" ? undefined : filter.value?.trim(),
      }));
    }
  } else {
    definition.funnel = undefined;
  }
  return definition;
}

function cloneDefinition(definition: ProViewWidgetDefinition) {
  return structuredClone(toRaw(definition));
}

function draftId() {
  if (typeof crypto !== "undefined" && "randomUUID" in crypto) {
    return crypto.randomUUID();
  }
  return `step-${Date.now()}-${Math.random().toString(36).slice(2)}`;
}
</script>

<template>
  <form class="pro-builder" @submit.prevent="submitPreview">
    <header class="builder-header">
      <div>
        <p class="builder-kicker">
          {{ initialDefinition ? "Edit definition" : "Unsaved draft" }}
        </p>
        <h2>{{ isFunnel ? "Build a funnel" : "Build a chart" }}</h2>
      </div>
      <button class="quiet-button" type="button" @click="emit('cancel')">
        Close
      </button>
    </header>

    <div class="builder-grid">
      <label class="wide-field">
        <span>Title</span>
        <input
          v-model="draft.title"
          autocomplete="off"
          maxlength="120"
          placeholder="Checkout intent"
          required
        />
      </label>

      <template v-if="!isFunnel">
        <label>
          <span>Source kind</span>
          <select v-model="chartSource.kind">
            <option value="event">Tracked event</option>
            <option value="rule">Tracking rule</option>
            <option value="future_event">Future event</option>
          </select>
        </label>

        <label v-if="chartSource.kind !== 'future_event'">
          <span>{{ sourceKindLabel }}</span>
          <select
            v-model="chartSource.id"
            required
            @change="updateSelectedSource"
          >
            <option disabled value="">Choose a source</option>
            <option
              v-for="source in filteredSources"
              :key="`${source.kind}:${source.id}`"
              :value="source.id"
            >
              {{ source.label }} · {{ source.meta }}
            </option>
          </select>
        </label>

        <label v-else>
          <span>Future event name</span>
          <input
            v-model="chartSource.name"
            autocomplete="off"
            maxlength="120"
            placeholder="trial_activated"
            required
            @input="chartSource.id = chartSource.name.trim()"
          />
        </label>

        <label class="wide-field">
          <span>Group by</span>
          <select v-model="draft.breakdown" @change="updateBreakdown">
            <option
              v-for="(label, value) in PRO_VIEW_BREAKDOWNS"
              :key="value"
              :value="value"
            >
              {{ label }}
            </option>
          </select>
          <small class="field-note"
            >Counts the selected event or rule. Category charts show up to 20
            groups. Untagged campaigns appear as “No campaign”; missing
            referrers appear as “Direct”.</small
          >
        </label>

        <fieldset class="wide-field chart-type-field">
          <legend>Chart shape</legend>
          <div class="chart-type-grid">
            <button
              v-for="type in PRO_VIEW_CHART_TYPES"
              :key="type"
              :aria-pressed="draft.visualization === type"
              type="button"
              @click="setChartType(type)"
            >
              <span class="shape-icon" aria-hidden="true">{{
                type === "line"
                  ? "↗"
                  : type === "bar"
                    ? "▥"
                    : type === "donut"
                      ? "◉"
                      : type === "pie"
                        ? "◕"
                        : type === "scatter"
                          ? "⠿"
                          : "◎"
              }}</span>
              {{ chartLabels[type] }}
            </button>
          </div>
          <small v-if="draft.visualization === 'map'" class="field-note">
            Maps group recorded events by country. Events without a known
            country cannot be placed on the map.
          </small>
        </fieldset>
      </template>

      <fieldset v-else class="wide-field funnel-fieldset">
        <legend>Ordered conversion cohort</legend>
        <p class="field-note">
          A person must meet each condition in order and inside the selected
          conversion window. The preview shows how many reach each step, where
          they drop off, and how long they take between steps.
        </p>
        <div class="funnel-controls">
          <label>
            <span>Conversion window</span>
            <select v-model="funnelConfig.conversion_window">
              <option value="same_session">Same session</option>
              <option value="1h">Within 1 hour</option>
              <option value="1d">Within 1 day</option>
              <option value="7d">Within 7 days</option>
              <option value="30d">Within 30 days</option>
            </select>
          </label>
          <label>
            <span>Step order</span>
            <select v-model="funnelConfig.order_mode">
              <option value="ordered">
                Ordered · intervening actions allowed
              </option>
              <option value="exact">
                Exact · no intervening tracked action
              </option>
            </select>
          </label>
          <label>
            <span>Entry</span>
            <select v-model="funnelConfig.entry_mode">
              <option value="closed">Closed · must begin at step 1</option>
              <option disabled value="open">Open · coming next</option>
            </select>
          </label>
        </div>
        <div class="funnel-step-list">
          <div
            v-for="(step, index) in funnelConfig.steps"
            :key="step.id"
            class="funnel-step"
          >
            <span class="step-number">{{ index + 1 }}</span>
            <div class="step-fields">
              <label>
                <span>Condition</span>
                <select
                  v-model="step.condition.kind"
                  @change="changeFunnelCondition(index)"
                >
                  <option value="page">Page path</option>
                  <option value="event">Tracked event</option>
                  <option value="rule">Tracking rule</option>
                </select>
              </label>
              <label v-if="step.condition.kind === 'page'">
                <span>Path</span>
                <input
                  v-model="step.condition.id"
                  maxlength="2048"
                  pattern="/.*"
                  placeholder="/pricing"
                  required
                  @input="step.condition.name = step.condition.id"
                />
              </label>
              <label v-else>
                <span>{{
                  step.condition.kind === "rule" ? "Rule" : "Event"
                }}</span>
                <select
                  v-model="step.condition.id"
                  required
                  @change="updateFunnelStep(index)"
                >
                  <option disabled value="">Choose a condition</option>
                  <option
                    v-for="source in funnelSources(step.condition.kind)"
                    :key="`${source.kind}:${source.id}`"
                    :value="source.id"
                  >
                    {{ source.label }} · {{ source.meta }}
                  </option>
                </select>
              </label>
              <label v-if="step.condition.kind === 'page'">
                <span>Path matching</span>
                <select v-model="step.condition.operator">
                  <option value="exact">Exact path</option>
                  <option value="starts_with">Starts with</option>
                </select>
              </label>
              <label class="step-label">
                <span>Step label</span>
                <input
                  v-model="step.name"
                  maxlength="120"
                  placeholder="Viewed pricing"
                  required
                />
              </label>
              <details class="property-filters">
                <summary>
                  Property filters
                  <small>
                    {{
                      stepPropertyFilters(step).filters.length
                        ? `${stepPropertyFilters(step).filters.length} configured`
                        : "optional"
                    }}
                  </small>
                </summary>
                <div class="property-filter-head">
                  <span>Match filters using</span>
                  <select
                    v-model="stepPropertyFilters(step).logic"
                    aria-label="Property filter logic"
                  >
                    <option value="and">All · AND</option>
                    <option value="or">Any · OR</option>
                  </select>
                </div>
                <div
                  v-for="(filter, filterIndex) in stepPropertyFilters(step)
                    .filters"
                  :key="filter.id"
                  class="property-filter-row"
                >
                  <input
                    v-model="filter.key"
                    aria-label="Property key"
                    maxlength="64"
                    pattern="[A-Za-z0-9_.-]+"
                    placeholder="plan"
                    required
                  />
                  <select
                    v-model="filter.operator"
                    aria-label="Property operator"
                  >
                    <option value="equals">Equals</option>
                    <option value="not_equals">Does not equal</option>
                    <option value="contains">Contains</option>
                    <option value="exists">Exists</option>
                  </select>
                  <input
                    v-if="filter.operator !== 'exists'"
                    v-model="filter.value"
                    aria-label="Property value"
                    maxlength="512"
                    placeholder="pro"
                    required
                  />
                  <span v-else class="filter-no-value">No value needed</span>
                  <button
                    type="button"
                    aria-label="Remove property filter"
                    @click="removePropertyFilter(index, filterIndex)"
                  >
                    −
                  </button>
                </div>
                <button
                  class="add-filter"
                  :disabled="stepPropertyFilters(step).filters.length >= 3"
                  type="button"
                  @click="addPropertyFilter(index)"
                >
                  + Add property filter
                </button>
                <p>
                  Campaign, plan, variant, and similar primitive properties are
                  API-compatible. Counts still come only from the cohort query.
                </p>
              </details>
            </div>
            <div class="step-actions">
              <button
                :disabled="index === 0"
                type="button"
                :aria-label="`Move funnel step ${index + 1} up`"
                @click="moveFunnelStep(index, -1)"
              >
                ↑
              </button>
              <button
                :disabled="index === funnelConfig.steps.length - 1"
                type="button"
                :aria-label="`Move funnel step ${index + 1} down`"
                @click="moveFunnelStep(index, 1)"
              >
                ↓
              </button>
              <button
                class="remove-step"
                :disabled="funnelConfig.steps.length <= 2"
                type="button"
                :aria-label="`Remove funnel step ${index + 1}`"
                @click="removeFunnelStep(index)"
              >
                −
              </button>
            </div>
          </div>
        </div>
        <button
          class="add-step"
          :disabled="funnelConfig.steps.length >= 10"
          type="button"
          @click="addFunnelStep"
        >
          + Add step
        </button>
      </fieldset>

      <label>
        <span>Duration (days)</span>
        <input
          v-model.number="dateRange.days"
          type="number"
          min="1"
          max="365"
          step="1"
          required
        />
        <small class="field-note"
          >Last 1–365 days, up to available retention. One day means the last 24
          hours.</small
        >
      </label>
      <label class="check-field">
        <input v-model="dateRange.compare_previous" type="checkbox" />
        <span
          >Compare with previous period<small
            >Compare with the preceding {{ dateRange.days }} days of equal
            length.</small
          ></span
        >
      </label>

      <label>
        <span>Card size</span>
        <select v-model="draft.display.size">
          <option value="wide">Wide</option>
          <option value="compact">Compact</option>
        </select>
      </label>

      <label v-if="!isFunnel">
        <span>Page path <small>optional</small></span>
        <input
          v-model="draft.filters.page"
          autocomplete="off"
          pattern="/.*"
          placeholder="/checkout"
        />
      </label>

      <label v-if="!isFunnel">
        <span>Country code <small>optional filter</small></span>
        <input
          v-model="draft.filters.country"
          autocomplete="off"
          maxlength="80"
          placeholder="IN"
        />
      </label>
    </div>

    <footer class="builder-actions">
      <button
        class="preview-button"
        :disabled="busy || !canQuery || !previewAvailable"
        type="submit"
      >
        {{
          busy
            ? "Querying…"
            : previewAvailable
              ? "Preview with real data"
              : "Chart data binding coming next"
        }}
      </button>
      <button
        class="save-button"
        :disabled="busy || !canQuery || !canSave"
        type="button"
        @click="submitSave"
      >
        {{ initialDefinition ? "Save changes" : "Add to Pro View" }}
      </button>
      <p v-if="!canSave">
        Saving stays locked until the dashboard API is available and your role
        can edit this app.
      </p>
      <p v-else-if="!previewAvailable">
        Chart definitions can be saved now. Real chart-series preview stays
        disabled until its query binding lands.
      </p>
    </footer>
  </form>
</template>

<style scoped>
.pro-builder {
  min-width: 0;
  border: 2px solid #151515;
  border-radius: 20px;
  background: #fffef9;
  padding: clamp(16px, 3vw, 28px);
  box-shadow: 6px 6px 0 #151515;
}

.builder-header,
.builder-actions,
.funnel-step {
  display: flex;
  align-items: center;
}

.builder-header {
  justify-content: space-between;
  gap: 16px;
  margin-bottom: 24px;
}

.builder-kicker {
  margin: 0 0 3px;
  color: #2847d6;
  font-family: var(--font-mono);
  font-size: 0.66rem;
  font-weight: 800;
  letter-spacing: 0.1em;
  text-transform: uppercase;
}

h2 {
  margin: 0;
  color: #151515;
  font-size: clamp(1.35rem, 3vw, 1.8rem);
  letter-spacing: -0.04em;
}

.quiet-button,
.preview-button,
.save-button,
.add-step,
.remove-step,
.chart-type-grid button {
  min-height: 42px;
  border: 1.5px solid #151515;
  border-radius: 999px;
  color: #151515;
  background: #fffef9;
  padding: 0 14px;
  font-weight: 760;
  cursor: pointer;
}

.quiet-button:hover,
.add-step:hover,
.chart-type-grid button:hover:not(:disabled) {
  background: #e9e4d8;
}

.builder-grid {
  display: grid;
  grid-template-columns: repeat(2, minmax(0, 1fr));
  gap: 16px;
}

label,
fieldset {
  min-width: 0;
}

label {
  display: grid;
  align-content: start;
  gap: 7px;
  color: #494842;
  font-size: 0.83rem;
  font-weight: 720;
}

label > span small {
  color: #6a6861;
  font-weight: 520;
}

input,
select {
  width: 100%;
  min-height: 46px;
  border: 1px solid #b7b0a3;
  border-radius: 10px;
  color: #151515;
  background: #fffef9;
  padding: 0 12px;
}

select {
  text-overflow: ellipsis;
}

input:focus,
select:focus {
  border-color: #3d5afe;
  outline: 2px solid #1e35c9;
  outline-offset: 2px;
}

.wide-field {
  grid-column: 1 / -1;
}

.check-field {
  display: flex;
  align-items: center;
  align-self: end;
  min-height: 46px;
  gap: 10px;
  border: 1px solid #d8d2c4;
  border-radius: 10px;
  background: #f4f1e8;
  padding: 9px 12px;
}

.check-field input {
  width: 18px;
  min-height: 18px;
  flex: 0 0 auto;
  accent-color: #3d5afe;
}

.check-field span {
  display: grid;
}

.check-field small {
  color: #6a6861;
  font-size: 0.7rem;
  font-weight: 520;
}

fieldset {
  margin: 0;
  border: 0;
  padding: 0;
}

legend {
  margin-bottom: 8px;
  color: #494842;
  font-size: 0.83rem;
  font-weight: 720;
}

.chart-type-grid {
  display: grid;
  grid-template-columns: repeat(6, minmax(0, 1fr));
  gap: 8px;
}

.chart-type-grid button {
  display: grid;
  min-height: 74px;
  place-items: center;
  align-content: center;
  gap: 3px;
  border-radius: 12px;
  padding: 8px;
}

.chart-type-grid button[aria-pressed="true"] {
  color: #fffef9;
  background: #3d5afe;
  box-shadow: 3px 3px 0 #151515;
}

.chart-type-grid button:disabled {
  cursor: not-allowed;
  opacity: 0.42;
}

.shape-icon {
  font-family: var(--font-mono);
  font-size: 1.25rem;
}

.field-note {
  display: block;
  margin: 8px 0 0;
  color: #6a6861;
  font-size: 0.75rem;
  line-height: 1.45;
}

.funnel-fieldset {
  border: 1px solid #d8d2c4;
  border-radius: 14px;
  background: #f4f1e8;
  padding: 16px;
}

.funnel-step-list {
  display: grid;
  gap: 10px;
  margin-top: 14px;
}

.funnel-controls {
  display: grid;
  grid-template-columns: repeat(3, minmax(0, 1fr));
  gap: 10px;
  margin-top: 14px;
}

.funnel-step {
  align-items: flex-start;
  gap: 10px;
  border: 1px solid #d8d2c4;
  border-radius: 12px;
  background: #fffef9;
  padding: 12px;
}

.step-fields {
  display: grid;
  min-width: 0;
  flex: 1;
  grid-template-columns: minmax(130px, 0.7fr) minmax(180px, 1.3fr);
  gap: 10px;
}

.step-label {
  grid-column: 1 / -1;
}

.step-number {
  display: grid;
  width: 34px;
  height: 34px;
  flex: 0 0 auto;
  place-items: center;
  border: 1.5px solid #151515;
  border-radius: 50%;
  color: #151515;
  background: #d8ff52;
  font-family: var(--font-mono);
  font-size: 0.76rem;
  font-weight: 820;
}

.step-actions {
  display: grid;
  flex: 0 0 auto;
  gap: 5px;
}

.step-actions button,
.remove-step {
  width: 42px;
  min-height: 36px;
  border: 1px solid #b7b0a3;
  border-radius: 9px;
  color: #151515;
  background: #f4f1e8;
  flex: 0 0 auto;
  padding: 0;
  font-size: 1.25rem;
}

.property-filters {
  grid-column: 1 / -1;
  border-top: 1px solid #d8d2c4;
  padding-top: 9px;
}

.property-filters summary {
  color: #494842;
  font-size: 0.76rem;
  font-weight: 720;
  cursor: pointer;
}

.property-filters summary small {
  margin-left: 5px;
  color: #6a6861;
  font-weight: 520;
}

.property-filter-head {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 10px;
  margin: 10px 0 7px;
  color: #6a6861;
  font-size: 0.72rem;
}

.property-filter-head select {
  width: auto;
  min-height: 36px;
}

.property-filter-row {
  display: grid;
  grid-template-columns:
    minmax(90px, 1fr) minmax(120px, 0.8fr) minmax(90px, 1fr)
    36px;
  gap: 6px;
  margin-top: 6px;
}

.property-filter-row input,
.property-filter-row select {
  min-height: 38px;
  font-size: 0.76rem;
}

.property-filter-row button {
  min-height: 38px;
  border: 1px solid #b7b0a3;
  border-radius: 9px;
  color: #9b2d23;
  background: #ffe9e5;
}

.filter-no-value {
  display: grid;
  min-height: 38px;
  place-items: center;
  border: 1px dashed #b7b0a3;
  border-radius: 9px;
  color: #6a6861;
  font-size: 0.7rem;
}

.property-filters .add-filter {
  min-height: 36px;
  margin-top: 8px;
  border: 1px solid #b7b0a3;
  border-radius: 999px;
  color: #151515;
  background: #fffef9;
  padding: 0 11px;
  font-size: 0.72rem;
  font-weight: 720;
}

.property-filters p {
  margin: 7px 0 0;
  color: #6a6861;
  font-size: 0.68rem;
  line-height: 1.45;
}

.add-step {
  margin-top: 12px;
}

.builder-actions {
  flex-wrap: wrap;
  gap: 10px;
  margin-top: 24px;
  border-top: 1px solid #d8d2c4;
  padding-top: 18px;
}

.preview-button {
  color: #151515;
  background: #d8ff52;
}

.save-button {
  color: #fffef9;
  background: #3d5afe;
}

.builder-actions button:disabled {
  cursor: not-allowed;
  opacity: 0.48;
}

.builder-actions p {
  flex-basis: 100%;
  margin: 0;
  color: #6a6861;
  font-size: 0.75rem;
  line-height: 1.45;
}

@media (max-width: 760px) {
  .builder-grid,
  .chart-type-grid {
    grid-template-columns: repeat(2, minmax(0, 1fr));
  }

  .wide-field {
    grid-column: 1 / -1;
  }

  .step-fields {
    grid-template-columns: 1fr;
  }

  .funnel-controls,
  .property-filter-row {
    grid-template-columns: 1fr;
  }

  .property-filter-row button {
    width: 100%;
  }

  .step-label {
    grid-column: auto;
  }
}

@media (max-width: 420px) {
  .pro-builder {
    padding: 14px;
    box-shadow: 3px 3px 0 #151515;
  }

  .builder-header {
    align-items: flex-start;
  }

  .builder-grid {
    grid-template-columns: 1fr;
  }

  .wide-field {
    grid-column: auto;
  }

  .funnel-step {
    align-items: end;
  }

  .builder-actions {
    align-items: stretch;
    flex-direction: column;
  }
}
</style>
