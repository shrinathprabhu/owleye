<script setup lang="ts">
import type { ConsoleSelectOption } from "~/types/console";
import { apiErrorMessage as requestError } from "~/utils/apiError";
import { routes } from "~/utils/routes";

type MemberRole = "admin" | "owner" | "read_only";
type SiteMember = {
  avatar_url?: string | null;
  can_remove: boolean;
  email: string;
  id: string;
  is_current_user: boolean;
  joined_at: string;
  name?: string | null;
  role: MemberRole;
};

const props = defineProps<{
  demo?: boolean;
  siteId?: string;
}>();
const { api } = useApi();

const members = ref<SiteMember[]>([]);
const email = ref("");
const role = ref("read_only");
const loading = ref(false);
const mutationKey = ref("");
const error = ref("");
const notice = ref("");
let requestController: AbortController | undefined;
let mutationSequence = 0;
const roleOptions: ConsoleSelectOption[] = [
  {
    description:
      "Can view analytics and shared work, but cannot change the app",
    label: "Read only",
    value: "read_only",
  },
  {
    description:
      "Can manage rules and members, but cannot change owner settings",
    label: "Admin",
    value: "admin",
  },
];

watch(
  () => props.siteId,
  () => {
    requestController?.abort();
    mutationSequence += 1;
    requestController = undefined;
    members.value = [];
    loading.value = false;
    mutationKey.value = "";
    error.value = "";
    notice.value = "";
    void loadMembers();
  },
  { immediate: true },
);

onBeforeUnmount(() => requestController?.abort());

async function loadMembers() {
  const siteId = props.siteId;
  if (!siteId) {
    members.value = [];
    loading.value = false;
    return;
  }
  requestController?.abort();
  const controller = new AbortController();
  requestController = controller;
  loading.value = true;
  error.value = "";
  try {
    const response = await api<SiteMember[]>(routes.sites.members(siteId), {
      signal: controller.signal,
    });
    if (controller.signal.aborted || props.siteId !== siteId) return;
    members.value = response;
  } catch (cause) {
    if (controller.signal.aborted || props.siteId !== siteId) return;
    members.value = [];
    error.value = requestError(cause, "App members could not be loaded.");
  } finally {
    if (requestController === controller) {
      requestController = undefined;
      loading.value = false;
    }
  }
}

async function addMember() {
  const siteId = props.siteId;
  if (!siteId || props.demo || mutationKey.value) return;
  const operation = ++mutationSequence;
  mutationKey.value = "add";
  error.value = "";
  notice.value = "";
  try {
    await api(routes.sites.members(siteId), {
      body: { email: email.value, role: role.value },
      method: "POST",
    });
    if (operation !== mutationSequence || props.siteId !== siteId) return;
    notice.value = `${email.value.trim()} now has app access.`;
    email.value = "";
    role.value = "read_only";
    await loadMembers();
  } catch (cause) {
    if (operation !== mutationSequence || props.siteId !== siteId) return;
    error.value = requestError(cause, "That member could not be added.");
  } finally {
    if (operation === mutationSequence) mutationKey.value = "";
  }
}

async function updateRole(member: SiteMember, nextRole: number | string) {
  const siteId = props.siteId;
  if (!siteId || props.demo || member.role === "owner" || mutationKey.value)
    return;
  const operation = ++mutationSequence;
  mutationKey.value = member.id;
  error.value = "";
  notice.value = "";
  try {
    await api(routes.sites.member(siteId, member.id), {
      body: { role: String(nextRole) },
      method: "PUT",
    });
    if (operation !== mutationSequence || props.siteId !== siteId) return;
    notice.value = `${member.name || member.email} is now ${String(nextRole).replace("_", " ")}.`;
    await loadMembers();
  } catch (cause) {
    if (operation !== mutationSequence || props.siteId !== siteId) return;
    error.value = requestError(cause, "That role could not be changed.");
  } finally {
    if (operation === mutationSequence) mutationKey.value = "";
  }
}

async function removeMember(member: SiteMember) {
  const siteId = props.siteId;
  if (!siteId || props.demo || !member.can_remove || mutationKey.value) return;
  const operation = ++mutationSequence;
  mutationKey.value = member.id;
  error.value = "";
  notice.value = "";
  try {
    await api(routes.sites.member(siteId, member.id), {
      method: "DELETE",
    });
    if (operation !== mutationSequence || props.siteId !== siteId) return;
    notice.value = `${member.name || member.email} no longer has app access.`;
    await loadMembers();
  } catch (cause) {
    if (operation !== mutationSequence || props.siteId !== siteId) return;
    error.value = requestError(cause, "That member could not be removed.");
  } finally {
    if (operation === mutationSequence) mutationKey.value = "";
  }
}

function initials(member: SiteMember) {
  return (member.name || member.email)
    .split(/[\s@._-]+/)
    .filter(Boolean)
    .slice(0, 2)
    .map((part) => part[0]?.toLocaleUpperCase())
    .join("");
}
</script>

<template>
  <section class="members-layout">
    <article class="management-card member-invite-card">
      <header class="management-card-header">
        <div>
          <p class="panel-kicker">Existing user email</p>
          <h2>Add a member</h2>
        </div>
        <span v-if="demo" class="status-badge neutral">Read only</span>
      </header>
      <p class="management-copy">
        Admins can manage rules and members. Read-only users can view analytics
        without making changes.
      </p>
      <form class="member-invite-form" @submit.prevent="addMember">
        <label>
          <span>Email</span>
          <input
            v-model="email"
            autocomplete="email"
            :disabled="demo"
            inputmode="email"
            placeholder="teammate@example.com"
            required
            type="text"
            autocapitalize="none"
            :spellcheck="false"
          />
        </label>
        <div class="explorer-select-field">
          <span id="new-member-role-label">Role</span>
          <ConsoleSelect
            :disabled="demo"
            :labelledby="'new-member-role-label'"
            :model-value="role"
            :options="roleOptions"
            @change="role = String($event)"
          />
        </div>
        <button class="button primary" :disabled="demo || Boolean(mutationKey)">
          {{ mutationKey === "add" ? "Adding…" : "Add member" }}
        </button>
      </form>
      <p v-if="demo" class="field-help">
        Roles are served by the API; changes are disabled in this workspace.
      </p>
    </article>

    <article class="management-card management-card-wide">
      <header class="management-card-header">
        <div>
          <p class="panel-kicker">App access</p>
          <h2>People with access</h2>
        </div>
        <button
          class="text-button"
          :disabled="loading"
          type="button"
          @click="loadMembers"
        >
          Refresh
        </button>
      </header>
      <div v-if="loading" class="management-loading" role="status">
        <span class="state-orbit" aria-hidden="true"></span>
        <span>Loading members…</span>
      </div>
      <div v-else-if="members.length" class="member-list">
        <article v-for="member in members" :key="member.id" class="member-row">
          <div class="member-avatar" aria-hidden="true">
            {{ initials(member) }}
          </div>
          <div class="member-copy">
            <strong>{{ member.name || member.email }}</strong>
            <span v-if="member.name">{{ member.email }}</span>
            <small v-if="member.is_current_user">You</small>
          </div>
          <span v-if="member.role === 'owner'" class="status-badge success"
            >Owner</span
          >
          <ConsoleSelect
            v-else
            class="member-role-select"
            :disabled="demo || Boolean(mutationKey) || member.is_current_user"
            :model-value="member.role"
            :options="roleOptions"
            :aria-label="`Role for ${member.name || member.email}`"
            @change="updateRole(member, $event)"
          />
          <button
            v-if="member.can_remove"
            class="button danger-quiet compact"
            :disabled="demo || Boolean(mutationKey)"
            type="button"
            @click="removeMember(member)"
          >
            {{ mutationKey === member.id ? "Working…" : "Remove" }}
          </button>
        </article>
      </div>
      <div v-else class="management-empty">
        <strong>No members returned</strong>
        <p>The owner can add the first collaborator above.</p>
      </div>
      <p v-if="notice" class="inline-notice" role="status">{{ notice }}</p>
      <p v-if="error" class="inline-error" role="alert">{{ error }}</p>
    </article>
  </section>
</template>
