<script setup lang="ts">
useHead({ title: "Tracking rules · OWLEYE" });
</script>

<template>
  <ConsoleSectionShell
    description="Turn intentional browser interactions into tidy, documented analytics events."
    eyebrow="No-code event capture"
    permission="rules_write"
    title="Rules"
    v-slot="{ demo, site }"
  >
    <RuleManager
      :read-only="demo"
      :site-id="site?.id"
      :site-name="site?.name"
    />
  </ConsoleSectionShell>
</template>
