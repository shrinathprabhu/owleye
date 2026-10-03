<script setup lang="ts">
const props = defineProps<{ value: string }>();
const status = ref("");
watch(() => props.value, () => { status.value = ""; });
async function copy() {
  try {
    await navigator.clipboard.writeText(props.value);
    status.value = "Tracking ID copied";
  } catch { status.value = "Could not copy. Select the tracking ID to copy it manually."; }
}
</script>
<template>
  <span class="copy-tracking">
    <button type="button" class="copy-tracking-button" aria-label="Copy tracking ID" title="Copy tracking ID" @click="copy">
      <svg aria-hidden="true" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8"><rect x="8" y="8" width="12" height="12" rx="2"/><path d="M16 8V5a2 2 0 0 0-2-2H5a2 2 0 0 0-2 2v9a2 2 0 0 0 2 2h3"/></svg>
    </button>
    <span class="copy-status" role="status">{{ status }}</span>
  </span>
</template>
<style scoped>
.copy-tracking { display: inline-flex; align-items: center; gap: .4rem; }
.copy-tracking-button { display: inline-grid; place-items: center; width: 40px; height: 40px; padding: 8px; border: 1px solid var(--line); border-radius: 8px; background: var(--surface, transparent); color: var(--brand-text-safe); cursor: pointer; flex-shrink: 0; }
.copy-tracking-button svg { width: 20px; height: 20px; }
.copy-status { font-size: .75rem; }
</style>
