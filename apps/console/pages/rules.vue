<script setup lang="ts">
useHead({ title: "Tracking rules · OWLEYE" });
</script>

<template>
  <ConsoleSectionShell
    description="Turn intentional browser interactions into tidy, documented analytics events."
    eyebrow="No-code event capture"
    permission="rules_write"
    title="Rules"
    v-slot="{ demo, site, apiBase }"
  >
    <OptionalTrackingSetup
      v-if="site"
      kind="rules"
      :site-id="site.tracking_id"
      :api-base="apiBase"
    />
    <RuleManager
      :read-only="demo"
      :site-id="site?.id"
      :site-name="site?.name"
    />
  </ConsoleSectionShell>
</template>
