<script setup lang="ts">
defineProps<{ siteId: string; apiBase: string }>();
</script>
<template>
  <OptionalTrackingSetup
    kind="performance"
    :site-id="siteId"
    :api-base="apiBase"
  />
</template>
