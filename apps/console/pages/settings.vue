<script setup lang="ts">
useHead({ title: "App settings · OWLEYE" });

const workspace = useConsoleWorkspace();

async function refreshWorkspace() {
  await workspace.initialize();
}
</script>

<template>
  <ConsoleSectionShell
    description="Rename the app, pause ingestion, control AI, and manage the owner-only sharp edges."
    eyebrow="Owner controls"
    permission="settings_manage"
    title="Settings"
    v-slot="{ apiBase, demo, site }"
  >
    <div class="settings-page-stack">
      <SiteSettingsManager
        :api-base="apiBase"
        :demo="demo"
        :site-id="site?.id"
        @changed="refreshWorkspace"
      >
        <template #app-details>
          <SiteManager
            :selected-site="site"
            :show-create="false"
            @changed="refreshWorkspace"
          />
        </template>
      </SiteSettingsManager>
      <PublicSharingManager v-if="!demo" :site-id="site?.id" />
      <AiSettings :site-id="site?.id" />
      <section
        id="analytics-api"
        class="settings-page-stack"
        aria-label="Analytics API keys and exports"
      >
        <ApiCredentialGuide
          current="analytics"
          :site-id="site?.tracking_id"
          :can-manage-analytics="true"
        />
        <DeveloperSettings
          :api-base="apiBase"
          :demo="demo"
          :site-id="site?.id"
        />
      </section>
    </div>
  </ConsoleSectionShell>
</template>
