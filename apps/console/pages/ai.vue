<script setup lang="ts">
useHead({ title: "AI analyst · OwlEye" });
</script>

<template>
  <ConsoleSectionShell
    description="Ask about traffic, locations, events, and campaign conversions. Review the supporting data alongside each answer."
    eyebrow="AI analyst · Beta"
    title="Your analytics assistant"
    v-slot="{ site }"
  >
    <AiMode :site-id="site?.id" :site-tracking-id="site?.tracking_id" />
  </ConsoleSectionShell>
</template>
