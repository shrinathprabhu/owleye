<script setup lang="ts">
definePageMeta({
  siteScoped: false,
  middleware: (to) => {
    if ("site" in to.query) {
      const { site: _site, ...query } = to.query;
      return navigateTo({ path: "/app/create", query }, { replace: true });
    }
  },
});
useHead({ title: "Create app · OWLEYE" });

async function changed(trackingId?: string) {
  if (trackingId) {
    await navigateTo({ path: "/", query: { site: trackingId } });
  }
}
</script>

<template>
  <ConsoleSectionShell
    title="Create app"
    eyebrow="Your workspace"
    description="Give your app a name and a domain to get started. Update or delete existing apps in Settings."
    :allow-empty="true"
    :permission="null"
    :show-site-selector="false"
  >
    <SiteManager :selected-site="null" :show-edit="false" @changed="changed" />
  </ConsoleSectionShell>
</template>
