<script setup lang="ts">
import { apiErrorMessage } from "~/utils/apiError";
const { api } = useApi();
const release = ref<{
  current: string;
  latest: string | null;
  url: string;
  update_available: boolean;
} | null>(null);
const error = ref("");
const pending = ref(false);
async function check() {
  pending.value = true;
  error.value = "";
  try {
    release.value = await api("/v1/system/release");
  } catch (e) {
    error.value = apiErrorMessage(e, "Release check unavailable.");
  } finally {
    pending.value = false;
  }
}
onMounted(check);
</script>
<template>
  <section class="management-card">
    <h2>Server updates</h2>
    <p v-if="release">
      Installed: {{ release.current }}.
      {{
        release.update_available
          ? `Version ${release.latest} is available.`
          : release.latest
            ? "You are up to date."
            : "No published release yet."
      }}
    </p>
    <p>
      Run <code>./update.sh</code> on your server to pull and redeploy. For an
      archive installation, upload the updated files and run
      <code>./redeploy.sh</code>.
    </p>
    <a
      v-if="release?.update_available"
      class="button primary"
      :href="release.url"
      target="_blank"
      rel="noopener noreferrer"
      >View release</a
    ><button class="button secondary" :disabled="pending" @click="check">
      {{ pending ? "Checking…" : "Check for updates" }}
    </button>
    <p v-if="error" class="page-alert" role="alert">{{ error }}</p>
  </section>
</template>
