<script setup lang="ts">
import { apiErrorMessage } from "~/utils/apiError";
const emit = defineEmits<{ updated: [name: string] }>();
const { api } = useApi();
const name = ref("");
const email = ref("");
const pending = ref(false);
const error = ref("");
const notice = ref("");
onMounted(async () => {
  try {
    const p = await api<{ name: string | null; email: string }>(
      "/v1/account/profile",
    );
    name.value = p.name ?? "";
    email.value = p.email;
  } catch (e) {
    error.value = apiErrorMessage(e, "Unable to load your profile.");
  }
});
async function save() {
  pending.value = true;
  error.value = "";
  notice.value = "";
  try {
    await api("/v1/account/profile", {
      method: "PUT",
      body: { name: name.value },
    });
    emit("updated", name.value);
    notice.value = "Profile saved.";
  } catch (e) {
    error.value = apiErrorMessage(e, "Unable to save.");
  } finally {
    pending.value = false;
  }
}
</script>
<template>
  <section class="management-card">
    <header class="management-card-header">
      <div>
        <p class="panel-kicker">Your account</p>
        <h2>Profile</h2>
      </div>
    </header>
    <form class="auth-form" @submit.prevent="save">
      <label
        ><span>Name</span
        ><input v-model="name" maxlength="120" required /></label
      ><label><span>Email</span><input :value="email" readonly /></label
      ><button class="button primary" :disabled="pending">Save profile</button>
    </form>
    <p v-if="error" role="alert" class="page-alert">{{ error }}</p>
    <p v-if="notice" role="status">{{ notice }}</p>
  </section>
</template>
