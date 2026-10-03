<script setup lang="ts">
import { sitePermissions } from "~/types/workspace";
useHead({ title: "API Keys · OWLEYE" });
</script>

<template>
  <ConsoleSectionShell
    title="API Keys"
    eyebrow="App credentials"
    description="Choose credentials for sending events or accessing this app’s analytics."
    permission="rules_write"
    v-slot="{ apiBase, demo, site }"
  >
    <div v-if="!demo" class="settings-page-stack">
      <ApiCredentialGuide
        current="ingestion"
        :site-id="site?.tracking_id"
        :can-manage-analytics="sitePermissions(site).settings_manage"
      />
      <section v-if="sitePermissions(site).settings_manage" id="analytics-api" aria-label="Analytics API access">
        <DeveloperSettings :api-base="apiBase" :demo="demo" :site-id="site?.id" />
      </section>
      <ApiKeyManager
        id="ingestion-keys"
        :site-id="site?.id"
        :site-name="site?.name"
      />
    </div>
    <p v-else class="empty-state">
      API key changes are unavailable in this read-only workspace.
    </p>
  </ConsoleSectionShell>
</template>
