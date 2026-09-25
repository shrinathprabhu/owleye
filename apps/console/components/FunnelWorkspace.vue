<script setup lang="ts">
import {
  defaultProViewDefinition,
  type ProViewWidgetDefinition,
} from "~/types/pro-view";
import type { WorkspaceSite } from "~/types/workspace";

const props = withDefaults(
  defineProps<{ demo?: boolean; site?: WorkspaceSite | null }>(),
  { demo: false, site: null },
);
const site = computed(() => props.site);
const demo = computed(() => props.demo);
const canWrite = computed(() =>
  Boolean(
    props.site && (props.site.role === "owner" || props.site.role === "admin"),
  ),
);
const {
  activeDashboard,
  canPersist,
  deleteDefinition,
  errorMessage,
  eventOptions,
  mutationPending,
  notice,
  persistenceStatus,
  preview,
  previewDefinition,
  previewError,
  previewPending,
  saveDefinition,
  workspacePending,
} = useProViewWorkspace({ canWrite, demo, site });

const builderOpen = ref(false);
const editing = shallowRef<ProViewWidgetDefinition | null>(null);
const inspected = shallowRef<ProViewWidgetDefinition | null>(null);
const confirmDelete = ref("");
const funnels = computed(() =>
  [...(activeDashboard.value?.widgets ?? [])]
    .filter((widget) => widget.kind === "funnel")
    .sort((left, right) => left.display.order - right.display.order),
);

function openBuilder(definition: ProViewWidgetDefinition | null = null) {
  editing.value = definition;
  inspected.value =
    definition ?? defaultProViewDefinition("funnel", eventOptions.value[0]);
  preview.value = null;
  previewError.value = "";
  builderOpen.value = true;
  nextTick(() =>
    document
      .querySelector("#funnel-builder")
      ?.scrollIntoView({ behavior: "smooth" }),
  );
}

function closeBuilder() {
  builderOpen.value = false;
  editing.value = null;
  inspected.value = null;
  preview.value = null;
  previewError.value = "";
}

async function handlePreview(definition: ProViewWidgetDefinition) {
  inspected.value = definition;
  await previewDefinition(definition);
}

async function handleSave(definition: ProViewWidgetDefinition) {
  inspected.value = definition;
  if (await saveDefinition(definition)) closeBuilder();
}

async function inspect(definition: ProViewWidgetDefinition) {
  openBuilder(definition);
  await handlePreview(definition);
}

async function remove(definition: ProViewWidgetDefinition) {
  if (await deleteDefinition(definition)) confirmDelete.value = "";
}
</script>

<template>
  <div class="funnel-workspace">
    <div class="funnel-action-bar">
      <div>
        <strong
          >{{ funnels.length }} saved
          {{ funnels.length === 1 ? "funnel" : "funnels" }}</strong
        >
        <span>Follow anonymous visitors through 2–10 conversion steps</span>
      </div>
      <button :disabled="!canPersist" type="button" @click="openBuilder()">
        + Create funnel
      </button>
    </div>

    <p v-if="errorMessage" class="page-alert" role="alert">
      {{ errorMessage }}
    </p>
    <p v-if="notice" class="funnel-notice" role="status">{{ notice }}</p>

    <section
      v-if="builderOpen"
      id="funnel-builder"
      class="funnel-builder-layout"
    >
      <ProViewBuilder
        :key="editing?.id ?? 'new-funnel'"
        :busy="mutationPending || previewPending"
        :can-save="canPersist"
        :event-options="eventOptions"
        :initial-definition="editing"
        kind="funnel"
        :preview-available="true"
        @cancel="closeBuilder"
        @preview="handlePreview"
        @save="handleSave"
      />
      <LazyProViewPreview
        :definition="inspected"
        :error="previewError"
        hydrate-on-visible
        :loading="previewPending"
        :preview="preview"
        @retry="inspected && handlePreview(inspected)"
      />
    </section>

    <section class="funnel-library">
      <header>
        <div>
          <p class="panel-kicker">Funnel library</p>
          <h2>Conversion journeys</h2>
        </div>
        <span>{{ activeDashboard?.name ?? "Default Pro View" }}</span>
      </header>

      <div v-if="workspacePending" class="funnel-state">
        <span class="state-orbit"></span
        ><strong>Loading funnel definitions…</strong>
      </div>
      <div v-else-if="funnels.length" class="funnel-grid">
        <article v-for="funnel in funnels" :key="funnel.id">
          <header>
            <div>
              <span>Ordered funnel</span>
              <h3>{{ funnel.title }}</h3>
            </div>
            <strong>{{ funnel.funnel?.steps.length }} steps</strong>
          </header>
          <ol>
            <li v-for="step in funnel.funnel?.steps" :key="step.id">
              <span></span>
              <div>
                <strong>{{ step.name }}</strong
                ><small
                  >{{ step.condition.kind }} · {{ step.condition.id }}</small
                >
              </div>
            </li>
          </ol>
          <footer>
            <span
              >{{
                funnel.funnel?.conversion_window.replace("_", " ")
              }}
              window</span
            >
            <div>
              <button type="button" @click="inspect(funnel)">
                Query &amp; edit
              </button>
              <button
                v-if="confirmDelete !== funnel.id"
                :disabled="!canPersist"
                type="button"
                @click="confirmDelete = funnel.id ?? ''"
              >
                Remove
              </button>
              <button
                v-else
                class="danger"
                type="button"
                @click="remove(funnel)"
              >
                Confirm
              </button>
            </div>
          </footer>
        </article>
      </div>
      <div v-else class="funnel-state">
        <span class="state-symbol">⇢</span>
        <strong>No funnel definitions yet</strong>
        <span
          >Build a journey from page paths, custom events, or tracking
          rules.</span
        >
        <button v-if="canPersist" type="button" @click="openBuilder()">
          Create your first funnel
        </button>
        <small v-else-if="persistenceStatus === 'available'"
          >Owner or admin access is required to create funnels.</small
        >
      </div>
    </section>
  </div>
</template>

<style scoped>
.funnel-workspace {
  display: grid;
  gap: 18px;
}
.funnel-action-bar,
.funnel-library > header,
article > header,
article > footer {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 16px;
}
.funnel-action-bar {
  border: 1px solid var(--border-subtle);
  border-radius: 14px;
  background: var(--bg-elevated);
  padding: 14px 16px;
}
.funnel-action-bar > div {
  display: grid;
  gap: 2px;
}
.funnel-action-bar strong,
h2,
h3 {
  color: var(--text-primary);
}
.funnel-action-bar span {
  color: var(--text-tertiary);
  font-size: 0.75rem;
}
.funnel-action-bar button,
.funnel-state button,
article footer button {
  min-height: 39px;
  border: 1.5px solid var(--text-primary);
  border-radius: 999px;
  color: var(--text-on-accent);
  background: var(--accent);
  padding: 0 14px;
  font-weight: 750;
  cursor: pointer;
}
.funnel-action-bar button:disabled {
  opacity: 0.45;
}
.funnel-notice {
  margin: 0;
  border: 1px solid var(--success-500);
  border-radius: 10px;
  color: var(--success-700);
  background: var(--success-50);
  padding: 10px 12px;
  font-size: 0.8rem;
}
.funnel-builder-layout {
  display: grid;
  grid-template-columns: minmax(0, 0.9fr) minmax(0, 1.1fr);
  align-items: start;
  gap: 20px;
  scroll-margin-top: 20px;
}
.funnel-library {
  min-width: 0;
  border: 1px solid var(--border-subtle);
  border-radius: 18px;
  background: var(--bg-elevated);
  padding: clamp(16px, 2.5vw, 23px);
}
.funnel-library > header {
  margin-bottom: 16px;
}
.funnel-library h2 {
  margin: 0;
  font-size: 1.08rem;
}
.funnel-library > header > span {
  color: var(--text-tertiary);
  font-family: var(--font-mono);
  font-size: 0.67rem;
}
.funnel-grid {
  display: grid;
  grid-template-columns: repeat(2, minmax(0, 1fr));
  gap: 13px;
}
.funnel-grid article {
  min-width: 0;
  border: 1px solid var(--border-subtle);
  border-radius: 14px;
  background: var(--bg-base);
  padding: 16px;
}
.funnel-grid article > header span {
  color: var(--brand-text-safe);
  font-family: var(--font-mono);
  font-size: 0.62rem;
  font-weight: 800;
  text-transform: uppercase;
}
.funnel-grid h3 {
  margin: 3px 0 0;
  font-size: 1rem;
}
.funnel-grid article > header > strong {
  border-radius: 999px;
  color: var(--text-on-brand);
  background: var(--brand);
  padding: 5px 9px;
  font-family: var(--font-mono);
  font-size: 0.65rem;
}
ol {
  display: grid;
  gap: 0;
  margin: 18px 0;
  padding: 0;
  list-style: none;
}
li {
  position: relative;
  display: flex;
  gap: 10px;
  min-width: 0;
  padding-bottom: 13px;
}
li:not(:last-child)::before {
  position: absolute;
  top: 14px;
  bottom: 0;
  left: 5px;
  width: 2px;
  background: var(--brand-200);
  content: "";
}
li > span {
  z-index: 1;
  width: 12px;
  height: 12px;
  flex: 0 0 auto;
  margin-top: 2px;
  border: 2px solid var(--brand);
  border-radius: 50%;
  background: var(--bg-elevated);
}
li div {
  display: grid;
  min-width: 0;
  gap: 2px;
}
li strong {
  color: var(--text-primary);
  font-size: 0.8rem;
}
li small {
  overflow: hidden;
  color: var(--text-tertiary);
  font-family: var(--font-mono);
  font-size: 0.62rem;
  text-overflow: ellipsis;
  white-space: nowrap;
}
article > footer {
  align-items: flex-end;
  border-top: 1px solid var(--border-subtle);
  padding-top: 12px;
}
article > footer > span {
  color: var(--text-tertiary);
  font-size: 0.68rem;
}
article > footer div {
  display: flex;
  flex-wrap: wrap;
  justify-content: flex-end;
  gap: 6px;
}
article footer button {
  min-height: 32px;
  color: var(--text-primary);
  background: var(--bg-elevated);
  font-size: 0.67rem;
}
article footer button.danger {
  color: #fff;
  background: var(--error-600);
}
.funnel-state {
  display: grid;
  min-height: 300px;
  place-items: center;
  align-content: center;
  gap: 8px;
  color: var(--text-tertiary);
  text-align: center;
}
.funnel-state > strong {
  color: var(--text-primary);
}
.funnel-state > span:last-of-type {
  font-size: 0.8rem;
}
.funnel-state small {
  font-size: 0.72rem;
}
@media (max-width: 1180px) {
  .funnel-builder-layout {
    grid-template-columns: 1fr;
  }
}
@media (max-width: 760px) {
  .funnel-grid {
    grid-template-columns: 1fr;
  }
  .funnel-action-bar {
    align-items: stretch;
    flex-direction: column;
  }
}
</style>
