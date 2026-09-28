<script setup lang="ts">
import type { ConsoleSelectOption } from "~/types/console";
import {
  widgetRangeLabel,
  breakdownLabel,
  defaultProViewDefinition,
  type ProViewDeleteScope,
  type ProViewWidgetDefinition,
  type ProViewWidgetKind,
} from "~/types/pro-view";

const consoleWorkspace = useConsoleWorkspace();
const selectedSite = consoleWorkspace.selectedSite;
const user = consoleWorkspace.user;
const isDemoUser = consoleWorkspace.isDemoUser;
const currentRole = consoleWorkspace.currentRole;
const canWriteProView = computed(() => currentRole.value !== "viewer");
const {
  activeDashboard,
  canPersist,
  dashboards,
  deleteDashboard,
  deleteDefinition,
  errorMessage,
  eventOptions,
  invitePreview,
  invitePreviewPending,
  inviteViewer,
  loadSelectedSite,
  mutationPending,
  notice,
  pendingInvites,
  persistenceStatus,
  preview,
  previewDefinition,
  previewError,
  previewPending,
  previewInvite,
  respondToInvite,
  revokeShare,
  saveDefinition,
  selectDashboard,
  shareCandidates,
  workspacePending,
} = useProViewWorkspace({
  canWrite: canWriteProView,
  demo: isDemoUser,
  site: selectedSite,
});

const builderOpen = ref(false);
const builderKind = ref<ProViewWidgetKind>("chart");
const editingDefinition = shallowRef<ProViewWidgetDefinition | null>(null);
const inspectedDefinition = shallowRef<ProViewWidgetDefinition | null>(null);
const confirmDeleteWidgetId = ref("");
const pendingDeleteScope = ref<ProViewDeleteScope | null>(null);

const widgets = computed(() =>
  [...(activeDashboard.value?.widgets ?? [])].sort(
    (left, right) => left.display.order - right.display.order,
  ),
);
const canShare = computed(
  () =>
    persistenceStatus.value === "available" &&
    (activeDashboard.value?.capabilities?.can_share ?? false),
);
const canDeleteAll = computed(
  () =>
    persistenceStatus.value === "available" &&
    (activeDashboard.value?.capabilities?.can_delete_all ??
      activeDashboard.value?.capabilities?.can_delete ??
      false),
);
const canRemoveForMe = computed(
  () =>
    persistenceStatus.value === "available" &&
    (activeDashboard.value?.capabilities?.can_remove_for_me ?? false),
);
const canRevokeOthers = computed(
  () =>
    persistenceStatus.value === "available" &&
    (activeDashboard.value?.capabilities?.can_revoke_for_others ?? false),
);
const isReadOnly = computed(
  () =>
    currentRole.value === "viewer" ||
    activeDashboard.value?.capabilities?.can_edit === false,
);
const dashboardOptions = computed<ConsoleSelectOption[]>(() =>
  dashboards.value.map((dashboard) => ({
    badge: dashboard.share_status === "accepted" ? "Shared" : "Owned",
    description:
      dashboard.created_by?.id && dashboard.created_by.id !== user.value?.id
        ? `By ${dashboard.created_by.name || dashboard.created_by.email}`
        : dashboard.is_default
          ? "Your default canvas"
          : "Your saved canvas",
    label: dashboard.name,
    meta: `View ID · ${dashboard.id}`,
    value: dashboard.id,
  })),
);

useHead({ title: "Pro View · OWLEYE" });

function openBuilder(
  kind: ProViewWidgetKind,
  definition: ProViewWidgetDefinition | null = null,
) {
  builderKind.value = kind;
  editingDefinition.value = definition;
  inspectedDefinition.value =
    definition ?? defaultProViewDefinition(kind, eventOptions.value[0]);
  preview.value = null;
  previewError.value = "";
  builderOpen.value = true;
  nextTick(() => {
    document.querySelector("#pro-builder")?.scrollIntoView({
      behavior: "smooth",
      block: "start",
    });
  });
}

function closeBuilder() {
  builderOpen.value = false;
  editingDefinition.value = null;
  inspectedDefinition.value = null;
  preview.value = null;
  previewError.value = "";
}

async function handlePreview(definition: ProViewWidgetDefinition) {
  inspectedDefinition.value = definition;
  await previewDefinition(definition);
}

async function previewSavedWidget(widget: ProViewWidgetDefinition) {
  openBuilder(widget.kind, widget);
  await handlePreview(widget);
}

async function handleSave(definition: ProViewWidgetDefinition) {
  inspectedDefinition.value = definition;
  if (await saveDefinition(definition)) closeBuilder();
}

async function confirmWidgetDelete(widget: ProViewWidgetDefinition) {
  if (await deleteDefinition(widget)) confirmDeleteWidgetId.value = "";
}

async function applyDashboardDelete() {
  if (!pendingDeleteScope.value) return;
  if (await deleteDashboard(pendingDeleteScope.value)) {
    pendingDeleteScope.value = null;
  }
}

async function handleInviteDecision(
  dashboardId: string,
  decision: "accept" | "dismiss",
) {
  await respondToInvite(dashboardId, decision);
}
</script>

<template>
  <ConsoleSectionShell
    description="Build custom event charts and ordered conversion funnels without turning Overview into spreadsheet soup."
    eyebrow="Custom analytics workspace"
    permission="analytics_read"
    title="Pro View"
  >
    <div class="pro-content">
      <section class="pro-action-bar">
        <div class="pro-view-context">
          <span :class="['role-chip', { readonly: isReadOnly }]">
            {{ isReadOnly ? "Read-only view" : `${currentRole} access` }}
          </span>
          <code v-if="selectedSite">{{ selectedSite.tracking_id }}</code>
        </div>
        <div v-if="dashboardOptions.length > 1" class="dashboard-switcher">
          <span id="dashboard-switcher-label">Canvas</span>
          <ConsoleSelect
            :disabled="workspacePending || mutationPending"
            labelledby="dashboard-switcher-label"
            :model-value="activeDashboard?.id ?? ''"
            :options="dashboardOptions"
            placeholder="Choose a Pro View"
            @change="selectDashboard(String($event))"
          />
        </div>
        <div class="header-actions">
          <button
            :disabled="!canPersist"
            type="button"
            @click="openBuilder('chart')"
          >
            + New chart
          </button>
          <button
            class="funnel-button"
            :disabled="!canPersist"
            type="button"
            @click="openBuilder('funnel')"
          >
            + New funnel
          </button>
        </div>
      </section>

      <section class="comparison-note">
        <span aria-hidden="true">↔</span>
        <div>
          <strong>The comparison model is sound—with matching buckets.</strong>
          <p>
            Today vs yesterday is hourly. Last 7 vs prior 7 and last 30 vs prior
            30 are daily. Overview remains capped at one year; Pro View
            deliberately skips yearly comparisons until the data earns them.
          </p>
        </div>
      </section>

      <p v-if="errorMessage" class="pro-alert" role="alert">
        {{ errorMessage }}
      </p>
      <p v-if="notice" class="pro-notice" role="status">{{ notice }}</p>

      <section
        v-if="persistenceStatus === 'unavailable'"
        class="availability-banner"
      >
        <div>
          <p class="pro-kicker">Frontend ready · storage pending</p>
          <h2>Design freely. Saving waits for the dashboard API.</h2>
          <p>
            Dashboard drafts are kept in memory; no sample analytics are
            invented. Drafts disappear when closed until server persistence and
            ClickHouse preview queries land.
          </p>
        </div>
        <span>HONEST EMPTY STATE</span>
      </section>

      <section
        v-else-if="persistenceStatus === 'error'"
        class="availability-banner error"
      >
        <div>
          <p class="pro-kicker">Saved views unavailable</p>
          <h2>The workspace API did not answer cleanly.</h2>
          <p>
            Retry before editing so a network nap never looks like an empty
            dashboard.
          </p>
        </div>
        <button type="button" @click="loadSelectedSite">Retry</button>
      </section>

      <section
        v-if="builderOpen"
        id="pro-builder"
        class="builder-layout"
        aria-label="Pro View builder"
      >
        <ProViewBuilder
          :key="editingDefinition?.id ?? `new-${builderKind}`"
          :busy="mutationPending || previewPending"
          :can-save="canPersist"
          :event-options="eventOptions"
          :initial-definition="editingDefinition"
          :kind="builderKind"
          :preview-available="true"
          @cancel="closeBuilder"
          @preview="handlePreview"
          @save="handleSave"
        />
        <LazyProViewPreview
          :definition="inspectedDefinition"
          :error="previewError"
          hydrate-on-visible
          :loading="previewPending"
          :preview="preview"
          @retry="inspectedDefinition && handlePreview(inspectedDefinition)"
        />
      </section>

      <section class="saved-section" aria-labelledby="saved-view-title">
        <header>
          <div>
            <p class="pro-kicker">Saved workspace</p>
            <h2 id="saved-view-title">
              {{ activeDashboard?.name || "Your custom canvas" }}
            </h2>
          </div>
          <span v-if="widgets.length">{{ widgets.length }} widgets</span>
        </header>

        <div v-if="workspacePending" class="workspace-state" role="status">
          <span class="pro-orbit" aria-hidden="true"></span>
          <strong>Loading saved definitions</strong>
        </div>

        <div v-else-if="widgets.length" class="widget-grid">
          <article
            v-for="widget in widgets"
            :key="widget.id"
            :class="['widget-card', widget.display.size]"
          >
            <header>
              <div>
                <p>{{ widget.kind }}</p>
                <h3>{{ widget.title }}</h3>
              </div>
              <span>{{ widget.visualization }}</span>
            </header>
            <dl>
              <div>
                <dt>Source</dt>
                <dd>
                  {{
                    widget.source?.name ||
                    `${widget.funnel?.steps.length ?? 0} ordered steps`
                  }}
                </dd>
              </div>
              <div>
                <dt>Date range</dt>
                <dd>{{ widgetRangeLabel(widget) }}</dd>
              </div>
              <div v-if="widget.kind === 'chart'">
                <dt>Group by</dt>
                <dd>{{ breakdownLabel(widget) }}</dd>
              </div>
            </dl>

            <div
              v-if="confirmDeleteWidgetId === widget.id"
              aria-live="assertive"
              class="widget-confirm"
              role="alertdialog"
            >
              <p>Remove this widget from the canonical Pro View?</p>
              <button
                autofocus
                :disabled="mutationPending"
                type="button"
                @click="confirmWidgetDelete(widget)"
              >
                Confirm remove
              </button>
              <button type="button" @click="confirmDeleteWidgetId = ''">
                Cancel
              </button>
            </div>
            <div v-else class="widget-actions">
              <button
                :disabled="previewPending"
                type="button"
                @click="previewSavedWidget(widget)"
              >
                {{ previewPending ? "Querying…" : "Preview" }}
              </button>
              <button
                :disabled="!canPersist"
                type="button"
                @click="openBuilder(widget.kind, widget)"
              >
                Edit
              </button>
              <button
                :disabled="!canPersist || !widget.id"
                type="button"
                @click="confirmDeleteWidgetId = widget.id ?? ''"
              >
                Remove
              </button>
            </div>
          </article>
        </div>

        <div v-else class="workspace-empty">
          <div class="empty-sketch" aria-hidden="true">
            <span></span><span></span><span></span><span></span>
          </div>
          <p class="pro-kicker">Empty by design</p>
          <h3>No custom charts. No mysterious default clutter.</h3>
          <p>
            Start with an event chart or a real ordered funnel. Both can compare
            equivalent periods when the preview query endpoint is available.
          </p>
          <div class="empty-actions">
            <button type="button" @click="openBuilder('chart')">
              Design a chart
            </button>
            <button type="button" @click="openBuilder('funnel')">
              Design a funnel
            </button>
          </div>
        </div>
      </section>

      <aside class="layout-note">
        <span aria-hidden="true">▦</span>
        <div>
          <strong>Resize and drag are intentionally deferred.</strong>
          <p>
            Compact and wide cards are stable today. Freeform dragging needs a
            tested grid, keyboard controls, collision rules, and mobile layout
            persistence—not a Friday-afternoon pointer listener.
          </p>
        </div>
      </aside>

      <ProViewSharing
        :can-delete-all="canDeleteAll"
        :can-remove-for-me="canRemoveForMe"
        :can-revoke-others="canRevokeOthers"
        :can-share="canShare"
        :candidates="shareCandidates"
        :invite-preview="invitePreview"
        :invite-preview-pending="invitePreviewPending"
        :pending="mutationPending"
        :pending-invites="pendingInvites"
        :shares="activeDashboard?.shares ?? []"
        @decide="handleInviteDecision"
        @delete="pendingDeleteScope = $event"
        @invite="inviteViewer"
        @preview-invite="previewInvite"
        @revoke="revokeShare"
      />

      <section v-if="pendingDeleteScope" class="delete-review" role="alert">
        <div>
          <strong>Review {{ pendingDeleteScope }} deletion</strong>
          <p>
            This action is sent to the server with an explicit scope. It is not
            simulated in the browser.
          </p>
        </div>
        <button
          :disabled="mutationPending"
          type="button"
          @click="applyDashboardDelete"
        >
          {{ mutationPending ? "Applying…" : "Apply deletion" }}
        </button>
        <button type="button" @click="pendingDeleteScope = null">Cancel</button>
      </section>
    </div>
  </ConsoleSectionShell>
</template>

<style scoped>
.pro-loading,
.pro-gate {
  display: grid;
  min-height: 100svh;
  place-items: center;
  align-content: center;
  gap: 12px;
  padding: 24px;
  text-align: center;
}

.pro-loading p {
  margin: 0;
  color: var(--text-tertiary);
}

.pro-gate {
  max-width: 680px;
  margin-inline: auto;
}

.pro-gate h1 {
  max-width: 12ch;
  margin: 8px 0 0;
  color: var(--text-primary);
  font-size: clamp(2.4rem, 7vw, 4.8rem);
  letter-spacing: -0.06em;
  line-height: 0.96;
}

.pro-gate > p:not(.pro-kicker) {
  max-width: 560px;
  margin: 4px 0 10px;
  color: var(--text-tertiary);
  line-height: 1.55;
}

.gate-button,
.header-actions button,
.availability-banner button,
.empty-actions button,
.widget-actions button,
.widget-confirm button,
.delete-review button {
  display: inline-flex;
  min-height: 42px;
  align-items: center;
  justify-content: center;
  border: 1.5px solid #151515;
  border-radius: 999px;
  color: #fffef9;
  background: #3d5afe;
  padding: 0 14px;
  font-weight: 760;
  text-decoration: none;
  cursor: pointer;
}

button:disabled {
  cursor: not-allowed;
  opacity: 0.46;
}

.pro-workspace {
  width: 100%;
  min-width: 0;
  max-width: var(--content-wide);
  margin-inline: auto;
  padding: 30px var(--layout-gutter) 64px;
}

.pro-header {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 32px;
  margin-bottom: 24px;
}

.pro-kicker {
  margin: 0 0 3px;
  color: #2847d6;
  font-family: var(--font-mono);
  font-size: 0.68rem;
  font-weight: 800;
  letter-spacing: 0.11em;
  text-transform: uppercase;
}

.pro-header h1 {
  margin: 0;
  color: #151515;
  font-size: clamp(2.3rem, 5vw, 4.2rem);
  letter-spacing: -0.06em;
  line-height: 0.98;
}

.pro-header p:last-child {
  max-width: 700px;
  margin: 8px 0 0;
  color: #6a6861;
  line-height: 1.5;
}

.header-actions,
.empty-actions,
.widget-actions,
.widget-confirm,
.delete-review {
  display: flex;
  flex-wrap: wrap;
  gap: 8px;
}

.pro-action-bar {
  display: grid;
  grid-template-columns: minmax(0, 1fr) minmax(220px, 360px) auto;
  align-items: end;
  gap: 14px;
  margin-bottom: 16px;
  border: 1.5px solid #151515;
  border-radius: 16px;
  background: #fffef9;
  padding: 14px;
}

.pro-view-context {
  display: flex;
  min-width: 0;
  align-items: center;
  flex-wrap: wrap;
  gap: 8px;
}

.pro-view-context code {
  overflow: hidden;
  color: #5c5a54;
  font-family: var(--font-mono);
  font-size: 0.7rem;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.dashboard-switcher {
  display: grid;
  min-width: 0;
  gap: 6px;
}

.dashboard-switcher > span {
  color: #494842;
  font-size: 0.74rem;
  font-weight: 730;
}

.header-actions {
  flex: 0 0 auto;
}

.header-actions .funnel-button,
.empty-actions button:last-child {
  color: #151515;
  background: #d8ff52;
}

.site-bar {
  display: flex;
  align-items: end;
  justify-content: space-between;
  gap: 24px;
  margin-bottom: 16px;
  border: 1.5px solid #151515;
  border-radius: 18px;
  background: #fffef9;
  padding: 16px;
}

.site-picker {
  width: min(100%, 520px);
}

.site-picker > span {
  display: block;
  margin-bottom: 7px;
  color: #494842;
  font-size: 0.82rem;
  font-weight: 730;
}

.site-context {
  display: flex;
  min-width: 0;
  align-items: center;
  flex-wrap: wrap;
  justify-content: flex-end;
  gap: 8px;
}

.site-context code,
.role-chip,
.saved-section > header > span {
  border: 1px solid #d8d2c4;
  border-radius: 999px;
  color: #494842;
  background: #f4f1e8;
  padding: 5px 9px;
  font-family: var(--font-mono);
  font-size: 0.64rem;
}

.role-chip {
  border-color: #8eb61b;
  color: #4d6500;
  background: #f3ffd0;
  font-weight: 760;
  text-transform: capitalize;
}

.role-chip.readonly {
  border-color: #ffb3a7;
  color: #9b2d23;
  background: #ffe9e5;
}

.comparison-note,
.layout-note {
  display: flex;
  align-items: flex-start;
  gap: 12px;
  margin-bottom: 16px;
  border: 1px solid #bac4ff;
  border-radius: 14px;
  background: #eef0ff;
  padding: 14px;
}

.comparison-note > span,
.layout-note > span {
  display: grid;
  width: 34px;
  height: 34px;
  flex: 0 0 auto;
  place-items: center;
  border: 1.5px solid #151515;
  border-radius: 50%;
  color: #151515;
  background: #d8ff52;
  font-weight: 820;
}

.comparison-note strong,
.layout-note strong {
  color: #151515;
  font-size: 0.86rem;
}

.comparison-note p,
.layout-note p {
  margin: 3px 0 0;
  color: #5c5a54;
  font-size: 0.78rem;
  line-height: 1.5;
}

.pro-alert,
.pro-notice {
  margin: 0 0 16px;
  border-radius: 12px;
  padding: 11px 13px;
  font-size: 0.82rem;
}

.pro-alert {
  border: 1px solid #ffb3a7;
  color: #9b2d23;
  background: #ffe9e5;
}

.pro-notice {
  border: 1px solid #8eb61b;
  color: #4d6500;
  background: #f3ffd0;
}

.availability-banner {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 32px;
  margin-bottom: 20px;
  border: 2px solid #151515;
  border-radius: 20px;
  color: #f7f7f2;
  background:
    linear-gradient(120deg, rgb(61 90 254 / 0.54), transparent 44%), #111214;
  padding: clamp(18px, 3vw, 26px);
  box-shadow: 5px 5px 0 #3d5afe;
}

.availability-banner .pro-kicker {
  color: #d8ff52;
}

.availability-banner h2 {
  margin: 0;
  color: #f7f7f2;
  font-size: clamp(1.3rem, 3vw, 2rem);
  letter-spacing: -0.04em;
}

.availability-banner p:last-child {
  max-width: 780px;
  margin: 7px 0 0;
  color: #c4c5c0;
  font-size: 0.82rem;
  line-height: 1.5;
}

.availability-banner > span {
  display: grid;
  padding: 14px;
  text-wrap: balance;
  width: 92px;
  height: 92px;
  flex: 0 0 auto;
  place-items: center;
  transform: rotate(4deg);
  border: 2px solid #151515;
  border-radius: 50%;
  color: #151515;
  background: #d8ff52;
  font-family: var(--font-mono);
  font-size: 0.62rem;
  font-weight: 850;
  letter-spacing: 0.08em;
  line-height: 1.2;
  text-align: center;
}

.availability-banner.error {
  background: #3a1714;
  box-shadow: 5px 5px 0 #ff705c;
}

.builder-layout {
  display: grid;
  grid-template-columns: minmax(0, 1.05fr) minmax(420px, 0.95fr);
  align-items: start;
  gap: 18px;
  scroll-margin-top: 24px;
  margin-bottom: 28px;
}

.builder-layout > :last-child {
  position: sticky;
  top: 20px;
}

.saved-section {
  margin: 28px 0 16px;
  border: 1.5px solid #151515;
  border-radius: 20px;
  background: #fffef9;
  padding: clamp(16px, 3vw, 26px);
}

.saved-section > header,
.widget-card > header {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 16px;
}

.saved-section > header {
  margin-bottom: 18px;
}

.saved-section h2 {
  margin: 0;
  color: #151515;
  font-size: clamp(1.35rem, 3vw, 1.9rem);
  letter-spacing: -0.04em;
}

.workspace-state,
.workspace-empty {
  display: grid;
  min-height: 330px;
  place-items: center;
  align-content: center;
  border: 1px dashed #b7b0a3;
  border-radius: 14px;
  background:
    linear-gradient(180deg, rgb(61 90 254 / 0.05), transparent), #f4f1e8;
  padding: 24px;
  text-align: center;
}

.workspace-state {
  gap: 10px;
}

.workspace-state strong,
.workspace-empty h3 {
  color: #151515;
}

.workspace-empty h3 {
  margin: 2px 0 0;
  font-size: clamp(1.25rem, 3vw, 1.8rem);
  letter-spacing: -0.035em;
}

.workspace-empty > p:not(.pro-kicker) {
  max-width: 620px;
  margin: 7px 0 16px;
  color: #6a6861;
  font-size: 0.84rem;
  line-height: 1.5;
}

.empty-sketch {
  display: grid;
  width: min(100%, 260px);
  height: 100px;
  grid-template-columns: repeat(4, 1fr);
  align-items: end;
  gap: 8px;
  margin-bottom: 18px;
  border-bottom: 2px solid #151515;
  padding: 0 10px;
}

.empty-sketch span {
  border: 1.5px solid #151515;
  border-radius: 8px 8px 0 0;
  background: #3d5afe;
}

.empty-sketch span:nth-child(1) {
  height: 38%;
  background: #d8ff52;
}

.empty-sketch span:nth-child(2) {
  height: 68%;
}

.empty-sketch span:nth-child(3) {
  height: 49%;
  background: #ff705c;
}

.empty-sketch span:nth-child(4) {
  height: 84%;
  background: #a391ff;
}

.widget-grid {
  display: grid;
  grid-template-columns: repeat(2, minmax(0, 1fr));
  gap: 12px;
}

.widget-card {
  min-width: 0;
  border: 1px solid #d8d2c4;
  border-radius: 14px;
  background: #f4f1e8;
  padding: 16px;
}

.widget-card.wide {
  grid-column: 1 / -1;
}

.widget-card header p {
  margin: 0 0 2px;
  color: #2847d6;
  font-family: var(--font-mono);
  font-size: 0.62rem;
  font-weight: 760;
  text-transform: uppercase;
}

.widget-card h3 {
  margin: 0;
  color: #151515;
  font-size: 1rem;
}

.widget-card header > span {
  border: 1px solid #151515;
  border-radius: 999px;
  padding: 4px 8px;
  font-family: var(--font-mono);
  font-size: 0.62rem;
  text-transform: uppercase;
}

.widget-card dl {
  display: grid;
  grid-template-columns: repeat(2, minmax(0, 1fr));
  gap: 8px;
  margin: 14px 0;
}

.widget-card dt {
  color: #6a6861;
  font-size: 0.66rem;
  font-weight: 720;
  text-transform: uppercase;
}

.widget-card dd {
  overflow: hidden;
  margin: 2px 0 0;
  color: #151515;
  font-size: 0.78rem;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.widget-actions button,
.widget-confirm button {
  min-height: 36px;
  color: #151515;
  background: #fffef9;
  font-size: 0.75rem;
}

.widget-confirm {
  align-items: center;
  border-top: 1px solid #d8d2c4;
  padding-top: 12px;
}

.widget-confirm p {
  flex: 1;
  margin: 0;
  color: #9b2d23;
  font-size: 0.76rem;
}

.widget-confirm button:first-of-type {
  color: #fffef9;
  background: #c23e35;
}

.layout-note {
  margin-bottom: 16px;
  border-color: #d8d2c4;
  background: #fffef9;
}

.layout-note > span {
  background: #a391ff;
}

.delete-review {
  align-items: center;
  margin-top: 12px;
  border: 1px solid #ffb3a7;
  border-radius: 14px;
  background: #ffe9e5;
  padding: 14px;
}

.delete-review > div {
  min-width: 0;
  flex: 1;
}

.delete-review strong {
  color: #9b2d23;
  text-transform: capitalize;
}

.delete-review p {
  margin: 3px 0 0;
  color: #6a6861;
  font-size: 0.76rem;
}

.delete-review button {
  color: #151515;
  background: #fffef9;
}

.delete-review button:first-of-type {
  color: #fffef9;
  background: #c23e35;
}

.pro-orbit {
  width: 28px;
  height: 28px;
  border: 2px solid #bac4ff;
  border-top-color: #3d5afe;
  border-radius: 50%;
  animation: pro-orbit 0.72s linear infinite;
}

@keyframes pro-orbit {
  to {
    transform: rotate(360deg);
  }
}

@media (max-width: 1200px) {
  .pro-action-bar {
    grid-template-columns: 1fr auto;
  }

  .dashboard-switcher {
    grid-column: 1 / -1;
    grid-row: 2;
  }

  .builder-layout {
    grid-template-columns: 1fr;
  }

  .builder-layout > :last-child {
    position: static;
  }
}

@media (max-width: 760px) {
  .pro-workspace {
    padding-top: 76px;
  }

  .pro-header,
  .site-bar,
  .availability-banner {
    align-items: flex-start;
    flex-direction: column;
  }

  .header-actions,
  .header-actions button,
  .site-picker {
    width: 100%;
  }

  .pro-action-bar {
    grid-template-columns: 1fr;
  }

  .dashboard-switcher {
    grid-column: auto;
    grid-row: auto;
  }

  .site-context {
    justify-content: flex-start;
  }

  .availability-banner > span {
    width: auto;
    height: auto;
    border-radius: 999px;
    padding: 7px 10px;
  }

  .widget-grid,
  .widget-card dl {
    grid-template-columns: 1fr;
  }

  .widget-card.wide {
    grid-column: auto;
  }
}

@media (max-width: 420px) {
  .pro-workspace {
    padding-inline: 14px;
  }

  .comparison-note,
  .layout-note {
    padding: 12px;
  }

  .saved-section {
    padding: 14px;
  }

  .empty-actions,
  .empty-actions button,
  .delete-review,
  .delete-review button {
    width: 100%;
  }

  .delete-review {
    align-items: stretch;
    flex-direction: column;
  }
}

@media (prefers-reduced-motion: reduce) {
  .pro-orbit {
    animation: none;
  }
}
</style>
