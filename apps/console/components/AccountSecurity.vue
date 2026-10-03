<script setup lang="ts">
import type { WorkspaceUser } from "~/types/workspace";
import { apiErrorMessage } from "~/utils/apiError";
import { downloadBrowserFile } from "~/utils/download";
const props = defineProps<{ apiBase: string; user: WorkspaceUser }>();
const emit = defineEmits<{ changed: [] }>();
const { api } = useApi();
const current = ref("");
const password = ref("");
const confirm = ref("");
const error = ref("");
const notice = ref("");
const pending = ref(false);
const secret = ref("");
const uri = ref("");
const code = ref("");
const action = ref<"export" | "delete" | null>(null);
const deletion = ref("");
async function run(fn: () => Promise<void>) {
  pending.value = true;
  error.value = "";
  notice.value = "";
  try {
    await fn();
  } catch (e) {
    error.value = apiErrorMessage(e, "Unable to complete this action.");
  } finally {
    pending.value = false;
  }
}
async function change() {
  await run(async () => {
    if (password.value !== confirm.value)
      throw new Error("Passwords do not match.");
    await api("/v1/auth/password", {
      method: "PUT",
      body: { current_password: current.value, password: password.value },
    });
    current.value = password.value = confirm.value = "";
    notice.value = "Password changed. Sign in again.";
    emit("changed");
  });
}
async function setup() {
  await run(async () => {
    const r = await api<{ secret: string; otpauth_url: string }>(
      "/v1/auth/2fa/setup",
      { method: "POST" },
    );
    secret.value = r.secret;
    uri.value = r.otpauth_url;
  });
}
async function twoFactor() {
  await run(async () => {
    await api(
      `/v1/auth/2fa/${props.user.two_factor_enabled ? "disable" : "enable"}`,
      {
        method: "POST",
        body: props.user.two_factor_enabled
          ? { code: code.value }
          : { code: code.value, secret: secret.value },
      },
    );
    secret.value = uri.value = code.value = "";
    emit("changed");
  });
}
async function verified() {
  const next = action.value;
  action.value = null;
  await run(async () => {
    if (next === "export") {
      await downloadBrowserFile(
        `${props.apiBase}/v1/account/export`,
        "owleye-account.json",
      );
    } else {
      await api("/v1/account", {
        method: "DELETE",
        body: { confirmation: deletion.value },
      });
      emit("changed");
    }
  });
}
</script>
<template>
  <div class="account-security-grid">
    <section class="management-card">
      <h2>Change password</h2>
      <p>
        At least 8 characters. Changing your password signs out all sessions.
      </p>
      <form class="auth-form" @submit.prevent="change">
        <label
          ><span>Current password</span
          ><input
            v-model="current"
            type="password"
            autocomplete="current-password"
            required /></label
        ><label
          ><span>New password</span
          ><input
            v-model="password"
            type="password"
            autocomplete="new-password"
            minlength="8"
            required /></label
        ><label
          ><span>Confirm new password</span
          ><input
            v-model="confirm"
            type="password"
            autocomplete="new-password"
            minlength="8"
            required /></label
        ><button
          class="button primary"
          :disabled="pending || password !== confirm"
        >
          Change password
        </button>
      </form>
    </section>
    <section class="management-card">
      <h2>Two-factor authentication</h2>
      <p>
        {{
          user.two_factor_enabled
            ? "Your authenticator is enabled."
            : "Add a code from your authenticator to each sign-in."
        }}
      </p>
      <button
        v-if="!user.two_factor_enabled && !secret"
        class="button secondary"
        :disabled="pending"
        @click="setup"
      >
        Set up authenticator</button
      ><AuthenticatorSetup v-if="secret" :secret="secret" :uri="uri" />
      <form
        v-if="secret || user.two_factor_enabled"
        class="auth-form"
        @submit.prevent="twoFactor"
      >
        <label
          ><span>Authenticator code</span
          ><input
            v-model="code"
            inputmode="numeric"
            maxlength="6"
            autocomplete="one-time-code"
            required /></label
        ><button class="button secondary" :disabled="pending">
          {{
            user.two_factor_enabled
              ? "Disable authenticator"
              : "Enable authenticator"
          }}
        </button>
      </form>
    </section>
    <section class="management-card">
      <h2>Account data</h2>
      <button
        class="button secondary"
        :disabled="pending"
        @click="action = 'export'"
      >
        Export my account</button
      ><template v-if="!user.is_admin"
        ><p>
          Type DELETE MY ACCOUNT to delete your account. Owned apps must be
          deleted first. App ownership transfer is not currently supported.
        </p>
        <input v-model="deletion" aria-label="Deletion confirmation" /><button
          class="button danger"
          :disabled="pending || deletion !== 'DELETE MY ACCOUNT'"
          @click="action = 'delete'"
        >
          Delete my account
        </button></template
      >
    </section>
    <Reauthenticate
      v-if="action"
      @verified="verified"
      @cancel="action = null"
    />
    <p v-if="error" class="page-alert" role="alert">{{ error }}</p>
    <p v-if="notice" role="status">{{ notice }}</p>
  </div>
</template>
