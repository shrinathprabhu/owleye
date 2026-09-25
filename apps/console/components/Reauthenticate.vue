<script setup lang="ts">
import { apiErrorMessage } from "~/utils/apiError";
const emit = defineEmits<{ verified: []; cancel: [] }>();
const { api } = useApi();
const challenge = ref("");
const method = ref("password");
const code = ref("");
const error = ref("");
const pending = ref(false);
async function start() {
  pending.value = true;
  try {
    const r = await api<{ challenge_id: string; method: string }>(
      "/v1/auth/step-up/start",
      { method: "POST" },
    );
    challenge.value = r.challenge_id;
    method.value = r.method;
    error.value = "";
  } catch (e) {
    error.value = apiErrorMessage(e, "Unable to start verification.");
  } finally {
    pending.value = false;
  }
}
onMounted(start);
async function verify() {
  pending.value = true;
  error.value = "";
  try {
    await api("/v1/auth/step-up/verify", {
      method: "POST",
      body: { challenge_id: challenge.value, code: code.value },
    });
    code.value = "";
    emit("verified");
  } catch (e) {
    error.value = apiErrorMessage(e, "Verification failed.");
  } finally {
    pending.value = false;
  }
}
</script>
<template>
  <section class="management-card" aria-label="Confirm your identity">
    <h2>Confirm your identity</h2>
    <form class="auth-form" @submit.prevent="verify">
      <label
        ><span>{{
          method === "totp" ? "Authenticator code" : "Current password"
        }}</span
        ><input
          v-model="code"
          :type="method === 'totp' ? 'text' : 'password'"
          :autocomplete="
            method === 'totp' ? 'one-time-code' : 'current-password'
          "
          required
      /></label>
      <p v-if="error" class="page-alert" role="alert">{{ error }}</p>
      <button class="button primary" :disabled="pending || !challenge">
        Verify</button
      ><button
        v-if="!challenge"
        class="button secondary"
        type="button"
        :disabled="pending"
        @click="start"
      >
        Retry</button
      ><button class="button secondary" type="button" @click="emit('cancel')">
        Cancel
      </button>
    </form>
  </section>
</template>
