<script setup lang="ts">
defineProps<{
  current: "ingestion" | "analytics";
  siteId?: string;
  canManageAnalytics: boolean;
}>();
</script>

<template>
  <section class="credential-guide" aria-label="Choose an API key by purpose">
    <div class="credential-options">
      <article
        class="management-card"
        :class="{ selected: current === 'ingestion' }"
      >
        <p class="panel-kicker">Send data to OwlEye</p>
        <h2>Event ingestion keys</h2>
        <p class="panel-copy">
          For a trusted backend that sends events for this app. These keys have
          <code>events:write</code> access only; they cannot read analytics or
          use AI.
        </p>
        <span v-if="current === 'ingestion'" class="status-badge neutral"
          >Manage below · owners and admins</span
        >
        <NuxtLink
          v-else
          class="text-button"
          :to="{
            path: '/api-keys',
            query: siteId ? { site: siteId } : {},
            hash: '#ingestion-keys',
          }"
        >
          Manage ingestion keys
        </NuxtLink>
      </article>
      <article
        class="management-card"
        :class="{ selected: current === 'analytics' }"
      >
        <p class="panel-kicker">Access data from OwlEye</p>
        <h2>Analytics API keys</h2>
        <p class="panel-copy">
          For reporting integrations and AI tools. Choose access to stats,
          privacy-safe events, or AI prompts, depending on your plan. These keys
          cannot send events or change app settings.
        </p>
        <span v-if="current === 'analytics'" class="status-badge neutral"
          >Manage below · owners only</span
        >
        <NuxtLink
          v-else-if="canManageAnalytics"
          class="text-button"
          :to="{
            path: '/api-keys',
            query: siteId ? { site: siteId } : {},
            hash: '#analytics-api',
          }"
        >
          Manage analytics keys
        </NuxtLink>
        <p v-else class="panel-copy">
          An app owner manages these keys on the API Keys page.
        </p>
      </article>
    </div>
    <p class="credential-browser-note">
      <strong>Installing browser tracking?</strong> Use the app’s public
      tracking ID with the browser SDK. Neither secret key belongs in browser
      code. Both key types are scoped to this app and must be kept on trusted
      servers.
    </p>
  </section>
</template>

<style scoped>
.credential-guide {
  min-width: 0;
}
.credential-options {
  display: grid;
  grid-template-columns: repeat(2, minmax(0, 1fr));
  gap: var(--space-4);
}
.credential-options .management-card {
  display: grid;
  align-content: start;
  justify-items: start;
  gap: var(--space-3);
  min-width: 0;
}
.credential-options .selected {
  border-color: var(--brand);
}
.credential-options h2 {
  font-size: 1.2rem;
}
.credential-options p,
.credential-options h2 {
  margin: 0;
}
.credential-options .text-button {
  text-align: left;
  white-space: normal;
}
.credential-browser-note {
  margin: var(--space-4) 0 0;
  color: var(--muted);
  line-height: 1.6;
}
@media (max-width: 1100px) {
  .credential-options {
    grid-template-columns: minmax(0, 1fr);
  }
}
</style>
