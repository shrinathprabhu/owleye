<script setup lang="ts">
import type { ConsoleSelectOption } from "~/types/console";
import type {
  ProViewDashboard,
  ProViewDeleteScope,
  ProViewShare,
  ProViewShareCandidate,
} from "~/types/pro-view";

const props = withDefaults(
  defineProps<{
    canDeleteAll?: boolean;
    canRemoveForMe?: boolean;
    canRevokeOthers?: boolean;
    canShare?: boolean;
    candidates?: ProViewShareCandidate[];
    invitePreview?: ProViewDashboard | null;
    invitePreviewPending?: boolean;
    pending?: boolean;
    pendingInvites?: ProViewDashboard[];
    shares?: ProViewShare[];
  }>(),
  {
    canDeleteAll: false,
    canRemoveForMe: false,
    canRevokeOthers: false,
    canShare: false,
    candidates: () => [],
    invitePreview: null,
    invitePreviewPending: false,
    pending: false,
    pendingInvites: () => [],
    shares: () => [],
  },
);

const emit = defineEmits<{
  decide: [dashboardId: string, decision: "accept" | "dismiss"];
  delete: [scope: ProViewDeleteScope];
  invite: [userId: string];
  "preview-invite": [dashboardId: string];
  revoke: [userId: string];
}>();

const selectedCandidateId = ref("");
const deleteScope = ref<ProViewDeleteScope>("all");

const sharedUserIds = computed(
  () =>
    new Set(
      props.shares
        .filter((share) => ["accepted", "pending"].includes(share.status))
        .map((share) => share.user_id),
    ),
);
const candidateOptions = computed<ConsoleSelectOption[]>(() =>
  props.candidates
    .filter((candidate) => !sharedUserIds.value.has(candidate.id))
    .map((candidate) => ({
      badge: candidate.role.replace("_", " "),
      description: candidate.email,
      label: candidate.name || candidate.email,
      meta: `Member ID · ${candidate.id}`,
      value: candidate.id,
    })),
);
const availableDeleteScopes = computed<ProViewDeleteScope[]>(() => {
  const scopes: ProViewDeleteScope[] = [];
  if (props.canRemoveForMe) scopes.push("self");
  if (props.canRevokeOthers) scopes.push("others");
  if (props.canDeleteAll) scopes.push("all");
  return scopes;
});

watch(
  availableDeleteScopes,
  (scopes) => {
    if (!scopes.includes(deleteScope.value)) {
      deleteScope.value = scopes[0] ?? "all";
    }
  },
  { immediate: true },
);

function invite() {
  if (!selectedCandidateId.value || !props.canShare || props.pending) return;
  emit("invite", selectedCandidateId.value);
  selectedCandidateId.value = "";
}

function canRevoke(share: ProViewShare) {
  return (
    props.canRevokeOthers &&
    (share.status === "accepted" || share.status === "pending")
  );
}
</script>

<template>
  <article class="sharing-card">
    <header class="sharing-header">
      <div>
        <p>Read-only sharing</p>
        <h2>Share the view, not the steering wheel.</h2>
      </div>
      <span>Preview first</span>
    </header>

    <section
      v-if="pendingInvites.length"
      class="incoming-invites"
      aria-labelledby="incoming-invites-heading"
    >
      <div>
        <p class="section-kicker">Waiting for you</p>
        <h3 id="incoming-invites-heading">Shared Pro View invites</h3>
      </div>

      <article v-for="inviteItem in pendingInvites" :key="inviteItem.id">
        <div class="invite-summary">
          <span>
            <strong>{{ inviteItem.name }}</strong>
            <small>
              From
              {{
                inviteItem.created_by?.name ||
                inviteItem.created_by?.email ||
                "an app teammate"
              }}
            </small>
          </span>
          <button
            :disabled="pending || invitePreviewPending"
            type="button"
            @click="emit('preview-invite', inviteItem.id)"
          >
            {{ invitePreviewPending ? "Loading…" : "Preview" }}
          </button>
        </div>

        <div v-if="invitePreview?.id === inviteItem.id" class="invite-preview">
          <p>
            {{ invitePreview.widgets?.length ?? 0 }} read-only
            {{
              (invitePreview.widgets?.length ?? 0) === 1 ? "widget" : "widgets"
            }}
          </p>
          <ul v-if="invitePreview.widgets?.length">
            <li
              v-for="widget in invitePreview.widgets"
              :key="widget.id || widget.title"
            >
              <strong>{{ widget.title }}</strong>
              <span>{{ widget.kind }} · {{ widget.visualization }}</span>
            </li>
          </ul>
          <p v-else class="availability-note">
            This shared canvas is empty. You can still keep it for future
            updates.
          </p>
          <div class="share-actions">
            <button
              :disabled="pending"
              type="button"
              @click="emit('decide', inviteItem.id, 'accept')"
            >
              Keep in my view
            </button>
            <button
              class="quiet-action"
              :disabled="pending"
              type="button"
              @click="emit('decide', inviteItem.id, 'dismiss')"
            >
              Dismiss invite
            </button>
          </div>
        </div>
      </article>
    </section>

    <div v-if="canRemoveForMe" class="incoming-share">
      <div>
        <strong>This is a teammate's read-only Pro View.</strong>
        <p>
          Removing it hides only your copy. The creator and other recipients
          keep theirs.
        </p>
      </div>
      <button
        class="quiet-action"
        :disabled="pending"
        type="button"
        @click="emit('delete', 'self')"
      >
        Remove for me
      </button>
    </div>

    <template v-if="canShare">
      <form class="invite-form" @submit.prevent="invite">
        <div class="share-select-field">
          <span id="pro-view-member-label">Invite an app member</span>
          <ConsoleSelect
            :disabled="pending || !candidateOptions.length"
            labelledby="pro-view-member-label"
            :model-value="selectedCandidateId"
            :options="candidateOptions"
            placeholder="Choose a teammate"
            @change="selectedCandidateId = String($event)"
          />
        </div>
        <button :disabled="pending || !selectedCandidateId" type="submit">
          Invite to preview
        </button>
      </form>
      <p v-if="!candidateOptions.length" class="availability-note">
        Every eligible app member already has an active invite or shared copy.
      </p>

      <div v-if="shares.length" class="share-list">
        <div v-for="share in shares" :key="share.user_id">
          <span>
            <strong>{{ share.display_name || share.email }}</strong>
            <small v-if="share.display_name">{{ share.email }}</small>
          </span>
          <span :class="['share-status', share.status]">{{
            share.status
          }}</span>
          <button
            v-if="canRevoke(share)"
            class="quiet-action compact-action"
            :disabled="pending"
            type="button"
            @click="emit('revoke', share.user_id)"
          >
            Revoke
          </button>
        </div>
      </div>
      <p v-else class="empty-shares">No viewers invited yet.</p>
    </template>

    <details v-if="availableDeleteScopes.length">
      <summary>Deletion controls and what they actually delete</summary>
      <div class="delete-options">
        <label v-if="canRemoveForMe">
          <input
            v-model="deleteScope"
            :disabled="pending"
            type="radio"
            value="self"
          />
          <span>
            <strong>Delete for me</strong>
            <small>Hide your shared copy. Everyone else keeps theirs.</small>
          </span>
        </label>
        <label v-if="canRevokeOthers">
          <input
            v-model="deleteScope"
            :disabled="pending"
            type="radio"
            value="others"
          />
          <span>
            <strong>Delete for others</strong>
            <small
              >Revoke shared copies while keeping your canonical view.</small
            >
          </span>
        </label>
        <label v-if="canDeleteAll">
          <input
            v-model="deleteScope"
            :disabled="pending"
            type="radio"
            value="all"
          />
          <span>
            <strong>Delete for all</strong>
            <small>Remove the canonical view and every shared copy.</small>
          </span>
        </label>
      </div>
      <button
        class="delete-button"
        :disabled="pending"
        type="button"
        @click="emit('delete', deleteScope)"
      >
        Review deletion
      </button>
    </details>
  </article>
</template>

<style scoped>
.sharing-card {
  border: 1.5px solid #151515;
  border-radius: 18px;
  background: #fffef9;
  padding: clamp(16px, 3vw, 24px);
}

.sharing-header,
.incoming-share,
.invite-form,
.share-list > div,
.invite-summary {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 16px;
}

.sharing-header {
  margin-bottom: 18px;
}

.sharing-header p,
.section-kicker {
  margin: 0 0 2px;
  color: #2847d6;
  font-family: var(--font-mono);
  font-size: 0.64rem;
  font-weight: 800;
  letter-spacing: 0.1em;
  text-transform: uppercase;
}

h2,
h3 {
  margin: 0;
  color: #151515;
  letter-spacing: -0.025em;
}

h2 {
  font-size: 1.05rem;
}

h3 {
  font-size: 0.96rem;
}

.sharing-header > span,
.share-status {
  border: 1px solid #b7b0a3;
  border-radius: 999px;
  color: #494842;
  background: #f4f1e8;
  padding: 4px 8px;
  font-family: var(--font-mono);
  font-size: 0.62rem;
  font-weight: 730;
  text-transform: uppercase;
  white-space: nowrap;
}

.incoming-invites {
  display: grid;
  gap: 10px;
  margin-bottom: 18px;
  border: 1px solid #bac4ff;
  border-radius: 14px;
  background: #eef0ff;
  padding: 14px;
}

.incoming-invites > article {
  border: 1px solid #cbd2ff;
  border-radius: 11px;
  background: #fffef9;
  padding: 11px;
}

.invite-summary > span,
.share-list > div > span:first-child {
  display: grid;
  min-width: 0;
}

.invite-summary small,
.share-list small,
.delete-options small {
  overflow: hidden;
  color: #6a6861;
  font-size: 0.7rem;
  text-overflow: ellipsis;
}

.invite-preview {
  margin-top: 10px;
  border-top: 1px solid #d8d2c4;
  padding-top: 10px;
}

.invite-preview > p {
  margin: 0 0 8px;
  color: #6a6861;
  font-size: 0.75rem;
}

.invite-preview ul {
  display: grid;
  gap: 6px;
  margin: 0 0 10px;
  padding: 0;
  list-style: none;
}

.invite-preview li {
  display: flex;
  justify-content: space-between;
  gap: 10px;
  border-radius: 8px;
  background: #f4f1e8;
  padding: 8px 10px;
  font-size: 0.72rem;
}

.invite-preview li span {
  color: #6a6861;
}

.incoming-share {
  align-items: flex-start;
  margin-bottom: 16px;
  border: 1px solid #bac4ff;
  border-radius: 12px;
  background: #eef0ff;
  padding: 14px;
}

.incoming-share strong {
  color: #151515;
  font-size: 0.86rem;
}

.incoming-share p,
.availability-note,
.empty-shares {
  margin: 3px 0 0;
  color: #6a6861;
  font-size: 0.76rem;
  line-height: 1.5;
}

.share-actions {
  display: flex;
  flex-wrap: wrap;
  gap: 7px;
}

.invite-form {
  align-items: end;
}

.share-select-field {
  display: grid;
  min-width: 0;
  flex: 1;
  gap: 7px;
  color: #494842;
  font-size: 0.8rem;
  font-weight: 720;
}

button {
  min-height: 42px;
  border: 1.5px solid #151515;
  border-radius: 999px;
  color: #151515;
  background: #d8ff52;
  padding: 0 14px;
  font-weight: 750;
  cursor: pointer;
}

button:disabled {
  cursor: not-allowed;
  opacity: 0.46;
}

.quiet-action {
  color: #151515;
  background: #fffef9;
}

.compact-action {
  min-height: 32px;
  padding-inline: 10px;
  font-size: 0.7rem;
}

.share-list {
  display: grid;
  gap: 8px;
  margin-top: 18px;
}

.share-list > div {
  border: 1px solid #d8d2c4;
  border-radius: 10px;
  background: #f4f1e8;
  padding: 10px 12px;
}

.share-list strong,
.delete-options strong {
  color: #151515;
  font-size: 0.8rem;
}

.share-status.accepted {
  border-color: #8eb61b;
  color: #4d6500;
  background: #f3ffd0;
}

.share-status.dismissed,
.share-status.revoked {
  opacity: 0.65;
}

.empty-shares {
  margin-top: 16px;
  border: 1px dashed #b7b0a3;
  border-radius: 10px;
  padding: 12px;
  text-align: center;
}

details {
  margin-top: 18px;
  border-top: 1px solid #d8d2c4;
  padding-top: 16px;
}

summary {
  color: #151515;
  font-size: 0.8rem;
  font-weight: 730;
  cursor: pointer;
}

.delete-options {
  display: grid;
  grid-template-columns: repeat(3, minmax(0, 1fr));
  gap: 8px;
  margin-top: 12px;
}

.delete-options label {
  display: flex;
  align-items: flex-start;
  gap: 8px;
  border: 1px solid #d8d2c4;
  border-radius: 10px;
  background: #f4f1e8;
  padding: 10px;
  cursor: pointer;
}

.delete-options input {
  margin: 3px 0 0;
  accent-color: #3d5afe;
}

.delete-options span {
  display: grid;
  gap: 2px;
}

.delete-button {
  margin-top: 12px;
  color: #fffef9;
  background: #c23e35;
}

@media (max-width: 720px) {
  .incoming-share,
  .invite-form {
    align-items: stretch;
    flex-direction: column;
  }

  .share-actions,
  .invite-form button {
    width: 100%;
  }

  .share-actions button {
    flex: 1;
  }

  .delete-options {
    grid-template-columns: 1fr;
  }
}

@media (max-width: 420px) {
  .sharing-header,
  .invite-summary,
  .share-list > div {
    align-items: flex-start;
    flex-direction: column;
  }

  .share-actions {
    flex-direction: column;
  }
}
</style>
