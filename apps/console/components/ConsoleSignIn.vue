<script setup lang="ts">
import { apiErrorMessage } from "~/utils/apiError";
const props = withDefaults(
  defineProps<{
    apiBase: string;
    error?: string;
    initialChallengeId?: string;
    privacyNoticeUrl?: string;
    termsUrl?: string;
  }>(),
  { error: "", initialChallengeId: "" },
);
const emit = defineEmits<{ authenticated: []; "clear-error": [] }>();
const { api } = useApi();
const email = ref("");
const password = ref("");
const code = ref("");
const challenge = ref(props.initialChallengeId);
const pending = ref(false);
const error = ref("");
async function login() {
  if (pending.value) return;
  pending.value = true;
  error.value = "";
  emit("clear-error");
  try {
    const result = await api<{ status: string; challenge_id?: string }>(
      challenge.value ? "/v1/auth/2fa/verify" : "/v1/auth/login",
      {
        method: "POST",
        body: challenge.value
          ? { challenge_id: challenge.value, code: code.value }
          : { email: email.value, password: password.value },
      },
    );
    password.value = "";
    if (result.status === "requires_2fa") {
      challenge.value = result.challenge_id ?? "";
      return;
    }
    emit("authenticated");
  } catch (cause) {
    error.value = apiErrorMessage(
      cause,
      "Unable to sign in. Check your credentials.",
    );
  } finally {
    pending.value = false;
  }
}
</script>
<template>
  <main id="main-content" class="auth-shell">
    <section class="auth-panel">
      <BrandLockup />
      <p class="eyebrow">Your analytics, on your server</p>
      <h1>Welcome back.</h1>
      <p>
        {{
          challenge
            ? "Enter your authenticator code."
            : "Sign in with the account created by your administrator."
        }}
      </p>
      <form class="auth-form" @submit.prevent="login">
        <template v-if="!challenge"
          ><label
            ><span>Email</span
            ><input
              v-model="email"
              type="email"
              autocomplete="username"
              required /></label
          ><label
            ><span>Password</span
            ><input
              v-model="password"
              type="password"
              autocomplete="current-password"
              required /></label
        ></template>
        <label v-else
          ><span>Authenticator code</span
          ><input
            v-model="code"
            inputmode="numeric"
            autocomplete="one-time-code"
            maxlength="6"
            required
        /></label>
        <p v-if="error || props.error" class="page-alert" role="alert">
          {{ error || props.error }}
        </p>
        <button class="button primary" :disabled="pending">
          {{ pending ? "Signing in…" : "Sign in" }}
        </button>
        <button
          v-if="challenge"
          class="button secondary"
          type="button"
          @click="
            challenge = '';
            code = '';
          "
        >
          Back to sign in
        </button>
      </form>
      <p class="panel-copy">
        Need access or a password reset? Contact your server administrator.
      </p>
    </section>
  </main>
</template>
