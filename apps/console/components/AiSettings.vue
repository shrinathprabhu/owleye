<script setup lang="ts">
import { apiErrorMessage } from "~/utils/apiError";
const props = defineProps<{ siteId?: string }>();
const { api } = useApi();
const enabled = ref(false);
const pending = ref(false);
const error = ref("");
const notice = ref("");
watch(
  () => props.siteId,
  async (id) => {
    if (!id) return;
    try {
      const r = await api<{ enabled: boolean }>(`/v1/sites/${id}/ai`);
      enabled.value = r.enabled;
    } catch (e) {
      error.value = apiErrorMessage(e, "Unable to load AI settings.");
    }
  },
  { immediate: true },
);
async function save() {
  pending.value = true;
  error.value = "";
  try {
    await api(`/v1/sites/${props.siteId}/ai`, {
      method: "PUT",
      body: { enabled: enabled.value },
    });
    notice.value = "AI settings saved.";
  } catch (e) {
    error.value = apiErrorMessage(e, "Unable to save AI settings.");
  } finally {
    pending.value = false;
  }
}
</script>
<template>
  <section class="management-card">
    <h2>AI assistant</h2>
    <p>
      Unlimited prompts for all app members. Your server's configured provider
      handles AI requests.
    </p>
    <form @submit.prevent="save">
      <label
        ><input v-model="enabled" type="checkbox" /> Enable AI for this
        app</label
      ><button class="button primary" :disabled="pending || !siteId">
        Save AI settings
      </button>
    </form>
    <p v-if="error" role="alert" class="page-alert">{{ error }}</p>
    <p v-if="notice" role="status">{{ notice }}</p>
  </section>
</template>
