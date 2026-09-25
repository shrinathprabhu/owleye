<script setup lang="ts">
import { apiErrorMessage } from "~/utils/apiError";
type User = {
  id: string;
  email: string;
  name: string | null;
  is_admin: boolean;
  enabled: boolean;
};
const { api } = useApi();
const users = ref<User[]>([]);
const email = ref("");
const password = ref("");
const error = ref("");
const pending = ref(false);
const notice = ref("");
const target = ref<User | null>(null);
const reset = ref("");
const operation = ref<"password" | "enabled">("password");
const verifying = ref(false);
async function load() {
  const r = await api<{ users: User[] }>("/v1/admin/users");
  users.value = r.users;
}
onMounted(() =>
  load().catch(
    (e) => (error.value = apiErrorMessage(e, "Unable to load users.")),
  ),
);
async function create() {
  pending.value = true;
  error.value = "";
  try {
    await api("/v1/admin/users", {
      method: "POST",
      body: { email: email.value, password: password.value },
    });
    email.value = password.value = "";
    notice.value = "User created. Share their credentials privately.";
    await load();
  } catch (e) {
    error.value = apiErrorMessage(e, "Unable to add user.");
  } finally {
    pending.value = false;
  }
}
async function update() {
  verifying.value = false;
  if (!target.value) return;
  pending.value = true;
  try {
    await api(`/v1/admin/users/${target.value.id}`, {
      method: "PUT",
      body:
        operation.value === "password"
          ? { password: reset.value }
          : { enabled: !target.value.enabled },
    });
    target.value = null;
    reset.value = "";
    notice.value = "User updated; their sessions have been revoked.";
    await load();
  } catch (e) {
    error.value = apiErrorMessage(e, "Unable to update user.");
  } finally {
    pending.value = false;
  }
}
</script>
<template>
  <section class="management-card">
    <h2>Instance users</h2>
    <p>
      Only administrators can create accounts. New users can view apps; grant
      app roles below.
    </p>
    <form class="auth-form" @submit.prevent="create">
      <label
        ><span>Email</span
        ><input
          v-model="email"
          type="email"
          autocomplete="off"
          required /></label
      ><label
        ><span>Initial password</span
        ><input
          v-model="password"
          type="password"
          autocomplete="new-password"
          minlength="8"
          required /></label
      ><button class="button primary" :disabled="pending">Add user</button>
    </form>
    <p v-if="error" class="page-alert" role="alert">{{ error }}</p>
    <p v-if="notice" role="status">{{ notice }}</p>
    <ul class="instance-user-list">
      <li v-for="u in users" :key="u.id">
        <div>
          <strong>{{ u.name || u.email }}</strong
          ><small
            >{{ u.is_admin ? "Administrator" : "User" }} ·
            {{ u.enabled ? "Active" : "Disabled" }}</small
          >
        </div>
        <button
          class="button secondary compact"
          @click="
            target = u;
            operation = 'password';
            reset = '';
          "
        >
          Reset password</button
        ><button
          v-if="!u.is_admin"
          class="button secondary compact"
          @click="
            target = u;
            operation = 'enabled';
          "
        >
          {{ u.enabled ? "Disable" : "Enable" }}
        </button>
      </li>
    </ul>
    <form
      v-if="target && !verifying"
      class="auth-form"
      @submit.prevent="verifying = true"
    >
      <h3>
        {{
          operation === "password" ? "Reset password for" : "Change access for"
        }}
        {{ target.email }}
      </h3>
      <label v-if="operation === 'password'"
        ><span>New password</span
        ><input
          v-model="reset"
          type="password"
          autocomplete="new-password"
          minlength="8"
          required /></label
      ><button class="button primary" :disabled="pending">
        Confirm identity and continue</button
      ><button type="button" class="button secondary" @click="target = null">
        Cancel
      </button>
    </form>
    <Reauthenticate
      v-if="verifying"
      @verified="update"
      @cancel="verifying = false"
    />
  </section>
</template>
<style scoped>
.instance-user-list {
  list-style: none;
  padding: 0;
}
.instance-user-list li {
  display: flex;
  gap: 12px;
  align-items: center;
  flex-wrap: wrap;
  padding: 18px 0;
  border-bottom: 1px solid var(--line);
}
.instance-user-list li div {
  flex: 1;
  min-width: 160px;
  overflow-wrap: anywhere;
}
.instance-user-list small {
  display: block;
}
</style>
